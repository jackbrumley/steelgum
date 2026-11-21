use gpui::{AnyElement, ClickEvent, Context, Window, div, prelude::*, px, rgb};
use steelgum::review::ObjectChanges;

use super::{
    ACCENT, BORDER, ERROR, Job, MUTED, ReviewWindow, RunState, SELECTED, Tool, action,
    matches_entry, note, sidebar_action,
};
use crate::editor::{label, object_rename::object_label};

const BUILD_PROFILES: [(&str, bool, &str); 2] = [
    ("Debug", false, "Build faster for development and testing."),
    (
        "Release",
        true,
        "Build with optimizations for better game performance.",
    ),
];

impl ReviewWindow {
    pub(super) fn changes_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let changes = self.project.review.changes();
        let query = self.child_query(Tool::Changes);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(note(if self.project.source.is_some() {
                "HEAD → Working tree"
            } else {
                "Example comparison"
            }))
            .when(changes.is_empty(), |panel| {
                panel.child(note("No scene changes."))
            })
            .children(
                changes
                    .iter()
                    .filter(|change| {
                        matches_entry(&self.object_title(&change.id), &change.id, &query)
                    })
                    .map(|change| {
                        self.object_row(&change.id, Some(change_kind(change.changes)), cx)
                    }),
            )
            .into_any_element()
    }

    pub(super) fn change_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .mt_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(self.revision_details())
                    .child(div().flex().gap_2().children(
                        [("Before", true), ("After", false)].map(|(title, before)| {
                            action(title, title, before == self.before)
                                .flex_1()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.before = before;
                                    view.reveal_selected_object();
                                    cx.notify();
                                }))
                        }),
                    )),
            )
            .when(self.selected.is_some(), |panel| {
                panel.child(self.review_request_control(cx))
            })
            .into_any_element()
    }

    fn revision_details(&self) -> AnyElement {
        let Some(id) = self.selected.as_deref() else {
            return note("Select an object to review its changes.").into_any_element();
        };
        if !self
            .project
            .review
            .changes()
            .iter()
            .any(|change| change.id == id)
        {
            return note("The selected object has no changes.").into_any_element();
        }
        let (before, after) = (
            self.project.review.before().object(id),
            self.project.review.after().object(id),
        );
        let (before, after) = match (before, after) {
            (Some(before), Some(after)) => (before, after),
            (Some(_), None) => return note("Removed · only in Before").into_any_element(),
            (None, Some(_)) => return note("Added · only in After").into_any_element(),
            (None, None) => {
                return note("This object is absent from both revisions.").into_any_element();
            }
        };
        if before.color != after.color {
            return div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .child(swatch(before.color))
                .child("→")
                .child(swatch(after.color))
                .into_any_element();
        }
        if before.size != after.size {
            let dimensions = |[x, y, z]: [f32; 3]| format!("{x} × {y} × {z}");
            return note(format!(
                "{} → {}",
                dimensions(before.size),
                dimensions(after.size)
            ))
            .into_any_element();
        }
        if before.name != after.name {
            return note(format!(
                "{} → {}",
                object_label(before),
                object_label(after)
            ))
            .into_any_element();
        }
        note(object_label(after)).into_any_element()
    }

    pub(super) fn scene_objects(
        &self,
        max_height: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scene = self.scene_revision();
        let list = div()
            .id("objects-list")
            .track_focus(&self.objects_focus)
            .tab_index(0)
            .role(gpui::accesskit::Role::List)
            .aria_label("Scene objects")
            .track_scroll(&self.objects_scroll)
            .max_h(max_height)
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .when(scene.objects().is_empty(), |pane| {
                pane.child(div().px_2().child(note("No objects in this revision.")))
            })
            .children(
                self.visible_objects()
                    .map(|object| self.object_row(&object.id, None, cx)),
            );
        div()
            .flex()
            .flex_col()
            .ml_3()
            .gap_1()
            .when_some(self.add_box_button(cx), |pane, button| pane.child(button))
            .child(list)
            .into_any_element()
    }

    pub(super) fn object_details(&self, cx: &mut Context<Self>) -> AnyElement {
        let selection = self
            .selected
            .as_deref()
            .and_then(|id| self.scene_revision().object(id));
        div()
            .flex_none()
            .border_t_1()
            .border_color(rgb(BORDER))
            .px_3()
            .py_2()
            .when_some(selection, |pane, object| {
                let [x, y, z] = object.size;
                pane.child(div().truncate().text_sm().child(object_label(object)))
                    .child(note(format!("{x} × {y} × {z}")))
                    .when_some(self.rename_field(&object.id, cx), |pane, input| {
                        pane.child(input)
                    })
                    .when_some(self.rename_control(cx), |pane, control| pane.child(control))
                    .when_some(self.open_scene_button(cx), |pane, button| {
                        pane.child(div().mt_2().child(button))
                    })
                    .when(
                        self.project
                            .review
                            .changes()
                            .iter()
                            .any(|change| change.id == object.id),
                        |pane| {
                            pane.child(
                                action("compare-selection", "Compare Changes", false)
                                    .mt_2()
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.show_tool(Tool::Changes);
                                        cx.notify();
                                    })),
                            )
                        },
                    )
                    .child(self.review_request_control(cx))
            })
            .when(selection.is_none(), |pane| {
                let other = if self.before {
                    self.project.review.after()
                } else {
                    self.project.review.before()
                };
                if let Some(object) = self.selected.as_deref().and_then(|id| other.object(id)) {
                    let (status, button) = if self.before {
                        ("Added · only in After", "Show After")
                    } else {
                        ("Removed · only in Before", "Show Before")
                    };
                    pane.child(div().truncate().text_sm().child(object_label(object)))
                        .child(note(status))
                        .child(
                            action("show-object-revision", button, false)
                                .mt_2()
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.before = !view.before;
                                    view.reveal_selected_object();
                                    cx.notify();
                                })),
                        )
                } else {
                    pane.child(note("Select an object to see its dimensions."))
                }
            })
            .into_any_element()
    }

    pub(in crate::editor) fn scene_revision(&self) -> &steelgum::Scene {
        if self.before {
            self.project.review.before()
        } else {
            self.project.review.after()
        }
    }

    pub(in crate::editor) fn visible_objects(
        &self,
    ) -> impl Iterator<Item = &steelgum::SceneObject> {
        let query = self.child_query(Tool::Objects);
        self.scene_revision()
            .objects()
            .iter()
            .filter(move |object| matches_entry(&object_label(object), &object.id, &query))
    }

    pub(in crate::editor) fn reveal_selected_object(&self) {
        if self.tool == Some(Tool::Objects)
            && let Some(id) = self.selected.as_deref()
            && let Some(index) = self.visible_objects().position(|object| object.id == id)
        {
            self.objects_scroll.scroll_to_item(index);
        }
    }

    pub(super) fn object_title(&self, id: &str) -> String {
        self.scene_revision()
            .object(id)
            .or_else(|| self.project.review.after().object(id))
            .or_else(|| self.project.review.before().object(id))
            .map_or_else(|| label(id), object_label)
    }

    fn object_row(&self, id: &str, detail: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let editable = detail.is_none();
        let title = self.object_title(id);
        let id = id.to_owned();
        let tool = if editable {
            Tool::Objects
        } else {
            Tool::Changes
        };
        let active = self.tool == Some(tool) && self.selected.as_deref() == Some(&id);
        div()
            .id(gpui::SharedString::from(format!("object-{id}")))
            .tab_index(0)
            .tab_stop(self.menu_open)
            .role(gpui::accesskit::Role::Button)
            .aria_toggled(if active {
                gpui::accesskit::Toggled::True
            } else {
                gpui::accesskit::Toggled::False
            })
            .focus_visible(|style| style.underline())
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_1()
            .flex()
            .flex_col()
            .when(active, |row| row.bg(rgb(SELECTED)).text_color(rgb(ACCENT)))
            .hover(|style| style.text_color(rgb(ACCENT)))
            .child(div().truncate().text_sm().child(title))
            .when_some(detail, |row, detail| row.child(note(detail)))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                view.show_tool(tool);
                view.select_object(id.clone(), cx);
                if editable {
                    view.objects_focus.focus(window, cx);
                }
            }))
            .into_any_element()
    }

    pub(super) fn build(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut pane = div().flex().flex_col().gap_3();
        if self.project.can_build() {
            pane =
                pane.child(note("Target · This computer"))
                    .child(div().flex().gap_2().children(
                        BUILD_PROFILES.into_iter().enumerate().map(
                            |(index, (title, release, hint))| {
                                sidebar_action(title, title, self.release == release)
                                    .track_focus(&self.build_profile_focus[index])
                                    .tab_stop(self.expanded_tool == Some(Tool::Build))
                                    .aria_description(hint)
                                    .flex_1()
                                    .on_hover(cx.listener(move |view, hovered, _, cx| {
                                        if view.expanded_tool != Some(Tool::Build) {
                                            return;
                                        }
                                        if *hovered {
                                            view.hovered_build_profile = Some(index);
                                        } else if view.hovered_build_profile == Some(index) {
                                            view.hovered_build_profile = None;
                                        }
                                        cx.notify();
                                    }))
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if !view.run_state.busy() {
                                            view.release = release;
                                            cx.notify();
                                        }
                                    }))
                            },
                        ),
                    ))
                    .child(
                        sidebar_action("run-build", "Build", false)
                            .on_click(cx.listener(|view, _, _, cx| view.run_game(Job::Build, cx))),
                    );
        } else if !self.project.can_run() {
            pane = pane.child(note(
                "Declare a Cargo runner in steelgum.json to build and play this project.",
            ));
        } else {
            pane = pane.child(note("The bundled game is ready to play."));
        }
        if let RunState::Running(job) = self.run_state {
            let title = if job == Job::Build {
                "Cancel Build"
            } else {
                "Stop Play"
            };
            pane = pane.child(
                sidebar_action("stop-job", title, false)
                    .on_click(cx.listener(|view, _, _, cx| view.cancel_job(cx))),
            );
        }
        if let Some(progress) = &self.job_progress {
            pane = pane.child(note(progress.clone()));
        }
        pane.when_some(self.run_state.message(), |pane, status| {
            pane.child(
                div()
                    .text_sm()
                    .text_color(rgb(
                        if matches!(self.run_state, RunState::Finished(Err(_))) {
                            ERROR
                        } else {
                            MUTED
                        },
                    ))
                    .child(status.to_owned()),
            )
        })
        .into_any_element()
    }

    pub(super) fn build_profile_hint(&self, window: &Window) -> Option<&'static str> {
        if self.expanded_tool != Some(Tool::Build) || !self.menu_open || !self.project.can_build() {
            return None;
        }
        let index = if window.last_input_was_keyboard() {
            self.build_profile_focus
                .iter()
                .position(|focus| focus.is_focused(window))
        } else {
            self.hovered_build_profile
        }?;
        Some(BUILD_PROFILES[index].2)
    }
}

fn change_kind(changes: ObjectChanges) -> String {
    match changes {
        ObjectChanges::Added => "Added".into(),
        ObjectChanges::Removed => "Removed".into(),
        ObjectChanges::Modified {
            name,
            color,
            center,
            size,
            solid,
        } => [
            (name, "Name"),
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
        .gap_1()
        .child(div().size(px(10.)).rounded_sm().bg(rgb(color)))
        .child(format!("#{color:06X}"))
}
