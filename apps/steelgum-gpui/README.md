# Steelgum desktop review experiment

This source-only experiment opens a GPUI window with a Changes panel and a
live 3D view of the selected scene revision. The packaged Steelgum application
has its own executable and release process.

From the Steelgum checkout, enter this directory and run:

```sh
cd apps/steelgum-gpui
cargo run --locked
```

The directory pins Rust 1.99.1. On Debian-based Linux systems, the build uses
`pkg-config`, `libxcb1-dev`, `libfreetype-dev`, `libfontconfig1-dev`,
`libwayland-dev`, `libxkbcommon-dev` and `libxkbcommon-x11-dev`. To open the
window you need a graphical session and a supported GPU adapter. Use the MSVC
Rust target and Windows build tools for the Windows experiment.

Select **Pressure plate**, switch **Before** and **After**, and drag to orbit
the room. Scroll to zoom. **Focus Selection** isolates the selected object.

**Open Folder** accepts a Git worktree with `scenes/room.json` or the
Door Game room in this repository. You can also pass another scene's path
relative to the Git worktree root:

```sh
cargo run --locked -- /path/to/worktree scenes/level.json
```

**Refresh** reads the file on disk again and compares it with the same scene
at the current `HEAD`. Opening or refreshing a scene reads and validates data.
The Changes panel shows per-object scene differences.

Steelgum owns the comparison and stable authored IDs. GPUI controls the
window and passes camera input to the viewport. Bevy draws offscreen on
GPUI's wgpu device, and GPUI samples the resulting GPU texture. Linux uses
GPUI's normal wgpu renderer. The Windows experiment requests a wgpu renderer
for its Win32 window and that integration needs a native runtime check.
