use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use gpui::{App, Context, FocusHandle, PathPromptOptions, ScrollHandle, Task, Window};
use steelgum::{project::ProjectManifest, review::SceneReview};

use crate::viewport::{View, Viewport};

pub(crate) mod agent;
mod assets;
mod bridge;
mod live_refresh;
mod object_rename;
mod pause_menu;
mod recent;
mod review_request;
mod runner;
mod workspace;

use live_refresh::SourceStamp;
pub(crate) use recent::state_directory;
use review_request::ReviewDraft;

use runner::{Job, JobControl, RunState};
use workspace::Tool;

const ROOM_PATHS: [&str; 2] = ["scenes/room.json", "examples/door-game/scenes/room.json"];
pub(crate) struct Project {
    pub(crate) name: String,
    manifest: Option<ProjectManifest>,
    pub(crate) source: Option<(PathBuf, PathBuf)>,
    initial_stamp: Option<SourceStamp>,
    pub(crate) review: Rc<SceneReview>,
}

impl Project {
    pub fn demo(review: SceneReview) -> Self {
        Self {
            name: "Door Game example".into(),
            manifest: None,
            source: None,
            initial_stamp: None,
            review: Rc::new(review),
        }
    }

    pub fn open(repo: PathBuf, path: PathBuf) -> Result<Self> {
        Ok(LoadedProject::read(repo, Some(path))?.into_project())
    }

    pub fn discover(repo: PathBuf) -> Result<Self> {
        Ok(LoadedProject::read(repo, None)?.into_project())
    }

    fn name_for(repo: &Path, manifest: Option<&ProjectManifest>) -> String {
        manifest.map_or_else(
            || {
                repo.file_name()
                    .unwrap_or(repo.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            },
            |manifest| manifest.name.clone(),
        )
    }
}

struct LoadedProject {
    manifest: Option<ProjectManifest>,
    repo: PathBuf,
    path: PathBuf,
    stamp: SourceStamp,
    review: SceneReview,
}

impl LoadedProject {
    fn read(repo: PathBuf, path: Option<PathBuf>) -> Result<Self> {
        let repo = repo.canonicalize()?;
        let manifest_stamp = SourceStamp::read_manifest(&repo);
        let manifest = ProjectManifest::discover(&repo)?;
        let path = path
            .or_else(|| manifest.as_ref().map(|manifest| manifest.scene.clone()))
            .or_else(|| {
                ROOM_PATHS
                    .iter()
                    .map(PathBuf::from)
                    .find(|path| repo.join(path).is_file())
            })
            .ok_or_else(|| anyhow!("No steelgum.json or supported scene in {}", repo.display()))?;
        // Observe before reading so an edit during the read is still eligible
        // for the next live refresh.
        let stamp = SourceStamp::read_with_manifest(&repo, &path, manifest_stamp);
        let review = SceneReview::from_git(&repo, &path)?;
        Ok(Self {
            manifest,
            repo,
            path,
            stamp,
            review,
        })
    }

    fn into_project(self) -> Project {
        let name = Project::name_for(&self.repo, self.manifest.as_ref());
        Project {
            name,
            manifest: self.manifest,
            source: Some((self.repo, self.path)),
            initial_stamp: Some(self.stamp),
            review: Rc::new(self.review),
        }
    }
}

pub(crate) struct ReviewWindow {
    project: Project,
    before: bool,
    selected: Option<String>,
    review_draft: ReviewDraft,
    assets: assets::AssetBrowser,
    rename: Option<object_rename::RenameDraft>,
    rename_focus: FocusHandle,
    pause_menu: pause_menu::MenuEditor,
    objects_focus: FocusHandle,
    review_focus: FocusHandle,
    objects_scroll: ScrollHandle,
    focus: bool,
    status: Option<String>,
    loading: bool,
    live_refresh: Option<Task<()>>,
    agent_bridge: Option<crate::live_bridge::Bridge>,
    refresh_requested: bool,
    live_issue: Option<String>,
    viewport: Viewport,
    viewport_fps: u32,
    top_down: bool,
    menu_open: bool,
    tool: Option<Tool>,
    closing_pane: Option<Tool>,
    search_focus: FocusHandle,
    workspace_focus: FocusHandle,
    search: String,
    release: bool,
    build_profile_focus: [FocusHandle; 2],
    hovered_build_profile: Option<usize>,
    run_state: RunState,
    job_control: Option<JobControl>,
    job_progress: Option<String>,
}

impl ReviewWindow {
    pub fn new(project: Project, cx: &mut Context<Self>) -> Self {
        Self::sample_viewport_fps(cx);
        let selected = first_selection(&project.review);
        let mut window = Self {
            project,
            before: false,
            selected,
            review_draft: ReviewDraft::default(),
            assets: assets::AssetBrowser::default(),
            rename: None,
            rename_focus: cx.focus_handle(),
            pause_menu: pause_menu::MenuEditor::new(cx),
            objects_focus: cx.focus_handle(),
            review_focus: cx.focus_handle().tab_stop(true),
            objects_scroll: ScrollHandle::new(),
            focus: false,
            status: None,
            loading: false,
            live_refresh: None,
            agent_bridge: None,
            refresh_requested: false,
            live_issue: None,
            viewport: Viewport::default(),
            viewport_fps: 0,
            top_down: false,
            menu_open: true,
            tool: Some(Tool::Changes),
            closing_pane: None,
            search_focus: cx.focus_handle().tab_stop(true),
            workspace_focus: cx.focus_handle(),
            search: String::new(),
            release: false,
            build_profile_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            hovered_build_profile: None,
            run_state: RunState::Idle,
            job_control: None,
            job_progress: None,
        };
        window.watch_project(cx);
        window
    }

