//! Graphical smoke test for the optional LibGodot development host.

use std::{io::Read, path::Path, process::Command, sync::Arc};

use anyhow::{Context as _, Result, ensure};
use gpui::{
    AppContext, Context, Corners, HeadlessAppContext, Render, RenderImage, Window, canvas,
    current_headless_renderer, prelude::*, px, size,
};
use gpui_wgpu::CosmicTextSystem;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 160;

struct FrameView(Arc<RenderImage>);

impl Render for FrameView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let image = Arc::clone(&self.0);
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                window
                    .paint_image(bounds, bounds, Corners::default(), image, 0, false)
                    .expect("paint the LibGodot frame");
            },
        )
        .size_full()
    }
}

fn read_image(stream: &mut impl Read, index: u32) -> Result<Arc<RenderImage>> {
    let mut read_integer = || -> std::io::Result<u32> {
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    };
    let header = [read_integer()?, read_integer()?, read_integer()?];
    ensure!(
        header == [index, WIDTH, HEIGHT],
        "unexpected frame header {header:?}"
    );

    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    stream.read_exact(&mut pixels)?;
    // GPUI uploads tightly packed BGRA8 images.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels).context("invalid frame size")?;
    Ok(Arc::new(RenderImage::new([image::Frame::new(image)])))
}

#[test]
#[ignore = "requires the LibGodot development host, shared library and graphics display"]
fn paints_two_live_libgodot_frames_in_gpui() -> Result<()> {
    let host = std::env::var_os("STEELGUM_LIBGODOT_HOST").context("set STEELGUM_LIBGODOT_HOST")?;
    let library = std::env::var_os("STEELGUM_LIBGODOT").context("set STEELGUM_LIBGODOT")?;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/libgodot-probe");
    let output = Command::new(host).arg(library).arg(project).output()?;
    ensure!(
        output.status.success(),
        "LibGodot host exited with {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let mut stream = output.stdout.as_slice();
    let mut magic = [0; 8];
    stream.read_exact(&mut magic)?;
    ensure!(&magic == b"SGRGBA01", "unexpected frame stream header");
    let first = read_image(&mut stream, 0)?;
    let second = read_image(&mut stream, 1)?;
    ensure!(stream.is_empty(), "unexpected data after the second frame");

    let mut app = HeadlessAppContext::with_platform(
        Arc::new(CosmicTextSystem::new("sans-serif")),
        Arc::new(()),
        current_headless_renderer,
    );
    let window = app.open_window(size(px(WIDTH as f32), px(HEIGHT as f32)), |_, cx| {
        cx.new(|_| FrameView(first))
    })?;
    let before = app.capture_screenshot(window.into())?;
    app.update(|cx| {
        window.update(cx, |view, _, cx| {
            view.0 = second;
            cx.notify();
        })
    })?;
    let after = app.capture_screenshot(window.into())?;

    let orange_count = |image: &image::RgbaImage| {
        image
            .pixels()
            .filter(|pixel| {
                let [r, g, b, _] = pixel.0.map(u16::from);
                r > 90 && r > g + 35 && g > b + 12
            })
            .count()
    };
    assert!(orange_count(&before) >= 100);
    assert!(orange_count(&after) >= 100);
    assert_ne!(
        before, after,
        "the second Godot camera must change the displayed image"
    );
    Ok(())
}
