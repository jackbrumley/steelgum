mod editor;
mod viewport;
#[cfg(windows)]
mod windows;

use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result, bail};
use gpui::{App, AppContext, Bounds, BoundsExt, WindowBounds, WindowOptions, px, size};
use steelgum::Scene;
use steelgum::review::SceneReview;

const DEMO: &str = include_str!("../../../examples/door-game/scenes/room.json");

fn main() -> Result<()> {
    let project = project()?;
    let startup_error = Rc::new(RefCell::new(None));
    let report = startup_error.clone();
    gpui::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(780.), px(480.))),
            ..Default::default()
        };
        #[cfg(windows)]
        let options = windows::with_wgpu(options);
        if let Err(error) = cx.open_window(options, move |window, cx| {
            window.set_window_title("Steelgum");
            cx.new(|cx| {
                let workspace = editor::ReviewWindow::new(project, cx);
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

fn project() -> Result<editor::Project> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (None, None, None) => Ok(editor::Project::demo(demo()?)),
        (Some(root), Some(path), None) => {
            editor::Project::open(PathBuf::from(root), PathBuf::from(path))
        }
        _ => bail!("Usage: steelgum-gpui [REPOSITORY SCENE_PATH_RELATIVE_TO_ROOT]"),
    }
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
