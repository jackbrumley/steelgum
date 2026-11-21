//! Semantic scene comparison independent of the desktop shell and renderer.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::{Scene, SceneError, SceneObject};

/// A change to one stable authored object ID.
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectChange {
    pub id: String,
    pub changes: ObjectChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectChanges {
    Added,
    Removed,
    Modified {
        color: bool,
        center: bool,
        size: bool,
        solid: bool,
    },
}

#[derive(Debug)]
pub struct SceneReview {
    base_revision: Option<String>,
    before: Scene,
    after: Scene,
    changes: Vec<ObjectChange>,
}

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("scene path must be a UTF-8 path relative to the worktree root: {0}")]
    InvalidScenePath(PathBuf),
    #[error("open the Git worktree root, rather than a subdirectory: {0}")]
    NotWorktreeRoot(PathBuf),
    #[error("scene path resolves outside the worktree: {0}")]
    OutsideWorktree(PathBuf),
    #[error("could not run Git in {path}: {source}")]
    RunGit { path: PathBuf, source: io::Error },
    #[error("Git could not read {revision} in {path}: {message}")]
    Git {
        revision: String,
        path: PathBuf,
        message: String,
    },
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("scene at Git revision {0} is not valid UTF-8")]
    InvalidGitText(String),
    #[error("{revision} scene {path} is invalid: {source}")]
    Scene {
        revision: &'static str,
        path: PathBuf,
        source: SceneError,
    },
}

impl SceneReview {
    /// Compare authored objects by their stable IDs. File and object order do not affect the diff.
    pub fn new(before: Scene, after: Scene) -> Self {
        let mut before_objects: HashMap<&str, &SceneObject> = before
            .objects()
            .iter()
            .map(|object| (object.id.as_str(), object))
            .collect();
        let mut changes: Vec<_> = after
            .objects()
            .iter()
            .filter_map(|after| {
                let changes = match before_objects.remove(after.id.as_str()) {
                    Some(before) => modified(before, after)?,
                    None => ObjectChanges::Added,
                };
                Some(ObjectChange {
                    id: after.id.clone(),
                    changes,
                })
            })
            .collect();
        changes.extend(before_objects.into_keys().map(|id| ObjectChange {
            id: id.to_owned(),
            changes: ObjectChanges::Removed,
        }));
        changes.sort_unstable_by(|left, right| left.id.cmp(&right.id));
        Self {
            base_revision: None,
            before,
            after,
            changes,
        }
    }

    /// Compare the committed scene at HEAD with the file on disk, including uncommitted edits.
    ///
    /// The scene path is relative to the selected Git worktree root.
    pub fn from_git(repo: &Path, scene_path: &Path) -> Result<Self, ReviewError> {
        let git_path = relative_git_path(scene_path)?;
        let worktree = git_output(
            repo,
            "worktree root",
            &["rev-parse", "--is-inside-work-tree", "--show-prefix"],
        )?;
        if worktree.trim_ascii() != b"true" {
            return Err(ReviewError::NotWorktreeRoot(repo.to_owned()));
        }

        let repo_root = canonicalize(repo)?;
        let working_path = canonicalize(&repo_root.join(scene_path))?;
        if !working_path.starts_with(&repo_root) {
            return Err(ReviewError::OutsideWorktree(working_path));
        }
        let working_text =
            std::fs::read_to_string(&working_path).map_err(|source| ReviewError::Read {
                path: working_path,
                source,
            })?;

        let head = git_output(repo, "HEAD", &["rev-parse", "--verify", "HEAD"])?;
        let head = std::str::from_utf8(head.trim_ascii())
            .map_err(|_| ReviewError::InvalidGitText("HEAD".into()))?;
        let revision = format!("{head}:{git_path}");
        let before = git_output(repo, &revision, &["cat-file", "blob", &revision])?;
        let before =
            std::str::from_utf8(&before).map_err(|_| ReviewError::InvalidGitText(revision))?;
        let mut review = Self::new(
            parse_scene(before, "HEAD", scene_path)?,
            parse_scene(&working_text, "working tree", scene_path)?,
        );
        review.base_revision = Some(head.to_owned());
        Ok(review)
    }

    /// The exact Git commit used as the comparison base, when loaded from a worktree.
    pub fn base_revision(&self) -> Option<&str> {
        self.base_revision.as_deref()
    }

    pub fn before(&self) -> &Scene {
        &self.before
    }

    pub fn after(&self) -> &Scene {
        &self.after
    }

    pub fn changes(&self) -> &[ObjectChange] {
        &self.changes
    }
}

fn modified(before: &SceneObject, after: &SceneObject) -> Option<ObjectChanges> {
    let color = before.color != after.color;
    let center = before.center != after.center;
    let size = before.size != after.size;
    let solid = before.solid != after.solid;
    (color || center || size || solid).then_some(ObjectChanges::Modified {
        color,
        center,
        size,
        solid,
    })
}

fn relative_git_path(path: &Path) -> Result<String, ReviewError> {
    let parts: Option<Vec<_>> = path
        .components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    parts
        .filter(|parts| !parts.is_empty())
        .map(|parts| parts.join("/"))
        .ok_or_else(|| ReviewError::InvalidScenePath(path.to_owned()))
}

fn canonicalize(path: &Path) -> Result<PathBuf, ReviewError> {
    path.canonicalize().map_err(|source| ReviewError::Read {
        path: path.to_owned(),
        source,
    })
}

