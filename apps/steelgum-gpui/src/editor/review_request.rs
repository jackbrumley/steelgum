use std::fmt::Write as _;
use std::io::Write as _;
use std::rc::Rc;

use anyhow::Context as _;

use gpui::{AnyElement, ClipboardItem, Context, KeyDownEvent, Window, div, prelude::*, rgb};
use steelgum::SceneObject;

use super::object_rename::object_label;
use super::workspace::{ACCENT, BORDER, MUTED, SHELL, TEXT, action, note};
use super::{Project, ReviewWindow, label};

#[derive(Default)]
pub(super) struct ReviewDraft {
    expanded: bool,
    note: String,
    replace: bool,
    copied: bool,
    include_screenshot: bool,
    pending_capture: Option<Rc<()>>,
    copy_error: Option<String>,
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
                    action(
                        "include-screenshot",
                        if draft.include_screenshot {
                            "✓ Include viewport screenshot"
                        } else {
                            "Include viewport screenshot"
                        },
                        draft.include_screenshot,
                    )
                    .aria_toggled(if draft.include_screenshot {
                        gpui::accesskit::Toggled::True
                    } else {
                        gpui::accesskit::Toggled::False
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.review_draft.include_screenshot =
                            !view.review_draft.include_screenshot;
                        view.review_draft.clear_copy();
                        cx.notify();
                    })),
                )
                .child(
                    action(
                        "copy-review-request",
                        if draft.pending_capture.is_some() {
                            "Saving screenshot…"
                        } else {
                            "Copy review request"
                        },
                        false,
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.copy_review_request(cx))),
                )
                .when(draft.copied, |pane| {
                    pane.child(note(if draft.include_screenshot {
                        "Copied with the saved PNG path."
                    } else {
                        "Copied to clipboard."
                    }))
                })
                .when_some(draft.copy_error.as_ref(), |pane, error| {
                    pane.child(note(error.clone()))
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
                draft.clear_copy();
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
                draft.clear_copy();
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
        let Some(id) = self.selected.as_deref() else {
            return;
        };
        if self.review_draft.pending_capture.is_some() {
            return;
        }
        let request = self
            .project
            .review_request(id, &self.review_draft.note, self.before);
        self.review_draft.clear_copy();
        if !self.review_draft.include_screenshot {
            cx.write_to_clipboard(ClipboardItem::new_string(request));
            self.review_draft.copied = true;
            cx.notify();
            return;
        }

        let capture = self.viewport.capture(&self.viewport_view());
        let capture = match capture {
            Ok(capture) => capture,
            Err(error) => {
                self.review_draft.copy_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let token = Rc::new(());
        self.review_draft.pending_capture = Some(token.clone());
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let task = cx.background_executor().spawn(async move {
                let png = capture()?;
                let mut file = tempfile::Builder::new()
                    .prefix("steelgum-review-")
                    .suffix(".png")
                    .tempfile()
                    .context("Could not create a viewport screenshot file")?;
                file.write_all(&png)
                    .context("Could not save the viewport screenshot")?;
                Ok::<_, anyhow::Error>(file)
            });
            let result = task.await;
            let _ = entity.update(cx, |view, cx| {
                let draft = &mut view.review_draft;
                if !draft
                    .pending_capture
                    .as_ref()
                    .is_some_and(|pending| Rc::ptr_eq(pending, &token))
                {
                    return;
                }
                draft.pending_capture = None;
                match result.and_then(|file| {
                    file.keep()
                        .context("Could not keep the viewport screenshot")
                }) {
                    Ok((_, path)) => {
                        let mut request = request;
                        let _ = writeln!(
                            request,
                            "\nViewport screenshot (attach or open this PNG): {}",
                            path.display()
                        );
                        cx.write_to_clipboard(ClipboardItem::new_string(request));
                        draft.copied = true;
                    }
                    Err(error) => draft.copy_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl ReviewDraft {
    fn clear_copy(&mut self) {
        self.copied = false;
        self.pending_capture = None;
        self.copy_error = None;
    }

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
        self.clear_copy();
    }
}

impl Project {
    pub(super) fn review_request(&self, id: &str, note: &str, before: bool) -> String {
        let old = self.review.before().object(id);
        let new = self.review.after().object(id);
        let object = if before { old.or(new) } else { new.or(old) };
        let title = object.map_or_else(|| label(id), object_label);
        let mut request = format!("Review {title} in Steelgum.\n");
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
        match (old, new) {
            (Some(old), Some(new)) => {
                request.push_str("Object diff (before → after)\n");
                let start = request.len();
                write_diff(&mut request, "name", &old.name, &new.name);
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
    if let Some(name) = &object.name {
        let _ = writeln!(output, "{prefix}name: {name:?}");
    }
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
