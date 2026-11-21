//! Validated edits of authored scene properties.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use thiserror::Error;

use crate::{Scene, SceneError, SceneObject, valid_name};

#[derive(Debug, Error)]
pub enum SceneEditError {
    #[error("scene path {0:?} must refer to a file inside the worktree")]
    InvalidPath(PathBuf),
    #[error("object {0:?} is missing from the working scene")]
    MissingObject(String),
    #[error("the scene changed on disk. Refresh the scene and try again")]
    Stale,
    #[error("could not access the scene at {path}: {source}")]
    Access { path: PathBuf, source: io::Error },
    #[error("No space for a box near the viewport centre.")]
    NoSpace,
    #[error(transparent)]
    InvalidScene(#[from] SceneError),
}

pub type RenameError = SceneEditError;

/// Replace a display name while preserving the object's stable ID and unrelated JSON text.
///
/// `expected` is the exact working-tree text the user reviewed. A mismatch before replacement
/// rejects the write so unrelated edits survive.
pub fn rename_object(
    worktree: &Path,
    scene_path: &Path,
    expected: &str,
    id: &str,
    name: &str,
) -> Result<(), RenameError> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(SceneError::InvalidName(id.to_owned()).into());
    }
    edit_scene(worktree, scene_path, expected, |source| {
        Ok((rename_text(source, id, name)?, ()))
    })
}

/// Append a unit box near the view target, preserving the current scene text.
pub fn add_box(
    worktree: &Path,
    scene_path: &Path,
    expected: &str,
    target: [f32; 3],
) -> Result<String, SceneEditError> {
    edit_scene(worktree, scene_path, expected, |source| {
        let scene = Scene::from_json(source)?;
        let id = (1..)
            .map(|number| format!("box-{number}"))
            .find(|id| scene.object(id).is_none())
            .expect("a finite scene cannot occupy all IDs");
        let center = box_center(&scene, target).ok_or(SceneEditError::NoSpace)?;
        let object = SceneObject {
            id: id.clone(),
            name: Some("Box".into()),
            center,
            size: [1.; 3],
            color: [131, 162, 202],
            solid: true,
        };
        #[derive(Deserialize)]
        struct Layout<'a> {
            #[serde(borrow)]
            objects: &'a RawValue,
        }
        let layout: Layout<'_> = serde_json::from_str(source).map_err(SceneError::from)?;
        let array = layout.objects.get();
        let offset = array.as_ptr().addr() - source.as_ptr().addr();
        let mut changed = source.to_owned();
        let entry = serde_json::to_string(&object).map_err(SceneError::from)?;
        crate::file_edit::append_json(
            &mut changed,
            offset..offset + array.len(),
            &entry,
            !scene.objects().is_empty(),
        );
        Ok((changed, id))
    })
}

fn box_center(scene: &Scene, target: [f32; 3]) -> Option<[f32; 3]> {
    if !target.iter().all(|coordinate| coordinate.is_finite()) {
        return None;
    }
    let [x, y, z] = target;
    (0_i32..=5).find_map(|radius| {
        (-radius..=radius)
            .flat_map(|dx| (-radius..=radius).map(move |dz| (dx, dz)))
            .filter(|(dx, dz)| dx.abs().max(dz.abs()) == radius)
            .find_map(|(dx, dz)| {
                let x = x + dx as f32 * 1.25;
                let z = z + dz as f32 * 1.25;
                let floor = scene
                    .objects()
                    .iter()
                    .filter(|object| object.solid && object.overlaps_footprint([x, z], 0.5))
                    .map(|object| object.center[1] + object.size[1] / 2.)
                    .filter(|top| *top <= y + 0.1)
                    .reduce(f32::max)
                    .unwrap_or(0.);
                let center = [x, floor + 0.5, z];
                scene
                    .objects()
                    .iter()
                    .all(|object| {
                        (0..3).any(|axis| {
                            (center[axis] - object.center[axis]).abs()
                                >= 0.5 + object.size[axis] / 2.
                        })
                    })
                    .then_some(center)
            })
    })
}

fn edit_scene<T>(
    worktree: &Path,
    scene_path: &Path,
    expected: &str,
    edit: impl FnOnce(&str) -> Result<(String, T), SceneEditError>,
) -> Result<T, SceneEditError> {
    if scene_path.as_os_str().is_empty()
        || !scene_path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(RenameError::InvalidPath(scene_path.to_owned()));
    }
    let root = worktree
        .canonicalize()
        .map_err(|source| access(worktree, source))?;
    let requested = root.join(scene_path);
    let target = requested
        .canonicalize()
        .map_err(|source| access(&requested, source))?;
    if !target.starts_with(&root) {
        return Err(RenameError::InvalidPath(scene_path.to_owned()));
    }
    verify_unchanged(&target, expected)?;
    let (changed, result) = edit(expected)?;
    if changed == expected {
        return Ok(result);
    }
    Scene::from_json(&changed)?;

    if !crate::file_edit::replace_unchanged(
        &requested,
        &target,
        expected.as_bytes(),
        changed.as_bytes(),
    )
    .map_err(|source| access(&target, source))?
    {
        return Err(RenameError::Stale);
    }
    Ok(result)
}