fn git_output(repo: &Path, revision: &str, args: &[&str]) -> Result<Vec<u8>, ReviewError> {
    let output = Command::new("git")
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .args(args)
        .output()
        .map_err(|source| ReviewError::RunGit {
            path: repo.to_owned(),
            source,
        })?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        let message = String::from_utf8_lossy(&output.stderr);
        Err(ReviewError::Git {
            revision: revision.to_owned(),
            path: repo.to_owned(),
            message: message.trim().to_owned(),
        })
    }
}

fn parse_scene(text: &str, revision: &'static str, path: &Path) -> Result<Scene, ReviewError> {
    Scene::from_json(text).map_err(|source| ReviewError::Scene {
        revision,
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE_PLATE: &str =
        r#"{"id":"plate","center":[0,0,1],"size":[1,1,1],"color":[45,149,236],"solid":false}"#;
    const RED_PLATE: &str =
        r#"{"id":"plate","center":[0,0,1],"size":[1,1,1],"color":[236,45,61],"solid":false}"#;
    const OLD_DOOR: &str =
        r#"{"id":"door","center":[1,0,0],"size":[1,1,1],"color":[1,2,3],"solid":true}"#;
    const NEW_DOOR: &str =
        r#"{"id":"door","center":[1,1,0],"size":[1,2,1],"color":[1,2,3],"solid":false}"#;
    const MARKER: &str =
        r#"{"id":"marker","center":[2,3,0],"size":[1,2,1],"color":[1,2,3],"solid":false}"#;

    fn scene(objects: &[&str]) -> Scene {
        let source = format!(r#"{{"version":1,"objects":[{}]}}"#, objects.join(","));
        Scene::from_json(&source).unwrap()
    }

    #[test]
    fn reviews_properties_on_stable_ids_including_the_pressure_plate_color() {
        let review = SceneReview::new(
            scene(&[BLUE_PLATE, OLD_DOOR, MARKER]),
            scene(&[NEW_DOOR, RED_PLATE]),
        );
        assert_eq!(
            review.changes(),
            &[
                ObjectChange {
                    id: "door".to_owned(),
                    changes: ObjectChanges::Modified {
                        color: false,
                        center: true,
                        size: true,
                        solid: true
                    }
                },
                ObjectChange {
                    id: "marker".to_owned(),
                    changes: ObjectChanges::Removed
                },
                ObjectChange {
                    id: "plate".to_owned(),
                    changes: ObjectChanges::Modified {
                        color: true,
                        center: false,
                        size: false,
                        solid: false
                    }
                }
            ]
        );
        assert_eq!(
            review.before().object("plate").unwrap().color,
            [45, 149, 236]
        );
        assert_eq!(review.after().object("plate").unwrap().color, [236, 45, 61]);
    }

    #[test]
    fn reports_added_objects_and_ignores_reordering() {
        let review = SceneReview::new(
            scene(&[OLD_DOOR, BLUE_PLATE]),
            scene(&[BLUE_PLATE, MARKER, OLD_DOOR]),
        );
        assert_eq!(
            review.changes(),
            &[ObjectChange {
                id: "marker".to_owned(),
                changes: ObjectChanges::Added
            }]
        );
    }

    #[test]
    fn rejects_scene_paths_outside_the_worktree_lexically() {
        assert!(relative_git_path(Path::new("scenes/../../other.json")).is_err());
        assert!(relative_git_path(Path::new("")).is_err());
        assert_eq!(
            relative_git_path(Path::new("scenes/room with spaces.json")).unwrap(),
            "scenes/room with spaces.json"
        );
    }

    #[test]
    fn git_review_compares_head_with_the_working_file_including_unstaged_edits() {
        let repo = TestRepo::new();
        let relative = Path::new("scenes/room with spaces.json");
        let room = repo.0.join(relative);
        std::fs::create_dir(repo.0.join("scenes")).unwrap();
        std::fs::write(&room, scene(&[BLUE_PLATE]).to_json().unwrap()).unwrap();
        repo.run(&["add", "--", "scenes/room with spaces.json"]);
        repo.run(&[
            "-c",
            "user.name=Steelgum Test",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=.git/disabled-hooks",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--quiet",
            "-m",
            "Add baseline",
        ]);
        std::fs::write(&room, scene(&[BLUE_PLATE, MARKER]).to_json().unwrap()).unwrap();
        repo.run(&["add", "--", "scenes/room with spaces.json"]);
        std::fs::write(&room, scene(&[RED_PLATE]).to_json().unwrap()).unwrap();

        let result = SceneReview::from_git(&repo.0, relative).unwrap();
        let head = git_output(&repo.0, "HEAD", &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(
            result.base_revision().map(str::as_bytes),
            Some(head.trim_ascii())
        );
        assert_eq!(
            result.before().object("plate").unwrap().color,
            [45, 149, 236]
        );
        assert_eq!(result.after().object("plate").unwrap().color, [236, 45, 61]);
        assert_eq!(result.changes().len(), 1);
        assert!(matches!(
            SceneReview::from_git(&repo.0.join("scenes"), Path::new("room with spaces.json")),
            Err(ReviewError::NotWorktreeRoot(_))
        ));
    }

    struct TestRepo(PathBuf);

    impl TestRepo {
        fn new() -> Self {
            use std::time::{SystemTime, UNIX_EPOCH};

            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("steelgum-review-{}-{stamp}", std::process::id()));
            std::fs::create_dir(&root).unwrap();
            let repo = Self(root);
            repo.run(&["init", "--quiet"]);
            repo
        }

        fn run(&self, args: &[&str]) {
            git_output(&self.0, "test fixture", args).unwrap();
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
