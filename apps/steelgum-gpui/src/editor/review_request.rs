use std::fmt::Write as _;

use gpui::{AnyElement, ClipboardItem, Context, KeyDownEvent, Window, div, prelude::*, rgb};
use steelgum::SceneObject;

use super::workspace::{ACCENT, BORDER, MUTED, SHELL, TEXT, action, note};
use super::{Project, ReviewWindow, label};

#[derive(Default)]
pub(super) struct ReviewDraft {
    expanded: bool,
    note: String,
    replace: bool,
    copied: bool,
}

impl ReviewWindow {
    pub(super) fn review_request_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let draft = &self.review_draft;
        let edit = self.review_focus.clone();
        div()
            .mt_2()
            .flex()
            .flex_col()
            .gap_2()
            .when(!draft.expanded, |pane| {
                pane.child(
                    action("flag-object", "Flag for Review", false).on_click(cx.listener(
                        |view, _, window, cx| {
                            view.review_draft.expanded = true;
                            view.review_focus.focus(window, cx);
                            cx.notify();
                        },
                    )),
                )
            })
            .when(draft.expanded, |pane| {
                pane.child(
                    div()
                        .id("review-note")
                        .track_focus(&edit)
                        .tab_index(0)
                        .role(gpui::accesskit::Role::TextInput)
                        .aria_label("Review note")
                        .aria_value(draft.note.clone())
                        .on_click(
                            cx.listener(|view, _, window, cx| view.review_focus.focus(window, cx)),
                        )
                        .border_1()
                        .border_color(rgb(BORDER))
                        .focus(|style| style.border_color(rgb(ACCENT)))
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .bg(rgb(SHELL))
                        .text_sm()
                        .text_color(rgb(if draft.note.is_empty() { MUTED } else { TEXT }))
                        .child(if draft.note.is_empty() {
                            "Add a note (optional)".to_owned()
                        } else {
                            draft.note.clone()
                        }),
                )
                .child(
                    action("copy-review-request", "Copy review request", false)
                        .on_click(cx.listener(|view, _, _, cx| view.copy_review_request(cx))),
                )
                .when(draft.copied, |pane| {
                    pane.child(note("Copied to clipboard."))
                })
            })
            .into_any_element()
    }

    pub(super) fn handle_review_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.review_draft.expanded || !self.review_focus.is_focused(window) {
            return false;
        }
        let draft = &mut self.review_draft;
        let stroke = &event.keystroke;
        let command = stroke.modifiers.control || stroke.modifiers.platform;
        match stroke.key.as_str() {
            "escape" => {
                draft.expanded = false;
                self.workspace_focus.focus(window, cx);
            }
            "enter" => self.copy_review_request(cx),
            "a" if command => draft.replace = true,
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    draft.insert(&text);
                }
            }
            "backspace" => {
                if std::mem::take(&mut draft.replace) {
                    draft.note.clear();
                } else {
                    draft.note.pop();
                }
                draft.copied = false;
            }
            _ if !command => {
                let Some(text) = &stroke.key_char else {
                    return false;
                };
                draft.insert(text);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn copy_review_request(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.as_deref() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.project.review_request(
                id,
                &self.review_draft.note,
                self.before,
            )));
            self.review_draft.copied = true;
            cx.notify();
        }
    }
}

impl ReviewDraft {
    fn insert(&mut self, text: &str) {
        if std::mem::take(&mut self.replace) {
            self.note.clear();
        }
        self.note.extend(
            text.chars()
                .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
                .filter(|ch| !ch.is_control())
                .take(512_usize.saturating_sub(self.note.chars().count())),
        );
        self.copied = false;
    }
}

impl Project {
    fn review_request(&self, id: &str, note: &str, before: bool) -> String {
        let mut request = format!("Review {} in Steelgum.\n", label(id));
        if !note.trim().is_empty() {
            let _ = writeln!(request, "\nReview note\n{}", note.trim());
        }
        if let Some((repo, path)) = &self.source {
            let _ = writeln!(
                request,
                "\nWorktree: {}\nScene: {}",
                repo.display(),
                path.display()
            );
        } else {
            request.push_str("\nScene: examples/door-game/scenes/room.json (built-in example)\n");
        }
        let _ = writeln!(request, "Object ID: {id}");
        if let Some(base) = self.review.base_revision() {
            let _ = writeln!(request, "Git base: {base}");
        }
        let _ = writeln!(
            request,
            "Revision viewed: {}\n",
            if before { "Before" } else { "After" }
        );
        let old = self.review.before().object(id);
        let new = self.review.after().object(id);
        match (old, new) {
            (Some(old), Some(new)) => {
                request.push_str("Object diff (before → after)\n");
                let start = request.len();
                write_diff(&mut request, "center", &old.center, &new.center);
                write_diff(&mut request, "size", &old.size, &new.size);
                write_diff(&mut request, "color", &old.color, &new.color);
                write_diff(&mut request, "solid", &old.solid, &new.solid);
                if request.len() == start {
                    request.push_str("No authored changes.\n");
                    write_object(&mut request, "", new);
                }
            }
            (None, Some(new)) => {
                request.push_str("Object added\n");
                write_object(&mut request, "+ ", new);
            }
            (Some(old), None) => {
                request.push_str("Object removed\n");
                write_object(&mut request, "- ", old);
            }
            (None, None) => {}
        }
        request
    }
}

fn write_diff<T: PartialEq + std::fmt::Debug>(output: &mut String, field: &str, old: &T, new: &T) {
    if old != new {
        let _ = writeln!(output, "- {field}: {old:?}\n+ {field}: {new:?}");
    }
}

fn write_object(output: &mut String, prefix: &str, object: &SceneObject) {
    let _ = writeln!(
        output,
        "{prefix}center: {:?}\n{prefix}size: {:?}\n{prefix}color: {:?}\n{prefix}solid: {}",
        object.center, object.size, object.color, object.solid,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_request_includes_the_selected_object_and_its_changes() {
        let project = Project::demo(crate::demo().unwrap());
        let request = project.review_request("plate", "Check the edge", true);
        for expected in [
            "Check the edge",
            "Object ID: plate",
            "Revision viewed: Before",
            "- color: [45, 149, 236]",
            "+ color: [224, 69, 81]",
        ] {
            assert!(request.contains(expected), "{request}");
        }
        assert!(!request.contains("size:"));
    }
}
