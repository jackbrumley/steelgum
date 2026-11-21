use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use serde_json::json;

use super::Project;

const USAGE: &str = "Usage: steelgum --agent <inspect|select|review> WORKTREE [--scene PATH] [--object ID] [--before] [--note TEXT]";

pub(crate) fn run(arguments: &[OsString]) -> Result<()> {
    let Some((operation, remaining)) = arguments.split_first() else {
        bail!(USAGE)
    };
    if operation == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    let Some((worktree, options)) = remaining.split_first() else {
        bail!(USAGE)
    };
    let mut scene = None;
    let mut object = None;
    let mut before = false;
    let mut note = None;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.to_str() {
            Some("--scene") => {
                scene = Some(PathBuf::from(
                    options.next().context("--scene needs a path")?,
                ))
            }
            Some("--object") => {
                object = Some(
                    options
                        .next()
                        .and_then(|value| value.to_str())
                        .context("--object needs a UTF-8 ID")?,
                )
            }
            Some("--note") => {
                note = Some(
                    options
                        .next()
                        .and_then(|value| value.to_str())
                        .context("--note needs UTF-8 text")?,
                )
            }
            Some("--before") => before = true,
            _ => bail!("Unknown agent option {}\n{USAGE}", option.to_string_lossy()),
        }
    }
    let worktree = PathBuf::from(worktree);
    let project = match scene {
        Some(scene) => Project::open(worktree, scene)?,
        None => Project::discover(worktree)?,
    };
    let (worktree, scene) = project.source.as_ref().context("Project has no worktree")?;
    let revision = if before {
        project.review.before()
    } else {
        project.review.after()
    };
    let data = match operation.to_str() {
        Some("inspect") => json!({
            "objects": revision.objects(),
            "changed_objects": project.review.changes().iter().map(|change| &change.id).collect::<Vec<_>>(),
            "runner": project.manifest.as_ref().and_then(|manifest| manifest.runner.as_ref()),
            "assets": project.manifest.as_ref().and_then(|manifest| manifest.assets.as_ref()),
        }),
        Some("select") => {
            let id = object.context("select requires --object ID")?;
            let selected = revision
                .object(id)
                .with_context(|| format!("No object {id} in the requested revision"))?;
            json!({ "selection": selected, "changed": project.review.changes().iter().any(|change| change.id == id) })
        }
        Some("review") => {
            let id = object.context("review requires --object ID")?;
            if project.review.before().object(id).is_none()
                && project.review.after().object(id).is_none()
            {
                bail!("No object {id} in either revision");
            }
            json!({ "object": id, "request": project.review_request(id, note.unwrap_or(""), before) })
        }
        _ => bail!(USAGE),
    };
    let result = json!({
        "protocol": 1,
        "project": project.name,
        "worktree": worktree,
        "scene": scene,
        "git_base": project.review.base_revision(),
        "revision": if before { "before" } else { "after" },
        "data": data,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
