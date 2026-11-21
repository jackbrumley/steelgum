//! Update discovery and installer handoff for the Steelgum desktop application.

mod install;

use std::{env, fs, io, path::PathBuf, time::Duration};

use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use install::{Installation, begin_install, installed_app};

pub const APP_ID: &str = "info.brumley.steelgum";
const REPOSITORY: &str = "jackbrumley/steelgum";
const INSTALL_SITE: &str = "https://brumley.info/steelgum";
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("invalid stable version {0:?}")]
    Version(String),
    #[error("could not check GitHub Releases: {0}")]
    Request(#[source] Box<ureq::Error>),
    #[error("could not read GitHub's release response: {0}")]
    Response(#[source] Box<ureq::Error>),
    #[error("could not resolve {0} for this user")]
    MissingPath(&'static str),
    #[error("{context}: {source}")]
    Io { context: String, source: io::Error },
    #[error("the updater exited before Steelgum could hand off ({0})")]
    WorkerExited(std::process::ExitStatus),
    #[error("the installer worker did not confirm startup; see the update log")]
    WorkerTimeout,
    #[error("this platform has no Steelgum installer")]
    UnsupportedPlatform,
    #[error("could not open {target}: {source}")]
    Open { target: String, source: io::Error },
}

impl UpdateError {
    fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: VersionLabel,
    pub latest_version: Option<VersionLabel>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub notes_url: Option<String>,
}

/// A plain MAJOR.MINOR.PATCH version that has passed validation.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct VersionLabel(String);

impl std::fmt::Display for VersionLabel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

pub fn check_for_updates(current_version: &str) -> Result<UpdateCheck, UpdateError> {
    let current = parse_version(current_version)?;
    let api_url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(CHECK_TIMEOUT))
        .max_redirects(0)
        .build()
        .into();
    let mut request = agent
        .get(&api_url)
        .header("User-Agent", "steelgum-update-checker")
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = env::var("STEELGUM_GITHUB_TOKEN") {
        request = request.header("Authorization", format!("Bearer {token}"));
    }

    let release = match request.call() {
        Ok(mut response) => Some(
            response
                .body_mut()
                .with_config()
                .limit(1024 * 1024)
                .read_json()
                .map_err(|error| UpdateError::Response(Box::new(error)))?,
        ),
        Err(ureq::Error::StatusCode(404)) => None,
        Err(error) => return Err(UpdateError::Request(Box::new(error))),
    };
    compare_release(current, release)
}

fn parse_version(label: &str) -> Result<Version, UpdateError> {
    let version = Version::parse(label).map_err(|_| UpdateError::Version(label.to_owned()))?;
    if version.pre.is_empty() && version.build.is_empty() {
        Ok(version)
    } else {
        Err(UpdateError::Version(label.to_owned()))
    }
}

fn compare_release(current: Version, release: Option<Release>) -> Result<UpdateCheck, UpdateError> {
    let release = release.filter(|release| !release.draft && !release.prerelease);
    let latest = release
        .map(|release| {
            let tag = release.tag_name;
            parse_version(
                tag.strip_prefix('v')
                    .ok_or_else(|| UpdateError::Version(tag.clone()))?,
            )
        })
        .transpose()?;
    let release_url = latest
        .as_ref()
        .map(|version| format!("https://github.com/{REPOSITORY}/releases/tag/v{version}"));

    Ok(UpdateCheck {
        current_version: VersionLabel(current.to_string()),
        latest_version: latest
            .as_ref()
            .map(|version| VersionLabel(version.to_string())),
        update_available: latest.is_some_and(|version| version > current),
        notes_url: release_url.clone(),
        release_url,
    })
}

pub fn update_log_path() -> Result<PathBuf, UpdateError> {
    let base = if cfg!(windows) {
        home_path()?.join(".config")
    } else if let Some(path) = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        path
    } else {
        home_path()?.join(".config")
    };
    Ok(base.join("steelgum").join("debug").join("update.log"))
}

fn home_path() -> Result<PathBuf, UpdateError> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(UpdateError::MissingPath(name))
}

pub fn log_message(message: &str) -> Result<(), UpdateError> {
    use io::Write;
    let path = update_log_path()?;
    let mut log = open_log(&path)?;
    writeln!(log, "{message}")
        .map_err(|error| UpdateError::io(format!("could not write {}", path.display()), error))
}

fn open_log(path: &std::path::Path) -> Result<fs::File, UpdateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            UpdateError::io(format!("could not create {}", parent.display()), error)
        })?;
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| UpdateError::io(format!("could not open {}", path.display()), error))
}

pub fn open_update_log() -> Result<(), UpdateError> {
    let path = update_log_path()?;
    open::that_detached(&path).map_err(|source| UpdateError::Open {
        target: path.display().to_string(),
        source,
    })
}

pub fn open_release_notes(check: &UpdateCheck) -> Result<(), UpdateError> {
    if let Some(url) = &check.notes_url {
        open::that_detached(url).map_err(|source| UpdateError::Open {
            target: url.clone(),
            source,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable(tag_name: &str) -> Release {
        Release {
            tag_name: tag_name.to_owned(),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn stable_versions_compare_numerically_and_use_canonical_notes_url() {
        let result = compare_release(
            parse_version("0.9.9").expect("version"),
            Some(stable("v0.10.0")),
        )
        .expect("check");
        assert!(result.update_available);
        let serialized = serde_json::to_value(result).expect("JSON");
        assert_eq!(serialized["latestVersion"], "0.10.0");
        assert_eq!(
            serialized["notesUrl"],
            "https://github.com/jackbrumley/steelgum/releases/tag/v0.10.0"
        );
    }

    #[test]
    fn releases_cannot_downgrade_or_use_prerelease_versions() {
        let current = parse_version("1.0.0").expect("version");
        let old = compare_release(current.clone(), Some(stable("v0.9.0"))).expect("check");
        assert!(!old.update_available);
        for tag in ["v2.0.0-beta.1", "2.0.0", "v2.0.0+metadata"] {
            assert!(compare_release(current.clone(), Some(stable(tag))).is_err());
        }
        let mut draft = stable("v2.0.0");
        draft.draft = true;
        let check = compare_release(current, Some(draft)).expect("check");
        assert!(check.latest_version.is_none());
    }
}
