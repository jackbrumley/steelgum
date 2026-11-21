use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use gpui::Context;
use steelgum::project::{MANIFEST_FILE, ProjectManifest};
use steelgum::review::SceneReview;

use super::{Project, ReviewDraft, ReviewWindow, Tool};

const POLL_INTERVAL: Duration = Duration::from_millis(750);

type FileStamp = Result<(SystemTime, u64), io::ErrorKind>;

#[derive(PartialEq, Eq)]
pub(super) struct SourceStamp {
    scene: FileStamp,
    manifest: FileStamp,
    head: Result<String, String>,
}

impl SourceStamp {
    fn read(repo: &Path, scene: &Path) -> Self {
        Self::read_with_manifest(repo, scene, Self::read_manifest(repo))
    }

    pub(super) fn read_manifest(repo: &Path) -> FileStamp {
        file_stamp(&repo.join(MANIFEST_FILE))
    }

    pub(super) fn read_with_manifest(repo: &Path, scene: &Path, manifest: FileStamp) -> Self {
        let scene = file_stamp(&repo.join(scene));
        let head = SceneReview::worktree_head(repo).map_err(|error| error.to_string());
        Self {
            scene,
            manifest,
            head,
        }
    }
}

fn file_stamp(path: &Path) -> FileStamp {
    std::fs::metadata(path)
        .and_then(|metadata| Ok((metadata.modified()?, metadata.len())))
        .map_err(|error| error.kind())
}

struct ProjectRefresh {
    manifest: Option<ProjectManifest>,
    review: SceneReview,
}

impl ProjectRefresh {
    fn read(repo: &Path, scene: &Path) -> Result<Self, String> {
        let manifest = ProjectManifest::discover(repo).map_err(|error| error.to_string())?;
        let review = SceneReview::from_git(repo, scene).map_err(|error| error.to_string())?;
        Ok(Self { manifest, review })
    }
}

struct RefreshDetector {
    handled: SourceStamp,
    pending: Option<SourceStamp>,
}

impl RefreshDetector {
    fn new(handled: SourceStamp) -> Self {
        Self {
            handled,
            pending: None,
        }
    }

    fn ready(&mut self, stamp: SourceStamp, manual: bool) -> Option<SourceStamp> {
        if manual || self.pending.as_ref() == Some(&stamp) {
            self.pending = None;
            Some(stamp)
        } else {
            self.pending = (self.handled != stamp).then_some(stamp);
            None
        }
    }
}

impl ReviewWindow {
    pub(super) fn watch_project(&mut self, cx: &mut Context<Self>) {
        // Dropping the old task cancels the foreground loop before its result
        // can be applied to the newly opened project.
        self.live_refresh = None;
        self.live_issue = None;
        self.refresh_requested = false;
        let Some(((repo, scene), stamp)) = self
            .project
            .source
            .clone()
            .zip(self.project.initial_stamp.take())
        else {
            return;
        };
        self.live_refresh = Some(cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let mut detector = RefreshDetector::new(stamp);
            loop {
                executor.timer(POLL_INTERVAL).await;
                let Ok((opening, manual)) = this.update(cx, |window, _| {
                    if window.loading {
                        (true, false)
                    } else {
                        (false, std::mem::take(&mut window.refresh_requested))
                    }
                }) else {
                    break;
                };
                if opening {
                    continue;
                }
                let stamp = {
                    let repo = repo.clone();
                    let scene = scene.clone();
                    executor
                        .spawn(async move { SourceStamp::read(&repo, &scene) })
                        .await
                };
                let Some(stamp) = detector.ready(stamp, manual) else {
                    continue;
                };
                let loaded = {
                    let repo = repo.clone();
                    let scene = scene.clone();
                    executor
                        .spawn(async move { ProjectRefresh::read(&repo, &scene) })
                        .await
                };
                let Ok(applied) = this.update(cx, |window, cx| {
                    if window.loading {
                        // The picker may be cancelled. Leave this stamp eligible
                        // for another check if the current project stays open.
                        return false;
                    }
                    window.apply_refresh(loaded, cx);
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if applied {
                    // Retry a failed parse only when the project files or Git base
                    // changes again, or the user requests a manual refresh.
                    detector.handled = stamp;
                }
            }
        }));
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.loading && self.live_refresh.is_some() {
            self.refresh_requested = true;
            self.status = None;
            cx.notify();
        }
    }

