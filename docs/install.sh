#!/usr/bin/env bash
set -euo pipefail

repo='jackbrumley/steelgum'
app_id='info.brumley.steelgum'
tag=''
relaunch=true
[[ ${STEELGUM_NO_RELAUNCH:-} == 1 ]] && relaunch=false
confirm=true
verify=true

usage() {
  cat <<'USAGE'
Steelgum installer for x64 Linux

Usage: install.sh [--appimage] [--version vX.Y.Z] [--yes] [--relaunch | --no-relaunch] [--insecure-skip-verify]

Installs a user AppImage to ~/.local/bin/steelgum. Use --version to select a
release instead of installing the latest. Use --no-relaunch for headless installs.
STEELGUM_GITHUB_TOKEN can supply a token for GitHub API requests.
STEELGUM_NO_RELAUNCH=1 suppresses launch unless --relaunch is passed.
USAGE
}

fail() { printf '[steelgum] ERROR: %s\n' "$*" >&2; exit 1; }
log() { printf '[steelgum] %s\n' "$*"; }
valid_tag() { [[ "$1" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; }

while (( $# )); do
  case "$1" in
    --appimage) ;;
    --system) fail 'Steelgum currently ships a per-user AppImage only.' ;;
    --version) (( $# > 1 )) || fail '--version requires a release tag'; tag="$2"; shift
      valid_tag "$tag" || fail "Invalid release tag: $tag" ;;
    --yes) confirm=false ;;
    --relaunch) relaunch=true ;;
    --no-relaunch) relaunch=false ;;
    --insecure-skip-verify) verify=false ;;
    -h|--help) usage; exit 0 ;;
    *) fail "Unknown option: $1" ;;
  esac
  shift
done

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || fail 'Only x64 Linux is supported.'
for command in curl python3 sha256sum flock tee pgrep; do
  command -v "$command" >/dev/null || fail "Required command is missing: $command"
done
if [[ "$relaunch" == true ]]; then
  [[ -n ${DISPLAY:-} || -n ${WAYLAND_DISPLAY:-} ]] ||
    fail 'No graphical session is available. Re-run with --no-relaunch.'
  command -v setsid >/dev/null || fail 'Relaunch requires setsid.'
fi

