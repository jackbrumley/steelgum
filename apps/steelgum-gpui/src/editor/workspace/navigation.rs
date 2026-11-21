use super::{
    ACCENT, BORDER, MUTED, PANEL, ReviewWindow, SELECTED, SHELL, TEXT, Tool, action, note, slide,
};
use gpui::{AnyElement, Context, Window, div, prelude::*, px, rgb};

const MENU_WIDTH: f32 = 182.;

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
                    .border_1()
                    .border_color(rgb(if focused { ACCENT } else { BORDER }))
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
            .when(tools.is_empty(), |menu| menu.child(note("No tools found")))
            .children(tools.into_iter().map(|tool| {
                let count = self.project.review.changes().len();
                let active = if tool == Tool::Ui {
                    self.definitions.ui_expanded
                } else {
                    self.tool == Some(tool)
                };
                let button = action(
                    tool.label(),
                    format!("{}   {}", tool.icon(), tool.label()),
                    active,
                )
                .bg(rgb(if active { SELECTED } else { PANEL }))
                .border_color(rgb(if active { SELECTED } else { PANEL }))
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
                .when(tool == Tool::Ui, |item| {
                    item.aria_expanded(self.definitions.ui_expanded)
                        .child(div().ml_auto().child(if self.definitions.ui_expanded {
                            "▾"
                        } else {
                            "▸"
                        }))
                })
                .tab_stop(self.menu_open)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if view.menu_open {
                        view.activate_tool(tool, cx);
                    }
                }));
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(button)
                    .when(tool == Tool::Ui && self.definitions.ui_expanded, |row| {
                        row.child(self.screen_navigation(cx))
                    })
            }));
        slide("navigation", MENU_WIDTH, self.menu_open, content)
    }
}
