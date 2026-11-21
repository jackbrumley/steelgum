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
            .into_any_element()
    }

    fn revision_details(&self) -> AnyElement {
        let Some(id) = self.selected.as_deref() else {
            return note("Select an object to review its changes.").into_any_element();
        };
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

    pub(super) fn scene_objects(&self, tool: Tool, cx: &mut Context<Self>) -> AnyElement {
        let scene = if self.before {
            self.project.review.before()
        } else {
            self.project.review.after()
        };
        let title = if tool == Tool::Scene {
            "Scene objects"
        } else {
            "Scene geometry"
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(note(title))
            .children(scene.objects().iter().map(|object| {
                let detail = (tool == Tool::Assets).then(|| {
                    let [x, y, z] = object.size;
                    format!("{x} × {y} × {z}")
                });
                self.object_row(&object.id, detail, cx)
            }))
            .into_any_element()
    }

    fn object_row(&self, id: &str, detail: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let id = id.to_owned();
        let active = self.selected.as_deref() == Some(&id);
        div()
            .id(gpui::SharedString::from(format!("object-{id}")))
            .tab_index(0)
            .tab_stop(self.tool.is_some())
            .role(gpui::accesskit::Role::Button)
            .focus_visible(|style| style.border_color(rgb(ACCENT)))
            .border_1()
            .border_color(rgb(PANEL))
            .cursor_pointer()
            .rounded_md()
            .px_2()
            .py_2()
            .flex()
            .flex_col()
            .bg(rgb(if active { SELECTED } else { PANEL }))
            .hover(|style| style.bg(rgb(SELECTED)))
            .child(div().text_sm().child(label(&id)))
            .when_some(detail, |row, detail| row.child(note(detail)))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.selected = Some(id.clone());
                cx.notify();
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
