# Steelgum architecture

Steelgum is a hobby project to learn game development by building a small
framework around established engine components. Its long-term purpose is to
make sophisticated 3D games practical to build in the same iterative way as a
normal software project. An agent should be able to open a game repo, edit
code or a scene, run the game, inspect the result, make a correction and hand
back a reproducible Git diff. Humans use the same project data and commands.
Windows and Linux are both primary targets. Linux Mint is the maintainer's
daily development environment.

## Ownership and dependencies

```text
Game source, scenes, assets and tests
    |
    v
Steelgum project model and commands  <--- future editor and MCP
    |
    |  scene data and gameplay state
    v
Bevy runner  ---------------------------> window and screenshot
```

- `crates/steelgum` owns the versioned scene model and validation. Stable
  authored IDs identify objects across loads, edits and backend instances.
  A project can be loaded and validated in a headless process.
- `examples/door-game` is a separate package that uses Steelgum's public API.
  It owns the pressure plate, door and other game rules. Keep the game in the
  same repository while the API is taking shape. Move or copy it to a sibling
  repo when it is useful to test the external consumer experience.
- `apps/door-game-bevy` maps the scene and current game state to Bevy and
  handles native input, display and capture. Its manifest is separate so
  headless Cargo operations do not resolve the graphics dependency graph.
  Bevy 0.20.1 is the first integration. Use upstream as a dependency.
- `apps/door-game-preview` draws an isometric developer preview with Tcl/Tk.
  It gets the scene and live state from the same Rust game using a local
  subprocess protocol, so it can run on a development machine without Bevy.
- `crates/steelgum-update` owns desktop update discovery, logging and handoff
  to the installers. It has no dependency on the game or renderer.

The persisted scene owns authored data. A play session owns its changing
state. The graphical runner projects that state into Bevy's world. Replacing
the renderer later means implementing the features the game renders through
Steelgum. Replacing an entire simulation runtime would also require matching
gameplay, timing, input and physics behaviour. Keep that decision open while
learning from the first game.

## First milestone

The first example is a room that opens a door when the player steps on a
pressure plate. Its authored file is `examples/door-game/scenes/room.json`.
The game runs a fixed 20 ms simulation step. The headless CLI can load an
edited scene, run a scripted playthrough and report its final state as JSON.
The Bevy runner consumes the same scene and game logic, with an interactive
window and a separate offscreen capture mode. Acceptance includes running
the room on graphics-capable Linux and Windows machines, inspecting the PNG
and checking the controls and goal feedback.

The public API only needs concepts the example actually uses. Keep GPUI,
networking, editor tooling, full physics, generated assets and a second
renderer outside this milestone. GPUI can be evaluated when a native editor
needs to embed a viewport. Bevy BRP MCP, Bevy MCP Harness and Godot MCP are
references for existing capability and possible focused reuse. Steelgum owns
the editing contract and project workflow.

## Parallel authoring direction

Start with normal Git worktrees and readable text scenes. Later, give each
agent a known base revision and an isolated preview. Changes should be
validated before commit. Accept independent changes against the same base
where possible and report overlapping changes or broken references explicitly.
Tie each capture or simulation report to the scene revision it used so results
can be reviewed after an edit lands. Design CLI, MCP and editor clients as
thin callers of a shared command path.

This is a direction for later authoring work. The first milestone focuses on
one game, one backend, and the ability to check the same logic with and without
a window. [Release and updates](release.md) records the desktop distribution
plan.

The GPUI workspace compares authored scene revisions using stable object IDs.
Viewport clicks resolve to authored objects through the renderer adapter.
The review request copies an optional user note, scene path, selected ID,
resolved Git base commit, and per-object property changes to the clipboard.
The workspace watches the open scene file and Git HEAD, coalesces saves,
and replaces the comparison after both revisions validate. Failed reads
leave the last valid comparison in place. Refresh uses the same validation
path. Worktree changes leave build and play under explicit user control.

## Project workspace

The versioned `steelgum.json` manifest locates the starting scene, optional
assets, and a Cargo binary for the game. Desktop and agent commands use the
same manifest discovery and Git comparison. The agent JSON interface
inspects projects, resolves selected objects by stable ID, and formats review
requests.

Scene names are optional display labels. Renaming uses the loaded worktree
bytes to detect stale edits and writes a validated scene atomically. IDs
remain the keys referenced by gameplay and Git comparison. Assets are files
under a separate project directory and are previewed by the workspace.

The installed workspace launches a sibling player for its bundled example.
Project builds use the shared Cargo CPU budget, and play sessions launch the
built binary with the saved scene.
