//! Semantic design tokens for Steelgum's editor chrome.

use gpui::{FontWeight, Pixels, Rgba, px};

const fn color(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.,
        g: ((hex >> 8) & 0xff) as f32 / 255.,
        b: (hex & 0xff) as f32 / 255.,
        a: 1.,
    }
}

pub const SHELL: Rgba = color(0x15181f);
pub const TOOLBAR: Rgba = color(0x20242e);
pub const PANEL: Rgba = color(0x1b1f28);
pub const BORDER: Rgba = color(0x343b4a);
pub const TEXT: Rgba = color(0xeef1f7);
pub const MUTED: Rgba = color(0xaab4c6);
pub const ACCENT: Rgba = color(0xbab5ff);
pub const FOCUS: Rgba = ACCENT;
pub const SELECTED: Rgba = color(0x353451);
pub const ON_ACCENT: Rgba = color(0x24203c);
pub const ACCENT_HOVER: Rgba = color(0xd3ceff);
pub const ERROR: Rgba = color(0xffb5b4);
pub const DANGER_TEXT: Rgba = color(0xffadaf);
pub const DANGER_SURFACE: Rgba = color(0x3f262c);
pub const DANGER_BORDER: Rgba = color(0x7a4349);
pub const DANGER_HOVER: Rgba = color(0x583037);
pub const CLEAR: Rgba = Rgba { a: 0., ..color(0) };
pub const VIEWPORT_SURFACE: Rgba = color(0x0e161e);
#[cfg(not(any(target_os = "linux", windows)))]
pub const VIEWPORT_UNAVAILABLE: Rgba = color(0x101820);
#[cfg(not(any(target_os = "linux", windows)))]
pub const VIEWPORT_UNAVAILABLE_TEXT: Rgba = color(0xadc1cd);
pub const PREVIEW_SURFACE: Rgba = color(0x16212a);
pub const PREVIEW_TEXT: Rgba = color(0xecf2f6);
pub const PREVIEW_BUTTON: Rgba = color(0x2a3b46);
pub const PREVIEW_DIMMER: Rgba = Rgba {
    a: 161. / 255.,
    ..color(0x05080a)
};

pub const SPACE_1: Pixels = px(4.);
pub const SPACE_2: Pixels = px(8.);
pub const SPACE_3: Pixels = px(12.);
pub const SPACE_4: Pixels = px(16.);
pub const SPACE_5: Pixels = px(20.);
pub const TYPE_CAPTION: Pixels = px(12.);
pub const TYPE_CONTROL: Pixels = px(14.);
pub const TYPE_HEADING: Pixels = px(18.);
pub const TYPE_TITLE: Pixels = px(20.);
pub const WEIGHT_HEADING: FontWeight = FontWeight::SEMIBOLD;
pub const RADIUS_CONTROL: Pixels = px(6.);
pub const RADIUS_SMALL: Pixels = px(4.);
pub const CONTROL_HEIGHT: Pixels = px(28.);
pub const TOOLBAR_HEIGHT: Pixels = px(56.);
pub const SEARCH_HEIGHT: Pixels = px(36.);
pub const ICON_SIZE: Pixels = px(18.);
pub const ICON_HITBOX: Pixels = px(28.);
pub const PANE_SPRING: (f32, f32, f32) = (360., 38., 1.);
