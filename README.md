# Steelgum

Steelgum is a Rust game development framework designed for humans and coding
agents to build games together. A game lives in a normal Git repository with
source code, authored scenes, assets and tests. Steelgum owns the project model
and development workflow. Bevy is the first graphical backend.

The first example is **Door Game**, a small 3D room with a pressure plate and a
door. It exercises the same game rules from a headless test and the native
window. [Architecture](docs/architecture.md) explains the design and priorities.

## Run from source

Play the visual prototype from a graphical terminal with Rust and Tcl/Tk
(`wish`).

```sh
cargo run -p door-game -- --preview
```

Use WASD or the arrow keys to move, Space to watch the game play itself, and
R to restart. The room, timer and goal status come from the Rust simulation.
[See the prototype in action](docs/assets/door-game-preview.png).

To run the same game rules without opening a window:

```sh
cargo run -p door-game -- --demo
cargo test --workspace
```

The demo prints the final game state as JSON and exits with an error if the
player fails to reach the goal. Inspect a room file with
`cargo run -p door-game -- --inspect examples/door-game/scenes/room.json`.

Use Rust 1.97.1 or newer for the Bevy runner. Interactive play requires a
working graphical session and graphics adapter.

```sh
cargo run --manifest-path apps/door-game-bevy/Cargo.toml
```

Use WASD or the arrow keys to step on the blue plate and reach the green goal
through the orange door. The door stays open for three seconds after you leave
the plate. Press R to restart and Esc to quit.

The capture command plays the scripted demo and saves the final frame to a
PNG. It requires a graphics adapter and runs without opening a window.

```sh
cargo run --manifest-path apps/door-game-bevy/Cargo.toml -- --capture door-game.png
```

Pass `--scene path/to/room.json` to the runner to play or capture an edited
scene. The headless demo accepts the path after `--demo`.

Press F6 in the Bevy window to check for updates. Packaged installations also
check in the background at startup and can update themselves. Source builds
can read release information and stay under Cargo’s control.

The [Steelgum install page](https://brumley.info/steelgum/) offers native x64
Linux packages through `apt` or `dnf` on supported releases and a user-local
AppImage. To switch from the AppImage, run the System Install command again.

Run `python3 scripts/verify.py` for local verification, or add
`--headless-only --offline` to use cached dependencies without the
Bevy runner. See
[agent guidelines](AGENTS.md) for development rules and [release and
updates](docs/release.md) for the first public release plan.
