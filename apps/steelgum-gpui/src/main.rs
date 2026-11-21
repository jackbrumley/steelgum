mod editor;
mod live_bridge;
mod updates;
mod viewport;
#[cfg(windows)]
mod windows;

use std::cell::RefCell;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result, bail};
use gpui::{App, AppContext, Bounds, BoundsExt, WindowBounds, WindowOptions, px, size};
use steelgum::Scene;
use steelgum::review::SceneReview;

const DEMO: &str = include_str!("../../../examples/door-game/scenes/room.json");

fn main() -> Result<()> {
    let mut arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--agent")
    {
        return editor::agent::run(&arguments[1..]);
    }
    if let [argument] = arguments.as_slice()
        && argument == "--mcp"
    {
        return live_bridge::run_mcp();
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--live")
    {
        return live_bridge::run_cli(&arguments[1..]);
    }
    if updates::handle_cli(&arguments)? {
        return Ok(());
    }
    match arguments.first().and_then(|argument| argument.to_str()) {
        Some("--help" | "-h") => {
            println!("Usage: steelgum [WORKTREE [SCENE_PATH_RELATIVE_TO_ROOT]]");
            println!("       steelgum --agent --help | --live --help | --mcp");
            println!("       steelgum --version | --check-updates | --update");
            println!("       steelgum [--scene PATH] [--capture PATH]");
            return Ok(());
        }
        Some("--scene" | "--capture") => {
            let player = packaged_player().context("Run the packaged workspace or use the Door Game Bevy runner for --scene and --capture")?;
            let status = std::process::Command::new(player)
                .args(&arguments)
                .status()?;
            if !status.success() {
                bail!("The bundled player exited with {status}");
            }
            return Ok(());
        }
        _ => {}
    }
    let update_failed = arguments.len() == 1 && arguments[0] == "--update-failed";
    if update_failed {
        arguments.clear();
    }
    let (project, notice) = project(&arguments)?;
    let startup_error = Rc::new(RefCell::new(None));
    let report = startup_error.clone();
    gpui::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(780.), px(480.))),
            app_id: Some(steelgum_update::APP_ID.to_owned()),
            ..Default::default()
        };
        #[cfg(windows)]
        let options = windows::with_wgpu(options);
        if let Err(error) = cx.open_window(options, move |window, cx| {
            window.set_window_title("Steelgum");
            updates::startup(window, cx, update_failed);
            cx.new(|cx| {
                let mut workspace = editor::ReviewWindow::new(project, cx);
                workspace.set_startup_notice(notice);
                workspace.start_agent_bridge(cx);
                workspace.focus(window, cx);
                workspace
            })
        }) {
            *report.borrow_mut() = Some(error);
            cx.quit();
        } else {
            cx.activate(true);
        }
    });
    match startup_error.take() {
        Some(error) => Err(error).context("Steelgum could not open its review window"),
        None => Ok(()),
    }
}

pub(crate) fn packaged_player() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let player = executable.with_file_name(format!("steelgum-player{}", env::consts::EXE_SUFFIX));
    player.is_file().then_some(player)
}

fn project(arguments: &[OsString]) -> Result<(editor::Project, Option<String>)> {
    let explicit = match arguments {
        [] => match editor::Project::reopen() {
            Ok(Some(project)) => return Ok((project, None)),
            Ok(None) => return Ok((editor::Project::demo(demo()?), None)),
            Err(error) => {
                return Ok((
                    editor::Project::demo(demo()?),
                    Some(format!("Could not reopen the last project: {error:#}")),
                ));
            }
        },
        [root] => editor::Project::discover(PathBuf::from(root))?,
        [root, path] => editor::Project::open(PathBuf::from(root), PathBuf::from(path))?,
        _ => bail!("Usage: steelgum [WORKTREE [SCENE_PATH_RELATIVE_TO_ROOT]]"),
    };
    let notice = explicit
        .remember()
        .err()
        .map(|error| format!("Could not remember this project: {error:#}"));
    Ok((explicit, notice))
}

fn demo() -> Result<SceneReview> {
    let before = Scene::from_json(DEMO)?;
    let after = DEMO
        .replace("\"color\": [45, 149, 236]", "\"color\": [224, 69, 81]")
        .replace("\"size\": [1.7, 1.2, 0.32]", "\"size\": [1.7, 1.6, 0.32]");
    Ok(SceneReview::new(before, Scene::from_json(&after)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_contains_the_pressure_plate_change() -> Result<()> {
        let review = demo()?;
        assert_eq!(
            review.before().object("plate").expect("sample plate").color,
            [45, 149, 236]
        );
        assert_eq!(
            review.after().object("plate").expect("sample plate").color,
            [224, 69, 81]
        );
        assert!(review.changes().iter().any(|change| change.id == "plate"));
        Ok(())
    }
}
