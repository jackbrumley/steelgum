use gpui::{
    AnyElement, Context, FocusHandle, KeyDownEvent, Window, div, prelude::*, px, rgb, rgba,
};
use steelgum::{pause_menu::PauseMenuConfig, project::save_pause_menu};

use super::workspace::{ACCENT, BORDER, SHELL, action, note};
use super::{ReviewWindow, Tool};

const FIELDS: [(&str, bool); 7] = [
    ("Title", false),
    ("Sensitivity label", false),
    ("Resume button", false),
    ("Default (%)", true),
    ("Minimum (%)", true),
    ("Maximum (%)", true),
    ("Step (%)", true),
];

pub(super) struct MenuEditor {
    focus: [FocusHandle; FIELDS.len()],
    draft: Option<MenuDraft>,
}

struct MenuDraft {
    original: PauseMenuConfig,
    fields: [String; FIELDS.len()],
    cursors: [usize; FIELDS.len()],
    replace: Option<usize>,
    saving: bool,
    issue: Option<String>,
}

impl MenuEditor {
    pub(super) fn new(cx: &mut Context<ReviewWindow>) -> Self {
        Self {
            focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            draft: None,
        }
    }

    pub(super) fn clear(&mut self) {
        self.draft = None;
    }

    pub(super) fn saving(&self) -> bool {
        self.draft.as_ref().is_some_and(|draft| draft.saving)
    }
}

impl MenuDraft {
    fn new(original: PauseMenuConfig) -> Self {
        let fields = [
            original.title.clone(),
            original.sensitivity_label.clone(),
            original.resume_label.clone(),
            original.sensitivity_default.to_string(),
            original.sensitivity_min.to_string(),
            original.sensitivity_max.to_string(),
            original.sensitivity_step.to_string(),
        ];
        let cursors = std::array::from_fn(|index| fields[index].len());
        Self {
            original,
            fields,
            cursors,
            replace: Some(0),
            saving: false,
            issue: None,
        }
    }

    fn config(&self) -> Result<PauseMenuConfig, &'static str> {
        let percent = |index: usize| {
            self.fields[index]
                .parse::<u8>()
                .map_err(|_| "Enter whole percentages from 1 to 255.")
        };
        let config = PauseMenuConfig {
            title: self.fields[0].clone(),
            sensitivity_label: self.fields[1].clone(),
            resume_label: self.fields[2].clone(),
            sensitivity_default: percent(3)?,
            sensitivity_min: percent(4)?,
            sensitivity_max: percent(5)?,
            sensitivity_step: percent(6)?,
        };
        config.validate()?;
        Ok(config)
    }
}

impl ReviewWindow {
    fn saved_pause_menu(&self) -> Option<&PauseMenuConfig> {
        self.project.manifest.as_ref()?.pause_menu.as_ref()
    }

