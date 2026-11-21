"""Rebuild and restart the GPUI workspace when its source files change."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "steelgum-gpui"
CORE = ROOT / "crates" / "steelgum"
POLL_INTERVAL = 0.25


def source_snapshot():
    paths = [
        APP / "Cargo.toml",
        APP / "Cargo.lock",
        APP / "rust-toolchain.toml",
        APP / "build.rs",
        CORE / "Cargo.toml",
        ROOT / "examples" / "door-game" / "scenes" / "room.json",
        *(APP / "src").rglob("*"),
        *(CORE / "src").rglob("*"),
    ]
    snapshot = {}
    for path in paths:
        try:
            stat = path.stat()
        except FileNotFoundError:
            # Atomic saves can remove a path between listing and reading it.
            continue
        if path.is_file():
            snapshot[path] = stat.st_mtime_ns, stat.st_size
    return snapshot


def build():
    result = subprocess.run(
        ["cargo", "build", "--locked", "--bin", "steelgum-gpui",
         "--message-format=json-render-diagnostics"],
        cwd=APP,
        stdout=subprocess.PIPE,
        encoding="utf-8",
        check=False,
    )
    if result.returncode:
        return None
    for line in result.stdout.splitlines():
        if not line.startswith("{"):
            print(line, flush=True)
            continue
        message = json.loads(line)
        if (
            message.get("reason") == "compiler-artifact"
            and message["target"]["name"] == "steelgum-gpui"
            and message.get("executable")
        ):
            return Path(message["executable"])
    raise RuntimeError("Cargo built Steelgum but reported no executable.")


def stop(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def watch(arguments, run_directory=None):
    process = None
    previous = None
    try:
        while True:
            current = source_snapshot()
            if current != previous:
                previous = current
                print("Building Steelgum…", flush=True)
                executable = build()
                if executable and source_snapshot() == current:
                    stop(process)
                    if run_directory is not None:
                        # Windows locks the running executable. Cargo rebuilds its
                        # own copy while the last successful build stays open.
                        executable = Path(shutil.copy2(executable, run_directory))
                    print("Starting Steelgum. Press Ctrl+C to stop watching.", flush=True)
                    process = subprocess.Popen([str(executable), *arguments], cwd=APP)
                elif not executable:
                    print("Build failed. Waiting for the next save.", flush=True)
            time.sleep(POLL_INTERVAL)
    finally:
        stop(process)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worktree", type=Path, nargs="?")
    parser.add_argument("scene", nargs="?", help="scene path relative to the worktree")
    args = parser.parse_args()
    if (args.worktree is None) != (args.scene is None):
        parser.error("pass both a worktree and scene path, or neither to open the example")
    arguments = [] if args.worktree is None else [str(args.worktree.resolve()), args.scene]
    try:
        if os.name == "nt":
            with tempfile.TemporaryDirectory(prefix="steelgum-dev-") as directory:
                watch(arguments, directory)
        else:
            watch(arguments)
    except KeyboardInterrupt:
        return
    except (OSError, RuntimeError) as error:
        parser.exit(1, f"Steelgum dev: {error}\n")


if __name__ == "__main__":
    main()
