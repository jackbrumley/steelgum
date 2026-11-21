"""Run the same verification commands locally and in CI."""

import argparse
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
RENDERER = Path("apps/door-game-bevy/Cargo.toml")


def workspace_checks(manifest, lock_flags):
    flags = ["--manifest-path", manifest.as_posix()]
    yield ["cargo", "fmt", *flags, "--all", "--check"]
    yield [
        "cargo", "clippy", *flags, "--workspace", *lock_flags,
        "--all-targets", "--", "-D", "warnings",
    ]
    yield ["cargo", "test", *flags, "--workspace", *lock_flags]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--headless-only", action="store_true", help="Skip the Bevy runner.")
    parser.add_argument("--offline", action="store_true", help="Use cached Cargo dependencies.")
    args = parser.parse_args()

    env = os.environ.copy()
    if args.offline:
        env["CARGO_NET_OFFLINE"] = "true"

    checks = [
        [sys.executable, "scripts/hygiene.py"],
        *workspace_checks(Path("Cargo.toml"), ["--locked"]),
        ["cargo", "run", "--locked", "-p", "door-game", "--", "--demo"],
    ]
    if not args.headless_only:
        checks.extend(workspace_checks(RENDERER, []))

    for command in checks:
        print(f"+ {' '.join(command)}", flush=True)
        subprocess.run(command, cwd=ROOT, env=env, check=True)

    scope = "Headless verification" if args.headless_only else "Verification"
    print(f"{scope} passed.")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
    except OSError as error:
        print(f"Could not run verification: {error}", file=sys.stderr)
        sys.exit(1)
