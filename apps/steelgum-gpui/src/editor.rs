use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Result, anyhow, bail};
use gpui::{AnyElement, Context, PathPromptOptions, Render, Window, div, prelude::*, px, rgb};
use steelgum::review::{ObjectChanges, SceneReview};

use crate::viewport::{View, Viewport};

const ROOM_PATHS: [&str; 2] = ["scenes/room.json", "examples/door-game/scenes/room.json"];
const SHELL: u32 = 0x131b22;
const PANEL: u32 = 0x1a2530;
const BORDER: u32 = 0x344453;
const TEXT: u32 = 0xe8f1f5;
const MUTED: u32 = 0xa9bec9;
const ACCENT: u32 = 0x96dfd5;
const SELECTED: u32 = 0x28484a;
const ERROR: u32 = 0xffc2bb;

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
        Ok(Self {
            name: format!("{} · {}", repo.display(), path.display()),
            source: Some((repo, path)),
            review: Rc::new(review),
        })
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
}

impl ReviewWindow {
    pub fn new(project: Project) -> Self {
        let selected = first_selection(&project.review);
        Self {
            project,
            before: false,
            selected,
            focus: false,
            status: None,
            loading: false,
            viewport: Viewport::default(),
        }
    }

    fn open_folder(&mut self, cx: &mut Context<Self>) {
        if self.loading {
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
                    Ok(Some((repo, path, review))) => window.apply_loaded(repo, path, review),
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
        if self
            .selected
            .as_ref()
            .is_none_or(|id| !review.changes().iter().any(|item| &item.id == id))
        {
            self.selected = first_selection(&review);
        }
        if self.selected.is_none() {
            self.focus = false;
        }
        self.project = Project {
            name: format!("{} · {}", repo.display(), path.display()),
            source: Some((repo, path)),
            review: Rc::new(review),
        };
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let changes = self.project.review.changes();
        div()
            .id("changes")
            .w(px(240.))
            .flex_none()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll()
            .child(div().text_color(rgb(TEXT)).child("Changes"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(if self.project.source.is_some() {
                        "HEAD → Working tree"
                    } else {
                        "Before → After · example"
                    }),
            )
            .when(changes.is_empty(), |panel| {
                panel.child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child("No scene changes."),
                )
            })
            .children(changes.iter().enumerate().map(|(index, change)| {
                let id = change.id.clone();
                let selected = self.selected.as_deref() == Some(id.as_str());
                div()
                    .id(("scene-change", index))
                    .cursor_pointer()
                    .rounded_md()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .bg(rgb(if selected { SELECTED } else { PANEL }))
                    .hover(|style| style.bg(rgb(SELECTED)))
                    .child(label(&id))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(change_kind(change.changes)),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.selected = Some(id.clone());
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    fn details(&self) -> AnyElement {
        let Some(id) = self.selected.as_deref() else {
            return div()
                .text_sm()
                .text_color(rgb(MUTED))
                .child("Select a scene change to compare it.")
                .into_any_element();
        };
        let before = self.project.review.before().object(id);
        let after = self.project.review.after().object(id);
        let mut row = div()
            .flex()
            .items_center()
            .gap_3()
            .text_sm()
            .child(label(id));
        match (before, after) {
            (Some(before), Some(after)) => {
                if before.color != after.color {
                    row = row
                        .child("Colour")
                        .child(swatch(before.color))
                        .child("→")
                        .child(swatch(after.color));
                } else {
                    row = row.child(change_kind(
                        self.project
                            .review
                            .changes()
                            .iter()
                            .find(|change| change.id == id)
                            .expect("selection comes from the current change list")
                            .changes,
                    ));
                }
            }
            (None, Some(_)) => row = row.child("Added"),
            (Some(_), None) => row = row.child("Removed"),
            (None, None) => {}
        }
        row.into_any_element()
    }
}

impl Render for ReviewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = View {
            review: self.project.review.clone(),
            before: self.before,
            selection: self.selected.clone(),
            focus: self.focus && self.selected.is_some(),
        };

        let open = action("open-project", "Open Folder", false)
            .on_click(cx.listener(|view, _, _, cx| view.open_folder(cx)));

        div()
            .size_full()
            .bg(rgb(SHELL))
            .text_color(rgb(TEXT))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_color(rgb(ACCENT)).child("Steelgum"))
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .text_color(rgb(MUTED))
                                    .child(self.project.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(self.project.source.is_some(), |actions| {
                                actions.child(
                                    action("refresh", "Refresh", false)
                                        .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
                                )
                            })
                            .child(open),
                    ),
            )
            .when(self.loading, |window| {
                window.child(
                    div()
                        .px_4()
                        .py_1()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child("Loading project…"),
                )
            })
            .when_some(self.status.clone(), |window, status| {
                window.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_sm()
                        .text_color(rgb(ERROR))
                        .child(status),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_4()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(div().flex_1().child(if self.focus {
                                        "Object"
                                    } else {
                                        "Scene"
                                    }))
                                    .child(action("before", "Before", self.before).on_click(
                                        cx.listener(|view, _, _, cx| {
                                            view.before = true;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(action("after", "After", !self.before).on_click(
                                        cx.listener(|view, _, _, cx| {
                                            view.before = false;
                                            cx.notify();
                                        }),
                                    ))
                                    .when(self.selected.is_some(), |controls| {
                                        controls.child(
                                            action(
                                                "focus",
                                                if self.focus {
                                                    "Show Room"
                                                } else {
                                                    "Focus Selection"
                                                },
                                                self.focus,
                                            )
                                            .on_click(
                                                cx.listener(|view, _, _, cx| {
                                                    view.focus = !view.focus;
                                                    cx.notify();
                                                }),
                                            ),
                                        )
                                    }),
                            )
                            .child(div().flex_1().min_h_0().child(self.viewport.element(view)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .px_4()
                                    .py_2()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .child(self.details())
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .child("Drag to orbit · Scroll to zoom"),
                                    ),
                            ),
                    ),
            )
    }
}

fn action(id: &'static str, text: &'static str, active: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(if active { SELECTED } else { PANEL }))
        .border_1()
        .border_color(rgb(BORDER))
        .hover(|style| style.bg(rgb(SELECTED)))
        .child(text)
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
        id.replace(['-', '_'], " ")
    }
}

fn change_kind(changes: ObjectChanges) -> String {
    match changes {
        ObjectChanges::Added => "Added".into(),
        ObjectChanges::Removed => "Removed".into(),
        ObjectChanges::Modified {
            color,
            center,
            size,
            solid,
        } => [
            (color, "Colour"),
            (center, "Position"),
            (size, "Size"),
            (solid, "Collision"),
        ]
        .into_iter()
        .filter_map(|(changed, label)| changed.then_some(label))
        .collect::<Vec<_>>()
        .join(", "),
    }
}

fn swatch([r, g, b]: [u8; 3]) -> impl IntoElement {
    let color = u32::from_be_bytes([0, r, g, b]);
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().size(px(12.)).rounded_sm().bg(rgb(color)))
        .child(format!("#{color:06X}"))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, HeadlessAppContext, current_headless_renderer, size};
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
        let window = app.open_window(size(px(980.), px(650.)), |_, cx| {
            cx.new(|_| ReviewWindow::new(sample))
        })?;
        let after = app.capture_screenshot(window.into())?;
        app.update(|cx| {
            window.update(cx, |view, window, cx| {
                view.before = true;
                window.refresh();
                cx.notify();
            })
        })?;
        let before = app.capture_screenshot(window.into())?;

        if let Some(directory) = std::env::var_os("STEELGUM_SNAPSHOT_DIR") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory)?;
            before.save(directory.join("steelgum-gpui-before.png"))?;
            after.save(directory.join("steelgum-gpui-after.png"))?;
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
