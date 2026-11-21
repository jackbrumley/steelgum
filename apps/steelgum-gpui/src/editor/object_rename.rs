use std::rc::Rc;

use gpui::{AnyElement, ClipboardItem, Context, KeyDownEvent, Window, div, prelude::*, rgb};
use steelgum::{MAX_DISPLAY_NAME_LENGTH, SceneObject, scene_edit::rename_object};

use super::workspace::{ACCENT, BORDER, MUTED, SHELL, TEXT, action, note};
use super::{ReviewWindow, Tool, label};

pub(super) struct RenameDraft {
    object_id: String,
    expected: String,
    original_name: String,
    name: String,
    cursor: usize,
    select_all: bool,
    saving: bool,
    issue: Option<String>,
    token: Rc<()>,
}

impl ReviewWindow {
    pub(super) fn rename_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selected = self.selected.as_deref()?;
        if self.before
            || self.project.review.working_text().is_none()
            || self.project.review.after().object(selected).is_none()
        {
            return None;
        }
        let control = if let Some(draft) = &self.rename {
            div()
                .mt_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(if draft.saving {
                    note("Saving name…")
                } else {
                    note("Enter to save · Esc to cancel")
                })
                .when_some(draft.issue.as_ref(), |row, issue| {
                    row.child(note(issue.clone()))
                })
                .into_any_element()
        } else {
            action("rename-object", "Rename (F2)", false)
                .mt_2()
                .on_click(cx.listener(|view, _, window, cx| {
                    view.begin_rename(window, cx);
                }))
                .into_any_element()
        };
        Some(control)
    }

    pub(super) fn rename_field(&self, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self
            .rename
            .as_ref()
            .filter(|draft| draft.object_id == id && !self.before)?;
        let cursor = draft.cursor;
        let (prefix, suffix) = draft.name.split_at(cursor);
        let value = if draft.select_all {
            draft.name.clone()
        } else {
            format!("{prefix}▏{suffix}")
        };
        Some(
            div()
                .id("object-name")
                .track_focus(&self.rename_focus)
                .tab_index(0)
                .role(gpui::accesskit::Role::TextInput)
                .aria_label("Display name")
                .aria_value(draft.name.clone())
                .border_1()
                .rounded_md()
                .border_color(rgb(BORDER))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .px_2()
                .py_1()
                .text_sm()
                .text_color(rgb(if draft.saving { MUTED } else { TEXT }))
                .bg(rgb(if draft.select_all { ACCENT } else { SHELL }))
                .when(draft.select_all, |field| field.text_color(rgb(SHELL)))
                .on_click(cx.listener(|view, _, window, cx| {
                    view.rename_focus.focus(window, cx);
                    cx.stop_propagation();
                }))
                .child(value)
                .into_any_element(),
        )
    }

    fn begin_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.before || self.loading || self.project.source.is_none() {
            return false;
        }
        if self.rename.is_some() {
            self.rename_focus.focus(window, cx);
            return true;
        }
        let Some((object, expected)) = self
            .selected
            .as_deref()
            .and_then(|id| self.project.review.after().object(id))
            .zip(self.project.review.working_text())
        else {
            return false;
        };
        let name = object_label(object);
        self.rename = Some(RenameDraft {
            object_id: object.id.clone(),
            expected: expected.to_owned(),
            original_name: name.clone(),
            cursor: name.len(),
            name,
            select_all: true,
            saving: false,
            issue: None,
            token: Rc::new(()),
        });
        self.reveal_selected_object();
        self.rename_focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn handle_object_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.expanded_tool != Some(Tool::Objects) {
            return false;
        }
        let stroke = &event.keystroke;
        if self.rename_focus.is_focused(window) && self.rename.is_some() {
            return self.edit_rename(event, window, cx);
        }
        if self.search_focus.is_focused(window)
            || self.review_focus.is_focused(window)
            || stroke.modifiers.control
            || stroke.modifiers.platform
            || stroke.modifiers.alt
        {
            return false;
        }
        if stroke.key == "f2" {
            return self.tool == Some(Tool::Objects) && self.begin_rename(window, cx);
        }
        if !self.objects_focus.contains_focused(window, cx) {
            return false;
        }
        let objects: Vec<_> = self.visible_objects().collect();
        if objects.is_empty() {
            return false;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|id| objects.iter().position(|object| &object.id == id));
        let next = match stroke.key.as_str() {
            "up" => current.map_or(objects.len() - 1, |index| index.saturating_sub(1)),
            "down" => current.map_or(0, |index| (index + 1).min(objects.len() - 1)),
            "home" => 0,
            "end" => objects.len() - 1,
            _ => return false,
        };
        let id = objects[next].id.clone();
        self.show_tool(Tool::Objects);
        self.select_object(id, cx);
        self.objects_focus.focus(window, cx);
        true
    }

    fn edit_rename(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft) = self.rename.as_mut() else {
            return false;
        };
        if draft.saving {
            return false;
        }
        let stroke = &event.keystroke;
        let command = stroke.modifiers.control || stroke.modifiers.platform;
        match stroke.key.as_str() {
            "escape" => {
                self.rename = None;
                self.objects_focus.focus(window, cx);
            }
            "enter" => {
                self.save_object_name(window, cx);
            }
            "a" if command => draft.select_all = true,
            "c" | "x" if command && draft.select_all => {
                cx.write_to_clipboard(ClipboardItem::new_string(draft.name.clone()));
                if stroke.key == "x" {
                    draft.insert("");
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    draft.insert(&text);
                }
            }
            "left" => draft.move_cursor(if draft.select_all {
                0
            } else {
                draft.previous()
            }),
            "right" => draft.move_cursor(if draft.select_all {
                draft.name.len()
            } else {
                draft.next()
            }),
            "home" => draft.move_cursor(0),
            "end" => draft.move_cursor(draft.name.len()),
            "backspace" => draft.delete(draft.previous(), draft.cursor),
            "delete" => draft.delete(draft.cursor, draft.next()),
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

    fn save_object_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.rename.as_mut() else {
            return;
        };
        let Some(source) = self.project.source.clone() else {
            return;
        };
        if draft.name.trim() == draft.original_name {
            self.rename = None;
            self.objects_focus.focus(window, cx);
            cx.notify();
            return;
        }
        let disk_source = source.clone();
        let name = draft.name.clone();
        let object_id = draft.object_id.clone();
        let expected = draft.expected.clone();
        let token = draft.token.clone();
        draft.saving = true;
        draft.issue = None;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    rename_object(&disk_source.0, &disk_source.1, &expected, &object_id, &name)
                })
                .await;
            let _ = cx.update(|window, cx| {
                let _ = view.update(cx, |view, cx| {
                    if view.project.source.as_ref() != Some(&source) {
                        return;
                    }
                    let still_editing = view
                        .rename
                        .as_ref()
                        .is_some_and(|draft| Rc::ptr_eq(&token, &draft.token));
                    match result {
                        Ok(()) => {
                            if still_editing {
                                view.rename = None;
                                view.objects_focus.focus(window, cx);
                            }
                            view.refresh(cx);
                        }
                        Err(error) => {
                            if let Some(draft) = view.rename.as_mut().filter(|_| still_editing) {
                                draft.saving = false;
                                draft.issue = Some(error.to_string());
                            } else {
                                view.status = Some(error.to_string());
                            }
                            cx.notify();
                        }
                    }
                });
            });
        })
        .detach();
    }
}

impl RenameDraft {
    fn previous(&self) -> usize {
        self.name[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.name[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |ch| self.cursor + ch.len_utf8())
    }

    fn move_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
        self.select_all = false;
    }

    fn insert(&mut self, value: &str) {
        if std::mem::take(&mut self.select_all) {
            self.name.clear();
            self.cursor = 0;
        }
        let value: String = value
            .chars()
            .filter(|ch| !ch.is_control())
            .take(MAX_DISPLAY_NAME_LENGTH.saturating_sub(self.name.chars().count()))
            .collect();
        self.name.insert_str(self.cursor, &value);
        self.cursor += value.len();
        self.issue = None;
    }

    fn delete(&mut self, start: usize, end: usize) {
        if self.select_all {
            self.insert("");
        } else {
            self.name.replace_range(start..end, "");
            self.cursor = start;
            self.issue = None;
        }
    }
}

pub(super) fn object_label(object: &SceneObject) -> String {
    object.name.clone().unwrap_or_else(|| label(&object.id))
}
