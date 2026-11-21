//! GPUI presentation boundary. The scene model never sees a GPU handle.

use std::rc::Rc;

use gpui::AnyElement;
#[cfg(not(any(target_os = "linux", windows)))]
use gpui::{IntoElement as _, div, prelude::*, rgb};
use steelgum::{Scene, review::SceneReview};

#[cfg(any(target_os = "linux", windows))]
mod bevy;

#[derive(Clone)]
pub(crate) struct View {
    pub review: Rc<SceneReview>,
    pub before: bool,
    pub selection: Option<String>,
    pub focus: bool,
}

impl View {
    pub fn scene(&self) -> &Scene {
        if self.before {
            self.review.before()
        } else {
            self.review.after()
        }
    }
}

#[derive(Default)]
pub(crate) struct Viewport {
    #[cfg(any(target_os = "linux", windows))]
    surface: bevy::Surface,
}

impl Viewport {
    pub fn element(&self, view: View) -> AnyElement {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.surface.element(view)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let object_count = view.scene().objects().len();
            let selection = view.selection.as_deref().unwrap_or("None");
            let scope = if view.focus {
                "Focused"
            } else {
                "Entire scene"
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .bg(rgb(0x101820))
                .text_color(rgb(0xadc1cd))
                .child("Live Bevy viewport is currently available on Linux and Windows.")
                .child(format!(
                    "{scope} · {object_count} objects · Selection: {selection}"
                ))
                .into_any_element()
        }
    }
}