config_home="${XDG_CONFIG_HOME:-}"
[[ "$config_home" == /* ]] || config_home="$HOME/.config"
data_home="${XDG_DATA_HOME:-}"
[[ "$data_home" == /* ]] || data_home="$HOME/.local/share"
debug_dir="$config_home/steelgum/debug"
bin_dir="$HOME/.local/bin"
desktop_dir="$data_home/applications"
icon_dir="$data_home/icons/hicolor/scalable/apps"
installed="$bin_dir/steelgum"
launcher="$desktop_dir/$app_id.desktop"
icon="$icon_dir/$app_id.svg"
mkdir -p -- "$debug_dir" "$bin_dir" "$desktop_dir" "$icon_dir"
log_file="$debug_dir/update.log"
touch -- "$log_file" || fail "Cannot write the update log: $log_file"
if [[ ${STEELGUM_UPDATE_LOG:-} != "$log_file" ]]; then
  exec > >(tee -a -- "$log_file") 2>&1
fi
exec 9>"$debug_dir/install.lock"
flock -n 9 || fail 'Another Steelgum installation or removal is running.'
app_stage=''
launcher_stage=''
icon_stage=''
extract_dir=''
cleanup() {
  local status=$?
  if [[ -n "$app_stage" ]]; then rm -f -- "$app_stage"; fi
  if [[ -n "$launcher_stage" ]]; then rm -f -- "$launcher_stage"; fi
  if [[ -n "$icon_stage" ]]; then rm -f -- "$icon_stage"; fi
  if [[ -n "$extract_dir" ]]; then rm -rf -- "$extract_dir"; fi
  if (( status != 0 )); then log "Installer stopped (exit $status)."; fi
}
trap cleanup EXIT
log "Installer started at $(date --iso-8601=seconds)"

if [[ "$confirm" == true && -t 0 ]]; then
  read -r -p "Install Steelgum to $installed? [y/N] " answer
  [[ "$answer" == [yY] ]] || fail 'Installation cancelled.'
fi

transport=(curl --silent --show-error --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 --connect-timeout 10)
headers=(-H 'Accept: application/vnd.github+json' -H 'User-Agent: steelgum-installer')
if [[ -n ${STEELGUM_GITHUB_TOKEN:-} ]]; then
  [[ ! "$STEELGUM_GITHUB_TOKEN" =~ [[:cntrl:]] ]] || fail 'Invalid GitHub token.'
  headers+=(-H "Authorization: Bearer $STEELGUM_GITHUB_TOKEN")
fi
api="https://api.github.com/repos/$repo/releases"

if [[ -z "$tag" ]]; then
  tag="$("${transport[@]}" --max-time 45 "${headers[@]}" "$api/latest" |
    python3 -c '
import json, sys

release = json.load(sys.stdin)
if release["draft"] is not False or release["prerelease"] is not False:
    sys.exit("GitHub returned a draft or prerelease")
print(release["tag_name"])
')" ||
    fail 'Could not read the latest stable Steelgum release from GitHub.'
  valid_tag "$tag" || fail 'GitHub returned an invalid release tag.'
fi

asset="steelgum-${tag#v}-linux-x64.AppImage"
url="https://github.com/$repo/releases/download/$tag/$asset"
digest="$("${transport[@]}" --max-time 45 "${headers[@]}" "$api/tags/$tag" |
  python3 -c '
import json, sys

tag, name, url = sys.argv[1:]
release = json.load(sys.stdin)
if release["tag_name"] != tag or release["draft"] is not False:
    sys.exit("GitHub returned a different release or a draft")
matches = [asset for asset in release["assets"] if asset["name"] == name]
if len(matches) != 1 or matches[0]["browser_download_url"] != url:
    sys.exit(f"Expected one release asset at {url}")
print(matches[0].get("digest") or "")
' "$tag" "$asset" "$url")" || fail "Could not identify $asset on GitHub."

expected=''
if [[ "$verify" == true ]]; then
  [[ "$digest" =~ ^sha256:([[:xdigit:]]{64})$ ]] || fail "GitHub did not supply a valid SHA-256 digest for $asset."
  expected="${BASH_REMATCH[1],,}"
else
  log 'WARNING: checksum verification disabled by request.'
fi

app_stage="$(mktemp -- "$bin_dir/.steelgum.XXXXXXXX")"
launcher_stage="$(mktemp -- "$desktop_dir/.steelgum.XXXXXXXX")"
icon_stage="$(mktemp -- "$icon_dir/.steelgum.XXXXXXXX")"
log "Downloading $asset"
"${transport[@]}" --max-time 600 --output "$app_stage" "$url" || fail "Download failed for $asset."
[[ -s "$app_stage" ]] || fail "GitHub returned an empty asset for $asset."

if [[ "$verify" == true ]]; then
  actual="$(sha256sum < "$app_stage")"
  [[ "${actual%% *}" == "$expected" ]] || fail "SHA-256 mismatch for $asset."
  log 'SHA-256 verified.'
fi

chmod 0755 -- "$app_stage"
extract_dir="$(mktemp -d)"
image_icon="usr/share/icons/hicolor/scalable/apps/$app_id.svg"
(
  cd -- "$extract_dir"
  "$app_stage" --appimage-extract "$image_icon"
) || fail 'Could not extract the desktop icon from the AppImage.'
[[ -f "$extract_dir/squashfs-root/$image_icon" && ! -L "$extract_dir/squashfs-root/$image_icon" ]] ||
  fail 'The AppImage is missing its desktop icon.'
install -m 0644 -- "$extract_dir/squashfs-root/$image_icon" "$icon_stage"

python3 - "$installed" > "$launcher_stage" <<'PY'
import sys

executable = sys.argv[1]
if any(ord(char) < 32 or ord(char) == 127 for char in executable):
    sys.exit("The install path contains a control character")
# Exec has two parsing layers: desktop-entry string escapes, then Exec quotes.
argument = "".join("\\" + char if char in '\\"$`' else char for char in executable)
argument = argument.replace("\\", "\\\\").replace("%", "%%")
print(f"""[Desktop Entry]
Type=Application
Name=Steelgum
Comment=Game development toolkit
Exec="{argument}"
Icon=info.brumley.steelgum
StartupWMClass=info.brumley.steelgum
Terminal=false
Categories=Development;""")
PY
chmod 0644 -- "$launcher_stage"

is_installed_process() {
  local pid="$1" image
  image="$(readlink -- "/proc/$pid/exe" 2>/dev/null)" || return 1
  [[ "$image" == "$installed" ]] ||
    { [[ -r "/proc/$pid/environ" ]] &&
      grep -zFxq -- "APPIMAGE=$installed" "/proc/$pid/environ" 2>/dev/null; }
}

# Limit the close to this user's installed application, including its AppImage
# subprocess. Leave other checkouts and similarly named processes alone.
running="$(pgrep -u "$EUID" -x steelgum)" || {
  status=$?
  (( status == 1 )) || fail 'Could not identify Steelgum processes.'
  running=''
}
while IFS= read -r pid; do
  [[ -n "$pid" ]] && is_installed_process "$pid" || continue
  log "Closing installed Steelgum (PID $pid)."
  if ! kill -TERM "$pid" 2>/dev/null && is_installed_process "$pid"; then
    fail "Could not close Steelgum (PID $pid)."
  fi
  for (( attempt=0; attempt<100; attempt++ )); do
    is_installed_process "$pid" || break
    sleep 0.1
  done
  if is_installed_process "$pid"; then
    fail 'Close installed Steelgum and run the installer again.'
  fi
done <<< "$running"

# Each rename is on the same filesystem as its destination. A failed transfer
# or verification leaves the old executable and launcher untouched.
mv -Tf -- "$icon_stage" "$icon"
icon_stage=''
mv -Tf -- "$launcher_stage" "$launcher"
launcher_stage=''
mv -Tf -- "$app_stage" "$installed"
app_stage=''
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$desktop_dir" || log 'WARNING: desktop menu cache could not be refreshed.'
fi
if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache --quiet --ignore-theme-index "$data_home/icons/hicolor" ||
    log 'WARNING: icon cache could not be refreshed.'
fi
log "Installed $tag to $installed"

if [[ "$relaunch" == true ]]; then
  log 'Starting Steelgum.'
  setsid --fork "$installed" </dev/null >> "$log_file" 2>&1 9>&-
fi
