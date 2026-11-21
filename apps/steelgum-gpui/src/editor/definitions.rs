use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px, rgb, rgba};
use steelgum::{
    project::{MANIFEST_FILE, ScreenDefinition, create_screen, resolve_project_path},
    scene_edit::add_box,
};

use super::workspace::{action, note};
use super::{ReviewWindow, Tool};

const PAUSE_MENU: &str = "pause-menu";

#[derive(Default)]
pub(super) struct DefinitionTools {
    screen: Option<String>,
    created_object: Option<String>,
    pub(super) writing: bool,
    pub(super) ui_expanded: bool,
}

impl ReviewWindow {
    fn screens(&self) -> &[ScreenDefinition] {
        self.project
            .manifest
            .as_ref()
            .map_or(&[], |manifest| &manifest.screens)
    }

    pub(super) fn custom_screen(&self) -> Option<&ScreenDefinition> {
        let manifest = self.project.manifest.as_ref()?;
        self.definitions
            .screen
            .as_deref()
            .and_then(|id| self.screens().iter().find(|screen| screen.id == id))
            .or_else(|| {
                manifest
                    .pause_menu
                    .is_none()
                    .then(|| self.screens().first())
                    .flatten()
            })
    }

    pub(super) fn toggle_ui_section(&mut self) {
        self.definitions.ui_expanded = !self.definitions.ui_expanded;
        if self.tool.is_some() {
            self.closing_pane = self.tool.take();
            self.hovered_build_profile = None;
        }
    }

    pub(super) fn select_screen(&mut self, id: String) {
        self.definitions.screen = Some(id);
        self.show_tool(Tool::Ui);
    }

    pub(super) fn screen_title(&self) -> &str {
        self.custom_screen()
            .map_or("Pause Menu", |screen| screen.title.as_str())
    }

