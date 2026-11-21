use crate::ui::tokens::{BORDER, SPACE_2, SPACE_3};
use crate::ui::{action, danger_button, note};
use std::path::PathBuf;

use gpui::{AnyElement, Context, div, prelude::*};
use steelgum::{
    project::{AuthoredScreen, delete_screen},
    scene_edit::delete_object,
};

use super::assets::remove_asset;

use super::{ReviewWindow, Tool, object_rename::object_label};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum DeleteTarget {
    Object(String, String),
    Screen(AuthoredScreen),
    Asset(PathBuf, PathBuf),
}

impl DeleteTarget {
    fn name(&self) -> String {
        match self {
            Self::Object(_, name) => name.clone(),
            Self::Screen(AuthoredScreen::Pause(_)) => "Pause Menu".into(),
            Self::Screen(AuthoredScreen::Custom(screen)) => screen.title.clone(),
            Self::Asset(_, path) => path.display().to_string(),
        }
    }
}

impl ReviewWindow {
    fn delete_target(&self) -> Option<DeleteTarget> {
        self.project.source.as_ref()?;
        match self.tool? {
            Tool::Objects | Tool::Changes if !self.before => {
                let id = self.selected.as_deref()?;
                let object = self.project.review.after().object(id)?;
                Some(DeleteTarget::Object(id.to_owned(), object_label(object)))
            }
            Tool::Ui => {
                let manifest = self.project.manifest.as_ref()?;
                let screen = if let Some(screen) = self.custom_screen() {
                    AuthoredScreen::Custom(screen.clone())
                } else {
                    AuthoredScreen::Pause(manifest.pause_menu.clone()?)
                };
                Some(DeleteTarget::Screen(screen))
            }
            Tool::Assets => {
                let (directory, path) = self.selected_asset()?;
                Some(DeleteTarget::Asset(directory.to_owned(), path.to_owned()))
            }
            _ => None,
        }
    }

    pub(super) fn deletion_footer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.delete_target()?;
        let confirming = self.pending_deletion.as_ref() == Some(&target);
        let danger = || danger_button("delete-definition", "Delete");
        let footer = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(SPACE_2)
            .p(SPACE_3)
            .border_t_1()
            .border_color(BORDER);
        Some(
            if confirming {
                footer
                    .child(note(format!("Permanently delete {}?", target.name())))
                    .child(
                        div()
                            .flex()
                            .gap(SPACE_2)
                            .child(
                                danger().on_click(
                                    cx.listener(|view, _, _, cx| view.confirm_delete(cx)),
                                ),
                            )
                            .child(action("cancel-deletion", "Cancel", false).on_click(
                                cx.listener(|view, _, _, cx| {
                                    view.pending_deletion = None;
                                    cx.notify();
                                }),
                            )),
                    )
            } else {
                footer.child(danger().on_click(cx.listener(move |view, _, _, cx| {
                    if !view.definitions.writing
                        && !view.pause_menu.saving()
                        && !view.importing_asset()
                    {
                        view.pending_deletion = Some(target.clone());
                        cx.notify();
                    }
                })))
            }
            .into_any_element(),
        )
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if self.loading
            || self.definitions.writing
            || self.pause_menu.saving()
            || self.importing_asset()
        {
            return;
        }
        let Some(target) = self.pending_deletion.take() else {
            return;
        };
        if self.delete_target().as_ref() != Some(&target) {
            cx.notify();
            return;
        }
        let Some((root, scene)) = self.project.source.clone() else {
            return;
        };
        let expected = self.project.review.working_text().map(str::to_owned);
        let request = target.clone();
        self.definitions.writing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match request {
                        DeleteTarget::Object(id, _) => {
                            let expected =
                                expected.ok_or_else(|| anyhow::anyhow!("No working scene"))?;
                            delete_object(&root, &scene, &expected, &id)
                                .map_err(anyhow::Error::from)
                        }
                        DeleteTarget::Screen(screen) => {
                            delete_screen(&root, &screen).map_err(anyhow::Error::from)
                        }
                        DeleteTarget::Asset(directory, path) => {
                            remove_asset(&root, &directory, &path)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.definitions.writing = false;
                match result {
                    Ok(()) => {
                        if view.delete_target().as_ref() == Some(&target) {
                            view.close_properties();
                        }
                        match target {
                            DeleteTarget::Object(id, _) => {
                                if view.selected.as_deref() == Some(&id) {
                                    view.selected = None;
                                    view.rename = None;
                                    view.focus = false;
                                    view.review_draft = Default::default();
                                }
                                view.refresh(cx);
                            }
                            DeleteTarget::Screen(screen) => {
                                if let Some(manifest) = view.project.manifest.as_mut() {
                                    match screen {
                                        AuthoredScreen::Pause(_) => manifest.pause_menu = None,
                                        AuthoredScreen::Custom(screen) => {
                                            manifest.screens.retain(|item| item.id != screen.id)
                                        }
                                    }
                                }
                                if view.tool != Some(Tool::Ui) {
                                    view.pause_menu.clear();
                                }
                                view.refresh(cx);
                            }
                            DeleteTarget::Asset(_, _) => view.refresh_assets(cx),
                        }
                    }
                    Err(error) => view.status = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
