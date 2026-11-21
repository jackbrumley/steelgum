#![forbid(unsafe_code)]

mod capture;
mod updates;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::window::{ExitCondition, PrimaryWindow};
use bevy::winit::WinitPlugin;
use door_game::{DOOR_ID, DoorGame, PLAYER_ID, STEP};

const TITLE: &str = "Steelgum | Door Game";
const WIDTH: u32 = 1100;
const HEIGHT: u32 = 760;

#[derive(Resource)]
struct Game(DoorGame);

#[derive(Component)]
struct PlayerMesh;

#[derive(Component)]
struct DoorMesh {
    closed_center: Vec3,
    height: f32,
}

enum Mode {
    Run {
        scene: Option<PathBuf>,
        capture: Option<PathBuf>,
    },
    Help,
    Version,
    CheckUpdates,
    UpdateFailed,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Mode, &'static str> {
    let mut args = args.into_iter();
    let mut scene = None;
    let mut capture = None;
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--scene") if scene.is_none() => {
                scene = Some(PathBuf::from(args.next().ok_or("--scene requires a path")?));
            }
            Some("--capture") if capture.is_none() => {
                let path = PathBuf::from(args.next().ok_or("--capture requires a PNG path")?);
                if !path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                {
                    return Err("the capture path must end in .png");
                }
                capture = Some(path);
            }
            Some("--help" | "-h") if scene.is_none() && capture.is_none() => {
                return standalone_mode(Mode::Help, args);
            }
            Some("--version" | "-V") if scene.is_none() && capture.is_none() => {
                return standalone_mode(Mode::Version, args);
            }
            Some("--check-updates") if scene.is_none() && capture.is_none() => {
                return standalone_mode(Mode::CheckUpdates, args);
            }
            Some("--update-failed") if scene.is_none() && capture.is_none() => {
                return standalone_mode(Mode::UpdateFailed, args);
            }
            _ => return Err("unknown or repeated argument; use --help for usage"),
        }
    }
    Ok(Mode::Run { scene, capture })
}

fn standalone_mode(
    mode: Mode,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Mode, &'static str> {
    if args.next().is_some() {
        return Err("unexpected extra argument; use --help for usage");
    }
    Ok(mode)
}

fn main() -> AppExit {
    let mode = match parse_args(std::env::args_os().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("Steelgum: {error}");
            return AppExit::error();
        }
    };

    match mode {
        Mode::Help => {
            println!("Usage: steelgum [--scene PATH.json] [--capture PATH.png]");
            println!("       steelgum --check-updates | --version | --help");
            println!("Move with WASD or arrow keys. Press R to restart or Esc to quit.");
            println!("Capture runs the scripted playthrough and saves its final view.");
            println!("Press F6 to check for updates.");
            AppExit::Success
        }
        Mode::Version => {
            println!("steelgum {}", env!("CARGO_PKG_VERSION"));
            AppExit::Success
        }
        Mode::CheckUpdates => check_for_updates(),
        Mode::UpdateFailed => {
            updates::warn_failed_install();
            run(None, None, false)
        }
        Mode::Run { scene, capture } => run(scene.as_deref(), capture, true),
    }
}

fn check_for_updates() -> AppExit {
    match steelgum_update::check_for_updates(env!("CARGO_PKG_VERSION"))
        .map(|check| serde_json::to_string_pretty(&check))
    {
        Ok(Ok(json)) => {
            println!("{json}");
            AppExit::Success
        }
        Ok(Err(error)) => {
            eprintln!("Steelgum: could not format update status: {error}");
            AppExit::error()
        }
        Err(error) => {
            eprintln!("Steelgum: {error}");
            AppExit::error()
        }
    }
}

