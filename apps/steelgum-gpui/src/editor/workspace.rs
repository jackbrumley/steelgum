use super::{Job, ReviewWindow, RunState};
use gpui::{
    AnimationExt as _, AnyElement, Context, Div, KeyDownEvent, Render, SpringAnimation,
    SpringConfig, Stateful, Window, div, prelude::*, px, rgb,
};

mod navigation;
mod panels;
mod tool;

pub(super) use tool::Tool;

pub(super) const SHELL: u32 = 0x15181f;
const TOOLBAR: u32 = 0x20242e;
const PANEL: u32 = 0x1b1f28;
pub(super) const BORDER: u32 = 0x343b4a;
pub(super) const TEXT: u32 = 0xeef1f7;
pub(super) const MUTED: u32 = 0xaab4c6;
pub(super) const ACCENT: u32 = 0xbab5ff;
const SELECTED: u32 = 0x353451;
const ERROR: u32 = 0xffb5b4;
const PANE_WIDTH: f32 = 258.;

impl ReviewWindow {
    pub(super) fn show_tool(&mut self, tool: Tool) {
        if self.tool != Some(tool) {
            self.hovered_build_profile = None;
        }
        self.definitions.ui_expanded = tool == Tool::Ui;
        self.tool = Some(tool);
        self.closing_pane = None;
        self.reveal_selected_object();
    }

