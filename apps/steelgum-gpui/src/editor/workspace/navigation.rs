use super::{
    ACCENT, BORDER, MUTED, PANEL, ReviewWindow, SHELL, TEXT, Tool, matches_entry, note,
    sidebar_action, slide,
};
use crate::editor::object_rename::object_label;
use gpui::{AnyElement, Context, Window, div, prelude::*, px, rgb, svg};

const MENU_WIDTH: f32 = 182.;
const CHEVRON_RIGHT: &[u8] = include_bytes!("../../../assets/tabler/chevron-right.svg");
const CHEVRON_DOWN: &[u8] = include_bytes!("../../../assets/tabler/chevron-down.svg");

impl ReviewWindow {
    pub(super) fn navigation(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.search_focus.is_focused(window);
        let search_empty = self.search.is_empty();
        let search = if search_empty && !focused {
            "Find a tool".to_owned()
        } else if focused {
            format!("{}▏", self.search)
        } else {
            self.search.clone()
        };
        let section_height = (window.viewport_size().height - px(370.)).max(px(96.));
        let searching = !self.search.trim().is_empty();
        let tools: Vec<_> = self.matching_tools().collect();
        let content = div()
            .w(px(MENU_WIDTH))
            .h_full()
            .flex_none()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("tool-search")
                    .track_focus(&self.search_focus)
                    .role(gpui::accesskit::Role::SearchInput)
                    .aria_label("Find a tool")
                    .tab_index(0)
                    .tab_stop(self.menu_open)
                    .aria_value(self.search.clone())
                    .on_click(cx.listener(|view, _, window, cx| view.focus_search(window, cx)))
                    .rounded_md()
                    .bg(rgb(SHELL))
                    .px_2()
                    .py_2()
                    .cursor_text()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(if focused || !search_empty {
                        TEXT
                    } else {
                        MUTED
                    }))
                    .child(format!("⌕  {search}")),
            )
            .child(
                div()
                    .id("tool-results")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .min_h_0()
                    .flex_1()
                    .when(searching, |results| results.overflow_y_scroll())
                    .when(tools.is_empty(), |menu| menu.child(note("No results")))
                    .children(tools.into_iter().map(|tool| {
                        let count = self.project.review.changes().len();
                        let active = self.expanded_tool == Some(tool);
                        let expanded = active || searching;
                        let asset = if expanded {
                            CHEVRON_DOWN
                        } else {
                            CHEVRON_RIGHT
                        };
                        let label = div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                svg()
                                    .data(asset)
                                    .size(px(16.))
                                    .flex_none()
                                    .text_color(rgb(if active { ACCENT } else { TEXT })),
                            )
                            .child(tool.label());
                        let button = sidebar_action(tool.label(), label, active)
                            .w_full()
                            .justify_start()
                            .gap_2()
                            .when(tool == Tool::Changes && count > 0, |item| {
                                item.child(
                                    div()
                                        .ml_auto()
                                        .text_color(rgb(MUTED))
                                        .child(count.to_string()),
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
                            .gap_1()
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
                                            .ml_3()
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
