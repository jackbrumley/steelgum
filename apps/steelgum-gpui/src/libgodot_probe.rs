//! Graphical smoke test for the optional LibGodot development host.

use std::{collections::HashMap, io::Read, path::Path, process::Command, sync::Arc};

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

fn read_image(stream: &mut impl Read, index: u32) -> Result<image::RgbaImage> {
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
    image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels).context("invalid frame size")
}

fn display_image(mut image: image::RgbaImage) -> Arc<RenderImage> {
    // GPUI uploads tightly packed BGRA8 images.
    for pixel in image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new([image::Frame::new(image)]))
}

fn check_fixture(image: &image::RgbaImage, stage: &str) -> Result<()> {
    let orange = image
        .pixels()
        .filter(|pixel| {
            let [r, g, b, a] = pixel.0.map(u16::from);
            a >= 128 && r > 90 && r > g + 35 && g > b + 12
        })
        .count();
    if orange < 100 {
        let mut counts = HashMap::new();
        for pixel in image.pixels() {
            *counts.entry(pixel.0).or_insert(0_usize) += 1;
        }
        let mut colors: Vec<_> = counts.into_iter().collect();
        colors.sort_unstable_by_key(|&(color, count)| (std::cmp::Reverse(count), color));
        colors.truncate(5);
        anyhow::bail!(
            "{stage}: found {orange} orange fixture pixels in {}×{}; most frequent RGBA colors {colors:?}",
            image.width(),
            image.height()
        );
    }
    Ok(())
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
    check_fixture(&first, "Godot readback, frame 0")?;
    check_fixture(&second, "Godot readback, frame 1")?;
    ensure!(
        first != second,
        "changing the camera produced identical Godot frames"
    );

    let mut app = HeadlessAppContext::with_platform(
        Arc::new(CosmicTextSystem::new("sans-serif")),
        Arc::new(()),
        current_headless_renderer,
    );
    let window = app.open_window(size(px(WIDTH as f32), px(HEIGHT as f32)), |_, cx| {
        cx.new(|_| FrameView(display_image(first)))
    })?;
    let before = app.capture_screenshot(window.into())?;
    app.update(|cx| {
        window.update(cx, |view, _, cx| {
            view.0 = display_image(second);
            cx.notify();
        })
    })?;
    let after = app.capture_screenshot(window.into())?;

    check_fixture(&before, "GPUI screenshot, frame 0")?;
    check_fixture(&after, "GPUI screenshot, frame 1")?;
    assert_ne!(
        before, after,
        "the second Godot camera must change the displayed image"
    );
    Ok(())
}
