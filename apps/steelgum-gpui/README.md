# Steelgum desktop review experiment

The native Steelgum workspace opens a project, previews its scenes, and
reviews authored changes against Git. Source development and packaged
installs share the workspace. Packaged installs include the Door Game player.

From the Steelgum checkout, enter this directory and run:

```sh
cd apps/steelgum-gpui
cargo run --locked
```

For automatic rebuilds, run this from the Steelgum repository root:

```sh
cargo dev
```

It watches the desktop source and Steelgum core. The previous window
stays open during the build and restarts once the build succeeds. A
compile error leaves the previous window open and shows diagnostics in
the terminal. Scene edits in an open worktree refresh inside the window.
Press **Ctrl+C** to stop the watcher.

On Linux, the watcher uses `taskset` and `nice` to build at lower priority
on at most half the available logical CPUs. On a single-CPU system it uses
one lower-priority job. The desktop window itself uses the normal CPU
settings. To leave more room for other apps, run `STEELGUM_BUILD_CPUS=2
cargo dev`. On Windows the watcher limits Cargo jobs to half the available
logical CPUs.
To open a project on launch, pass its worktree to the watcher. An optional
second argument selects a scene. Launching without arguments reopens the
last project.

```sh
cargo dev /path/to/worktree scenes/room.json
```

Both commands use Cargo and the Rust toolchain. The installed Steelgum
application runs directly.

The directory pins Rust 1.99.1. On Debian-based Linux systems, the build uses
`pkg-config`, `libxcb1-dev`, `libfreetype-dev`, `libfontconfig1-dev`,
`libwayland-dev`, `libxkbcommon-dev` and `libxkbcommon-x11-dev`. To open the
window you need a graphical session and a supported GPU adapter. Use the MSVC
Rust target and Windows build tools for the Windows experiment.

Run `cargo dev --build-only` from the repository root to build and link the
same binary the watcher starts, without opening a window. The full repository
verification runs this command. If the linker reports `-lxkbcommon-x11`
missing on Linux Mint, Ubuntu, or Debian, install the development package:

```sh
sudo apt install libxkbcommon-x11-dev
```

## Open and review a game

- Use the top-left menu button to collapse the tools. **Ctrl+K** opens and
  focuses **Find a tool**. Type part of a tool name and press **Enter** to
  open the first match.
- **Changes** opens the right pane. Select **Pressure plate** and switch
  between **Before** and **After** to see the colour change. **Objects**
  lists the selected revision's objects and shows the selection's dimensions.
  Click a row or viewport object to select it. Use Up, Down, Home, and End in
  the focused list. Press F2 in After, or choose Rename, to edit its display
  name. Enter saves, and Escape cancels. Saves preserve the stable object ID
  and reject changes when the scene on disk differs from the loaded copy.
- Use **Flag for Review** on the selection, add an optional note, then
  choose **Copy review request**. The clipboard text includes the scene
  path, stable object ID, exact Git base for a worktree, and changed object
  properties. Select **Include viewport screenshot** to attach the path to
  a captured PNG. **Assets** browses files under the project asset directory
  and previews PNG and JPEG images.
- Left-drag to orbit in **3D**. Middle-drag to pan and scroll to zoom in
  either view. Choose **2D** for a top-down view. **Focus Object** isolates the selected object. Closing either
  sidebar gives the viewport more space. The FPS readout counts newly
  rendered scene frames. It shows **FPS idle** while the viewport is still.
- **Build Game** opens the build pane. Choose **Debug** or **Release** and
  click **Build**. The build displays Cargo diagnostics and supports **Cancel
  Build**. **Play** builds the project and starts its game window. The toolbar
  changes to **Stop** while the play session is active. Linux builds share the
  development watcher's CPU budget. The bundled example plays without Cargo.
- Press **F6** in the installed workspace to check for updates.

**Open Folder** accepts a Git worktree with a `steelgum.json` manifest.
Legacy Door Game worktrees and worktrees with `scenes/room.json` can also
be opened. You can pass a scene path relative to the worktree:

```sh
cargo run --locked -- /path/to/worktree scenes/level.json
```

An open worktree refreshes after the scene file or Git `HEAD` changes. The
viewport keeps its camera and selected object while **Changes** updates.
An invalid or temporarily missing scene leaves the last valid scene visible
with an error. The next valid save clears the error. **Refresh** is also
available to request a fresh read.

## Project manifest

Place `steelgum.json` in the game repository root:

```json
{
  "version": 1,
  "name": "Signal Room",
  "scene": "scenes/control-room.json",
  "assets": "assets",
  "runner": { "manifest": "Cargo.toml", "binary": "signal-room" }
}
```

Scene, asset, and runner paths are relative to the Git worktree. Commit a
baseline scene before opening it. The runner is optional for review-only
projects. When present, Steelgum builds the selected Cargo binary and
launches it from the worktree with `--scene` and the saved scene path.
Build and Play run the project's code and require a compatible Rust
toolchain. Use them with worktrees you trust. Reopen the folder after editing
its manifest.

## Agent commands

The installed `steelgum` executable provides a local JSON interface for
coding agents. These commands open the same comparison used by the
workspace and do not require a graphical session.

```sh
steelgum --agent inspect /path/to/game
steelgum --agent select /path/to/game --object beacon
steelgum --agent review /path/to/game --object beacon --note "Check the beacon visibility"
```

Use `--before` to inspect the Git base or `--scene` to choose another
worktree-relative scene. The response includes a protocol version, Git base,
and selected scene. Agents can make changes in the worktree and see them
appear live in the open workspace.

Steelgum owns the comparison and stable authored IDs. GPUI controls the
window and passes camera input to the viewport. Bevy draws offscreen on
GPUI's wgpu device, and GPUI samples the resulting GPU texture. Linux uses
GPUI's normal wgpu renderer. The Windows experiment requests a wgpu renderer
for its Win32 window and that integration needs a native runtime check.