    fn begin_menu_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config) = self.saved_pause_menu().cloned() {
            self.pause_menu.draft = Some(MenuDraft::new(config));
            self.pause_menu.focus[0].focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn pause_menu_panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(config) = self.saved_pause_menu() else {
            return note("This project has no pause menu declared in steelgum.json.")
                .into_any_element();
        };
        let saved = MenuDraft::new(config.clone());
        let editing = self.pause_menu.draft.as_ref();
        let draft = editing.unwrap_or(&saved);
        let active = self.tool == Some(Tool::Ui);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .children(FIELDS.into_iter().enumerate().map(|(index, (name, _))| {
                let focus = &self.pause_menu.focus[index];
                let value = &draft.fields[index];
                let edited = editing.is_some() && !draft.saving;
                let focused = edited && focus.is_focused(window);
                let field = div()
                    .id(name)
                    .border_1()
                    .rounded_md()
                    .border_color(rgb(BORDER))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .min_h(px(28.))
                    .bg(rgb(SHELL))
                    .truncate()
                    .child(if focused && draft.replace != Some(index) {
                        let (left, right) = value.split_at(draft.cursors[index]);
                        format!("{left}▏{right}")
                    } else {
                        value.clone()
                    })
                    .when(edited, |field| {
                        field
                            .track_focus(focus)
                            .tab_index(0)
                            .tab_stop(active)
                            .role(gpui::accesskit::Role::TextInput)
                            .aria_label(name)
                            .aria_value(value.clone())
                            .focus(|style| style.border_color(rgb(ACCENT)))
                            .when(focused && draft.replace == Some(index), |field| {
                                field.bg(rgb(ACCENT)).text_color(rgb(SHELL))
                            })
                    });
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(note(name))
                    .child(field)
            }))
            .when(editing.is_none(), |pane| {
                pane.child(
                    action("edit-pause-menu", "Edit settings", false)
                        .tab_stop(active)
                        .on_click(
                            cx.listener(|view, _, window, cx| view.begin_menu_edit(window, cx)),
                        ),
                )
            })
            .when_some(editing, |pane, draft| {
                if draft.saving {
                    pane.child(note("Saving pause menu…"))
                } else {
                    pane.child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                action("save-pause-menu", "Save", false)
                                    .tab_stop(active)
                                    .on_click(cx.listener(|view, _, _, cx| view.save_menu(cx))),
                            )
                            .child(
                                action("discard-pause-menu", "Discard", false)
                                    .tab_stop(active)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.pause_menu.clear();
                                        view.workspace_focus.focus(window, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when_some(draft.issue.as_ref(), |pane, issue| {
                        pane.child(note(issue.clone()))
                    })
                }
            })
            .into_any_element()
    }

    pub(super) fn pause_menu_preview(&self) -> Option<AnyElement> {
        if self.tool != Some(Tool::Ui) {
            return None;
        }
        let saved = self.saved_pause_menu()?;
        let pending = self
            .pause_menu
            .draft
            .as_ref()
            .and_then(|draft| draft.config().ok());
        let menu = pending.as_ref().unwrap_or(saved);
        let button = |label: &str| {
            div()
                .bg(rgb(0x2a3b46))
                .px_4()
                .py_2()
                .text_center()
                .child(label.to_owned())
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .block_mouse_except_scroll()
                .bg(rgba(0x05080aa1))
                .flex()
                .items_center()
                .justify_center()
                .p_3()
                .overflow_hidden()
                .child(
                    div()
                        .w_full()
                        .max_w(px(320.))
                        .bg(rgb(0x16212a))
                        .text_color(rgb(0xecf2f6))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p_5()
                        .child(div().text_xl().text_center().child(menu.title.clone()))
                        .child(
                            div()
                                .text_sm()
                                .text_center()
                                .child(menu.sensitivity_label.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_4()
                                .child(button("-"))
                                .child(format!("{}%", menu.sensitivity_default))
                                .child(button("+")),
                        )
                        .child(button(&menu.resume_label).w_full()),
                )
                .into_any_element(),
        )
    }

    pub(super) fn handle_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tool != Some(Tool::Ui) || self.pause_menu.saving() || self.custom_screen().is_some()
        {
            return false;
        }
        let Some(index) = self
            .pause_menu
            .focus
            .iter()
            .position(|focus| focus.is_focused(window))
        else {
            return false;
        };
        let Some(draft) = self.pause_menu.draft.as_mut() else {
            return false;
        };
        let stroke = &event.keystroke;
        let command = stroke.modifiers.control || stroke.modifiers.platform;
        let key = stroke.key.as_str();
        let input = match key {
            "escape" | "enter" => {
                self.workspace_focus.focus(window, cx);
                return true;
            }
            "a" if command => {
                draft.replace = Some(index);
                cx.notify();
                return true;
            }
            "c" if command => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(draft.fields[index].clone()));
                return true;
            }
            "v" if command => cx.read_from_clipboard().and_then(|item| item.text()),
            "left" | "right" | "home" | "end" => {
                let length = draft.fields[index].len();
                let caret = &mut draft.cursors[index];
                *caret = match key {
                    "left" if draft.replace == Some(index) => 0,
                    "left" => caret.saturating_sub(1),
                    "right" => (*caret + 1).min(length),
                    "home" => 0,
                    _ => length,
                };
                draft.replace = None;
                cx.notify();
                return true;
            }
            "backspace" | "delete" => Some(String::new()),
            "tab" => return false,
            _ if !command && !stroke.modifiers.alt => stroke.key_char.clone(),
            _ => return false,
        };
        let Some(input) = input else {
            return false;
        };
        let numeric = FIELDS[index].1;
        if !input.bytes().all(|byte| {
            if numeric {
                byte.is_ascii_digit()
            } else {
                matches!(byte, b' '..=b'~')
            }
        }) {
            return true;
        }
        let replace = draft.replace == Some(index);
        let value = &mut draft.fields[index];
        let caret = &mut draft.cursors[index];
        let remaining = if replace { 0 } else { value.len() };
        let max_length = if numeric { 3 } else { 24 };
        if remaining + input.len() <= max_length {
            if replace {
                value.clear();
                *caret = 0;
            }
            if input.is_empty() {
                if key == "backspace" && *caret > 0 {
                    *caret -= 1;
                    value.remove(*caret);
                } else if key == "delete" && *caret < value.len() {
                    value.remove(*caret);
                }
            } else {
                value.insert_str(*caret, &input);
                *caret += input.len();
            }
            draft.replace = None;
            draft.issue = None;
            cx.notify();
        }
        true
    }

    fn save_menu(&mut self, cx: &mut Context<Self>) {
        let Some(((root, _), draft)) = self
            .project
            .source
            .as_ref()
            .zip(self.pause_menu.draft.as_mut())
        else {
            return;
        };
        if draft.saving || self.loading {
            return;
        }
        let config = match draft.config() {
            Ok(config) => config,
            Err(error) => {
                draft.issue = Some(error.into());
                cx.notify();
                return;
            }
        };
        let root = root.clone();
        let expected = draft.original.clone();
        let saved = config.clone();
        draft.saving = true;
        draft.issue = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { save_pause_menu(&root, &expected, &config) })
                .await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(()) => {
                        if let Some(menu) = view
                            .project
                            .manifest
                            .as_mut()
                            .and_then(|manifest| manifest.pause_menu.as_mut())
                            && view
                                .pause_menu
                                .draft
                                .as_ref()
                                .is_some_and(|draft| draft.original == *menu)
                        {
                            *menu = saved;
                        }
                        view.pause_menu.clear();
                        view.refresh(cx);
                    }
                    Err(error) => {
                        if let Some(draft) = view.pause_menu.draft.as_mut() {
                            draft.saving = false;
                            draft.issue = Some(error.to_string());
                        }
                        view.refresh_requested = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use anyhow::{Result, ensure};
    use gpui::{AppContext, HeadlessAppContext, Keystroke, current_headless_renderer, size};
    use gpui_wgpu::CosmicTextSystem;
    use serde_json::json;
    use steelgum::project::{MANIFEST_FILE, ProjectManifest};

    use super::*;
    use crate::editor::Project;

    #[test]
    #[ignore = "requires an available Vulkan adapter"]
    fn previews_and_saves_pause_menu_edits() -> Result<()> {
        let project_root = tempfile::tempdir()?;
        let root = project_root.path();
        let scene = PathBuf::from("room.json");
        std::fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "name": "Pause menu test",
                "scene": scene,
                "pause_menu": PauseMenuConfig::default(),
            }))?,
        )?;
        let mut project = Project::demo(crate::demo()?);
        project.manifest = ProjectManifest::discover(root)?;
        project.source = Some((root.to_owned(), scene));
        let mut app = HeadlessAppContext::with_platform(
            Arc::new(CosmicTextSystem::new("sans-serif")),
            Arc::new(()),
            current_headless_renderer,
        );
        let window = app.open_window(size(px(1024.), px(700.)), |window, cx| {
            cx.new(|cx| {
                let mut view = ReviewWindow::new(project, cx);
                view.toggle_ui_section();
                assert!(view.definitions.ui_expanded && view.tool.is_none());
                assert!(view.screen_preview().is_none());
                view.select_screen("pause-menu".into());
                view.begin_menu_edit(window, cx);
                view
            })
        })?;
        let before = app.capture_screenshot(window.into())?;
        app.update_window(window.into(), |_, window, cx| {
            for key in ["h", "o", "l", "d"] {
                assert!(window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx));
            }
        })?;
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert_eq!(view.pause_menu.draft.as_ref().unwrap().fields[0], "hold");
            })
        })?;
        app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
        let edited = app.capture_screenshot(window.into())?;
        let snapshot_dir = std::env::var_os("STEELGUM_SNAPSHOT_DIR").map(PathBuf::from);
        if let Some(directory) = &snapshot_dir {
            std::fs::create_dir_all(directory)?;
            before.save(directory.join("steelgum-pause-menu-original.png"))?;
            edited.save(directory.join("steelgum-pause-menu.png"))?;
        }
        let (width, height) = edited.dimensions();
        assert!((height * 3 / 8..height / 2).any(|y| {
            (width * 2 / 5..width * 11 / 20)
                .any(|x| before.get_pixel(x, y) != edited.get_pixel(x, y))
        }));
        app.update(|cx| window.update(cx, |view, _, cx| view.save_menu(cx)))?;
        let start = Instant::now();
        loop {
            app.run_until_parked();
            if !app.update(|cx| window.update(cx, |view, _, _| view.pause_menu.saving()))? {
                break;
            }
            ensure!(
                start.elapsed() < Duration::from_secs(5),
                "menu save timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let saved = ProjectManifest::discover(root)?
            .unwrap()
            .pause_menu
            .unwrap();
        assert_eq!(saved.title, "hold");
        app.update(|cx| window.update(cx, |view, _, cx| view.new_screen(cx)))?;
        let start = Instant::now();
        loop {
            app.run_until_parked();
            if !app.update(|cx| window.update(cx, |view, _, _| view.definitions.writing))? {
                break;
            }
            ensure!(
                start.elapsed() < Duration::from_secs(5),
                "screen creation timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let manifest = ProjectManifest::discover(root)?.unwrap();
        assert_eq!(manifest.screens[0].id, "main-menu");
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                assert_eq!(view.custom_screen(), manifest.screens.first());
            })
        })?;
        app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
        let main_menu = app.capture_screenshot(window.into())?;
        assert_ne!(main_menu, edited);
        if let Some(directory) = &snapshot_dir {
            main_menu.save(directory.join("steelgum-main-menu.png"))?;
        }
        app.update(|cx| {
            window.update(cx, |view, _, _| {
                view.show_tool(Tool::Objects);
                assert!(!view.definitions.ui_expanded && view.screen_preview().is_none());
            })
        })?;
        Ok(())
    }
}
