"""Exercise the Linux installer with deterministic GitHub release responses."""

import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "jackbrumley/steelgum"
APP_ID = "info.brumley.steelgum"


def appimage_fixture(tag):
    return f"""#!/bin/sh
set -eu
test "$1" = --appimage-extract
mkdir -p -- "squashfs-root/$(dirname "$2")"
printf '%s\\n' '<svg data-release="{tag}" xmlns="http://www.w3.org/2000/svg"/>' > "squashfs-root/$2"
""".encode()


def mock_curl(args):
    tag = os.environ["STEELGUM_TEST_TAG"]
    digest_type = os.environ["STEELGUM_TEST_DIGEST"]
    data = appimage_fixture(tag)
    asset = f"steelgum-{tag[1:]}-linux-x64.AppImage"
    api = f"https://api.github.com/repos/{REPOSITORY}/releases"
    url = f"https://github.com/{REPOSITORY}/releases/download/{tag}/{asset}"
    if any(args[args.index(flag) + 1] != "=https" for flag in ("--proto", "--proto-redir")):
        raise SystemExit("The installer did not restrict transport and redirects to HTTPS.")

    endpoint = args[-1]
    if endpoint == f"{api}/latest":
        print(json.dumps({"draft": False, "prerelease": False, "tag_name": tag}))
    elif endpoint == f"{api}/tags/{tag}":
        release_asset = {"name": asset, "browser_download_url": url}
        if digest_type == "valid":
            release_asset["digest"] = f"sha256:{hashlib.sha256(data).hexdigest()}"
        elif digest_type == "wrong":
            release_asset["digest"] = "sha256:" + "0" * 64
        elif digest_type != "missing":
            raise SystemExit(f"Unknown digest fixture: {digest_type}.")
        print(json.dumps({"draft": False, "tag_name": tag, "assets": [release_asset]}))
    elif endpoint == url:
        Path(args[args.index("--output") + 1]).write_bytes(data)
    else:
        raise SystemExit(f"Unexpected URL: {endpoint}.")


def run_script(name, env, *args):
    return subprocess.run(
        ["bash", str(ROOT / "docs" / name), *args],
        env=env,
        check=False,
        capture_output=True,
        text=True,
        timeout=15,
    )


def check_result(result, status, message):
    output = result.stdout + result.stderr
    if result.returncode != status or message not in output:
        raise AssertionError(f"Expected exit {status} and {message!r}, got {result.returncode}:\n{output}")


def test_installer():
    with tempfile.TemporaryDirectory(prefix="steelgum-install-") as temporary:
        folder = Path(temporary)
        home = folder / "home with spaces %"
        commands = folder / "bin"
        commands.mkdir()
        curl = commands / "curl"
        command = shlex.join([sys.executable, str(Path(__file__).resolve()), "--mock-curl"])
        curl.write_text(f'#!/bin/sh\nexec {command} "$@"\n', encoding="utf-8")
        curl.chmod(0o755)

        env = os.environ.copy()
        for key in ("STEELGUM_UPDATE_LOG", "STEELGUM_GITHUB_TOKEN"):
            env.pop(key, None)
        env.update(
            HOME=str(home),
            XDG_CONFIG_HOME=str(folder / "config"),
            XDG_DATA_HOME=str(folder / "data"),
            PATH=os.pathsep.join([str(commands), os.environ["PATH"]]),
        )

        installed = home / ".local" / "bin" / "steelgum"
        launcher = folder / "data" / "applications" / f"{APP_ID}.desktop"
        icon = folder / "data" / "icons" / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg"
        debug_log = folder / "config" / "steelgum" / "debug" / "update.log"
        initial_tag = "v0.1.0"
        next_tag = "v0.2.0"
        args = ("--appimage", "--yes", "--no-relaunch")

        def install(tag, digest, *options):
            response = env | {"STEELGUM_TEST_TAG": tag, "STEELGUM_TEST_DIGEST": digest}
            return run_script("install.sh", response, *args, *options)

        check_result(install(initial_tag, "valid"), 0, "SHA-256 verified.")
        if installed.read_bytes() != appimage_fixture(initial_tag):
            raise AssertionError("The initial verified package was not installed.")
        if not launcher.is_file() or initial_tag not in icon.read_text(encoding="utf-8"):
            raise AssertionError("The initial icon or launcher was not installed.")

        for digests, failure in (
            ("wrong", "SHA-256 mismatch"),
            ("missing", "did not supply a valid SHA-256 digest"),
        ):
            check_result(install(next_tag, digests), 1, failure)
            if installed.read_bytes() != appimage_fixture(initial_tag):
                raise AssertionError("A failed update replaced the original executable.")
            if initial_tag not in icon.read_text(encoding="utf-8"):
                raise AssertionError("A failed update replaced the original icon.")
            if any(installed.parent.glob(".steelgum.*")):
                raise AssertionError("A failed update left a temporary executable behind.")

        check_result(install(next_tag, "valid", "--version", next_tag), 0, "SHA-256 verified.")
        if installed.read_bytes() != appimage_fixture(next_tag):
            raise AssertionError("The verified update did not replace the old package.")
        if next_tag not in icon.read_text(encoding="utf-8"):
            raise AssertionError("The verified update did not replace the old icon.")

        check_result(run_script("uninstall.sh", env, "--yes"), 0, "Removed Steelgum.")
        if any(path.exists() for path in (installed, launcher, icon)):
            raise AssertionError("Uninstall did not remove every installed file.")
        if debug_log.read_text(encoding="utf-8").count("SHA-256 verified.") != 2:
            raise AssertionError("The update log did not preserve both verified installs.")

    print("Linux installer tests passed.")


if __name__ == "__main__":
    if sys.argv[1:2] == ["--mock-curl"]:
        mock_curl(sys.argv[2:])
    elif len(sys.argv) == 1:
        test_installer()
    else:
        raise SystemExit("Usage: python3 scripts/test_install.py")
