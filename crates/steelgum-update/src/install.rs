use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{INSTALL_SITE, UpdateError, VersionLabel, open_log, update_log_path};

#[derive(Debug)]
pub struct Installation {
    executable: PathBuf,
}

pub fn installed_app() -> Result<Option<Installation>, UpdateError> {
    let running = env::current_exe()
        .map_err(|error| UpdateError::io("could not find running executable", error))?;
    #[cfg(target_os = "linux")]
    let (running, installed) = (
        env::var_os("APPIMAGE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or(running),
        crate::home_path()?.join(".local/bin/steelgum"),
    );
    #[cfg(windows)]
    let installed = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(UpdateError::MissingPath("LOCALAPPDATA"))?
        .join("Programs")
        .join("Steelgum")
        .join("steelgum.exe");
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = running;
        return Ok(None);
    }

    #[cfg(any(target_os = "linux", windows))]
    {
        // Windows install directories can be returned with different ASCII casing.
        let matched = if cfg!(windows) {
            running
                .as_os_str()
                .eq_ignore_ascii_case(installed.as_os_str())
        } else {
            running == installed
        };
        Ok(matched.then_some(Installation {
            executable: running,
        }))
    }
}

pub fn begin_install(
    installation: &Installation,
    version: &VersionLabel,
) -> Result<(), UpdateError> {
    let log_path = update_log_path()?;
    let log = open_log(&log_path)?;
    let error_log = log
        .try_clone()
        .map_err(|source| UpdateError::io("could not duplicate update log handle", source))?;
    let handshake = tempfile::tempdir()
        .map_err(|source| UpdateError::io("could not create update handshake directory", source))?;
    let ready = handshake.path().join("ready");
    let mut command = worker_command()?;
    command
        .env("STEELGUM_UPDATE_PARENT", std::process::id().to_string())
        .env("STEELGUM_UPDATE_READY", &ready)
        .env("STEELGUM_UPDATE_SITE", INSTALL_SITE)
        .env("STEELGUM_UPDATE_TAG", format!("v{version}"))
        .env("STEELGUM_UPDATE_LOG", &log_path)
        .env("STEELGUM_UPDATE_EXECUTABLE", &installation.executable)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(error_log);
    let working_dir = installation
        .executable
        .parent()
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| Path::new("."));
    command.current_dir(working_dir);
    let mut child = command
        .spawn()
        .map_err(|error| UpdateError::io("could not start the installer worker", error))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| UpdateError::io("could not inspect the installer worker", error))?
        {
            return Err(UpdateError::WorkerExited(status));
        }
        if ready.is_file() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            match child.kill() {
                Ok(()) => {
                    child.wait().map_err(|source| {
                        UpdateError::io("could not reap installer worker", source)
                    })?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
                Err(error) => {
                    return Err(UpdateError::io("could not stop installer worker", error));
                }
            }
            return Err(UpdateError::WorkerTimeout);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(target_os = "linux")]
fn worker_command() -> Result<Command, UpdateError> {
    let mut command = Command::new("setsid");
    command.args(["bash", "-c", include_str!("update-linux.sh")]);
    // AppImage exports mount-specific dynamic library paths to child processes.
    for name in [
        "APPDIR",
        "APPIMAGE",
        "ARGV0",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
    ] {
        command.env_remove(name);
    }
    Ok(command)
}

#[cfg(windows)]
fn worker_command() -> Result<Command, UpdateError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::windows::process::CommandExt;

    let windows = env::var_os("SystemRoot").ok_or(UpdateError::MissingPath("SystemRoot"))?;
    let powershell = PathBuf::from(windows).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let script = include_str!("update-windows.ps1")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut command = Command::new(powershell);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
    ]);
    command.arg(STANDARD.encode(script));
    Ok(command)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn worker_command() -> Result<Command, UpdateError> {
    Err(UpdateError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    #[test]
    fn detached_workers_preserve_the_failure_argument_and_finish_install_after_parent_exit() {
        let linux = include_str!("update-linux.sh");
        assert!(linux.starts_with("set -o pipefail"));
        assert!(linux.contains("kill -0"));
        assert!(linux.contains("--update-failed"));

        let windows = include_str!("update-windows.ps1");
        assert!(windows.contains("WaitForExit"));
        assert!(windows.contains("--update-failed"));
        assert!(windows.contains("$LASTEXITCODE"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_failed_script_download_reopens_the_original_installation() {
        use std::{fs, os::unix::fs::PermissionsExt};

        let folder = tempfile::tempdir().expect("fixture directory");
        let curl = folder.path().join("curl");
        let installed = folder.path().join("Steelgum's install");
        let report = folder.path().join("arguments");
        let ready = folder.path().join("ready");
        fs::write(
            &curl,
            r#"#!/bin/bash
while [[ $1 != --output ]]; do shift; done
printf 'printf bad > "$REPORT"\n' > "$2"
exit 22
"#,
        )
        .expect("mock downloader");
        fs::write(
            &installed,
            "#!/bin/bash\nprintf '%s\\n' \"$@\" > \"$REPORT\"\n",
        )
        .expect("mock installed executable");
        for file in [&curl, &installed] {
            fs::set_permissions(file, fs::Permissions::from_mode(0o755))
                .expect("fixture is executable");
        }
        let path = std::env::join_paths(std::iter::once(folder.path().to_owned()).chain(
            std::env::split_paths(&std::env::var_os("PATH").expect("system PATH")),
        ))
        .expect("fixture PATH");
        let mut parent = std::process::Command::new("true")
            .spawn()
            .expect("mock parent");
        let parent_id = parent.id();
        assert!(parent.wait().expect("mock parent has exited").success());
        let result = std::process::Command::new("bash")
            .args(["-c", include_str!("update-linux.sh")])
            .env("PATH", path)
            .env("REPORT", &report)
            .env("STEELGUM_UPDATE_PARENT", parent_id.to_string())
            .env("STEELGUM_UPDATE_READY", &ready)
            .env("STEELGUM_UPDATE_SITE", "https://example.invalid")
            .env("STEELGUM_UPDATE_TAG", "v1.2.3")
            .env("STEELGUM_UPDATE_EXECUTABLE", &installed)
            .output()
            .expect("worker exits");
        assert!(result.status.success());
        assert!(ready.is_file());
        assert_eq!(
            fs::read_to_string(report).expect("original installation was relaunched"),
            "--update-failed\n"
        );
    }
}
