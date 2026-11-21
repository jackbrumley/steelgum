//! Optional Steelgum features for Bevy game runners.

use std::time::Duration;

use bevy::{
    camera::visibility::RenderLayers,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
    sprite::Anchor,
    window::PrimaryWindow,
};

const HUD_LAYER: RenderLayers = RenderLayers::layer(31);
const HUD_CAMERA_ORDER: isize = 100;
const SCREEN_INSET: f32 = 12.;
const REFRESH_PERIOD: Duration = Duration::from_millis(250);

/// Draw a compact, green frame-rate readout in the primary game window.
///
/// Add this after Bevy's default plugins. The overlay reserves render layer
/// 31 and uses camera order 100 so it draws over the game's cameras.
pub struct FpsOverlayPlugin;

impl Plugin for FpsOverlayPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_systems(Startup, spawn_fps_overlay)
            .add_systems(Update, update_fps_overlay);
    }
}

#[derive(Component)]
struct FpsLabel;

fn spawn_fps_overlay(
    mut commands: Commands,
    primary: Option<Single<&Window, With<PrimaryWindow>>>,
) {
    let Some(window) = primary else {
        return;
    };
    commands.spawn((
        Camera2d,
        Camera {
            order: HUD_CAMERA_ORDER,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        HUD_LAYER,
    ));
    commands.spawn((
        Text2d::new("-- FPS"),
        TextFont {
            font_size: FontSize::Px(14.),
            ..default()
        },
        TextColor(Color::srgb_u8(142, 237, 113)),
        Transform::from_translation(label_position(&window)),
        Anchor::TOP_RIGHT,
        HUD_LAYER,
        FpsLabel,
    ));
}

fn update_fps_overlay(
    window: Single<&Window, With<PrimaryWindow>>,
    label: Single<(&mut Text2d, &mut Transform), With<FpsLabel>>,
    diagnostics: Res<DiagnosticsStore>,
    clock: Res<Time<Real>>,
    mut last_sample: Local<Duration>,
) {
    let (mut text, mut transform) = label.into_inner();
    let position = label_position(&window);
    if transform.translation != position {
        transform.translation = position;
    }
    let elapsed = clock.elapsed();
    if elapsed.saturating_sub(*last_sample) >= REFRESH_PERIOD
        && let Some(fps) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|entry| entry.smoothed())
    {
        *last_sample = elapsed;
        let next = format!("{fps:.0} FPS");
        if text.0 != next {
            text.0 = next;
        }
    }
}

fn label_position(window: &Window) -> Vec3 {
    Vec3::new(
        window.width() / 2. - SCREEN_INSET,
        window.height() / 2. - SCREEN_INSET,
        0.,
    )
}
