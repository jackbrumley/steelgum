use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use gpui::{App, Context, FocusHandle, PathPromptOptions, Window};
use steelgum::review::SceneReview;

use crate::viewport::Viewport;

mod runner;
mod workspace;

use runner::{Job, RunState};
use workspace::Tool;

const ROOM_PATHS: [&str; 2] = ["scenes/room.json", "examples/door-game/scenes/room.json"];
pub(crate) struct Project {
    name: String,
    source: Option<(PathBuf, PathBuf)>,
    review: Rc<SceneReview>,
}

impl Project {
    pub fn demo(review: SceneReview) -> Self {
        Self {
            name: "Door Game example".into(),
            source: None,
            review: Rc::new(review),
        }
    }

    pub fn open(repo: PathBuf, path: PathBuf) -> Result<Self> {
        let review = SceneReview::from_git(&repo, &path)?;
        Ok(Self::loaded(repo, path, review))
    }

    fn loaded(repo: PathBuf, path: PathBuf, review: SceneReview) -> Self {
        let name = repo.file_name().unwrap_or(repo.as_os_str());
        Self {
            name: name.to_string_lossy().into_owned(),
            source: Some((repo, path)),
            review: Rc::new(review),
        }
    }
}

pub(crate) struct ReviewWindow {
    project: Project,
    before: bool,
    selected: Option<String>,
    focus: bool,
    status: Option<String>,
    loading: bool,
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
    run_state: RunState,
}

impl ReviewWindow {
    pub fn new(project: Project, cx: &mut Context<Self>) -> Self {
        Self::sample_viewport_fps(cx);
        let selected = first_selection(&project.review);
        Self {
            project,
            before: false,
            selected,
            focus: false,
            status: None,
            loading: false,
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
            run_state: RunState::Idle,
        }
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
                        let fps = ((frames - previous_frames) as f64
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
        if self.loading || self.run_state.busy() {
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
                    .spawn(async move { open_first_room(repo) })
                    .await
                    .map(Some),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |window, cx| {
                window.loading = false;
                match loaded {
                    Ok(Some((repo, path, review))) => {
                        window.apply_loaded(repo, path, review);
                        window.before = false;
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

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some((repo, path)) = self.project.source.clone() else {
            return;
        };
        self.loading = true;
        self.status = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let review = SceneReview::from_git(&repo, &path)?;
                    Ok::<_, anyhow::Error>((repo, path, review))
                })
                .await;
            let _ = this.update(cx, |window, cx| {
                window.loading = false;
                match loaded {
                    Ok((repo, path, review)) => window.apply_loaded(repo, path, review),
                    Err(error) => window.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_loaded(&mut self, repo: PathBuf, path: PathBuf, review: SceneReview) {
        if self.selected.as_ref().is_none_or(|id| {
            review.before().object(id).is_none() && review.after().object(id).is_none()
        }) {
            self.selected = first_selection(&review);
        }
        if self.selected.is_none() {
            self.focus = false;
        }
        self.project = Project::loaded(repo, path, review);
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

fn open_first_room(repo: PathBuf) -> Result<(PathBuf, PathBuf, SceneReview)> {
    let Some(scene) = ROOM_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| repo.join(path).is_file())
    else {
        bail!(
            "No supported room at {}. Try scenes/room.json or pass a relative scene path on launch.",
            repo.display()
        );
    };
    let review = SceneReview::from_git(&repo, &scene)?;
    Ok((repo, scene, review))
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
        app.update_window(window.into(), |_, window, cx| middle_drag(window, cx))?;
        let panned_3d = app.capture_screenshot(window.into())?;
        assert_ne!(after, panned_3d, "middle drag should pan the 3D view");

        app.update(|cx| {
            window.update(cx, |view, window, cx| {
                view.before = true;
                view.viewport.reset_pan();
                window.refresh();
                cx.notify();
            })
        })?;
        let before = app.capture_screenshot(window.into())?;

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

    fn middle_drag(window: &mut Window, cx: &mut App) {
        let start = point(px(420.), px(320.));
        let end = point(px(480.), px(345.));
        let button = MouseButton::Middle;
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