    fn viewport_view(&self) -> View {
        View {
            review: self.project.review.clone(),
            before: self.before,
            selection: self.selected.clone(),
            focus: self.focus && self.selected.is_some(),
            top_down: self.top_down,
        }
    }

    pub(crate) fn set_startup_notice(&mut self, notice: Option<String>) {
        self.status = notice;
    }

    fn sample_viewport_fps(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut previous_frames = 0;
            let mut sampled_at = Instant::now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        let now = Instant::now();
                        let frames = view.viewport.rendered_frames();
                        let fps = (frames.saturating_sub(previous_frames) as f64
                            / now.duration_since(sampled_at).as_secs_f64())
                        .round() as u32;
                        previous_frames = frames;
                        sampled_at = now;
                        if view.viewport_fps != fps {
                            view.viewport_fps = fps;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut App) {
        self.workspace_focus.focus(window, cx);
    }

    fn open_folder(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.run_state.busy() || self.pause_menu.saving() {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open a Steelgum worktree".into()),
        });
        self.loading = true;
        self.status = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let selection = match picker.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|mut paths| paths.pop())),
                Ok(Err(error)) => Err(error),
                Err(error) => Err(anyhow!("Could not open the project picker: {error}")),
            };
            let loaded = match selection {
                Ok(Some(repo)) => cx
                    .background_executor()
                    .spawn(async move { LoadedProject::read(repo, None) })
                    .await
                    .map(Some),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |window, cx| {
                window.loading = false;
                match loaded {
                    Ok(Some(project)) => {
                        window.selected = first_selection(&project.review);
                        window.project = project.into_project();
                        if let Err(error) = window.project.remember() {
                            window.status =
                                Some(format!("Could not remember this project: {error:#}"));
                        }
                        window.watch_project(cx);
                        window.before = false;
                        window.review_draft = ReviewDraft::default();
                        window.rename = None;
                        window.pause_menu.clear();
                        window.assets = Default::default();
                        window.focus = false;
                        window.viewport.reset_pan();
                        window.run_state = RunState::Idle;
                        window.show_tool(Tool::Changes);
                    }
                    Ok(None) => {}
                    Err(error) => window.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn first_selection(review: &SceneReview) -> Option<String> {
    review
        .changes()
        .iter()
        .find(|change| change.id == "plate")
        .or_else(|| review.changes().first())
        .map(|change| change.id.clone())
}

fn label(id: &str) -> String {
    if id == "plate" {
        "Pressure plate".into()
    } else {
        let name = id.replace(['-', '_'], " ");
        let mut chars = name.chars();
        chars.next().map_or_else(String::new, |first| {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;

    use gpui::{
        AppContext, HeadlessAppContext, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, PlatformInput, current_headless_renderer, point, px, size,
    };
    use gpui_wgpu::CosmicTextSystem;

    use super::*;

    #[test]
    #[ignore = "requires an available Vulkan adapter"]
    fn renders_the_two_revisions_into_a_gpui_surface() -> Result<()> {
        let mut app = HeadlessAppContext::with_platform(
            Arc::new(CosmicTextSystem::new("sans-serif")),
            Arc::new(()),
            current_headless_renderer,
        );
        let sample = Project::demo(crate::demo()?);
        let window = app.open_window(size(px(980.), px(650.)), |window, cx| {
            cx.new(|cx| {
                let workspace = ReviewWindow::new(sample, cx);
                workspace.focus(window, cx);
                workspace
            })
        })?;
        let after = app.capture_screenshot(window.into())?;
        let (capture, framed, restored) = app.update(|cx| {
            window.update(cx, |review, _, _| -> Result<_> {
                let view = review.viewport_view();
                let capture = review.viewport.capture(&view)?;
                let framed = review.viewport.capture_framed(&View {
                    selection: Some("door".into()),
                    focus: true,
                    ..view.clone()
                })?;
                let restored = review.viewport.capture(&view)?;
                Ok((capture, framed, restored))
            })
        })??;
        let pixels = capture()?;
        let image = image::load_from_memory(&pixels)?;
        assert!(image.width() > 100 && image.height() > 100);
        assert_ne!(pixels, framed()?, "the object should get its own framing");
        assert_eq!(
            pixels,
            restored()?,
            "capture must restore the visible frame"
        );
        app.update(|cx| {
            window.update(cx, |view, _, cx| {
                view.show_tool(Tool::Objects);
                cx.notify();
            })
        })?;
        let assets = app.capture_screenshot(window.into())?;
        app.update_window(window.into(), |_, window, cx| {
            click(window, cx, point(px(815.), px(131.)));
        })?;
        let selected = app.capture_screenshot(window.into())?;
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert_eq!(view.selected.as_deref(), Some("floor"));
            })
        })?;
        // Exclude the tools and scene chrome. Selection must change the scene image.
        assert!(
            (assets.height() / 5..assets.height() * 9 / 10).any(|y| {
                (assets.width() / 4..assets.width() * 7 / 10)
                    .any(|x| assets.get_pixel(x, y) != selected.get_pixel(x, y))
            }),
            "asset selection should move the viewport outline"
        );
        app.update_window(window.into(), |_, window, cx| middle_drag(window, cx))?;
        let panned_3d = app.capture_screenshot(window.into())?;
        assert_ne!(selected, panned_3d, "middle drag should pan the 3D view");
        app.update(|cx| {
            window.update(cx, |view, _, cx| {
                view.viewport.reset_pan();
                cx.notify();
            })
        })?;
        let _ = app.capture_screenshot(window.into())?;
        app.update_window(window.into(), |_, window, cx| {
            click(window, cx, point(px(420.), px(385.)));
        })?;
        app.run_until_parked();
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert_eq!(view.selected.as_deref(), Some("plate"));
            })
        })?;

        app.update(|cx| {
            window.update(cx, |view, window, cx| {
                view.show_tool(Tool::Changes);
                view.selected = Some("plate".into());
                view.before = true;
                view.viewport.reset_pan();
                window.refresh();
                cx.notify();
            })
        })?;
        let before = app.capture_screenshot(window.into())?;
        app.update_window(window.into(), |_, window, cx| {
            drag(window, cx, MouseButton::Left);
        })?;
        app.run_until_parked();
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert_eq!(
                    view.selected.as_deref(),
                    Some("plate"),
                    "orbit must not pick"
                );
            })
        })?;
        let orbited = app.capture_screenshot(window.into())?;
        assert_ne!(before, orbited, "left drag should orbit the 3D view");

        app.update(|cx| {
            window.update(cx, |view, _, cx| {
                view.menu_open = false;
                view.top_down = true;
                cx.notify();
            })
        })?;
        let top_down = app.capture_screenshot(window.into())?;
        assert_ne!(before, top_down, "2D should change the displayed frame");
        app.update_window(window.into(), |_, window, cx| middle_drag(window, cx))?;
        let panned_2d = app.capture_screenshot(window.into())?;
        assert_ne!(top_down, panned_2d, "middle drag should pan the 2D view");

        app.update_window(window.into(), |_, window, cx| {
            assert!(window.dispatch_keystroke(Keystroke::parse("ctrl-k").unwrap(), cx));
        })?;
        let _ = app.capture_screenshot(window.into())?;
        for key in ["b", "u", "enter"] {
            app.update_window(window.into(), |_, window, cx| {
                assert!(window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx));
            })?;
        }
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert!(view.menu_open, "search should open the collapsed menu");
                assert!(
                    view.tool == Some(Tool::Build),
                    "search should open the matching tool"
                );
            })
        })?;

        if let Some(directory) = std::env::var_os("STEELGUM_SNAPSHOT_DIR") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory)?;
            before.save(directory.join("steelgum-gpui-before.png"))?;
            after.save(directory.join("steelgum-gpui-after.png"))?;
            assets.save(directory.join("steelgum-gpui-objects.png"))?;
            top_down.save(directory.join("steelgum-gpui-top-down.png"))?;
        }
        let (blue_before, red_before) = count_plate_pixels(&before);
        let (blue_after, red_after) = count_plate_pixels(&after);
        assert!(
            blue_before > blue_after + 40,
            "expected blue plate in the before view ({blue_before} before, {blue_after} after)"
        );
        assert!(
            red_after > red_before + 40,
            "expected red plate in the after view ({red_before} before, {red_after} after)"
        );
        Ok(())
    }

    fn click(window: &mut Window, cx: &mut App, position: gpui::Point<gpui::Pixels>) {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    }

    fn middle_drag(window: &mut Window, cx: &mut App) {
        drag(window, cx, MouseButton::Middle);
    }

    fn drag(window: &mut Window, cx: &mut App, button: MouseButton) {
        let start = point(px(420.), px(320.));
        let end = point(px(480.), px(345.));
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position: start,
                button,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(button),
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position: end,
                button,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    }

    fn count_plate_pixels(image: &image::RgbaImage) -> (usize, usize) {
        let mut blue = 0;
        let mut red = 0;
        let minimum_x = image.width() / 4;
        let viewport_y = image.height() / 5..image.height() * 9 / 10;
        for (x, y, pixel) in image.enumerate_pixels() {
            if x < minimum_x || !viewport_y.contains(&y) {
                continue;
            }
            let [r, g, b, _] = pixel.0.map(i16::from);
            blue += usize::from(b - r > 36 && b - g > 12 && g - r > 8);
            red += usize::from(r - g > 44 && r - b > 35 && (g - b).abs() < 40);
        }
        (blue, red)
    }
}
