use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use super::Project;

#[derive(Deserialize, Serialize)]
struct RecentProject {
    worktree: PathBuf,
    scene: Option<PathBuf>,
}

impl Project {
    pub(crate) fn reopen() -> Result<Option<Self>> {
        let Some(path) = state_path() else {
            return Ok(None);
        };
        let contents = match std::fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("Could not read {}", path.display()));
            }
        };
        let recent: RecentProject = serde_json::from_slice(&contents)?;
        match recent.scene {
            Some(scene) => Self::open(recent.worktree, scene).map(Some),
            None => Self::discover(recent.worktree).map(Some),
        }
    }

    pub(crate) fn remember(&self) -> Result<()> {
        let (Some(path), Some((worktree, scene))) = (state_path(), &self.source) else {
            return Ok(());
        };
        let directory = path.parent().context("Recent project file has no parent")?;
        std::fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(
            temporary.as_file_mut(),
            &RecentProject {
                worktree: worktree.clone(),
                scene: self
                    .manifest
                    .as_ref()
                    .is_none_or(|manifest| &manifest.scene != scene)
                    .then(|| scene.clone()),
            },
        )?;
        temporary.write_all(b"\n")?;
        temporary.persist(path)?;
        Ok(())
    }
}

fn state_path() -> Option<PathBuf> {
    let override_directory = std::env::var_os("STEELGUM_STATE_DIR").map(PathBuf::from);
    #[cfg(windows)]
    let directory = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let directory = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));
    override_directory
        .or_else(|| directory.map(|directory| directory.join("steelgum")))
        .map(|directory| directory.join("last-project.json"))
}
