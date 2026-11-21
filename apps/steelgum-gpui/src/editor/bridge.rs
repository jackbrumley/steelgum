use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail, ensure};
use gpui::Context;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use steelgum::review::{ObjectChange, ObjectChanges};

use crate::live_bridge::{EDITOR_POLL_INTERVAL, Query, Revision, Session, take_requests};
use crate::viewport::PendingScreenshot;

use super::{Job, ReviewWindow, RunState, state_directory};

enum Reply {
    Json(Value),
    Screenshot(PendingScreenshot, Value),
}

impl ReviewWindow {
    pub(crate) fn start_agent_bridge(&mut self, cx: &mut Context<Self>) {
        let result = state_directory()
            .context("No Steelgum state directory is available")
            .and_then(|path| Session::start(&path));
        match result {
            Ok(session) => self.serve_agent_bridge(session, cx),
            Err(error) => eprintln!("Could not start the Steelgum live agent bridge: {error:#}"),
        }
    }

    fn serve_agent_bridge(&mut self, session: Session, cx: &mut Context<Self>) {
        let path = session.path().to_owned();
        let task = cx.spawn(async move |window, cx| {
            let background = cx.background_executor().clone();
            loop {
                background.timer(EDITOR_POLL_INTERVAL).await;
                let request_path = path.clone();
                let requests = match background
                    .spawn(async move { take_requests(&request_path) })
                    .await
                {
                    Ok(requests) => requests,
                    Err(error) => {
                        eprintln!("Could not read Steelgum live requests: {error:#}");
                        break;
                    }
                };
                for incoming in requests {
                    let query = &incoming.query;
                    let Ok(reply) = window.update(cx, |window, _| window.agent_reply(query)) else {
                        return;
                    };
                    let answered = background
                        .spawn(async move {
                            let result = reply
                                .and_then(finish_reply)
                                .map_err(|error| format!("{error:#}"));
                            incoming.respond(result)
                        })
                        .await;
                    if let Err(error) = answered {
                        eprintln!("Could not answer a Steelgum live request: {error:#}");
                    }
                }
            }
        });
        self.agent_bridge = Some(session.attach(task));
    }

    fn agent_reply(&self, query: &Query) -> Result<Reply> {
        match query {
            Query::Status => Ok(Reply::Json(self.agent_status())),
            Query::ListObjects => Ok(Reply::Json(self.agent_objects())),
            Query::Capture {
                object_id,
                revision,
            } => self.agent_capture(object_id.as_deref(), *revision),
        }
    }

    fn agent_status(&self) -> Value {
        let phase = match self.run_state {
            RunState::Idle => "idle",
            RunState::Running(Job::Build) => "building",
            RunState::Running(Job::Play) => "playing",
            RunState::Finished(Ok(_)) => "finished",
            RunState::Finished(Err(_)) => "failed",
        };
        json!({
            "protocol": 1,
            "steelgum_version": env!("CARGO_PKG_VERSION"),
            "project": self.agent_project(self.before),
            "manifest_runner": self.project.manifest.as_ref().and_then(|manifest| manifest.runner.as_ref()),
            "can_build": self.project.can_build(),
            "can_play": self.project.can_run(),
            "selected_object": self.selected,
            "viewport": { "projection": if self.top_down { "2d" } else { "3d" }, "focused": self.focus },
            "opening_project": self.loading,
            "project_error": self.status,
            "scene_error": self.live_issue,
            "job": {
                "phase": phase,
                "message": self.run_state.message(),
                "progress": self.job_progress,
                "recent_log": self.job_control.as_ref().map(|job| job.output()),
            },
        })
    }

    fn agent_objects(&self) -> Value {
        json!({
            "project": self.agent_project(self.before),
            "selected_object": self.selected,
            "objects": self.scene_revision().objects(),
            "changes": self.project.review.changes().iter().map(agent_change).collect::<Vec<_>>(),
        })
    }

