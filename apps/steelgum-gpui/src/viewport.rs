//! GPUI presentation boundary. The scene model never sees a GPU handle.

use std::rc::Rc;

use gpui::AnyElement;

pub(crate) type PendingScreenshot = Box<dyn FnOnce() -> anyhow::Result<Vec<u8>> + Send>;
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
    pub top_down: bool,
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
    pub fn capture(&self, expected: &View) -> anyhow::Result<PendingScreenshot> {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.surface.capture(expected)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = expected;
            anyhow::bail!("Viewport screenshots are available on Linux and Windows.")
        }
    }

    pub fn capture_framed(&self, view: &View) -> anyhow::Result<PendingScreenshot> {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.surface.capture_framed(view)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = view;
            anyhow::bail!("Viewport screenshots are available on Linux and Windows.")
        }
    }

    pub fn rendered_frames(&self) -> u64 {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.surface.rendered_frames()
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            0
        }
    }

    pub fn reset_pan(&self) {
        #[cfg(any(target_os = "linux", windows))]
        self.surface.reset_pan();
    }

    pub fn element(&self, view: View, select: impl Fn(String) + 'static) -> AnyElement {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.surface.element(view, select)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = select;
            let object_count = view.scene().objects().len();
            let selection = view.selection.as_deref().unwrap_or("None");
            let projection = if view.top_down { "2D" } else { "3D" };
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
                    "{projection} · {scope} · {object_count} objects · Selection: {selection}"
                ))
                .into_any_element()
        }
    }
}
