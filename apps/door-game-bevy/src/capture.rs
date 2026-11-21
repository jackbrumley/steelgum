use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::app::AppExit;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};

use crate::{HEIGHT, WIDTH};

const WARMUP_FRAMES: u8 = 8;
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Resource)]
pub(crate) struct Job {
    path: PathBuf,
    started: Instant,
    warmup_frames: u8,
    pending: bool,
}

impl Job {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            started: Instant::now(),
            warmup_frames: WARMUP_FRAMES,
            pending: false,
        }
    }
}

pub(crate) fn setup(
    mut commands: Commands,
    camera: Single<Entity, With<Camera3d>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut image = Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    commands
        .entity(*camera)
        .insert(RenderTarget::from(images.add(image)));
}

pub(crate) fn request(
    mut commands: Commands,
    mut job: ResMut<Job>,
    target: Single<&RenderTarget, With<Camera3d>>,
    mut exit: MessageWriter<AppExit>,
) {
    if job.started.elapsed() >= TIMEOUT {
        eprintln!("Steelgum: timed out capturing {}", job.path.display());
        exit.write(AppExit::error());
        return;
    }
    if job.warmup_frames > 0 {
        job.warmup_frames -= 1;
        return;
    }
    if job.pending {
        return;
    }
    let Some(image) = target.as_image() else {
        eprintln!("Steelgum: the capture camera has no image target");
        exit.write(AppExit::error());
        return;
    };
    job.pending = true;
    commands
        .spawn(Screenshot::image(image.clone()))
        .observe(save);
}

fn save(capture: On<ScreenshotCaptured>, job: Res<Job>, mut exit: MessageWriter<AppExit>) {
    let image = match capture.image.clone().try_into_dynamic() {
        Ok(image) => image,
        Err(error) => {
            eprintln!("Steelgum: could not convert the captured image: {error}");
            exit.write(AppExit::error());
            return;
        }
    };
    if let Err(error) = image.save(&job.path) {
        eprintln!("Steelgum: could not save {}: {error}", job.path.display());
        exit.write(AppExit::error());
        return;
    }
    println!("Captured {}", job.path.display());
    exit.write(AppExit::Success);
}
