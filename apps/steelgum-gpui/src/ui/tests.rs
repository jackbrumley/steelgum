use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use gpui::{
    AppContext, Context, FocusHandle, HeadlessAppContext, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PlatformInput, Render, Window, current_headless_renderer, div,
    point, px, size,
};
use gpui_wgpu::CosmicTextSystem;

use super::{tokens::*, *};

struct Gallery {
    query: String,
    focus: FocusHandle,
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(PANEL)
            .text_color(TEXT)
            .p(SPACE_4)
            .flex()
            .flex_col()
            .gap(SPACE_3)
            .child(search_field(
                &self.query,
                &self.focus,
                window,
                true,
                cx.listener(|gallery, _, window, cx| {
                    gallery.query.clear();
                    gallery.focus.focus(window, cx);
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .flex()
                    .gap(SPACE_2)
                    .child(action("default", "Open Folder", false))
                    .child(primary_button(
                        "primary",
                        Icon::Play.label("Play", ON_ACCENT),
                    ))
                    .child(danger_button("danger", "Delete"))
                    .child(pending_button("pending", "Saving…")),
            )
            .child(field_frame("property").child("Property value"))
            .child(create_action("create", "New Screen"))
            .child(sidebar_row("selected", "Selected screen", true))
            .child(sidebar_row("quiet", "Another screen", false))
    }
}

#[test]
#[ignore = "requires an available Vulkan adapter"]
fn shared_controls_render_and_clear_search() -> Result<()> {
    let mut app = HeadlessAppContext::with_platform(
        Arc::new(CosmicTextSystem::new("sans-serif")),
        Arc::new(()),
        current_headless_renderer,
    );
    let window = app.open_window(size(px(440.), px(320.)), |window, cx| {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            Gallery {
                query: "main menu".into(),
                focus,
            }
        })
    })?;
    let filled = app.capture_screenshot(window.into())?;
    app.update_window(window.into(), |_, window, cx| {
        let position = point(px(402.), px(34.));
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })?;
    app.update(|cx| {
        window.update(cx, |gallery, window, _| {
            assert!(gallery.query.is_empty());
            assert!(gallery.focus.is_focused(window));
        })
    })?;
    app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
    let cleared = app.capture_screenshot(window.into())?;
    assert_ne!(filled, cleared);
    if let Some(directory) = std::env::var_os("STEELGUM_SNAPSHOT_DIR").map(PathBuf::from) {
        std::fs::create_dir_all(&directory)?;
        filled.save(directory.join("steelgum-ui-kit.png"))?;
        cleared.save(directory.join("steelgum-ui-kit-search-cleared.png"))?;
    }
    Ok(())
}
