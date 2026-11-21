use super::{ReviewWindow, Tool, matches_entry, slide};
use crate::editor::object_rename::object_label;
use crate::ui::tokens::{ACCENT, BORDER, MUTED, PANEL, SPACE_1, SPACE_2, SPACE_3, TEXT};
use crate::ui::{Icon, note, search_field, sidebar_action};
use gpui::{AnyElement, Context, Window, div, prelude::*, px};

const MENU_WIDTH: f32 = 182.;
impl ReviewWindow {
    pub(super) fn navigation(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let section_height = (window.viewport_size().height - px(370.)).max(px(96.));
        let searching = !self.search.trim().is_empty();
        let tools: Vec<_> = self.matching_tools().collect();
        let content = div()
            .w(px(MENU_WIDTH))
            .h_full()
            .flex_none()
            .bg(PANEL)
            .border_r_1()
            .border_color(BORDER)
            .p(SPACE_3)
            .flex()
            .flex_col()
            .gap(SPACE_2)
            .child(
                search_field(
                    &self.search,
                    &self.search_focus,
                    window,
                    self.menu_open,
                    cx.listener(|view, _, window, cx| {
                        view.search.clear();
                        view.focus_search(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_click(cx.listener(|view, _, window, cx| view.focus_search(window, cx))),
            )
            .child(
                div()
                    .id("tool-results")
                    .flex()
                    .flex_col()
                    .gap(SPACE_2)
                    .min_h_0()
                    .flex_1()
                    .when(searching, |results| results.overflow_y_scroll())
                    .when(tools.is_empty(), |menu| menu.child(note("No results")))
                    .children(tools.into_iter().map(|tool| {
                        let count = self.project.review.changes().len();
                        let active = self.expanded_tool == Some(tool);
                        let expanded = active || searching;
                        let chevron = if expanded {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        };
                        let label = chevron.label(tool.label(), if active { ACCENT } else { TEXT });
                        let button = sidebar_action(tool.label(), label, active)
                            .w_full()
                            .justify_start()
                            .gap(SPACE_2)
                            .when(tool == Tool::Changes && count > 0, |item| {
                                item.child(
                                    div().ml_auto().text_color(MUTED).child(count.to_string()),
                                )
                            })
                            .aria_expanded(expanded)
                            .tab_stop(self.menu_open)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if view.menu_open {
                                    view.activate_tool(tool, cx);
                                }
                            }));
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .gap(SPACE_1)
                            .child(button)
                            .when(expanded, |row| {
                                row.child(match tool {
                                    Tool::Ui => {
                                        self.screen_navigation(section_height - px(32.), cx)
                                    }
                                    Tool::Objects => {
                                        self.scene_objects(section_height - px(32.), cx)
                                    }
                                    Tool::Assets => {
                                        self.assets_navigation(section_height - px(74.), cx)
                                    }
                                    Tool::Changes | Tool::Build => {
                                        let body = match tool {
                                            Tool::Changes => self.changes_navigation(cx),
                                            Tool::Build => self.build(cx),
                                            _ => unreachable!(),
                                        };
                                        div()
                                            .id("section-items")
                                            .ml(SPACE_3)
                                            .max_h(section_height)
                                            .overflow_y_scroll()
                                            .child(body)
                                            .into_any_element()
                                    }
                                })
                            })
                    })),
            );
        slide("navigation", MENU_WIDTH, self.menu_open, content)
    }

    pub(in crate::editor) fn child_query(&self, tool: Tool) -> String {
        let query = self.search.trim().to_lowercase();
        if tool.label().to_lowercase().contains(&query) {
            String::new()
        } else {
            query
        }
    }

    pub(super) fn has_nested_match(&self, tool: Tool, query: &str) -> bool {
        self.first_nested_match(tool, query).is_some()
    }

    fn first_nested_match(&self, tool: Tool, query: &str) -> Option<SidebarResult> {
        match tool {
            Tool::Objects => self
                .scene_revision()
                .objects()
                .iter()
                .find(|object| matches_entry(&object_label(object), &object.id, query))
                .map(|object| SidebarResult::Object(object.id.clone())),
            Tool::Changes => self
                .project
                .review
                .changes()
                .iter()
                .find(|change| matches_entry(&self.object_title(&change.id), &change.id, query))
                .map(|change| SidebarResult::Object(change.id.clone())),
            Tool::Assets => self
                .asset_paths()
                .iter()
                .find(|path| matches_entry(&path.to_string_lossy(), "", query))
                .cloned()
                .map(SidebarResult::Asset),
            Tool::Ui => self
                .screen_choices()
                .find(|(id, title)| matches_entry(title, id, query))
                .map(|(id, _)| SidebarResult::Screen(id.to_owned())),
            Tool::Build => None,
        }
    }

    pub(super) fn activate_search_result(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let query = self.child_query(tool);
        match (!query.is_empty())
            .then(|| self.first_nested_match(tool, &query))
            .flatten()
        {
            Some(SidebarResult::Object(id)) => {
                self.show_tool(tool);
                self.select_object(id, cx);
            }
            Some(SidebarResult::Screen(id)) => {
                self.select_screen(id);
                cx.notify();
            }
            Some(SidebarResult::Asset(path)) => {
                self.show_tool(tool);
                self.preview_asset(path, cx);
            }
            None => {
                self.expand_tool(Some(tool));
                if tool == Tool::Assets {
                    self.refresh_assets(cx);
                }
                cx.notify();
            }
        }
    }
}

enum SidebarResult {
    Object(String),
    Screen(String),
    Asset(std::path::PathBuf),
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, HeadlessAppContext};
    use gpui_wgpu::CosmicTextSystem;

    use super::*;
    use crate::editor::Project;

    #[test]
    fn search_matches_nested_objects_and_their_changes() {
        let project = Project::demo(crate::demo().unwrap());
        let mut app = HeadlessAppContext::new(Arc::new(CosmicTextSystem::new("sans-serif")));
        let view = app.new(|cx| ReviewWindow::new(project, cx));
        app.update(|cx| {
            view.update(cx, |view, cx| {
                view.search = " plate ".into();
                let results = view.matching_tools().collect::<Vec<_>>();
                assert!(results == [Tool::Objects, Tool::Changes]);
                assert!(view.tool.is_none());
                view.activate_search_result(Tool::Objects, cx);
                assert!(view.tool == Some(Tool::Objects));
                assert_eq!(view.selected.as_deref(), Some("plate"));
            });
        });
    }
}
