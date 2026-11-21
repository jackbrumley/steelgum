//! Local request/reply transport between a running editor and a stdio agent.

mod mcp;

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail, ensure};
use gpui::Task;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tempfile::{NamedTempFile, TempDir};

pub(super) const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const EDITOR_POLL_INTERVAL: Duration = Duration::from_millis(250);

const ENDPOINT: &str = "live-editor.json";
const SESSION_PREFIX: &str = "live-editor-";
const REQUEST_PREFIX: &str = "request-";
const REQUEST: &str = "request.json";
const PROCESSING: &str = "processing.json";
const RESPONSE: &str = "response.json";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Revision {
    Before,
    After,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Query {
    Status,
    ListObjects,
    Capture {
        object_id: Option<String>,
        revision: Option<Revision>,
    },
}

#[derive(Deserialize, Serialize)]
struct Endpoint {
    directory: PathBuf,
}

#[derive(Deserialize, Serialize)]
struct Response {
    result: Result<Value, String>,
}

pub(crate) struct Bridge {
    // Drop the polling task before removing its private request directory.
    _task: Task<()>,
    _directory: TempDir,
}

pub(crate) struct Session {
    directory: TempDir,
}

impl Session {
    pub(crate) fn start(state: &Path) -> Result<Self> {
        fs::create_dir_all(state)?;
        let state = state.canonicalize()?;
        let temporary = std::env::temp_dir().canonicalize()?;
        let directory = private_directory(&temporary, SESSION_PREFIX)?;
        write_json(
            &state.join(ENDPOINT),
            &Endpoint {
                directory: directory.path().to_owned(),
            },
        )?;
        Ok(Self { directory })
    }

    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(crate) fn attach(self, task: Task<()>) -> Bridge {
        Bridge {
            _task: task,
            _directory: self.directory,
        }
    }
}

pub(crate) struct Incoming {
    directory: PathBuf,
    pub(crate) query: Query,
}

impl Incoming {
    pub(crate) fn respond(self, result: Result<Value, String>) -> Result<()> {
        let path = self.directory.join(RESPONSE);
        let mut response = Response { result };
        let mut bytes = serde_json::to_vec(&response)?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            response.result = Err("The live editor response exceeds the size limit.".to_owned());
            bytes = serde_json::to_vec(&response)?;
        }
        // A request can disappear when the invoking client times out or exits.
        match write_bytes(&path, &bytes) {
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(())
            }
            result => result,
        }
    }
}

pub(crate) fn take_requests(session: &Path) -> Result<Vec<Incoming>> {
    let mut requests = Vec::new();
    for entry in fs::read_dir(session)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(REQUEST_PREFIX)
            || !entry.file_type()?.is_dir()
        {
            continue;
        }
        let directory = entry.path();
        let ready = directory.join(REQUEST);
        let processing = directory.join(PROCESSING);
        match fs::rename(ready, &processing) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        match read_json(&processing, MAX_REQUEST_BYTES) {
            Ok(query) => requests.push(Incoming { directory, query }),
            Err(error) => {
                Incoming {
                    directory,
                    query: Query::Status,
                }
                .respond(Err(format!("Invalid live editor request: {error}")))?;
            }
        }
        if requests.len() == 16 {
            break;
        }
    }
    Ok(requests)
}

fn query_live(query: Query) -> Result<Value> {
    let state =
        crate::editor::state_directory().context("No Steelgum state directory is available")?;
    query_at(&state, query)
}

pub(crate) fn query_at(state: &Path, query: Query) -> Result<Value> {
    const NOT_RUNNING: &str = "Open a Steelgum window before using the live tools.";
    let state = state.canonicalize().context(NOT_RUNNING)?;
    let endpoint: Endpoint =
        read_json(&state.join(ENDPOINT), MAX_REQUEST_BYTES).context(NOT_RUNNING)?;
    let directory = endpoint.directory.canonicalize().context(NOT_RUNNING)?;
    let temporary = std::env::temp_dir().canonicalize()?;
    ensure!(
        directory.parent() == Some(temporary.as_path())
            && directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(SESSION_PREFIX)),
        "Invalid Steelgum live editor address."
    );
    let timeout = if matches!(query, Query::Capture { .. }) {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(5)
    };
    let request = private_directory(&directory, REQUEST_PREFIX).context(NOT_RUNNING)?;
    write_json(&request.path().join(REQUEST), &query)?;
    let started = Instant::now();
    loop {
        let path = request.path().join(RESPONSE);
        match read_json::<Response>(&path, MAX_RESPONSE_BYTES) {
            Ok(response) => return response.result.map_err(anyhow::Error::msg),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) => {}
            Err(error) => return Err(error),
        }
        ensure!(
            directory.is_dir(),
            "The Steelgum window closed before responding."
        );
        ensure!(
            started.elapsed() < timeout,
            "The Steelgum window did not respond in time."
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn private_directory(parent: &Path, prefix: &str) -> std::io::Result<TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir_in(parent)
}

fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Expected a regular request file."
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Live editor message exceeds the size limit."
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_bytes(path, &serde_json::to_vec(value)?)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Live editor message has no parent directory")?;
    let mut file = NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn run_mcp() -> Result<()> {
    mcp::run()
}

pub(crate) fn run_cli(arguments: &[OsString]) -> Result<()> {
    const USAGE: &str =
        "Usage: steelgum --live status | list-objects | capture [OBJECT_ID [before|after]]";
    let query = match arguments {
        [argument] if argument == "--help" || argument == "-h" => {
            println!("{USAGE}");
            return Ok(());
        }
        [argument] if argument == "status" => Query::Status,
        [argument] if argument == "list-objects" => Query::ListObjects,
        [argument] if argument == "capture" => Query::Capture {
            object_id: None,
            revision: None,
        },
        [argument, id] if argument == "capture" => Query::Capture {
            object_id: Some(
                id.to_str()
                    .context("Expected a UTF-8 object ID")?
                    .to_owned(),
            ),
            revision: None,
        },
        [argument, id, revision] if argument == "capture" => Query::Capture {
            object_id: Some(
                id.to_str()
                    .context("Expected a UTF-8 object ID")?
                    .to_owned(),
            ),
            revision: Some(match revision.to_str() {
                Some("before") => Revision::Before,
                Some("after") => Revision::After,
                _ => bail!(USAGE),
            }),
        },
        _ => bail!(USAGE),
    };
    println!("{}", serde_json::to_string_pretty(&query_live(query)?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn client_reads_a_reply_from_the_active_session() -> Result<()> {
        let state = tempfile::tempdir()?;
        let editor = Session::start(state.path())?;
        thread::scope(|scope| -> Result<()> {
            let client = scope.spawn(|| query_at(state.path(), Query::Status));
            let started = Instant::now();
            loop {
                if let Some(request) = take_requests(editor.path())?.pop() {
                    assert!(matches!(request.query, Query::Status));
                    request.respond(Ok(json!({"project": "Groundsector"})))?;
                    break;
                }
                ensure!(
                    started.elapsed() < Duration::from_secs(2),
                    "Client never sent its request"
                );
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(client.join().unwrap()?, json!({"project": "Groundsector"}));
            Ok(())
        })
    }

    #[test]
    fn an_ended_session_is_reported_as_closed() -> Result<()> {
        let state = tempfile::tempdir()?;
        drop(Session::start(state.path())?);
        let error = query_at(state.path(), Query::Status).unwrap_err();
        assert!(error.to_string().contains("Open a Steelgum window"));
        Ok(())
    }
}
