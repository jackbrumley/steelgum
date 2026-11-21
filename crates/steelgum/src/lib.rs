//! Steelgum's authored scene data and validation.

pub mod build;
mod file_edit;
pub mod pause_menu;
pub mod project;
pub mod review;
pub mod scene_edit;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SCENE_VERSION: u32 = 1;
pub const MAX_DISPLAY_NAME_LENGTH: usize = 128;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneObject {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [u8; 3],
    pub solid: bool,
}

impl SceneObject {
    /// The authored display name, falling back to the stable ID.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// Tests a square footprint against the object's bounds in the XZ plane.
    pub fn overlaps_footprint(&self, position: [f32; 2], half_width: f32) -> bool {
        (position[0] - self.center[0]).abs() < self.size[0] / 2.0 + half_width
            && (position[1] - self.center[2]).abs() < self.size[2] / 2.0 + half_width
    }
}

#[derive(Debug)]
pub struct Scene(SceneDocument);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SceneDocument {
    version: u32,
    objects: Vec<SceneObject>,
}

#[derive(Debug, Error)]
pub enum SceneError {
    #[error("could not parse scene JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported scene version {0}, expected {SCENE_VERSION}")]
    UnsupportedVersion(u32),
    #[error(
        "object ID {0:?} must contain only lowercase ASCII letters, digits, hyphens, or underscores"
    )]
    InvalidId(String),
    #[error("duplicate scene object ID {0:?}")]
    DuplicateId(String),
    #[error(
        "object {0:?} needs a non-empty display name of up to {MAX_DISPLAY_NAME_LENGTH} characters without control characters"
    )]
    InvalidName(String),
    #[error("object {0:?} must have a finite center and finite, positive size")]
    InvalidBounds(String),
}

impl Scene {
    pub fn from_json(source: &str) -> Result<Self, SceneError> {
        let scene: SceneDocument = serde_json::from_str(source)?;
        if scene.version != SCENE_VERSION {
            return Err(SceneError::UnsupportedVersion(scene.version));
        }

        let mut ids = HashSet::new();
        for object in &scene.objects {
            if object.id.is_empty()
                || !object.id.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
            {
                return Err(SceneError::InvalidId(object.id.clone()));
            }
            if !ids.insert(&object.id) {
                return Err(SceneError::DuplicateId(object.id.clone()));
            }
            if object.name.as_deref().is_some_and(|name| !valid_name(name)) {
                return Err(SceneError::InvalidName(object.id.clone()));
            }
            if !object.center.iter().all(|value| value.is_finite())
                || !object
                    .size
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
            {
                return Err(SceneError::InvalidBounds(object.id.clone()));
            }
        }
        Ok(Self(scene))
    }

    pub fn objects(&self) -> &[SceneObject] {
        &self.0.objects
    }

    pub fn object(&self, id: &str) -> Option<&SceneObject> {
        self.objects().iter().find(|object| object.id == id)
    }

    pub fn to_json(&self) -> Result<String, SceneError> {
        Ok(serde_json::to_string_pretty(&self.0)?)
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_DISPLAY_NAME_LENGTH
        && !name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJECT: &str =
        r#"{"id":"wall","center":[0,0,1],"size":[2,3,4],"color":[123,111,22],"solid":true}"#;

    #[test]
    fn round_trip_preserves_authored_identity() {
        let scene = Scene::from_json(&format!(r#"{{"version":1,"objects":[{OBJECT}]}}"#))
            .expect("valid scene");
        let reloaded =
            Scene::from_json(&scene.to_json().expect("serializable scene")).expect("reload scene");
        assert_eq!(
            reloaded.object("wall").expect("stable ID").center,
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn rejects_duplicate_identity() {
        let error = Scene::from_json(&format!(r#"{{"version":1,"objects":[{OBJECT},{OBJECT}]}}"#))
            .expect_err("duplicate IDs are ambiguous");
        assert!(matches!(error, SceneError::DuplicateId(id) if id == "wall"));
    }
}
