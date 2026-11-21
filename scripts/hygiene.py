"""Check repository boundaries and source size budgets."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
SOFT_LIMIT = 600
HARD_LIMIT = 1_000
SOURCE_EXTENSIONS = {".rs", ".py", ".sh", ".ps1", ".mjs", ".tcl"}


def source_sizes():
    listing = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
    )
    for name in sorted(set(listing.split(b"\0"))):
        path = ROOT / os.fsdecode(name)
        if path.suffix in SOURCE_EXTENSIONS and path.is_file():
            with path.open(encoding="utf-8") as source:
                yield path.relative_to(ROOT).as_posix(), sum(1 for _ in source)


def boundary_violations():
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT
        )
    )
    packages = {package["name"]: package for package in metadata["packages"]}
    engine = packages.get("steelgum")
    game = packages.get("door-game")
    if engine is None or game is None:
        yield "The headless workspace must contain the steelgum and door-game packages."
    elif not any(
        dependency["name"] == "steelgum"
        and dependency["kind"] is None
        and dependency.get("path") is not None
        and Path(dependency["path"]) == Path(engine["manifest_path"]).parent
        for dependency in game["dependencies"]
    ):
        yield "door-game must consume the workspace's steelgum package as a dependency."

    for package in packages.values():
        names = {package["name"], *(dep["name"] for dep in package["dependencies"])}
        for name in sorted(names):
            if name == "bevy" or name.startswith("bevy_"):
                yield f"{package['name']} brings {name} into the headless workspace."


def desktop_ui_violations():
    desktop = ROOT / "apps/steelgum-gpui/src"
    paths = [
        *desktop.joinpath("editor").rglob("*.rs"),
        *desktop.joinpath("ui").rglob("*.rs"),
        desktop / "viewport.rs",
        desktop / "viewport/bevy.rs",
    ]
    for path in paths:
        if path == desktop / "ui/tokens.rs":
            continue
        source = path.read_text(encoding="utf-8")
        checks = [
            (r"\brgba?\(\s*(?:0x[\da-fA-F]+|\d+)\s*\)", "Use an editor colour token"),
        ]
        if path != desktop / "ui/icon.rs":
            checks.append((r"\bsvg\s*\(", "Render editor icons through ui::Icon"))
        for pattern, help_text in checks:
            for match in re.finditer(pattern, source):
                line = source.count("\n", 0, match.start()) + 1
                yield f"{path.relative_to(ROOT)}:{line}: {help_text}."


def main():
    failures = [*boundary_violations(), *desktop_ui_violations()]
    for path, count in source_sizes():
        if count > HARD_LIMIT:
            failures.append(f"{path} has {count} lines (hard limit {HARD_LIMIT}).")
        elif count > SOFT_LIMIT:
            print(f"Plan a split for {path} ({count} lines, soft limit {SOFT_LIMIT}).")

    for failure in failures:
        print(f"Hygiene error: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("Hygiene passed.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Could not run hygiene: {error}", file=sys.stderr)
        sys.exit(1)
