//! Project discovery shared by desktop and command-line clients.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use thiserror::Error;

use crate::pause_menu::PauseMenuConfig;

pub const MANIFEST_FILE: &str = "steelgum.json";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    version: u32,
    pub name: String,
    pub scene: PathBuf,
    pub runner: Option<CargoRunner>,
    pub assets: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_menu: Option<PauseMenuConfig>,
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
    #[error("could not save {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("steelgum.json changed on disk. Reload the pause menu before saving.")]
    Stale,
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
        Self::parse(&path, &content).map(Some)
    }

    fn parse(path: &Path, content: &[u8]) -> Result<Self, ProjectError> {
        let invalid = |reason| ProjectError::Invalid {
            path: path.to_owned(),
            reason,
        };
        let manifest: Self =
            serde_json::from_slice(content).map_err(|source| ProjectError::Parse {
                path: path.to_owned(),
                source,
            })?;
        if manifest.version != 1 {
            return Err(invalid("unsupported project version"));
        }
        if manifest.name.trim().is_empty() || manifest.name.chars().any(char::is_control) {
            return Err(invalid("the project name must be a single nonempty line"));
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
                return Err(invalid(
                    "the runner requires a Cargo.toml path and a Cargo binary name",
                ));
            }
        }
        if let Some(menu) = &manifest.pause_menu {
            menu.validate().map_err(invalid)?;
        }
        Ok(manifest)
    }
}

/// Update the declared pause menu while preserving the rest of the manifest text.
pub fn save_pause_menu(
    root: &Path,
    expected: &PauseMenuConfig,
    menu: &PauseMenuConfig,
) -> Result<(), ProjectError> {
    let relative = Path::new(MANIFEST_FILE);
    let requested = root.join(relative);
    let path = resolve_project_path(root, relative)?;
    let original = std::fs::read_to_string(&path).map_err(|source| ProjectError::Read {
        path: path.clone(),
        source,
    })?;
    let manifest = ProjectManifest::parse(&path, original.as_bytes())?;
    if manifest.pause_menu.as_ref() != Some(expected) {
        return Err(ProjectError::Stale);
    }
    if menu == expected {
        return Ok(());
    }

    #[derive(Deserialize)]
    struct MenuLayout<'a> {
        #[serde(borrow)]
        pause_menu: &'a RawValue,
    }
    let parse_error = |source| ProjectError::Parse {
        path: path.clone(),
        source,
    };
    let layout: MenuLayout<'_> = serde_json::from_str(&original).map_err(parse_error)?;
    let previous = layout.pause_menu.get();
    let offset = previous.as_ptr().addr() - original.as_ptr().addr();
    let serialized = serde_json::to_string_pretty(menu).map_err(parse_error)?;
    let line = original[..offset].rsplit('\n').next().unwrap_or_default();
    let indent = &line[..line.len() - line.trim_start().len()];
    let mut changed = original.clone();
    changed.replace_range(
        offset..offset + previous.len(),
        &serialized.replace('\n', &format!("\n{indent}")),
    );
    ProjectManifest::parse(&path, changed.as_bytes())?;
    if !crate::file_edit::replace_unchanged(
        &requested,
        &path,
        original.as_bytes(),
        changed.as_bytes(),
    )
    .map_err(|source| ProjectError::Write { path, source })?
    {
        return Err(ProjectError::Stale);
    }
    Ok(())
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
    fn pause_menu_edits_validate_and_preserve_other_manifest_text() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join(MANIFEST_FILE);
        let existing = PauseMenuConfig::default();
        let manifest = format!(
            "{{\"version\":1, \"name\":\"Edited by another agent\", \"scene\":\"room.json\", \"pause_menu\":{}}}\n",
            serde_json::to_string(&existing).unwrap(),
        );
        std::fs::write(&path, &manifest).unwrap();
        let mut edited = PauseMenuConfig {
            title: "Break time".into(),
            sensitivity_default: 151,
            ..existing.clone()
        };
        assert!(matches!(
            save_pause_menu(root, &existing, &edited),
            Err(ProjectError::Invalid { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), manifest);

        edited.sensitivity_default = 35;
        save_pause_menu(root, &existing, &edited).unwrap();
        assert!(std::fs::read_to_string(path).unwrap().starts_with(
            "{\"version\":1, \"name\":\"Edited by another agent\", \"scene\":\"room.json\", \"pause_menu\":"
        ));
        let loaded = ProjectManifest::discover(root).unwrap().unwrap();
        assert_eq!(loaded.pause_menu.as_ref(), Some(&edited));
        assert!(matches!(
            save_pause_menu(root, &existing, &existing),
            Err(ProjectError::Stale)
        ));
    }

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
