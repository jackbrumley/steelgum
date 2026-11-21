# LibGodot renderer probe

This Linux developer tool exercises the official LibGodot 4.6 ABI. A host
loads LibGodot, starts Godot on the host's main thread, and requests two
views of a small 3D fixture. Godot renders each view into a SubViewport. The
host copies RGBA pixels from Godot-owned memory into a framed stdout stream.
Engine diagnostics go to stderr. Each frame has a little-endian sequence
number, width, and height followed by tightly packed RGBA8 pixels.

Run from the Steelgum root with a matching Godot 4.6 checkout. The same
checkout supplies the generated ABI headers and shared library. A graphical
X11 or Wayland session with a supported graphics driver is required. Godot's
headless display driver has no renderer.

```sh
scons -j1 platform=linuxbsd target=template_release library_type=shared_library \
  disable_path_overrides=no \
  optimize=none scu_build=yes scu_limit=8 modules_enabled_by_default=no \
  module_gdscript_enabled=yes module_text_server_fb_enabled=yes \
  module_glslang_enabled=yes
```

Run that build from the Godot checkout. It keeps both Linux graphics
backends and the modules used by the fixture.

```sh
godot_source="$HOME/Coding/godot-4.6"
build_dir=$(mktemp -d)
g++ -std=c++17 -Wall -Wextra -Werror -isystem "$godot_source" \
  -isystem "$godot_source/platform/linuxbsd" \
  tools/libgodot-probe/host.cpp -ldl -o "$build_dir/host"
"$build_dir/host" \
  "$godot_source/bin/libgodot.linuxbsd.template_release.x86_64.so" \
  "$PWD/tools/libgodot-probe" > "$build_dir/frames.bin"
python3 tools/libgodot-probe/check.py "$build_dir/frames.bin"
```

The checker verifies that both frames contain the fixture and that the camera
change produces different pixels. This isolates renderer startup and readback
before work on the interactive GPUI adapter. The next acceptance test is
switching renderers in the Steelgum viewport while dragging the camera and
scrubbing time of day.

For startup and ABI validation without a graphical display, pass `--abi-only`
after the project path. That check does not render.

To check GPUI presentation, run the ignored desktop test with the same host
and library. It loads the two frames from the host's stdout, uploads them as
images, and checks the pixels in the GPUI screenshots.

```sh
cd apps/steelgum-gpui
STEELGUM_LIBGODOT_HOST="$build_dir/host" \
STEELGUM_LIBGODOT="$godot_source/bin/libgodot.linuxbsd.template_release.x86_64.so" \
  cargo test paints_two_live_libgodot_frames_in_gpui -- --ignored --test-threads=1
```

## Live worker

Run the same host with `--worker` after the project path for the desktop
adapter. It keeps one Godot instance open and reads requests from stdin.
A request has three little-endian u32 fields (JSON size in bytes, width,
height) followed by UTF-8 scene JSON. Frame numbers start at zero and each
response reuses the probe's RGBA stream layout. The worker redraws on
request and has no continuous frame loop. Close stdin to shut it down.
`--worker-check` accepts a single request in headless mode, which can check
scene loading without a graphical session.

On Linux, set `STEELGUM_LIBGODOT_HOST` and `STEELGUM_LIBGODOT` before starting
Steelgum to make **Godot 4.6 (experimental)** available under **Settings →
Viewport → Preview renderer** in a viewport build that includes the adapter.
The worker uses a native graphics context window minimized by Godot.
The scene image and editor navigation are inside Steelgum. This preview
uses CPU frame transfer and currently draws scene boxes and lighting.
