use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use gpui::Context;
use steelgum::project::resolve_project_path;

use super::{Project, ReviewWindow, Tool};

const DOOR_MANIFEST: &str = "apps/door-game-bevy/Cargo.toml";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Job {
    Build,
    Play,
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
            Self::Running(Job::Play) => Some("Preparing the game…"),
            Self::Finished(Ok(message)) => Some(message),
            Self::Finished(Err(error)) => Some(error),
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct JobControl(Arc<JobSignals>);

#[derive(Default)]
struct JobSignals {
    cancelled: AtomicBool,
    output: Mutex<VecDeque<String>>,
}

impl JobControl {
    fn record(&self, line: String) {
        if let Ok(mut output) = self.0.output.lock() {
            if output.len() == 12 {
                output.pop_front();
            }
            output.push_back(line);
        }
    }

    fn output(&self) -> String {
        self.0
            .output
            .lock()
            .map(|lines| {
                lines
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    fn latest(&self) -> Option<String> {
        self.0.output.lock().ok()?.back().cloned()
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
    }
}

enum Program {
    Cargo {
        manifest: PathBuf,
        binary: String,
        checkout_toolchain: bool,
    },
    Installed(PathBuf),
}

struct Runner {
    program: Program,
    worktree: PathBuf,
    scene: Option<PathBuf>,
}

impl Project {
    fn runner(&self) -> Result<Option<Runner>> {
        if let Some((root, scene)) = &self.source {
            let (manifest, binary, checkout_toolchain) = match &self.manifest {
                Some(project) => {
                    let Some(runner) = &project.runner else {
                        return Ok(None);
                    };
                    (
                        resolve_project_path(root, &runner.manifest)?,
                        runner.binary.clone(),
                        false,
                    )
                }
                None if root.join(DOOR_MANIFEST).is_file() => (
                    resolve_project_path(root, Path::new(DOOR_MANIFEST))?,
                    "steelgum".into(),
                    true,
                ),
                None => return Ok(None),
            };
            return Ok(Some(Runner {
                program: Program::Cargo {
                    manifest,
                    binary,
                    checkout_toolchain,
                },
                worktree: root.clone(),
                scene: Some(resolve_project_path(root, scene)?),
            }));
        }
        if let Some(helper) = crate::packaged_player() {
            return Ok(Some(Runner {
                worktree: helper
                    .parent()
                    .context("Bundled player has no parent folder")?
                    .to_owned(),
                program: Program::Installed(helper),
                scene: None,
            }));
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest = root.join(DOOR_MANIFEST);
        if !manifest.is_file() {
            return Ok(None);
        }
        Ok(Some(Runner {
            program: Program::Cargo {
                manifest: manifest.canonicalize()?,
                binary: "steelgum".into(),
                checkout_toolchain: true,
            },
            worktree: root.canonicalize()?,
            scene: None,
        }))
    }

    pub(super) fn can_run(&self) -> bool {
        match &self.source {
            Some((root, _)) => self.manifest.as_ref().map_or_else(
                || root.join(DOOR_MANIFEST).is_file(),
                |manifest| manifest.runner.is_some(),
            ),
            None => crate::packaged_player().is_some() || checkout_has_demo(),
        }
    }

    pub(super) fn can_build(&self) -> bool {
        if self.source.is_some() {
            self.can_run()
        } else {
            crate::packaged_player().is_none() && checkout_has_demo()
        }
    }
}

fn checkout_has_demo() -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DOOR_MANIFEST)
        .is_file()
}

impl Runner {
    fn execute(
        self,
        job: Job,
        release: bool,
        example_scene: Option<String>,
        control: JobControl,
    ) -> Result<&'static str> {
        let executable = match &self.program {
            Program::Cargo {
                manifest,
                binary,
                checkout_toolchain,
            } => {
                let Some(executable) =
                    build(manifest, binary, *checkout_toolchain, release, &control)?
                else {
                    return Ok("Build cancelled");
                };
                executable
            }
            Program::Installed(helper) => helper.clone(),
        };
        if job == Job::Build {
            return Ok("Build succeeded");
        }
        if control.cancelled() {
            return Ok("Play cancelled");
        }
        let example = example_scene
            .map(|json| {
                let directory =
                    tempfile::tempdir().context("Could not prepare the example scene")?;
                std::fs::write(directory.path().join("room.json"), json)?;
                Ok::<_, anyhow::Error>(directory)
            })
            .transpose()?;
        let scene = self
            .scene
            .or_else(|| example.as_ref().map(|dir| dir.path().join("room.json")));
        let mut player = Command::new(executable);
        player.current_dir(self.worktree).stderr(Stdio::piped());
        if let Some(scene) = scene {
            player.arg("--scene").arg(scene);
        }
        control.record("Playing game…".to_owned());
        let mut child = spawn(&mut player).context("Could not start the game")?;
        let stderr = child.stderr.take().context("Game stderr unavailable")?;
        let status = std::thread::scope(|scope| -> Result<_> {
            let diagnostics = scope.spawn(|| read_diagnostics(stderr, &control));
            let status = wait(&mut child, &control)?;
            diagnostics
                .join()
                .map_err(|_| anyhow::anyhow!("Game log reader stopped"))??;
            Ok(status)
        })?;
        match status {
            Some(status) if status.success() => Ok("Play session ended"),
            Some(status) => bail!("Game exited with {status}\n{}", control.output()),
            None => Ok("Play session stopped"),
        }
    }
}

fn build(
    manifest: &Path,
    binary: &str,
    checkout_toolchain: bool,
    release: bool,
    control: &JobControl,
) -> Result<Option<PathBuf>> {
    let built_with = Path::new(env!("CARGO"));
    let use_checkout_toolchain = checkout_toolchain && built_with.is_file();
    let cargo = if use_checkout_toolchain {
        built_with.as_os_str()
    } else {
        OsStr::new("cargo")
    };
    let mut command =
        steelgum::build::build_command(cargo).map_err(|error| anyhow::anyhow!("{error}"))?;
    command
        .current_dir(
            manifest
                .parent()
                .context("Cargo manifest has no parent folder")?,
        )
        .env_remove("RUSTUP_TOOLCHAIN")
        .arg("--manifest-path")
        .arg(manifest)
        .args(["--bin", binary, "--message-format=json", "--color=never"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if use_checkout_toolchain {
        let rustc = built_with.with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        if rustc.is_file() {
            command.env("RUSTC", rustc);
        }
    }
    if release {
        command.arg("--release");
    }
    let mut child = spawn(&mut command)
        .context("Could not run Cargo. Install Rust and Cargo to build this project")?;
    let stdout = child.stdout.take().context("Cargo stdout unavailable")?;
    let stderr = child.stderr.take().context("Cargo stderr unavailable")?;
    let (status, artifact) = std::thread::scope(|scope| -> Result<_> {
        let messages = scope.spawn(|| read_build_messages(stdout, binary, control));
        let diagnostics = scope.spawn(|| read_diagnostics(stderr, control));
        let status = wait(&mut child, control)?;
        diagnostics
            .join()
            .map_err(|_| anyhow::anyhow!("Cargo log reader stopped"))??;
        let artifact = messages
            .join()
            .map_err(|_| anyhow::anyhow!("Cargo output reader stopped"))??;
        Ok((status, artifact))
    })?;
    match status {
        None => Ok(None),
        Some(status) if status.success() => artifact
            .map(Some)
            .context("Cargo did not report the built executable"),
        Some(status) => bail!("Cargo build exited with {status}\n{}", control.output()),
    }
}

fn read_diagnostics(input: impl std::io::Read, control: &JobControl) -> Result<()> {
    for line in BufReader::new(input).lines() {
        let line = line?;
        eprintln!("{line}");
        control.record(line);
    }
    Ok(())
}

fn read_build_messages(
    input: impl std::io::Read,
    binary: &str,
    control: &JobControl,
) -> Result<Option<PathBuf>> {
    let mut executable = None;
    for line in BufReader::new(input).lines() {
        let line = line?;
        if !line.starts_with('{') {
            continue;
        }
        let message: serde_json::Value = serde_json::from_str(&line)?;
        match message["reason"].as_str() {
            Some("compiler-artifact") if message["target"]["name"] == binary => {
                if let Some(path) = message["executable"].as_str() {
                    executable = Some(PathBuf::from(path));
                }
            }
            Some("compiler-message") => {
                if let Some(rendered) = message["message"]["rendered"].as_str() {
                    eprint!("{rendered}");
                    for line in rendered.lines().filter(|line| line.len() < 1000) {
                        control.record(line.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(executable)
}

fn spawn(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    command.spawn()
}

fn wait(child: &mut Child, control: &JobControl) -> Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if control.cancelled() {
            let pid = child.id();
            #[cfg(unix)]
            let stopped = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .status();
            #[cfg(windows)]
            let stopped = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status();
            #[cfg(not(any(unix, windows)))]
            let stopped: std::io::Result<ExitStatus> = Err(std::io::ErrorKind::Unsupported.into());
            if !stopped.is_ok_and(|status| status.success()) && child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(75));
    }
}

impl ReviewWindow {
    pub(super) fn cancel_job(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = &self.job_control {
            job.cancel();
            self.job_progress = Some("Stopping…".into());
            cx.notify();
        }
    }

    pub(super) fn run_game(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.loading || self.run_state.busy() {
            return;
        }
        match self.prepare_job(job) {
            Ok(Some((runner, example))) => self.spawn_job(job, runner, example, cx),
            Ok(None) => {
                self.show_tool(Tool::Build);
                cx.notify();
            }
            Err(error) => {
                self.show_tool(Tool::Build);
                self.run_state = RunState::Finished(Err(format!("{error:#}")));
                cx.notify();
            }
        }
    }

    fn prepare_job(&self, job: Job) -> Result<Option<(Runner, Option<String>)>> {
        let Some(runner) = self.project.runner()? else {
            return Ok(None);
        };
        if job == Job::Build && matches!(runner.program, Program::Installed(_)) {
            return Ok(None);
        }
        let example = (job == Job::Play && self.project.source.is_none())
            .then(|| self.project.review.after().to_json())
            .transpose()?;
        Ok(Some((runner, example)))
    }

    fn spawn_job(
        &mut self,
        job: Job,
        runner: Runner,
        example: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if job == Job::Build {
            self.show_tool(Tool::Build);
        }
        self.run_state = RunState::Running(job);
        self.job_progress = None;
        let control = JobControl::default();
        self.job_control = Some(control.clone());
        let monitor = control.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(active) = this.update(cx, |view, cx| {
                    let active = view
                        .job_control
                        .as_ref()
                        .is_some_and(|job| Arc::ptr_eq(&job.0, &monitor.0));
                    if active && !monitor.cancelled() {
                        let progress = monitor.latest();
                        if progress != view.job_progress {
                            view.job_progress = progress;
                            cx.notify();
                        }
                    }
                    active
                }) else {
                    break;
                };
                if !active {
                    break;
                }
            }
        })
        .detach();

        cx.notify();
        let release = self.release;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    runner
                        .execute(job, release, example, control)
                        .map_err(|error| format!("{error:#}"))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if result.is_err() {
                    view.show_tool(Tool::Build);
                }
                view.job_control = None;
                view.job_progress = None;
                view.run_state = RunState::Finished(result);
                cx.notify();
            });
        })
        .detach();
    }
}

impl Drop for ReviewWindow {
    fn drop(&mut self) {
        if let Some(job) = &self.job_control {
            job.cancel();
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn cancellation_stops_and_reaps_a_player_process() -> Result<()> {
        let mut child = spawn(Command::new("sleep").arg("30"))?;
        let control = JobControl::default();
        control.cancel();
        assert!(wait(&mut child, &control)?.is_none());
        assert!(child.try_wait()?.is_some());
        Ok(())
    }
}