fn access(path: &Path, source: io::Error) -> RenameError {
    RenameError::Access {
        path: path.to_owned(),
        source,
    }
}

fn verify_unchanged(path: &Path, expected: &str) -> Result<(), RenameError> {
    let current = fs::read(path).map_err(|source| access(path, source))?;
    if current != expected.as_bytes() {
        return Err(RenameError::Stale);
    }
    Ok(())
}

#[derive(Deserialize)]
struct ObjectLayout<'a> {
    #[serde(borrow)]
    objects: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct NameLayout<'a> {
    #[serde(borrow, default, deserialize_with = "present_name")]
    name: Option<&'a RawValue>,
}

// Preserve the distinction between a missing field and an explicitly null value.
fn present_name<'de, D>(deserializer: D) -> Result<Option<&'de RawValue>, D::Error>
where
    D: Deserializer<'de>,
{
    <&RawValue>::deserialize(deserializer).map(Some)
}

fn rename_text(source: &str, id: &str, name: &str) -> Result<String, RenameError> {
    let scene = Scene::from_json(source)?;
    let index = scene
        .objects()
        .iter()
        .position(|object| object.id == id)
        .ok_or_else(|| RenameError::MissingObject(id.to_owned()))?;
    if scene.objects()[index].name.as_deref() == Some(name) {
        return Ok(source.to_owned());
    }
    let layout: ObjectLayout<'_> = serde_json::from_str(source).map_err(SceneError::from)?;
    let object = layout.objects[index].get();
    let layout: NameLayout<'_> = serde_json::from_str(object).map_err(SceneError::from)?;
    let serialized_name = serde_json::to_string(name).map_err(SceneError::from)?;
    let mut changed = source.to_owned();
    if let Some(name) = layout.name {
        let old = name.get();
        let offset = old.as_ptr().addr() - source.as_ptr().addr();
        changed.replace_range(offset..offset + old.len(), &serialized_name);
    } else {
        let offset = object.as_ptr().addr() - source.as_ptr().addr() + 1;
        let fields = &object[1..];
        let indentation = &fields[..fields.len() - fields.trim_start().len()];
        changed.insert_str(
            offset,
            &format!("{indentation}\"name\": {serialized_name},"),
        );
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "{\"version\":1,\"objects\":[\n  {\"id\":\"plate\",\"center\":[0,0,1],\"size\":[2,3,4],\"color\":[3,2,1],\"solid\":false}\n]}\n";

    #[test]
    fn adds_a_box_near_the_target_and_rejects_stale_scene_text() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = Path::new("scene.json");
        std::fs::write(root.join(path), ORIGINAL).unwrap();
        let first = add_box(root, path, ORIGINAL, [6., 0., 2.]).unwrap();
        let edited = std::fs::read_to_string(root.join(path)).unwrap();
        let scene = Scene::from_json(&edited).unwrap();
        assert_eq!(scene.object(&first).unwrap().center, [6., 0.5, 2.]);
        assert!(edited.starts_with(ORIGINAL.trim_end_matches("\n]}\n")));
        assert!(matches!(
            add_box(root, path, ORIGINAL, [6., 0., 2.]),
            Err(SceneEditError::Stale)
        ));
        let second = add_box(root, path, &edited, [6., 0., 2.]).unwrap();
        let scene = Scene::from_json(&std::fs::read_to_string(root.join(path)).unwrap()).unwrap();
        assert_ne!(first, second);
        assert_ne!(scene.object(&second).unwrap().center, [6., 0.5, 2.]);
    }

    #[test]
    fn changes_only_the_name_and_keeps_the_stable_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = Path::new("scene.json");
        let disk_path = directory.path().join(path);
        fs::write(&disk_path, ORIGINAL).unwrap();

        rename_object(directory.path(), path, ORIGINAL, "plate", "Lever α").unwrap();
        let text = fs::read_to_string(disk_path).unwrap();
        assert_eq!(
            text,
            ORIGINAL.replace("{\"id\"", "{\"name\": \"Lever α\",\"id\"")
        );
        let scene = Scene::from_json(&text).unwrap();
        assert_eq!(scene.object("plate").unwrap().display_name(), "Lever α");
    }

    #[test]
    fn refuses_to_overwrite_an_external_edit() {
        let directory = tempfile::tempdir().unwrap();
        let path = Path::new("scene.json");
        let disk_path = directory.path().join(path);
        let edited = ORIGINAL.replace("[3,2,1]", "[0,2,1]");
        fs::write(&disk_path, &edited).unwrap();

        let result = rename_object(directory.path(), path, ORIGINAL, "plate", "New name");
        assert!(matches!(result, Err(RenameError::Stale)));
        assert_eq!(fs::read_to_string(disk_path).unwrap(), edited);
    }

    #[test]
    fn replaces_an_explicit_null_without_adding_a_duplicate_key() {
        let text = ORIGINAL.replace("{\"id\"", "{\"name\":null,\"id\"");
        let changed = rename_text(&text, "plate", "A \"quote\"").unwrap();
        let scene = Scene::from_json(&changed).unwrap();
        assert_eq!(scene.object("plate").unwrap().display_name(), "A \"quote\"");
    }
}
