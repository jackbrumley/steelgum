//! Project discovery shared by desktop and command-line clients.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MANIFEST_FILE: &str = "steelgum.json";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    version: u32,
    pub name: String,
    pub scene: PathBuf,
    pub runner: Option<CargoRunner>,
    pub assets: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CargoRunner {
    pub manifest: PathBuf,
    pub binary: String,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid {path}: {reason}")]
    Invalid { path: PathBuf, reason: &'static str },
}

impl ProjectManifest {
    /// Read an explicit project manifest, or return None for a legacy worktree.
    pub fn discover(root: &Path) -> Result<Option<Self>, ProjectError> {
        let path = root.join(MANIFEST_FILE);
        let content = match std::fs::read(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ProjectError::Read { path, source }),
        };
        let manifest: Self =
            serde_json::from_slice(&content).map_err(|source| ProjectError::Parse {
                path: path.clone(),
                source,
            })?;
        if manifest.version != 1 {
            return Err(ProjectError::Invalid {
                path,
                reason: "unsupported project version",
            });
        }
        if manifest.name.trim().is_empty() || manifest.name.chars().any(char::is_control) {
            return Err(ProjectError::Invalid {
                path,
                reason: "the project name must be a single nonempty line",
            });
        }
        validate_relative(&manifest.scene)?;
        if let Some(assets) = &manifest.assets {
            validate_relative(assets)?;
        }
        if let Some(runner) = &manifest.runner {
            validate_relative(&runner.manifest)?;
            if runner.manifest.file_name().and_then(|value| value.to_str()) != Some("Cargo.toml")
                || runner.binary.is_empty()
                || !runner
                    .binary
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(ProjectError::Invalid {
                    path,
                    reason: "the runner requires a Cargo.toml path and a Cargo binary name",
                });
            }
        }
        Ok(Some(manifest))
    }
}

fn validate_relative(path: &Path) -> Result<(), ProjectError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProjectError::Invalid {
            path: path.to_owned(),
            reason: "expected a path relative to the worktree",
        });
    }
    Ok(())
}

/// Resolve an existing project file and reject symlinks that leave the worktree.
pub fn resolve_project_path(root: &Path, path: &Path) -> Result<PathBuf, ProjectError> {
    validate_relative(path)?;
    let resolved_root = canonicalize(root)?;
    let resolved = canonicalize(&resolved_root.join(path))?;
    if !resolved.starts_with(&resolved_root) {
        return Err(ProjectError::Invalid {
            path: path.to_owned(),
            reason: "path resolves outside the worktree",
        });
    }
    Ok(resolved)
}

fn canonicalize(path: &Path) -> Result<PathBuf, ProjectError> {
    path.canonicalize().map_err(|source| ProjectError::Read {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_manifest_and_rejects_paths_leaving_the_project() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join(MANIFEST_FILE);
        assert!(ProjectManifest::discover(root.path()).unwrap().is_none());
        std::fs::write(
            &manifest,
            r#"{
            "version":1,"name":"Signal Room","scene":"scenes/room.json",
            "runner":{"manifest":"Cargo.toml","binary":"signal-room"}
        }"#,
        )
        .unwrap();
        let project = ProjectManifest::discover(root.path()).unwrap().unwrap();
        assert_eq!(project.scene, Path::new("scenes/room.json"));
        assert_eq!(project.runner.unwrap().binary, "signal-room");
        std::fs::write(
            &manifest,
            r#"{"version":1,"name":"Signal Room","scene":"../outside.json"}"#,
        )
        .unwrap();
        assert!(matches!(
            ProjectManifest::discover(root.path()),
            Err(ProjectError::Invalid { .. })
        ));
    }
}