    fn apply_refresh(&mut self, loaded: Result<ProjectRefresh, String>, cx: &mut Context<Self>) {
        match loaded {
            Ok(ProjectRefresh { manifest, review }) => {
                let old_assets = self
                    .project
                    .manifest
                    .as_ref()
                    .and_then(|project| project.assets.as_ref());
                let new_assets = manifest
                    .as_ref()
                    .and_then(|project| project.assets.as_ref());
                let refresh_assets = self.tool == Some(Tool::Assets) && old_assets != new_assets;
                if let Some((repo, _)) = &self.project.source {
                    self.project.name = Project::name_for(repo, manifest.as_ref());
                }
                self.project.manifest = manifest;
                if refresh_assets {
                    self.refresh_assets(cx);
                }
                if self.selected.as_ref().is_some_and(|id| {
                    review.before().object(id).is_none() && review.after().object(id).is_none()
                }) {
                    self.selected = None;
                    self.rename = None;
                    self.review_draft = ReviewDraft::default();
                    self.focus = false;
                }
                self.project.review = Rc::new(review);
                self.select_created_object(cx);
                self.reveal_selected_object();
                self.live_issue = None;
            }
            Err(error) => self.live_issue = Some(error),
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::process::Command;
    use std::sync::Arc;

    use anyhow::{Result, ensure};
    use gpui::{AppContext, HeadlessAppContext};
    use gpui_wgpu::CosmicTextSystem;

    use super::*;

    #[test]
    fn live_refresh_preserves_the_scene_on_failure_and_recovers() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let repo = directory.path();
        let relative = Path::new("room.json");
        let path = repo.join(relative);
        let sample = crate::demo()?;
        std::fs::write(&path, sample.before().to_json()?)?;
        git(repo, &["init", "-q"])?;
        git(repo, &["add", "room.json"])?;
        git(repo, &["commit", "-qm", "Initial scene"])?;
        let project = Project::open(repo.to_owned(), relative.to_owned())?;
        let mut app = HeadlessAppContext::new(Arc::new(CosmicTextSystem::new("sans-serif")));
        let view = app.new(|cx| {
            let mut view = ReviewWindow::new(project, cx);
            view.selected = Some("plate".into());
            view.before = true;
            view.focus = true;
            view.top_down = true;
            view
        });
        app.run_until_parked();

        std::fs::write(&path, "{")?;
        tick(&app);
        app.update(|cx| {
            assert!(
                view.read(cx).live_issue.is_none(),
                "debounce an unfinished save"
            )
        });
        tick(&app);
        app.update(|cx| {
            let view = view.read(cx);
            assert!(view.live_issue.is_some());
            assert_eq!(
                view.project.review.after().object("plate").unwrap().color,
                [45, 149, 236]
            );
        });

        std::fs::write(&path, sample.after().to_json()?)?;
        tick(&app);
        tick(&app);
        let old_head = app.update(|cx| {
            let view = view.read(cx);
            assert!(view.live_issue.is_none());
            assert_eq!(view.selected.as_deref(), Some("plate"));
            assert!(view.before && view.focus && view.top_down);
            assert_eq!(
                view.project.review.after().object("plate").unwrap().color,
                [224, 69, 81]
            );
            assert!(
                view.project
                    .review
                    .changes()
                    .iter()
                    .any(|change| change.id == "plate")
            );
            view.project.review.base_revision().unwrap().to_owned()
        });

        git(repo, &["add", "room.json"])?;
        git(repo, &["commit", "-qm", "Updated scene"])?;
        tick(&app);
        tick(&app);
        app.update(|cx| {
            let view = view.read(cx);
            assert_ne!(view.project.review.base_revision(), Some(old_head.as_str()));
            assert!(view.project.review.changes().is_empty());
            assert_eq!(view.selected.as_deref(), Some("plate"));
            assert!(!view.project.can_run());
        });

        // The manifest can gain a runner without touching the open scene or Git HEAD.
        std::fs::write(
            repo.join(MANIFEST_FILE),
            r#"{
                "version": 1,
                "name": "Live runner",
                "scene": "room.json",
                "runner": {"manifest": "Cargo.toml", "binary": "live-runner"}
            }"#,
        )?;
        tick(&app);
        tick(&app);
        app.update(|cx| {
            let view = view.read(cx);
            assert!(view.project.can_run());
            assert_eq!(view.project.name, "Live runner");
            assert_eq!(view.selected.as_deref(), Some("plate"));
        });

        std::fs::write(repo.join(MANIFEST_FILE), "{")?;
        tick(&app);
        tick(&app);
        app.update(|cx| {
            let view = view.read(cx);
            assert!(
                view.live_issue
                    .as_deref()
                    .is_some_and(|issue| issue.contains(MANIFEST_FILE))
            );
            assert!(view.project.can_run(), "keep the last valid runner");
        });
        Ok(())
    }

    fn tick(app: &HeadlessAppContext) {
        app.advance_clock(POLL_INTERVAL);
        app.run_until_parked();
    }

    fn git(repo: &Path, args: &[&str]) -> Result<()> {
        let output = Command::new("git")
            .current_dir(repo)
            .args([
                "-c",
                "user.name=Steelgum",
                "-c",
                "user.email=steelgum@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()?;
        ensure!(
            output.status.success(),
            "Git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}
