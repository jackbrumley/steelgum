//! Project discovery shared by desktop and command-line clients.

use std::collections::HashSet;
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screens: Vec<ScreenDefinition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenDefinition {
    pub id: String,
    pub title: String,
    pub button_label: String,
}

#[derive(Clone, PartialEq, Eq)]
pub enum AuthoredScreen {
    Pause(PauseMenuConfig),
    Custom(ScreenDefinition),
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
    #[error("steelgum.json changed on disk. Refresh the project and try again.")]
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
        let mut ids = HashSet::new();
        for screen in &manifest.screens {
            if !crate::valid_id(&screen.id) || screen.id == "pause-menu" || !ids.insert(&screen.id)
            {
                return Err(invalid(
                    "screen IDs must be unique and pause-menu is reserved",
                ));
            }
            if !crate::pause_menu::valid_label(&screen.title)
                || !crate::pause_menu::valid_label(&screen.button_label)
            {
                return Err(invalid(
                    "screen labels need 1 to 24 printable ASCII characters",
                ));
            }
        }
        Ok(manifest)
    }
}

/// Append a screen template to the existing manifest and return its stable ID.
pub fn create_screen(root: &Path) -> Result<ScreenDefinition, ProjectError> {
    edit_manifest(root, |path, original, manifest| {
        let (id, title, button_label) = if manifest.screens.is_empty() {
            ("main-menu".to_owned(), "Main Menu", "Play")
        } else {
            let id = (2..)
                .map(|number| format!("screen-{number}"))
                .find(|id| manifest.screens.iter().all(|screen| screen.id != *id))
                .expect("a finite screen list cannot occupy all IDs");
            (id, "New Screen", "Continue")
        };
        let screen = ScreenDefinition {
            id,
            title: title.into(),
            button_label: button_label.into(),
        };
        #[derive(Deserialize)]
        struct ScreenLayout<'a> {
            #[serde(borrow)]
            screens: Option<&'a RawValue>,
        }
        let error = |source| ProjectError::Parse {
            path: path.to_owned(),
            source,
        };
        let layout: ScreenLayout<'_> = serde_json::from_str(original).map_err(error)?;
        let entry = serde_json::to_string(&screen).map_err(error)?;
        let mut changed = original.to_owned();
        if let Some(screens) = layout.screens {
            let list = screens.get();
            let offset = list.as_ptr().addr() - original.as_ptr().addr();
            crate::file_edit::append_json(
                &mut changed,
                offset..offset + list.len(),
                &entry,
                !manifest.screens.is_empty(),
            );
        } else {
            let end = original.trim_end().len();
            crate::file_edit::append_json(
                &mut changed,
                0..end,
                &format!("\"screens\": [{entry}]"),
                true,
            );
        }
        Ok((changed, screen))
    })
}

/// Remove the reviewed screen while preserving unrelated manifest text.
pub fn delete_screen(root: &Path, expected: &AuthoredScreen) -> Result<(), ProjectError> {
    edit_manifest(root, |path, original, manifest| {
        #[derive(Deserialize)]
        struct Layout<'a> {
            #[serde(borrow)]
            pause_menu: Option<&'a RawValue>,
            #[serde(default, borrow)]
            screens: Vec<&'a RawValue>,
        }
        let layout: Layout<'_> =
            serde_json::from_str(original).map_err(|source| ProjectError::Parse {
                path: path.to_owned(),
                source,
            })?;
        let (start, end) = match expected {
            AuthoredScreen::Pause(menu) => {
                if manifest.pause_menu.as_ref() != Some(menu) {
                    return Err(ProjectError::Stale);
                }
                let raw = layout.pause_menu.ok_or(ProjectError::Stale)?.get();
                let value = raw.as_ptr().addr() - original.as_ptr().addr();
                let key = original[..value]
                    .trim_end()
                    .trim_end_matches(':')
                    .trim_end();
                let start = key
                    .strip_suffix('"')
                    .and_then(|key| key.rfind('"'))
                    .ok_or(ProjectError::Stale)?;
                (start, value + raw.len())
            }
            AuthoredScreen::Custom(screen) => {
                let index = manifest
                    .screens
                    .iter()
                    .position(|item| item == screen)
                    .ok_or(ProjectError::Stale)?;
                let raw = layout.screens[index].get();
                let start = raw.as_ptr().addr() - original.as_ptr().addr();
                (start, start + raw.len())
            }
        };
        Ok((crate::file_edit::remove_json(original, start, end), ()))
    })
}