    fn agent_capture(&self, object_id: Option<&str>, revision: Option<Revision>) -> Result<Reply> {
        let mut view = self.viewport_view();
        if let Some(id) = object_id {
            view.before = revision.map_or(view.before, |revision| revision == Revision::Before);
            ensure!(
                view.scene().object(id).is_some(),
                "Object {id} is absent from the {} scene.",
                revision_label(view.before)
            );
            view.selection = Some(id.to_owned());
            view.focus = true;
        } else if revision.is_some() {
            bail!("Choose an object ID to capture a particular revision.");
        }
        let scene = view.scene().to_json()?;
        let project = self.agent_project(view.before);
        let capture = if object_id.is_some() {
            self.viewport.capture_framed(&view)?
        } else {
            self.viewport.capture(&view)?
        };
        Ok(Reply::Screenshot(
            capture,
            json!({
                "project": project,
                "scene_sha256": format!("{:x}", Sha256::digest(scene.as_bytes())),
                "object_id": view.selection,
                "framed_object": object_id.is_some(),
                "projection": if view.top_down { "2d" } else { "3d" },
            }),
        ))
    }

    fn agent_project(&self, before: bool) -> Value {
        let (worktree, scene) = self.project.source.as_ref().map_or_else(
            || (None, PathBuf::from("examples/door-game/scenes/room.json")),
            |(worktree, scene)| (Some(worktree.as_path()), scene.clone()),
        );
        json!({
            "name": self.project.name,
            "worktree": worktree,
            "scene": scene,
            "revision": revision_label(before),
            "git_base": self.project.review.base_revision(),
        })
    }
}

fn finish_reply(reply: Reply) -> Result<Value> {
    match reply {
        Reply::Json(json) => Ok(json),
        Reply::Screenshot(capture, mut metadata) => {
            let bytes = capture()?;
            let mut file = tempfile::Builder::new()
                .prefix("steelgum-live-")
                .suffix(".png")
                .tempfile()?;
            file.write_all(&bytes)?;
            let (_, path) = file.keep()?;
            metadata["png_path"] = json!(path);
            Ok(metadata)
        }
    }
}

fn revision_label(before: bool) -> &'static str {
    if before { "before" } else { "after" }
}

fn agent_change(change: &ObjectChange) -> Value {
    match change.changes {
        ObjectChanges::Added => json!({"id": change.id, "kind": "added"}),
        ObjectChanges::Removed => json!({"id": change.id, "kind": "removed"}),
        ObjectChanges::Modified {
            name,
            center,
            size,
            color,
            solid,
        } => {
            let fields = [
                (name, "name"),
                (center, "center"),
                (size, "size"),
                (color, "color"),
                (solid, "solid"),
            ]
            .into_iter()
            .filter_map(|(changed, field)| changed.then_some(field))
            .collect::<Vec<_>>();
            json!({"id": change.id, "kind": "modified", "fields": fields})
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use gpui::{AppContext, HeadlessAppContext};
    use gpui_wgpu::CosmicTextSystem;

    use super::*;
    use crate::editor::Project;

    #[test]
    fn live_status_and_objects_describe_the_open_window() -> Result<()> {
        let mut app = HeadlessAppContext::new(Arc::new(CosmicTextSystem::new("sans-serif")));
        let project = Project::demo(crate::demo()?);
        let directory = tempfile::tempdir()?;
        let session = Session::start(directory.path())?;
        let window = app.new(|cx| {
            let mut window = ReviewWindow::new(project, cx);
            window.serve_agent_bridge(session, cx);
            window
        });
        app.run_until_parked();
        app.update(|cx| {
            window.update(cx, |window, _| {
                window.before = true;
                window.live_issue = Some("Invalid scene save".to_owned());
                window.run_state = RunState::Finished(Err("Could not build".to_owned()));
                let data = window.agent_objects();
                assert!(
                    data["changes"].as_array().unwrap().iter().any(|item| {
                        item["id"] == "plate" && item["fields"] == json!(["color"])
                    })
                );
            });
        });
        let status = thread::scope(|scope| -> Result<Value> {
            let client =
                scope.spawn(|| crate::live_bridge::query_at(directory.path(), Query::Status));
            let started = Instant::now();
            while !client.is_finished() {
                ensure!(
                    started.elapsed() < Duration::from_secs(3),
                    "Live editor did not answer the test client"
                );
                app.advance_clock(EDITOR_POLL_INTERVAL);
                app.run_until_parked();
                thread::sleep(Duration::from_millis(10));
            }
            client.join().unwrap()
        })?;
        assert_eq!(status["project"]["revision"], "before");
        assert_eq!(status["scene_error"], "Invalid scene save");
        assert_eq!(status["job"]["message"], "Could not build");
        assert_eq!(status["selected_object"], "plate");
        Ok(())
    }
}
