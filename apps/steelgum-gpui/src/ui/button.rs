use gpui::{AppContext, Context, Div, ElementId, Render, Stateful, Window, div, prelude::*};

use super::{Icon, tokens::*};

#[derive(Clone, Copy)]
enum Variant {
    Default,
    Primary,
    Quiet,
    Danger,
    Pending,
}

fn button(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    variant: Variant,
    selected: bool,
) -> Stateful<Div> {
    let (background, text, border, hover) = match variant {
        Variant::Default if selected => (SELECTED, ACCENT, BORDER, SELECTED),
        Variant::Default => (TOOLBAR, TEXT, BORDER, SELECTED),
        Variant::Primary => (ACCENT, ON_ACCENT, ACCENT, ACCENT_HOVER),
        Variant::Quiet if selected => (SELECTED, ACCENT, CLEAR, SELECTED),
        Variant::Quiet => (CLEAR, TEXT, CLEAR, CLEAR),
        Variant::Danger => (DANGER_SURFACE, DANGER_TEXT, DANGER_BORDER, DANGER_HOVER),
        Variant::Pending => (TOOLBAR, MUTED, BORDER, TOOLBAR),
    };
    let quiet = matches!(variant, Variant::Quiet);
    div()
        .id(id)
        .tab_index(0)
        .role(gpui::accesskit::Role::Button)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .min_h(CONTROL_HEIGHT)
        .px(SPACE_2)
        .py(SPACE_1)
        .rounded(RADIUS_CONTROL)
        .text_size(TYPE_CONTROL)
        .when(!matches!(variant, Variant::Pending), |button| {
            button.cursor_pointer()
        })
        .when(matches!(variant, Variant::Pending), |button| {
            button.tab_stop(false).aria_description("In progress")
        })
        .bg(background)
        .text_color(text)
        .border_1()
        .border_color(border)
        .when(quiet, |button| button.border_0().justify_start())
        .focus_visible(move |style| {
            if quiet {
                style.underline()
            } else {
                style.border_color(FOCUS)
            }
        })
        .hover(move |style| {
            if quiet {
                style.text_color(ACCENT)
            } else {
                style.bg(hover)
            }
        })
        .child(content)
}

pub(crate) fn action(id: &'static str, content: impl IntoElement, selected: bool) -> Stateful<Div> {
    button(id, content, Variant::Default, selected)
}

pub(crate) fn primary_button(id: &'static str, content: impl IntoElement) -> Stateful<Div> {
    button(id, content, Variant::Primary, false)
}

pub(crate) fn danger_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    button(id, label, Variant::Danger, false)
}

pub(crate) fn pending_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    button(id, label, Variant::Pending, false)
}

pub(crate) fn sidebar_action(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    selected: bool,
) -> Stateful<Div> {
    button(id, content, Variant::Quiet, selected)
}

pub(crate) fn sidebar_row(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    selected: bool,
) -> Stateful<Div> {
    sidebar_action(id, content, selected)
        .w_full()
        .aria_toggled(if selected {
            gpui::accesskit::Toggled::True
        } else {
            gpui::accesskit::Toggled::False
        })
}

pub(crate) fn create_action(id: &'static str, label: &'static str) -> Stateful<Div> {
    sidebar_action(id, Icon::Plus.label(label, ACCENT), false).text_color(ACCENT)
}

pub(crate) fn icon_button(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    selected: bool,
    quiet: bool,
) -> Stateful<Div> {
    let variant = if quiet {
        Variant::Quiet
    } else {
        Variant::Default
    };
    let color = if selected {
        ACCENT
    } else if quiet {
        MUTED
    } else {
        TEXT
    };
    button(id, icon.element(color), variant, selected)
        .size(ICON_HITBOX)
        .p_0()
        .justify_center()
        .aria_label(label)
        .tooltip(move |_, cx| cx.new(|_| IconTooltip(label)).into())
}

struct IconTooltip(&'static str);

impl Render for IconTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(TOOLBAR)
            .text_color(TEXT)
            .text_size(TYPE_CAPTION)
            .border_1()
            .border_color(BORDER)
            .rounded(RADIUS_CONTROL)
            .px(SPACE_2)
            .py(SPACE_1)
            .child(self.0)
    }
}
