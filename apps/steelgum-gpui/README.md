# Steelgum desktop review experiment

This source-only GPUI workspace opens directly into the Door Room example.
The packaged Steelgum application has its own executable and release process.

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
To reopen the same project after each restart, pass the worktree and scene
path to the watcher:

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

- Use the top-left menu button to collapse the tools. **Ctrl+K** opens and
  focuses **Find a tool**. Type part of a tool name and press **Enter** to
  open the first match.
- **Changes** opens the right pane. Select **Pressure plate** and switch
  between **Before** and **After** to see the colour change. **Objects**
  lists the selected revision's objects and shows the selection's dimensions.
  Click a row or the object in the viewport to select it. The selection
  stays in place when changing revisions.
- Use **Flag for Review** on the selection, add an optional note, then
  choose **Copy review request**. The clipboard text includes the scene
  path, stable object ID, exact Git base for a worktree, and changed object
  properties.
- Left-drag to orbit in **3D**. Middle-drag to pan and scroll to zoom in
  either view. Choose **2D** for a top-down view. **Focus Object** isolates the selected object. Closing either
  sidebar gives the viewport more space. The FPS readout counts newly
  rendered scene frames. It shows **FPS idle** while the viewport is still.
- **Build Game** opens the build pane. Choose **Debug** or **Release** and
  click **Build** to compile for the host. **Play** uses the selected profile
  and opens a separate game window. The bundled example plays the **After**
  scene. An open Door Game worktree plays the saved scene on disk.

**Open Folder** accepts a Git worktree with `scenes/room.json` or the
Door Game room in this repository. You can also pass another scene's path
relative to the Git worktree root:

```sh
cargo run --locked -- /path/to/worktree scenes/level.json
```

An open worktree refreshes after the scene file or Git `HEAD` changes. The
viewport keeps its camera and selected object while **Changes** updates.
An invalid or temporarily missing scene leaves the last valid scene visible
with an error. The next valid save clears the error. **Refresh** is also
available to request a fresh read.

Build and Play currently use `apps/door-game-bevy/Cargo.toml` in the
Steelgum checkout or opened worktree. Other supported worktrees can still be
opened for review. Build and Play execute the runner's Cargo project, so use
them with worktrees you trust.

Steelgum owns the comparison and stable authored IDs. GPUI controls the
window and passes camera input to the viewport. Bevy draws offscreen on
GPUI's wgpu device, and GPUI samples the resulting GPU texture. Linux uses
GPUI's normal wgpu renderer. The Windows experiment requests a wgpu renderer
for its Win32 window and that integration needs a native runtime check.
