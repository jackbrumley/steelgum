mod button;
mod field;
mod icon;
pub(crate) mod tokens;

pub(crate) use button::{
    action, create_action, danger_button, icon_button, pending_button, primary_button,
    sidebar_action, sidebar_row,
};
pub(crate) use field::{field_frame, focus_text_field, search_field};
pub(crate) use icon::Icon;

use gpui::{Div, SharedString, Styled, div, prelude::*};
use tokens::{MUTED, TYPE_CAPTION};

pub(crate) fn note(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(TYPE_CAPTION)
        .text_color(MUTED)
        .child(text.into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