/// Update the declared pause menu while preserving the rest of the manifest text.
pub fn save_pause_menu(
    root: &Path,
    expected: &PauseMenuConfig,
    menu: &PauseMenuConfig,
) -> Result<(), ProjectError> {
    edit_manifest(root, |path, original, manifest| {
        if manifest.pause_menu.as_ref() != Some(expected) {
            return Err(ProjectError::Stale);
        }
        if menu == expected {
            return Ok((original.to_owned(), ()));
        }
        #[derive(Deserialize)]
        struct MenuLayout<'a> {
            #[serde(borrow)]
            pause_menu: &'a RawValue,
        }
        let parse_error = |source| ProjectError::Parse {
            path: path.to_owned(),
            source,
        };
        let layout: MenuLayout<'_> = serde_json::from_str(original).map_err(parse_error)?;
        let previous = layout.pause_menu.get();
        let offset = previous.as_ptr().addr() - original.as_ptr().addr();
        let serialized = serde_json::to_string_pretty(menu).map_err(parse_error)?;
        let line = original[..offset].rsplit('\n').next().unwrap_or_default();
        let indent = &line[..line.len() - line.trim_start().len()];
        let mut changed = original.to_owned();
        changed.replace_range(
            offset..offset + previous.len(),
            &serialized.replace('\n', &format!("\n{indent}")),
        );
        Ok((changed, ()))
    })
}

fn edit_manifest<T>(
    root: &Path,
    edit: impl FnOnce(&Path, &str, &ProjectManifest) -> Result<(String, T), ProjectError>,
) -> Result<T, ProjectError> {
    let relative = Path::new(MANIFEST_FILE);
    let requested = root.join(relative);
    let path = resolve_project_path(root, relative)?;
    let original = std::fs::read_to_string(&path).map_err(|source| ProjectError::Read {
        path: path.clone(),
        source,
    })?;
    let manifest = ProjectManifest::parse(&path, original.as_bytes())?;
    let (changed, result) = edit(&path, &original, &manifest)?;
    if changed == original {
        return Ok(result);
    }
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
    Ok(result)
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
    fn creates_multiple_screens_and_preserves_existing_definitions() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join(MANIFEST_FILE);
        let original = "{\"version\":1,\"name\":\"Keep this formatting\",\"scene\":\"room.json\"";
        std::fs::write(&path, format!("{original}}}\n")).unwrap();
        let main = create_screen(root).unwrap();
        let other = create_screen(root).unwrap();
        assert_eq!(
            (main.id.as_str(), main.button_label.as_str()),
            ("main-menu", "Play")
        );
        assert_ne!(main.id, other.id);
        assert!(std::fs::read_to_string(path).unwrap().starts_with(original));
        assert_eq!(
            ProjectManifest::discover(root).unwrap().unwrap().screens,
            [main.clone(), other.clone()]
        );
        delete_screen(root, &AuthoredScreen::Custom(main)).unwrap();
        assert_eq!(
            ProjectManifest::discover(root).unwrap().unwrap().screens,
            [other]
        );
    }

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
        assert!(matches!(
            delete_screen(root, &AuthoredScreen::Pause(existing)),
            Err(ProjectError::Stale)
        ));
        delete_screen(root, &AuthoredScreen::Pause(edited)).unwrap();
        assert!(
            ProjectManifest::discover(root)
                .unwrap()
                .unwrap()
                .pause_menu
                .is_none()
        );
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
