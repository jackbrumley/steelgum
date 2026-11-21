# Steelgum agent guidelines

Start with [Architecture](docs/architecture.md) for the project goals and
decisions. [Release and updates](docs/release.md) records the distribution
plan and the remaining work before a public release.

Steelgum follows the shared [App Standard](https://github.com/jackbrumley/jackbrumley.github.io/blob/main/APP_STANDARD.md)
and [Release and Update Standard](https://github.com/jackbrumley/jackbrumley.github.io/blob/main/RELEASE_STANDARD.md).
Apply the relevant requirements at the point the feature ships. Record any
intentional Steelgum variation in the relevant project document. Steelgum
uses native Rust and Bevy, so the Tauri, WebKit and JavaScript application
implementation details do not apply.

## Working in the repository

- Assume the maintainer and multiple agents are working concurrently, including
  in the same checkout. Treat changes you did not make as intentional. Never
  discard, stash, or overwrite them to get a clean working tree.
- Stage and commit only your own changes. Review the staged diff before
  committing. If someone else has edited the same file, stage only your hunks
  and coordinate any overlap you cannot cleanly separate. Use separate worktrees
  where useful and coordinate changes to shared public contracts.
- Use `dev` for development once the default release and development branches
  are set up. Promote `main` from a verified release tag. Follow the shared
  standard for site-only exceptions. Commit, push, tag, or promote only when
  requested.
- Use Conventional Commit subjects and explain the reason for the change.
  Jack Brumley is the sole author of record. Keep AI attribution out of
  commits, tags, pull requests and release notes.
- Keep documentation alongside changes to a public API, scene format, command,
  or release process. Describe planned capabilities as plans. Every command
  described as available must exist and work from the documented entry point.

## Code and architecture

- Fix errors at their source. Validate files and other external data at their
  boundary. Report invalid data with context and surface operation failures.
  Silent fallback, swallowed errors and fabricated defaults hide defects.
- Give each rule or piece of state a single owner. Keep functions and modules
  small, use descriptive names, and explain the reason for non-obvious code.
  Remove dead code in the same change. Split by responsibility when a source
  file passes 600 lines. The hard limit is 1,000 lines.
- Change affected callers together. Avoid speculative abstractions, temporary
  compatibility shims and lint bypasses. Any narrow lint exception needs a
  specific reason beside it. A versioned migration for shipped project data is
  a supported feature and needs a test.
- Keep the project model, validation and gameplay tests independent of Bevy.
  Use Steelgum's authored data and stable IDs at public boundaries. Bevy types,
  entities, schedules and assets belong in the graphical runner.
- Keep gameplay rules in the game package. CLI, editor and eventual MCP tools
  must call the same Steelgum operations. A renderer consumes scene and game
  state. Its own objects cannot become a second source of project data.
- Keep changes reviewable in Git and keep authored scene data separate from
  play-session state. When persisted writes are introduced, validate the full
  edit before writing and detect a stale project revision before overwriting it.
- Prefer the standard library and maintained dependencies with suitable
  licences. Keep renderer and platform dependencies scoped to their package.
- Treat Windows and Linux as first-class platforms. Keep OS-specific code at
  the relevant boundary. Cover Wayland and X11 when testing Linux windowing.

## Release variations

Steelgum packages Linux x64 as `.deb`, `.rpm` and AppImage, and Windows x64
with a per-user installer. The native window uses a standard OS frame and
exposes Updates with F6 while there is no Settings window. Keep the development
preview and installed application separate. The packaged updater and fresh
installation share the same installers. See [Release and updates](docs/release.md).

## Verification

Run `python3 scripts/verify.py` before submitting a change. It runs the hygiene,
formatting, Clippy and test checks. CI checks Windows and Linux. Use
`python3 scripts/hygiene.py` for the repository rules alone. `--headless-only`
is useful when developing the simulation or when graphics dependencies are
unavailable. Report any checks the environment prevented you from running.

For desktop changes, run `cargo dev --build-only` with the normal toolchain
and system library paths before committing. The full verification script runs
this check too. Cargo check, Clippy, and builds that need a local linker
workaround cannot verify the normal desktop build.

Use deterministic simulation tests for game rules and failure cases. For
changes to input, rendering or capture, also run the native application on a
graphics-capable machine. A successful compilation or headless simulation
cannot verify what appears in a frame.
