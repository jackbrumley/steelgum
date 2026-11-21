use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use gpui::Context;

use super::{Project, ReviewWindow, Tool};

const MANIFEST: &str = "apps/door-game-bevy/Cargo.toml";

#[derive(Clone, Copy)]
pub(super) enum Job {
    Build,
    Play,
}

impl Job {
    fn command(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Play => "run",
        }
    }

    fn completed(self) -> &'static str {
        match self {
            Self::Build => "Build succeeded",
            Self::Play => "Play session ended",
        }
    }
}

pub(super) enum RunState {
    Idle,
    Running(Job),
    Finished(Result<&'static str, String>),
}

impl RunState {
    pub(super) fn busy(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    pub(super) fn message(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Running(Job::Build) => Some("Building game…"),
            Self::Running(Job::Play) => Some("Starting or playing the game…"),
            Self::Finished(Ok(message)) => Some(message),
            Self::Finished(Err(error)) => Some(error),
        }
    }
}

struct Runner {
    manifest: PathBuf,
    scene: Option<PathBuf>,
}

impl Project {
    fn runner(&self) -> Option<Runner> {
        let (root, scene) = if let Some((root, scene)) = &self.source {
            (root.clone(), Some(root.join(scene)))
        } else {
            (Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."), None)
        };
        let manifest = root.join(MANIFEST);
        manifest.is_file().then_some(Runner { manifest, scene })
    }

    pub(super) fn can_run(&self) -> bool {
        self.runner().is_some()
    }
}

impl Runner {
    fn execute(
        self,
        job: Job,
        release: bool,
        example_scene: Option<String>,
    ) -> Result<&'static str> {
        let manifest = self
            .manifest
            .canonicalize()
            .context("Could not locate the game runner")?;
        // Reuse the workspace toolchain even when the binary is opened
        // outside Cargo and the system default Rust is older.
        let cargo_path = Path::new(env!("CARGO"));
        let rustc = cargo_path.with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        let mut cargo = Command::new(cargo_path);
        if rustc.is_file() {
            cargo.env("RUSTC", rustc);
        }
        cargo
            .current_dir(manifest.parent().expect("manifest belongs to a directory"))
            .arg(job.command())
            .arg("--manifest-path")
            .arg(manifest);
        if release {
            cargo.arg("--release");
        }
        let example = example_scene
            .map(|json| {
                let directory =
                    tempfile::tempdir().context("Could not prepare the example scene")?;
                std::fs::write(directory.path().join("room.json"), json)
                    .context("Could not write the example scene")?;
                Ok::<_, anyhow::Error>(directory)
            })
            .transpose()?;
        let scene = self
            .scene
            .or_else(|| example.as_ref().map(|dir| dir.path().join("room.json")));
        if matches!(job, Job::Play)
            && let Some(scene) = scene
        {
            cargo.arg("--").arg("--scene").arg(
                scene
                    .canonicalize()
                    .with_context(|| format!("Could not open {}", scene.display()))?,
            );
        }
        let output = cargo
            .output()
            .context("Could not run Cargo. Check that Rust and Cargo are installed")?;
        if !output.status.success() {
            // Show the last diagnostics in the pane and retain the full output
            // in the development terminal.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            eprint!("{stdout}{stderr}");
            let diagnostics = if stderr.trim().is_empty() {
                &stdout
            } else {
                &stderr
            };
            let lines: Vec<_> = diagnostics.lines().collect();
            let tail = lines[lines.len().saturating_sub(10)..].join("\n");
            bail!(
                "Cargo {} exited with {}\n{tail}",
                job.command(),
                output.status
            );
        }
        Ok(job.completed())
    }
}

impl ReviewWindow {
    pub(super) fn run_game(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.loading || self.run_state.busy() {
            return;
        }
        let Some(runner) = self.project.runner() else {
            self.show_tool(Tool::Build);
            cx.notify();
            return;
        };
        let example_scene = if matches!(job, Job::Play) && self.project.source.is_none() {
            match self.project.review.after().to_json() {
                Ok(json) => Some(json),
                Err(error) => {
                    self.show_tool(Tool::Build);
                    self.run_state = RunState::Finished(Err(format!(
                        "Could not prepare the example scene: {error}"
                    )));
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        if matches!(job, Job::Build) {
            self.show_tool(Tool::Build);
        }
        self.run_state = RunState::Running(job);
        cx.notify();
        let release = self.release;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    runner
                        .execute(job, release, example_scene)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if result.is_err() {
                    view.show_tool(Tool::Build);
                }
                view.run_state = RunState::Finished(result);
                cx.notify();
            });
        })
        .detach();
    }
}
