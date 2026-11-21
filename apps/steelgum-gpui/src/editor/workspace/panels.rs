use gpui::{AnyElement, ClickEvent, Context, div, prelude::*, px, rgb};
use steelgum::review::ObjectChanges;

use super::{
    ACCENT, BORDER, ERROR, Job, MUTED, PANEL, ReviewWindow, RunState, SELECTED, Tool, action, note,
};
use crate::editor::label;

impl ReviewWindow {
    pub(super) fn changes(&self, cx: &mut Context<Self>) -> AnyElement {
        let changes = self.project.review.changes();
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
                changes.iter().map(|change| {
                    self.object_row(&change.id, Some(change_kind(change.changes)), cx)
                }),
            )
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
        let (Some(before), Some(after)) = (
            self.project.review.before().object(id),
            self.project.review.after().object(id),
        ) else {
            return note("This object exists in one revision.").into_any_element();
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
        note(label(id)).into_any_element()
    }

    pub(super) fn scene_objects(&self, cx: &mut Context<Self>) -> AnyElement {
        let scene = self.scene_revision();
        div()
            .id("objects-list")
            .track_scroll(&self.objects_scroll)
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .when(scene.objects().is_empty(), |pane| {
                pane.child(div().px_2().child(note("No objects in this revision.")))
            })
            .children(
                scene
                    .objects()
                    .iter()
                    .map(|object| self.object_row(&object.id, None, cx)),
            )
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
                pane.child(div().truncate().text_sm().child(label(&object.id)))
                    .child(note(format!("{x} × {y} × {z}")))
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
                pane.child(note("Select an object to see its dimensions."))
            })
            .into_any_element()
    }

    pub(super) fn scene_revision(&self) -> &steelgum::Scene {
        if self.before {
            self.project.review.before()
        } else {
            self.project.review.after()
        }
    }

    fn object_row(&self, id: &str, detail: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let id = id.to_owned();
        let active = self.selected.as_deref() == Some(&id);
        div()
            .id(gpui::SharedString::from(format!("object-{id}")))
            .tab_index(0)
            .tab_stop(self.tool.is_some())
            .role(gpui::accesskit::Role::Button)
            .aria_toggled(if active {
                gpui::accesskit::Toggled::True
            } else {
                gpui::accesskit::Toggled::False
            })
            .focus_visible(|style| style.border_color(rgb(ACCENT)))
            .border_1()
            .border_color(rgb(if active { ACCENT } else { PANEL }))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_1()
            .flex()
            .flex_col()
            .bg(rgb(if active { SELECTED } else { PANEL }))
            .when(!active, |row| {
                row.hover(|style| style.bg(rgb(super::TOOLBAR)))
            })
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(if active { ACCENT } else { super::TEXT }))
                    .child(label(&id)),
            )
            .when_some(detail, |row, detail| row.child(note(detail)))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.select_object(id.clone(), cx);
            }))
            .into_any_element()
    }

    pub(super) fn build(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut pane = div().flex().flex_col().gap_3();
        if self.project.can_run() {
            pane =
                pane.child(note("Target · This computer"))
                    .child(div().flex().gap_2().children(
                        [("Debug", false), ("Release", true)].map(|(title, release)| {
                            action(title, title, self.release == release)
                                .flex_1()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if !view.run_state.busy() {
                                        view.release = release;
                                        cx.notify();
                                    }
                                }))
                        }),
                    ))
                    .child(
                        action("run-build", "Build", false)
                            .on_click(cx.listener(|view, _, _, cx| view.run_game(Job::Build, cx))),
                    );
        } else {
            pane = pane.child(note(
                "Build and Play are available for Door Game projects with the Bevy runner.",
            ));
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
        .gap_1()
        .child(div().size(px(10.)).rounded_sm().bg(rgb(color)))
        .child(format!("#{color:06X}"))
}
