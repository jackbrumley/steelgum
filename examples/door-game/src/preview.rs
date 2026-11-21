//! Local development preview. The display consumes the same simulation as the CLI.

use std::env;
use std::ffi::OsStr;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;

use door_game::{DoorGame, STEP, Status};

pub fn launch(scene: Option<&OsStr>) -> io::Result<()> {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/door-game-preview/play.tcl");
    let mut command = Command::new("wish");
    command.arg(script).arg(env::current_exe()?);
    if let Some(path) = scene {
        // Tcl parses operator-like arguments in pipeline lists. Always pass a full pathname.
        command.arg(std::path::absolute(path)?);
    }
    let status = command.status().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not launch Tcl/Tk wish: {error}"),
        )
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "visual preview exited with {status}"
        )))
    }
}

pub fn session(game: &mut DoorGame, input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    writeln!(output, "step-ms {}", STEP.as_millis())?;
    for object in game.scene().objects() {
        let [x, y, z] = object.center;
        let [width, height, depth] = object.size;
        let [red, green, blue] = object.color;
        writeln!(
            output,
            "box {} {x} {y} {z} {width} {height} {depth} #{red:02x}{green:02x}{blue:02x}",
            object.id
        )?;
    }
    write_status(&mut output, game.status())?;

    for line in input.lines() {
        let line = line?;
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["step", x, z] => game.step([axis(x)?, axis(z)?]),
            ["restart"] => game.restart(),
            ["quit"] => break,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid preview command",
                ));
            }
        }
        write_status(&mut output, game.status())?;
    }
    Ok(())
}

fn axis(value: &str) -> io::Result<f32> {
    match value {
        "-1" => Ok(-1.0),
        "0" => Ok(0.0),
        "1" => Ok(1.0),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid movement axis",
        )),
    }
}

fn write_status(output: &mut impl Write, status: Status) -> io::Result<()> {
    let [x, y, z] = status.player;
    writeln!(
        output,
        "frame {x} {y} {z} {} {} {} {}",
        u8::from(status.door_open),
        u8::from(status.plate_pressed),
        status.door_hold_ms,
        u8::from(status.won),
    )?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_session_drives_the_real_game_and_restarts() {
        let mut game = DoorGame::demo_scene().expect("valid scene");
        let commands = "step 0 -1\n".repeat(120) + "restart\nquit\n";
        let mut output = Vec::new();
        session(&mut game, commands.as_bytes(), &mut output).expect("completed session");
        let output = String::from_utf8(output).expect("text session");
        let frames: Vec<_> = output
            .lines()
            .filter(|line| line.starts_with("frame "))
            .collect();
        assert_eq!(frames.first(), frames.last());
        assert!(frames.iter().any(|frame| frame.ends_with(" 1")));
    }
}