fn run(scene_path: Option<&Path>, capture_path: Option<PathBuf>, auto_check: bool) -> AppExit {
    let headless = capture_path.is_some();
    let game = scene_path.map_or_else(DoorGame::demo_scene, DoorGame::load_scene);
    let game = match game.and_then(|mut game| {
        if headless {
            game.run_demo()?;
        }
        Ok(game)
    }) {
        Ok(game) => game,
        Err(error) => {
            eprintln!("Steelgum: {error}");
            return AppExit::error();
        }
    };

    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: (!headless).then(|| Window {
                title: TITLE.to_string(),
                name: Some(steelgum_update::APP_ID.into()),
                resolution: (WIDTH, HEIGHT).into(),
                ..default()
            }),
            exit_condition: if headless {
                ExitCondition::DontExit
            } else {
                ExitCondition::OnPrimaryClosed
            },
            ..default()
        })
        .set(RenderPlugin {
            synchronous_pipeline_compilation: headless,
            ..default()
        });

    let mut app = App::new();
    app.insert_resource(Game(game))
        .insert_resource(ClearColor(Color::srgb_u8(17, 23, 32)))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, sync_scene);

    if let Some(path) = capture_path {
        app.add_plugins(plugins.disable::<WinitPlugin>())
            .add_plugins(ScheduleRunnerPlugin::run_loop(STEP))
            .insert_resource(capture::Job::new(path))
            .add_systems(Startup, capture::setup.after(setup_scene))
            .add_systems(PostUpdate, capture::request);
    } else {
        app.add_plugins(plugins)
            .insert_non_send(updates::Updates::new(auto_check))
            .add_systems(Startup, updates::startup)
            .add_systems(Update, updates::poll)
            .insert_resource(Time::<Fixed>::from_duration(STEP))
            .add_systems(FixedUpdate, step_game)
            .add_systems(Update, handle_controls.before(sync_scene));
    }
    app.run()
}

fn setup_scene(
    mut commands: Commands,
    game: Res<Game>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for object in game.0.scene().objects() {
        let [width, height, depth] = object.size;
        let [red, green, blue] = object.color;
        let center = Vec3::from_array(object.center);
        let mut entity = commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(width, height, depth))),
            MeshMaterial3d(materials.add(Color::srgb_u8(red, green, blue))),
            Transform::from_translation(center),
        ));
        match object.id.as_str() {
            PLAYER_ID => {
                entity.insert(PlayerMesh);
            }
            DOOR_ID => {
                entity.insert(DoorMesh {
                    closed_center: center,
                    height,
                });
            }
            _ => {}
        }
    }

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.5, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-4.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn step_game(keys: Res<ButtonInput<KeyCode>>, mut game: ResMut<Game>) {
    let right = keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);
    let left = keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
    let back = keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]);
    let forward = keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]);
    game.0.step([
        f32::from(u8::from(right)) - f32::from(u8::from(left)),
        f32::from(u8::from(back)) - f32::from(u8::from(forward)),
    ]);
}

fn handle_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<Game>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        game.0.restart();
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn sync_scene(
    game: Res<Game>,
    mut players: Query<&mut Transform, (With<PlayerMesh>, Without<DoorMesh>)>,
    mut doors: Query<(&DoorMesh, &mut Transform)>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    updates: Option<NonSend<updates::Updates>>,
) {
    let status = game.0.status();
    for mut transform in &mut players {
        transform.translation = Vec3::from_array(status.player);
    }
    for (door, mut transform) in &mut doors {
        transform.translation = door.closed_center;
        if status.door_open {
            transform.translation.y += door.height;
        }
    }

    if let Ok(mut window) = window.single_mut() {
        let guidance = if status.won {
            "Goal reached! Press R to restart"
        } else if status.plate_pressed {
            "Plate pressed. Go through the open door"
        } else if status.door_open {
            "Door open. Reach the goal"
        } else {
            "WASD or arrows to move. Step on the plate"
        };
        let update_status = updates
            .as_ref()
            .map_or("F6 Updates", |value| value.window_status());
        let title = format!("{TITLE} | {guidance} | {update_status}");
        if window.title != title {
            window.title = title;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_a_custom_scene_with_offscreen_capture() {
        let args = ["--capture", "room view.png", "--scene", "test room.json"];
        let mode = parse_args(args.map(OsString::from)).expect("valid flags");
        let Mode::Run { scene, capture } = mode else {
            panic!("expected run mode");
        };
        assert_eq!(scene, Some(PathBuf::from("test room.json")));
        assert_eq!(capture, Some(PathBuf::from("room view.png")));
    }

    #[test]
    fn rejects_missing_or_ambiguous_arguments() {
        for args in [
            ["--scene"].as_slice(),
            ["--capture", "room.jpg"].as_slice(),
            ["--scene", "a.json", "--scene", "b.json"].as_slice(),
            ["--help", "--scene", "room.json"].as_slice(),
        ] {
            assert!(parse_args(args.iter().copied().map(OsString::from)).is_err());
        }
    }
}
