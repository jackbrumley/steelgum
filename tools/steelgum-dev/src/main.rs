use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, SystemTime};

const APP_NAME: &str = "steelgum-gpui";
const POLL_INTERVAL: Duration = Duration::from_millis(250);

type Snapshot = BTreeMap<PathBuf, (SystemTime, u64)>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.as_mut_slice() {
        [] => {}
        [flag] if flag == "--help" || flag == "-h" => {
            println!("Usage: cargo dev [WORKTREE SCENE_PATH_RELATIVE_TO_WORKTREE]");
            return Ok(());
        }
        [worktree, _] => *worktree = std::path::absolute(Path::new(worktree))?.into_os_string(),
        _ => return Err("Usage: cargo dev [WORKTREE SCENE_PATH_RELATIVE_TO_WORKTREE]".into()),
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let app = root.join("apps").join(APP_NAME);
    let stopping = Arc::new(AtomicBool::new(false));
    ctrlc::set_handler({
        let stopping = stopping.clone();
        move || stopping.store(true, Ordering::Relaxed)
    })?;
    watch(&root, &app, &arguments, &stopping)
}

fn watch(
    root: &Path,
    app: &Path,
    arguments: &[OsString],
    stopping: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let launch_directory = tempfile::tempdir()?;
    let mut running = RunningApp::default();
    let mut previous = None;
    while !stopping.load(Ordering::Relaxed) {
        let current = snapshot(root, app)?;
        if previous.as_ref() != Some(&current) {
            println!("Building Steelgum…");
            let built = build(app)?;
            if stopping.load(Ordering::Relaxed) {
                break;
            }
            match built {
                Some(executable) if snapshot(root, app)? == current => {
                    running.stop()?;
                    #[cfg(windows)]
                    let executable = {
                        // Keep Cargo's executable writable while Steelgum runs.
                        let copy = launch_directory.path().join(format!("{APP_NAME}.exe"));
                        fs::copy(executable, &copy)?;
                        copy
                    };
                    println!("Starting Steelgum. Press Ctrl+C to stop watching.");
                    running.0 = Some(
                        Command::new(executable)
                            .args(arguments)
                            .current_dir(app)
                            .spawn()?,
                    );
                }
                Some(_) => {}
                None => println!("Build failed. Waiting for the next save."),
            }
            previous = Some(current);
        }
        thread::sleep(POLL_INTERVAL);
    }
    running.stop()?;
    Ok(())
}

fn snapshot(root: &Path, app: &Path) -> io::Result<Snapshot> {
    let core = root.join("crates/steelgum");
    let mut files = Snapshot::new();
    for path in [
        root.join("Cargo.toml"),
        app.join("Cargo.toml"),
        app.join("Cargo.lock"),
        app.join("rust-toolchain.toml"),
        app.join("build.rs"),
        core.join("Cargo.toml"),
        root.join("examples/door-game/scenes/room.json"),
        app.join("src"),
        core.join("src"),
    ] {
        collect_files(path, &mut files)?;
    }
    Ok(files)
}

fn collect_files(path: PathBuf, files: &mut Snapshot) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_dir() {
        for entry in fs::read_dir(&path)? {
            collect_files(entry?.path(), files)?;
        }
    } else {
        files.insert(path, (metadata.modified()?, metadata.len()));
    }
    Ok(())
}

fn build(app: &Path) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let output = Command::new("cargo")
        .current_dir(app)
        // Let the app's rust-toolchain.toml select its own toolchain.
        .env_remove("RUSTUP_TOOLCHAIN")
        .args([
            "build",
            "--locked",
            "--bin",
            APP_NAME,
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    for line in output.stdout.lines() {
        let line = line?;
        if !line.starts_with('{') {
            println!("{line}");
            continue;
        }
        let message: serde_json::Value = serde_json::from_str(&line)?;
        let executable = message["executable"].as_str().filter(|_| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == APP_NAME
        });
        if let Some(executable) = executable {
            return Ok(Some(PathBuf::from(executable)));
        }
    }
    Err("Cargo built Steelgum but reported no executable".into())
}

#[derive(Default)]
struct RunningApp(Option<Child>);

impl RunningApp {
    fn stop(&mut self) -> io::Result<()> {
        if let Some(child) = self.0.as_mut() {
            if child.try_wait()?.is_none() {
                child.kill()?;
                child.wait()?;
            }
            self.0 = None;
        }
        Ok(())
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Could not stop Steelgum: {error}");
        }
    }
}
