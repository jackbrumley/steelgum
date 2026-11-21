mod preview;

use std::{env, error::Error, io};

use door_game::DoorGame;

const USAGE: &str = "Usage: door-game --demo [scene.json] | --inspect [scene.json] | --preview [scene.json] | --version";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let command = args.next();
    let command = command
        .as_deref()
        .map(|argument| argument.to_str().unwrap_or_default());
    let path = args.next();
    if args.next().is_some() {
        return Err(invalid_usage().into());
    }
    match command {
        Some("--version") if path.is_none() => {
            println!("door-game {}", env!("CARGO_PKG_VERSION"));
        }
        None | Some("--help" | "-h") if path.is_none() => println!("{USAGE}"),
        Some("--preview") => preview::launch(path.as_deref())?,
        Some("--preview-session") => {
            let mut game = match path {
                Some(path) => DoorGame::load_scene(path)?,
                None => DoorGame::demo_scene()?,
            };
            preview::session(&mut game, io::stdin().lock(), io::stdout().lock())?;
        }
        Some("--demo" | "--inspect") => {
            let mut game = if let Some(path) = path {
                DoorGame::load_scene(path)?
            } else {
                DoorGame::demo_scene()?
            };
            if command == Some("--demo") {
                game.run_demo()?;
                println!("{}", serde_json::to_string_pretty(&game.status())?);
            } else {
                println!("{}", game.scene().to_json()?);
            }
        }
        _ => return Err(invalid_usage().into()),
    }
    Ok(())
}

fn invalid_usage() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}
