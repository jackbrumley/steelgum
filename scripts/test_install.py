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
    formats = {"AppImage": appimage_fixture(tag)}
    if os.environ.get("STEELGUM_TEST_NATIVE"):
        formats["deb"] = b"test deb package\n"
    api = f"https://api.github.com/repos/{REPOSITORY}/releases"
    assets = []
    downloads = {}
    for extension, data in formats.items():
        name = f"steelgum-{tag[1:]}-linux-x64.{extension}"
        url = f"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"
        asset = {"name": name, "browser_download_url": url}
        if digest_type == "valid":
            asset["digest"] = f"sha256:{hashlib.sha256(data).hexdigest()}"
        elif digest_type == "wrong":
            asset["digest"] = "sha256:" + "0" * 64
        elif digest_type != "missing":
            raise SystemExit(f"Unknown digest fixture: {digest_type}.")
        assets.append(asset)
        downloads[url] = data
    if any(args[args.index(flag) + 1] != "=https" for flag in ("--proto", "--proto-redir")):
        raise SystemExit("The installer did not restrict transport and redirects to HTTPS.")

    endpoint = args[-1]
    if endpoint == f"{api}/latest":
        print(json.dumps({"draft": False, "prerelease": False, "tag_name": tag}))
    elif endpoint == f"{api}/tags/{tag}":
        print(json.dumps({"draft": False, "tag_name": tag, "assets": assets}))
    elif endpoint in downloads:
        Path(args[args.index("--output") + 1]).write_bytes(downloads[endpoint])
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
        def mock(name, body):
            path = commands / name
            path.write_text(f"#!/bin/sh\nset -eu\n{body}\n", encoding="utf-8")
            path.chmod(0o755)

        command = shlex.join([sys.executable, str(Path(__file__).resolve()), "--mock-curl"])
        mock("curl", f'exec {command} "$@"')
        mock("sudo", 'test "$1" = -n\nshift\nexec "$@"')
        mock("dpkg-query", "exit 1")
        mock("apt-get", '''if [ "$1" = update ]; then exit 0; fi
test "$1" = install
for package do :; done
test "$(cat -- "$package")" = 'test deb package'
printf '%s\\n' "$package" > "$STEELGUM_TEST_APT_CALL"
exit 42''')

        env = os.environ.copy()
        for key in ("STEELGUM_UPDATE_LOG", "STEELGUM_GITHUB_TOKEN"):
            env.pop(key, None)
        env.update(
            HOME=str(home),
            XDG_CONFIG_HOME=str(folder / "config"),
            XDG_DATA_HOME=str(folder / "data"),
            STEELGUM_TEST_APT_CALL=str(folder / "apt-call"),
            PATH=os.pathsep.join([str(commands), os.environ["PATH"]]),
        )

        installed = home / ".local" / "bin" / "steelgum"
        launcher = folder / "data" / "applications" / f"{APP_ID}.desktop"
        icon = folder / "data" / "icons" / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg"
        debug_log = folder / "config" / "steelgum" / "debug" / "update.log"
        initial_tag = "v0.1.0"
        next_tag = "v0.2.0"
        args = ("--yes", "--no-relaunch")

        def install(tag, digest, *options):
            response = env | {"STEELGUM_TEST_TAG": tag, "STEELGUM_TEST_DIGEST": digest}
            return run_script("install.sh", response, *args, *options)

        check_result(install(initial_tag, "valid"), 0, "has no deb package. Using the AppImage.")
        if installed.read_bytes() != appimage_fixture(initial_tag):
            raise AssertionError("The initial verified package was not installed.")
        if not launcher.is_file() or initial_tag not in icon.read_text(encoding="utf-8"):
            raise AssertionError("The initial icon or launcher was not installed.")

        for digests, failure in (
            ("wrong", "SHA-256 mismatch"),
            ("missing", "did not supply a valid SHA-256 digest"),
        ):
            check_result(install(next_tag, digests, "--appimage"), 1, failure)
            if installed.read_bytes() != appimage_fixture(initial_tag):
                raise AssertionError("A failed update replaced the original executable.")
            if initial_tag not in icon.read_text(encoding="utf-8"):
                raise AssertionError("A failed update replaced the original icon.")
            if any(installed.parent.glob(".steelgum.*")):
                raise AssertionError("A failed update left a temporary executable behind.")

        check_result(install(initial_tag, "valid", "--system"), 1, "does not provide a supported package (deb)")
        native = env | {"STEELGUM_TEST_TAG": next_tag, "STEELGUM_TEST_NATIVE": "1"}
        bad_native = native | {"STEELGUM_TEST_DIGEST": "wrong"}
        check_result(run_script("install.sh", bad_native, *args), 1, "SHA-256 mismatch")
        if (folder / "apt-call").exists():
            raise AssertionError("An unverified package reached apt.")
        verified_native = native | {"STEELGUM_TEST_DIGEST": "valid"}
        check_result(run_script("install.sh", verified_native, *args), 1, "apt could not install Steelgum")
        if not (folder / "apt-call").read_text(encoding="utf-8").strip().endswith("-linux-x64.deb"):
            raise AssertionError("Default install selected the wrong asset for apt.")
        if installed.read_bytes() != appimage_fixture(initial_tag) or not launcher.is_file():
            raise AssertionError("Failed migration removed the existing user install.")

        check_result(run_script("install.sh", verified_native, *args, "--appimage", "--version", next_tag), 0, "SHA-256 verified.")
        if installed.read_bytes() != appimage_fixture(next_tag):
            raise AssertionError("The verified update did not replace the old package.")
        if next_tag not in icon.read_text(encoding="utf-8"):
            raise AssertionError("The verified update did not replace the old icon.")

        check_result(run_script("uninstall.sh", env, "--yes"), 0, "Removed Steelgum.")
        if any(path.exists() for path in (installed, launcher, icon)):
            raise AssertionError("Uninstall did not remove every installed file.")
        if debug_log.read_text(encoding="utf-8").count("SHA-256 verified.") != 3:
            raise AssertionError("The update log did not preserve the verified install attempts.")

    print("Linux installer tests passed.")


if __name__ == "__main__":
    if sys.argv[1:2] == ["--mock-curl"]:
        mock_curl(sys.argv[2:])
    elif len(sys.argv) == 1:
        test_installer()
    else:
        raise SystemExit("Usage: python3 scripts/test_install.py")
