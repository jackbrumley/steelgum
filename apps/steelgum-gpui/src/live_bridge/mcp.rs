use std::fs::File;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{MAX_REQUEST_BYTES, Query, Revision, query_live};

const LATEST_PROTOCOL: &str = "2025-11-25";
const STRUCTURED_PROTOCOL: &str = "2025-06-18";
const LEGACY_PROTOCOL: &str = "2025-03-26";
const MAX_IMAGE_BYTES: usize = 1024 * 1024;

const STATUS_TOOL: &str = "steelgum_status";
const OBJECTS_TOOL: &str = "steelgum_list_objects";
const CAPTURE_TOOL: &str = "steelgum_capture";

pub(super) fn run() -> Result<()> {
    serve(io::stdin().lock(), io::stdout().lock(), query_live)
}

fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    mut invoke: impl FnMut(Query) -> Result<Value>,
) -> Result<()> {
    let mut session = Session::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let count = (&mut input)
            .take(MAX_REQUEST_BYTES + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        if count as u64 > MAX_REQUEST_BYTES {
            write_message(
                &mut output,
                &error(Value::Null, -32600, "MCP message exceeds the size limit"),
            )?;
            return Ok(());
        }
        let response = match serde_json::from_slice(&line) {
            Ok(message) => session.handle(message, &mut invoke),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(response) = response {
            write_message(&mut output, &response)?;
        }
    }
}

fn write_message(output: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *output, response)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

#[derive(Default)]
struct Session {
    protocol: Option<&'static str>,
}

impl Session {
    fn handle(
        &mut self,
        message: Value,
        invoke: &mut impl FnMut(Query) -> Result<Value>,
    ) -> Option<Value> {
        let valid_id = message
            .get("id")
            .is_none_or(|id| id.is_string() || id.as_i64().is_some() || id.as_u64().is_some());
        let method = message.get("method").and_then(Value::as_str);
        if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || !valid_id
            || method.is_none()
        {
            return Some(error(Value::Null, -32600, "Invalid request"));
        }
        // JSON-RPC notifications, including notifications/initialized, have no reply.
        let id = message.get("id")?.clone();
        let params = &message["params"];
        Some(match method? {
            "initialize" => self.initialize(id, params),
            "ping" => success(id, json!({})),
            "tools/list" | "tools/call" if self.protocol.is_none() => {
                error(id, -32002, "Initialize the Steelgum MCP session first")
            }
            "tools/list" => success(id, json!({"tools": tools()})),
            "tools/call" => self.call_tool(id, params, invoke),
            _ => error(id, -32601, "Method not found"),
        })
    }

    fn initialize(&mut self, id: Value, params: &Value) -> Value {
        if self.protocol.is_some() {
            return error(id, -32600, "Steelgum MCP is already initialized");
        }
        let Some(requested) = params["protocolVersion"].as_str() else {
            return error(id, -32602, "initialize requires a protocol version");
        };
        let protocol = match requested {
            LEGACY_PROTOCOL => LEGACY_PROTOCOL,
            STRUCTURED_PROTOCOL => STRUCTURED_PROTOCOL,
            _ => LATEST_PROTOCOL,
        };
        self.protocol = Some(protocol);
        success(
            id,
            json!({
                "protocolVersion": protocol,
                "capabilities": {"tools": {}},
                "serverInfo": {
                    "name": "steelgum",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    fn call_tool(
        &self,
        id: Value,
        params: &Value,
        invoke: &mut impl FnMut(Query) -> Result<Value>,
    ) -> Value {
        let Some(name) = params["name"].as_str() else {
            return error(id, -32602, "tools/call requires a tool name");
        };
        let arguments = match params.get("arguments") {
            Some(Value::Object(arguments)) => arguments.clone(),
            None => Map::new(),
            Some(_) => return error(id, -32602, "Tool arguments must be an object"),
        };
        let query = match name {
            STATUS_TOOL | OBJECTS_TOOL if !arguments.is_empty() => {
                Err(anyhow::anyhow!("This tool takes no arguments"))
            }
            STATUS_TOOL => Ok(Query::Status),
            OBJECTS_TOOL => Ok(Query::ListObjects),
            CAPTURE_TOOL => parse_capture(arguments),
            _ => return error(id, -32602, format!("Unknown tool {name}")),
        };
        let result = match query.and_then(invoke) {
            Ok(data) => tool_result(data, name == CAPTURE_TOOL, self.protocol),
            Err(failure) => json!({
                "content": [{"type": "text", "text": format!("{failure:#}")}],
                "isError": true
            }),
        };
        success(id, result)
    }
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: Value, code: i32, message: impl AsRef<str>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message.as_ref()}
    })
}

fn tools() -> Value {
    let no_arguments = json!({"type": "object", "additionalProperties": false});
    json!([
        {
            "name": STATUS_TOOL,
            "description": "Read the running Steelgum window's project, runner, selection, and errors.",
            "inputSchema": no_arguments,
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": OBJECTS_TOOL,
            "description": "List objects and changes from the running Steelgum window.",
            "inputSchema": no_arguments,
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": CAPTURE_TOOL,
            "description": "Capture a PNG of the visible viewport, or frame an object by its stable ID.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "object_id": {"type": "string", "minLength": 1},
                    "revision": {"type": "string", "enum": ["before", "after"]}
                },
                "dependentRequired": {"revision": ["object_id"]}
            }
        }
    ])
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureArgs {
    object_id: Option<String>,
    revision: Option<Revision>,
}

fn parse_capture(arguments: Map<String, Value>) -> Result<Query> {
    let CaptureArgs {
        object_id,
        revision,
    } = serde_json::from_value(Value::Object(arguments))?;
    if object_id.as_ref().is_some_and(String::is_empty) {
        anyhow::bail!("object_id must contain a stable object ID");
    }
    if revision.is_some() && object_id.is_none() {
        anyhow::bail!("A capture revision requires an object ID");
    }
    Ok(Query::Capture {
        object_id,
        revision,
    })
}

fn tool_result(data: Value, include_image: bool, protocol: Option<&str>) -> Value {
    let mut content = vec![json!({"type": "text", "text": data.to_string()})];
    if include_image && let Some(image) = screenshot_content(&data) {
        content.push(image);
    }
    let mut result = json!({"content": content});
    if protocol != Some(LEGACY_PROTOCOL) && data.is_object() {
        result["structuredContent"] = data;
    }
    result
}

fn screenshot_content(data: &Value) -> Option<Value> {
    // Only a successful response from the authenticated local bridge supplies the path.
    let path = Path::new(data["png_path"].as_str()?);
    if !path.is_absolute()
        || path.extension()? != "png"
        || !std::fs::symlink_metadata(path).ok()?.is_file()
    {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    Some(json!({
        "type": "image",
        "mimeType": "image/png",
        "data": STANDARD.encode(bytes)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_routes_calls_without_replying_to_notifications() {
        let input = b"
{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}
{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}
{\"jsonrpc\":\"2.0\",\"id\":\"capture\",\"method\":\"tools/call\",\"params\":{\"name\":\"steelgum_capture\",\"arguments\":{\"object_id\":\"crate\",\"revision\":\"before\"}}}
";
        let mut output = Vec::new();
        serve(input.strip_prefix(b"\n").unwrap(), &mut output, |query| {
            assert!(
                matches!(query, Query::Capture { object_id: Some(ref id), revision: Some(Revision::Before) } if id == "crate")
            );
            Ok(json!({"revision": "before"}))
        })
        .unwrap();
        let replies: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["result"]["protocolVersion"], STRUCTURED_PROTOCOL);
        assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 3);
        assert_eq!(replies[2]["id"], "capture");
        assert_eq!(
            replies[2]["result"]["structuredContent"]["revision"],
            "before"
        );
    }

    #[test]
    fn distinguishes_tool_errors_from_protocol_errors() {
        let mut session = Session {
            protocol: Some(LATEST_PROTOCOL),
        };
        let mut invoke = |_| -> Result<Value> { anyhow::bail!("Open Steelgum before querying it") };
        let status = session
            .handle(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name": STATUS_TOOL}}),
                &mut invoke,
            )
            .unwrap();
        assert_eq!(status["result"]["isError"], true);
        let unknown = session
            .handle(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"unknown"}}),
                &mut invoke,
            )
            .unwrap();
        assert_eq!(unknown["error"]["code"], -32602);
    }
}
