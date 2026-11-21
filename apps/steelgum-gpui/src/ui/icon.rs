use gpui::{Div, Rgba, Styled, div, prelude::*, svg};

use super::tokens::{ICON_SIZE, SPACE_2};

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Search,
    Close,
    Menu,
    ChevronRight,
    ChevronDown,
    Plus,
    Play,
    Stop,
    Check,
}

impl Icon {
    pub(crate) fn element(self, color: Rgba) -> impl IntoElement {
        let svg_data: &[u8] = match self {
            Self::Search => include_bytes!("../../assets/tabler/search.svg"),
            Self::Close => include_bytes!("../../assets/tabler/x.svg"),
            Self::Menu => include_bytes!("../../assets/tabler/menu-2.svg"),
            Self::ChevronRight => include_bytes!("../../assets/tabler/chevron-right.svg"),
            Self::ChevronDown => include_bytes!("../../assets/tabler/chevron-down.svg"),
            Self::Plus => include_bytes!("../../assets/tabler/plus.svg"),
            Self::Play => include_bytes!("../../assets/tabler/player-play.svg"),
            Self::Stop => include_bytes!("../../assets/tabler/player-stop.svg"),
            Self::Check => include_bytes!("../../assets/tabler/check.svg"),
        };
        svg()
            .data(svg_data)
            .size(ICON_SIZE)
            .flex_none()
            .text_color(color)
    }

    pub(crate) fn label(self, text: &'static str, color: Rgba) -> Div {
        div()
            .flex()
            .items_center()
            .gap(SPACE_2)
            .child(self.element(color))
            .child(text)
    }
}
