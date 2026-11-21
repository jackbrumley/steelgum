use gpui::{App, ClickEvent, Div, FocusHandle, SharedString, Stateful, Window, div, prelude::*};

use super::{Icon, icon_button, tokens::*};

pub(crate) fn field_frame(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .min_h(CONTROL_HEIGHT)
        .border_1()
        .border_color(BORDER)
        .rounded(RADIUS_CONTROL)
        .bg(SHELL)
        .px(SPACE_2)
        .py(SPACE_1)
        .text_size(TYPE_CONTROL)
        .truncate()
}

pub(crate) fn focus_text_field(
    frame: Stateful<Div>,
    focus: &FocusHandle,
    label: &'static str,
    value: impl Into<SharedString>,
    tab_stop: bool,
) -> Stateful<Div> {
    frame
        .track_focus(focus)
        .tab_index(0)
        .tab_stop(tab_stop)
        .role(gpui::accesskit::Role::TextInput)
        .aria_label(label)
        .aria_value(value.into())
        .focus(|style| style.border_color(FOCUS))
}

pub(crate) fn search_field(
    query: &str,
    focus: &FocusHandle,
    window: &Window,
    visible: bool,
    clear: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let focused = focus.is_focused(window);
    let display = if query.is_empty() && !focused {
        "Find a tool".into()
    } else if focused {
        format!("{query}▏")
    } else {
        query.into()
    };
    div()
        .id("tool-search")
        .track_focus(focus)
        .tab_index(0)
        .tab_stop(visible)
        .aria_label("Find a tool")
        .aria_value(query.to_owned())
        .role(gpui::accesskit::Role::SearchInput)
        .rounded(RADIUS_CONTROL)
        .bg(SHELL)
        .px(SPACE_2)
        .h(SEARCH_HEIGHT)
        .flex_none()
        .flex()
        .items_center()
        .gap(SPACE_1)
        .focus_visible(|style| style.underline())
        .cursor_text()
        .child(Icon::Search.element(MUTED))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(TYPE_CONTROL)
                .text_color(if focused || !query.is_empty() {
                    TEXT
                } else {
                    MUTED
                })
                .child(display),
        )
        .when(!query.is_empty(), |field| {
            field.child(
                icon_button("clear-search", Icon::Close, "Clear search", false, true)
                    .tab_stop(visible)
                    .on_click(clear),
            )
        })
}