    fn screen_choices(&self) -> impl Iterator<Item = (&str, &str)> {
        self.project
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.pause_menu.as_ref())
            .map(|_| (PAUSE_MENU, "Pause Menu"))
            .into_iter()
            .chain(
                self.screens()
                    .iter()
                    .map(|screen| (screen.id.as_str(), screen.title.as_str())),
            )
    }

    pub(super) fn screen_matches(&self, query: &str) -> bool {
        self.screen_choices()
            .any(|(_, title)| title.to_lowercase().contains(query))
    }

    pub(super) fn screen_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .custom_screen()
            .map_or(PAUSE_MENU, |screen| screen.id.as_str());
        let active = self.menu_open;
        let list = div()
            .id("screen-list")
            .max_h(px(120.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .children(self.screen_choices().map(|(id, title)| {
                let id = id.to_owned();
                div().id(SharedString::from(format!("screen-{id}"))).child(
                    action(
                        "screen",
                        title.to_owned(),
                        self.tool == Some(Tool::Ui) && id == current,
                    )
                    .w_full()
                    .justify_start()
                    .overflow_hidden()
                    .tab_stop(active)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.select_screen(id.clone());
                        cx.notify();
                    })),
                )
            }));
        div()
            .ml_3()
            .flex()
            .flex_col()
            .gap_1()
            .child(list)
            .when(
                self.project.source.is_some() && self.project.manifest.is_some(),
                |nav| {
                    nav.child(
                        action("new-screen", "+ New Screen", false)
                            .justify_start()
                            .tab_stop(active)
                            .on_click(cx.listener(|view, _, _, cx| view.new_screen(cx))),
                    )
                },
            )
            .when(self.project.manifest.is_none(), |nav| {
                nav.child(note("No project manifest"))
            })
            .into_any_element()
    }

    pub(super) fn screen_properties(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let active = self.tool == Some(Tool::Ui);
        let screen = self.custom_screen();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(self.project.source.is_some(), |pane| {
                pane.child(
                    action("open-screen-definition", "Open Definition", false)
                        .tab_stop(active)
                        .on_click(cx.listener(|view, _, _, cx| view.open_definition(true, cx))),
                )
            })
            .when(screen.is_none(), |pane| {
                pane.child(self.pause_menu_panel(window, cx))
            })
            .when_some(screen, |pane, screen| {
                pane.child(note(format!("Screen ID · {}", screen.id)))
                    .child(note(format!("Button · {}", screen.button_label)))
            })
            .into_any_element()
    }

    pub(super) fn screen_preview(&self) -> Option<AnyElement> {
        if self.tool != Some(Tool::Ui) {
            return None;
        }
        let Some(screen) = self.custom_screen() else {
            return self.pause_menu_preview();
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
                        .gap_4()
                        .p_5()
                        .child(div().text_xl().text_center().child(screen.title.clone()))
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .bg(rgb(0x2a3b46))
                                .py_2()
                                .child(screen.button_label.clone()),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn new_screen(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.definitions.writing || self.pause_menu.saving() {
            return;
        }
        let Some((root, _)) = self.project.source.clone() else {
            return;
        };
        self.definitions.writing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { create_screen(&root) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.definitions.writing = false;
                match result {
                    Ok(screen) => {
                        view.select_screen(screen.id.clone());
                        if let Some(manifest) = view.project.manifest.as_mut()
                            && !manifest.screens.iter().any(|item| item.id == screen.id)
                        {
                            manifest.screens.push(screen);
                        }
                        view.refresh(cx);
                    }
                    Err(error) => view.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn object_definition_tools(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap_1()
            .flex_wrap()
            .px_3()
            .pt_2()
            .when(self.project.source.is_some(), |row| {
                row.when(!self.before, |row| {
                    row.child(
                        action("add-box", "Add Box", false)
                            .tab_stop(self.tool == Some(Tool::Objects))
                            .on_click(cx.listener(|view, _, _, cx| view.add_scene_box(cx))),
                    )
                })
                .child(
                    action("open-scene-definition", "Open Definition", false)
                        .tab_stop(self.tool == Some(Tool::Objects))
                        .on_click(cx.listener(|view, _, _, cx| view.open_definition(false, cx))),
                )
            })
            .into_any_element()
    }

    fn add_scene_box(&mut self, cx: &mut Context<Self>) {
        if self.before || self.loading || self.definitions.writing || self.pause_menu.saving() {
            return;
        }
        let Some(((root, scene), expected)) = self
            .project
            .source
            .clone()
            .zip(self.project.review.working_text())
        else {
            return;
        };
        let expected = expected.to_owned();
        let target = self.viewport.target(&self.viewport_view());
        self.definitions.writing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { add_box(&root, &scene, &expected, target) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.definitions.writing = false;
                match result {
                    Ok(id) => {
                        view.definitions.created_object = Some(id);
                        view.refresh(cx);
                    }
                    Err(error) => view.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn select_created_object(&mut self, cx: &mut Context<Self>) {
        if self
            .definitions
            .created_object
            .as_deref()
            .is_some_and(|id| self.project.review.after().object(id).is_some())
            && let Some(id) = self.definitions.created_object.take()
            && self.tool == Some(Tool::Objects)
        {
            self.before = false;
            self.select_object(id, cx);
        }
    }

    fn open_definition(&mut self, screen: bool, cx: &mut Context<Self>) {
        let Some((root, scene)) = self.project.source.as_ref() else {
            return;
        };
        let path = if screen {
            std::path::Path::new(MANIFEST_FILE)
        } else {
            scene.as_path()
        };
        let result = resolve_project_path(root, path)
            .map_err(anyhow::Error::from)
            .and_then(|path| {
                let editor = ["VISUAL", "EDITOR"].into_iter().find_map(|variable| {
                    std::env::var(variable)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                });
                match editor {
                    Some(editor) => open::with_detached(&path, editor),
                    None => open::that_detached(&path),
                }
                .map_err(anyhow::Error::from)
            });
        if let Err(error) = result {
            self.status = Some(format!("Could not open {}: {error}", path.display()));
            cx.notify();
        }
    }
}
