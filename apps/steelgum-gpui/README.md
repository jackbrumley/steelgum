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

It watches the desktop source, Steelgum core, and bundled demo scene. A
successful build restarts the window. A compile error leaves the previous
window open, with diagnostics in the terminal. Press **Ctrl+C** to stop.
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

- Use the top-left menu button to collapse the tools. **Ctrl+K** opens and
  focuses **Find a tool**. Type part of a tool name and press **Enter** to
  open the first match.
- **Changes** opens the right pane. Select **Pressure plate** and switch
  between **Before** and **After** to see the colour change. Use **Scene**
  or **Assets** to browse objects in the selected revision. Selecting an
  object highlights its row and outlines it in the viewport. **Assets**
  shows the selected object's dimensions below the list.
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

**Refresh** reads the file on disk again and compares it with the same scene
at the current `HEAD`. Opening or refreshing a scene reads and validates data.
The Changes panel shows per-object scene differences.

Build and Play currently use `apps/door-game-bevy/Cargo.toml` in the
Steelgum checkout or opened worktree. Other supported worktrees can still be
opened for review. Build and Play execute the runner's Cargo project, so use
them with worktrees you trust.

Steelgum owns the comparison and stable authored IDs. GPUI controls the
window and passes camera input to the viewport. Bevy draws offscreen on
GPUI's wgpu device, and GPUI samples the resulting GPU texture. Linux uses
GPUI's normal wgpu renderer. The Windows experiment requests a wgpu renderer
for its Win32 window and that integration needs a native runtime check.