    fn activate_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if tool == Tool::Ui {
            self.toggle_ui_section();
        } else {
            self.show_tool(tool);
        }
        if tool == Tool::Assets {
            self.refresh_assets(cx);
        }
        cx.notify();
    }

    fn select_from_viewport(&mut self, id: String, cx: &mut Context<Self>) {
        let changed = self
            .project
            .review
            .changes()
            .iter()
            .any(|change| change.id == id);
        if self.tool != Some(Tool::Changes) || !changed {
            self.show_tool(Tool::Objects);
        }
        self.select_object(id, cx);
    }

    pub(super) fn select_object(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected.as_ref() != Some(&id) {
            if self.focus {
                self.viewport.reset_pan();
            }
            self.selected = Some(id);
            self.rename = None;
            self.review_draft = Default::default();
        }
        self.reveal_selected_object();
        cx.notify();
    }

    fn matching_tools(&self) -> impl Iterator<Item = Tool> + '_ {
        let query = self.search.trim().to_lowercase();
        [
            Tool::Objects,
            Tool::Assets,
            Tool::Changes,
            Tool::Ui,
            Tool::Build,
        ]
        .into_iter()
        .filter(move |tool| {
            tool.label().to_lowercase().contains(&query)
                || (*tool == Tool::Ui && self.screen_matches(&query))
        })
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.search_focus.focus(window, cx);
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "f6" {
            crate::updates::check(window, cx, true);
            cx.stop_propagation();
            return;
        }
        if self.handle_menu_key(event, window, cx)
            || self.handle_object_key(event, window, cx)
            || self.handle_review_key(event, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        let stroke = &event.keystroke;
        if (stroke.modifiers.control || stroke.modifiers.platform) && stroke.key == "k" {
            self.focus_search(window, cx);
        } else if self.menu_open && self.search_focus.is_focused(window) {
            match stroke.key.as_str() {
                "escape" => {
                    self.search.clear();
                    self.workspace_focus.focus(window, cx);
                }
                "enter" => {
                    let first = self.matching_tools().next();
                    if let Some(tool) = first {
                        self.activate_tool(tool, cx);
                        self.workspace_focus.focus(window, cx);
                    }
                }
                "backspace" => {
                    self.search.pop();
                }
                "a" if stroke.modifiers.control || stroke.modifiers.platform => self.search.clear(),
                _ if !stroke.modifiers.control && !stroke.modifiers.platform => {
                    if let Some(text) = &stroke.key_char
                        && self.search.len() < 128
                        && !text.chars().any(char::is_control)
                    {
                        self.search.push_str(text);
                    }
                }
                _ => return,
            }
            cx.notify();
        } else {
            return;
        }
        cx.stop_propagation();
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let playing = matches!(self.run_state, RunState::Running(Job::Play));
        bar()
            .h(px(56.))
            .child(
                action("menu", "☰", self.menu_open)
                    .aria_label("Toggle tool menu")
                    .aria_expanded(self.menu_open)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.menu_open = !view.menu_open;
                        if !view.menu_open && view.search_focus.is_focused(window) {
                            view.workspace_focus.focus(window, cx);
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_lg()
                    .child("Steelgum"),
            )
            .child(div().ml_2().h(px(18.)).w(px(1.)).bg(rgb(BORDER)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(self.project.name.clone()),
            )
            .when(self.project.source.is_some(), |row| {
                row.child(
                    action("refresh", "Refresh", false)
                        .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
                )
            })
            .child(
                action("open-project", "Open Folder", false)
                    .on_click(cx.listener(|view, _, _, cx| view.open_folder(cx))),
            )
            .child(
                action("search-tools", "Search  Ctrl+K", false)
                    .on_click(cx.listener(|view, _, window, cx| view.focus_search(window, cx))),
            )
            .child(
                action("build-game", "Build Game", self.tool == Some(Tool::Build)).on_click(
                    cx.listener(|view, _, _, cx| {
                        view.show_tool(Tool::Build);
                        cx.notify();
                    }),
                ),
            )
            .child(
                control("play", if playing { "■  Stop" } else { "▶  Play" }, playing)
                    .bg(rgb(ACCENT))
                    .text_color(rgb(0x24203c))
                    .hover(|style| style.bg(rgb(0xd3ceff)))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if playing {
                            view.cancel_job(cx);
                        } else {
                            view.run_game(Job::Play, cx);
                        }
                    })),
            )
    }

    fn pane(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tool = self.tool.or(self.closing_pane);
        let content = div()
            .w(px(PANE_WIDTH))
            .h_full()
            .flex_none()
            .bg(rgb(PANEL))
            .border_l_1()
            .border_color(rgb(BORDER))
            .flex()
            .flex_col()
            .when_some(tool, |pane, tool| {
                let scroll = |content: AnyElement| {
                    div()
                        .id(tool.label())
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .p_3()
                        .child(content)
                        .into_any_element()
                };
                pane.child(
                    bar()
                        .child(
                            div()
                                .flex_1()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(if tool == Tool::Ui {
                                    self.screen_title().to_owned()
                                } else {
                                    tool.label().to_owned()
                                }),
                        )
                        .child(
                            action("close-pane", "×", false)
                                .aria_label("Close tool pane")
                                .tab_stop(self.tool.is_some())
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.closing_pane = view.tool.take();
                                    view.hovered_build_profile = None;
                                    view.workspace_focus.focus(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(match tool {
                    Tool::Objects => self.scene_objects(cx),
                    Tool::Assets => scroll(self.assets_panel(cx)),
                    Tool::Changes => scroll(self.changes(cx)),
                    Tool::Ui => scroll(self.screen_properties(window, cx)),
                    Tool::Build => scroll(self.build(cx)),
                })
                .when(tool == Tool::Objects, |pane| {
                    pane.child(self.object_details(cx))
                })
            });
        slide("tool-pane", PANE_WIDTH, self.tool.is_some(), content)
    }

    fn scene(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let screen = self.screen_preview();
        let previewing_screen = screen.is_some();
        let view = self.viewport_view();
        let source = self.project.source.as_ref();
        let title = source.and_then(|(_, path)| path.file_stem()).map_or_else(
            || "Door Room".into(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let selection_missing = self.focus
            && self
                .selected
                .as_deref()
                .is_some_and(|id| view.scene().object(id).is_none());
        let surface = if selection_missing {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(note("The selected object is missing from this revision."))
                .into_any_element()
        } else {
            let entity = cx.weak_entity();
            let app = cx.to_async();
            self.viewport.element(view, move |id| {
                let entity = entity.clone();
                app.spawn(async move |cx| {
                    let _ = entity.update(cx, |view, cx| view.select_from_viewport(id, cx));
                })
                .detach();
            })
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                bar()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(note(if self.before { "Before" } else { "After" }).flex_none())
                    .children([("3D", false), ("2D", true)].map(|(title, top_down)| {
                        action(title, title, top_down == self.top_down).on_click(cx.listener(
                            move |view, _, _, cx| {
                                view.top_down = top_down;
                                cx.notify();
                            },
                        ))
                    }))
                    .child(
                        div()
                            .w(px(63.))
                            .flex_none()
                            .text_right()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(if self.viewport_fps == 0 {
                                "FPS idle".to_owned()
                            } else {
                                format!("{} FPS", self.viewport_fps)
                            }),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(surface)
                    .when_some(screen, |viewport, preview| {
                        viewport.child(preview)
                    })
                    .when_some(self.live_issue.as_deref(), |viewport, issue| {
                        viewport.child(
                            div()
                                .absolute()
                                .top_3()
                                .left_3()
                                .right_3()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .bg(rgb(SHELL))
                                .p_2()
                                .text_sm()
                                .text_color(rgb(ERROR))
                                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Showing the last valid project"))
                                .child(issue.to_owned()),
                        )
                    }),
            )
            .child(
                bar()
                    .border_b_0()
                    .border_t_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(self.build_profile_hint(window).unwrap_or(if previewing_screen {
                                "UI preview"
                            } else if self.top_down {
                                "Click to select · Middle-drag to pan · Scroll to zoom"
                            } else {
                                "Click to select · Left-drag to orbit · Middle-drag to pan · Scroll to zoom"
                            })),
                    )
                    .when(self.selected.is_some() && !previewing_screen, |bar| {
                        let title = if self.focus {
                            "Show Room"
                        } else {
                            "Focus Object"
                        };
                        bar.child(
                            action("focus-object", title, self.focus).on_click(cx.listener(
                                |view, _, _, cx| {
                                    view.focus = !view.focus;
                                    view.viewport.reset_pan();
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            )
    }
}

impl Render for ReviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("workspace")
            .track_focus(&self.workspace_focus)
            .tab_group()
            .on_key_down(cx.listener(Self::handle_key))
            .size_full()
            .bg(rgb(SHELL))
            .text_color(rgb(TEXT))
            .flex()
            .flex_col()
            .child(self.toolbar(cx))
            .when(self.loading, |app| {
                app.child(status_line("Loading project…", false))
            })
            .when_some(self.status.as_deref(), |app, status| {
                app.child(status_line(status, true))
            })
            .when(self.tool != Some(Tool::Build), |app| {
                app.when_some(self.run_state.message(), |app, status| {
                    app.child(status_line(
                        self.job_progress.as_deref().unwrap_or(status),
                        matches!(self.run_state, RunState::Finished(Err(_))),
                    ))
                })
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.navigation(window, cx))
                    .child(self.scene(window, cx))
                    .child(self.pane(window, cx)),
            )
    }
}

pub(super) fn note(text: impl Into<gpui::SharedString>) -> Div {
    div().text_xs().text_color(rgb(MUTED)).child(text.into())
}

fn status_line(text: &str, error: bool) -> Div {
    div()
        .px_4()
        .py_1()
        .text_sm()
        .truncate()
        .text_color(rgb(if error { ERROR } else { MUTED }))
        .child(text.lines().next().unwrap_or(text).to_owned())
}

fn bar() -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .bg(rgb(TOOLBAR))
        .border_b_1()
        .border_color(rgb(BORDER))
}

pub(super) fn action(
    id: &'static str,
    text: impl Into<gpui::SharedString>,
    active: bool,
) -> Stateful<Div> {
    control(id, text, active).hover(|style| style.bg(rgb(SELECTED)))
}

fn control(id: &'static str, text: impl Into<gpui::SharedString>, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .tab_index(0)
        .role(gpui::accesskit::Role::Button)
        .focus_visible(|style| style.border_color(rgb(ACCENT)))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .px_2()
        .py_1()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .bg(rgb(if active { SELECTED } else { TOOLBAR }))
        .text_color(rgb(if active { ACCENT } else { TEXT }))
        .border_1()
        .border_color(rgb(BORDER))
        .child(text.into())
}

fn slide(id: &'static str, width: f32, open: bool, content: Div) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .overflow_hidden()
        .child(content)
        .with_spring(
            id,
            SpringAnimation::new(SpringConfig::new(360., 38., 1.))
                .to(px(if open { width } else { 0. }))
                .with_epsilon(0.5),
            move |pane, progress| pane.w(px(progress.0.clamp(0., width))),
        )
        .into_any_element()
}
