#!/usr/bin/env bash
set -euo pipefail

repo='jackbrumley/steelgum'
app_id='info.brumley.steelgum'
tag=''
relaunch=true
[[ ${STEELGUM_NO_RELAUNCH:-} == 1 ]] && relaunch=false
confirm=true
verify=true
mode=auto

usage() {
  cat <<'USAGE'
Steelgum installer for x64 Linux

Usage: install.sh [--system | --appimage] [--version vX.Y.Z] [--yes] [--relaunch | --no-relaunch] [--insecure-skip-verify]

Installs a system package through apt or dnf when possible. Otherwise installs
a user AppImage to ~/.local/bin/steelgum. Use --system to require a system
install, or --appimage to install without admin access. Use --version to select
a specific release and --no-relaunch for headless installs.
STEELGUM_GITHUB_TOKEN can supply a token for GitHub API requests.
STEELGUM_NO_RELAUNCH=1 suppresses launch unless --relaunch is passed.
USAGE
}

fail() { printf '[steelgum] ERROR: %s\n' "$*" >&2; exit 1; }
log() { printf '[steelgum] %s\n' "$*"; }
valid_tag() { [[ "$1" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; }

while (( $# )); do
  case "$1" in
    --appimage|--system)
      [[ "$mode" == auto || "$mode" == "${1#--}" ]] || fail 'Choose either --system or --appimage.'
      mode="${1#--}" ;;
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
user_binary="$bin_dir/steelgum"
system_binary='/usr/bin/steelgum'
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
work_dir=''
cleanup() {
  local status=$?
  if [[ -n "$app_stage" ]]; then rm -f -- "$app_stage"; fi
  if [[ -n "$launcher_stage" ]]; then rm -f -- "$launcher_stage"; fi
  if [[ -n "$icon_stage" ]]; then rm -f -- "$icon_stage"; fi
  if [[ -n "$work_dir" ]]; then rm -rf -- "$work_dir"; fi
  if (( status != 0 )); then log "Installer stopped (exit $status)."; fi
}
trap cleanup EXIT
log "Installer started at $(date --iso-8601=seconds)"

extension=AppImage
elevate=()
if [[ "$mode" != appimage ]]; then
  if command -v apt-get >/dev/null && command -v dpkg-query >/dev/null; then
    extension=deb
  elif command -v dnf >/dev/null && command -v rpm >/dev/null; then
    extension=rpm
  elif [[ "$mode" == system ]]; then
    fail 'System installation requires apt or dnf. Use --appimage for user-local installation.'
  fi
  if [[ "$extension" != AppImage && "$EUID" != 0 ]]; then
    if command -v sudo >/dev/null && sudo -n true 2>/dev/null; then
      elevate=(sudo -n)
    elif command -v sudo >/dev/null && ( : </dev/tty ) 2>/dev/null; then
      elevate=(sudo)
    elif command -v pkexec >/dev/null && [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]]; then
      elevate=(pkexec)
    elif [[ "$mode" == system ]]; then
      fail 'System installation requires administrator authentication.'
    else
      log 'Administrator authentication is unavailable. Using the AppImage.'
      extension=AppImage
    fi
  fi
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

selected="$("${transport[@]}" --max-time 45 "${headers[@]}" "$api/tags/$tag" |
  python3 -c '
import json, sys

tag, repo, extension, mode = sys.argv[1:]
release = json.load(sys.stdin)
if release["tag_name"] != tag or release["draft"] is not False:
    sys.exit("GitHub returned a different release or a draft")
formats = [extension]
if mode == "auto" and extension != "AppImage":
    formats.append("AppImage")
for candidate in formats:
    name = f"steelgum-{tag[1:]}-linux-x64.{candidate}"
    matches = [asset for asset in release["assets"] if asset["name"] == name]
    if not matches:
        continue
    url = f"https://github.com/{repo}/releases/download/{tag}/{name}"
    if len(matches) != 1 or matches[0]["browser_download_url"] != url:
        sys.exit(f"Expected one release asset at {url}")
    digest = matches[0].get("digest") or ""
    if any(ord(char) < 32 or ord(char) == 127 for char in digest):
        sys.exit(f"Invalid digest for {name}")
    print(candidate, digest)
    break
else:
    sys.exit(f"Release {tag} does not provide a supported package ({extension})")
' "$tag" "$repo" "$extension" "$mode")" || fail "Could not identify the Steelgum package on GitHub."
read -r selected_extension digest <<< "$selected"
if [[ "$selected_extension" != "$extension" ]]; then
  log "Release $tag has no $extension package. Using the AppImage."
  extension="$selected_extension"
fi
asset="steelgum-${tag#v}-linux-x64.$extension"
url="https://github.com/$repo/releases/download/$tag/$asset"
installed="$system_binary"
if [[ "$extension" == AppImage ]]; then installed="$user_binary"; fi

if [[ "$confirm" == true && -t 0 ]]; then
  read -r -p "Install Steelgum to $installed? [y/N] " answer
  [[ "$answer" == [yY] ]] || fail 'Installation cancelled.'
fi

expected=''
if [[ "$verify" == true ]]; then
  [[ "$digest" =~ ^sha256:([[:xdigit:]]{64})$ ]] || fail "GitHub did not supply a valid SHA-256 digest for $asset."
  expected="${BASH_REMATCH[1],,}"
else
  log 'WARNING: checksum verification disabled by request.'
fi

work_dir="$(mktemp -d -- /tmp/steelgum-install.XXXXXXXX)"
if [[ "$extension" == AppImage ]]; then
  app_stage="$(mktemp -- "$bin_dir/.steelgum.XXXXXXXX")"
  download_path="$app_stage"
else
  download_path="$work_dir/$asset"
fi
log "Downloading $asset"
"${transport[@]}" --max-time 600 --output "$download_path" "$url" || fail "Download failed for $asset."
[[ -s "$download_path" ]] || fail "GitHub returned an empty asset for $asset."

if [[ "$verify" == true ]]; then
  actual="$(sha256sum < "$download_path")"
  [[ "${actual%% *}" == "$expected" ]] || fail "SHA-256 mismatch for $asset."
  log 'SHA-256 verified.'
fi

prepare_appimage() {
  launcher_stage="$(mktemp -- "$desktop_dir/.steelgum.XXXXXXXX")"
  icon_stage="$(mktemp -- "$icon_dir/.steelgum.XXXXXXXX")"
  chmod 0755 -- "$app_stage"
  local image_icon="usr/share/icons/hicolor/scalable/apps/$app_id.svg"
  (
    cd -- "$work_dir"
    "$app_stage" --appimage-extract "$image_icon"
  ) || fail 'Could not extract the desktop icon from the AppImage.'
  [[ -f "$work_dir/squashfs-root/$image_icon" && ! -L "$work_dir/squashfs-root/$image_icon" ]] ||
    fail 'The AppImage is missing its desktop icon.'
  install -m 0644 -- "$work_dir/squashfs-root/$image_icon" "$icon_stage"

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
}

if [[ "$extension" == AppImage ]]; then prepare_appimage; fi

is_installed_process() {
  local pid="$1" image
  image="$(readlink -- "/proc/$pid/exe" 2>/dev/null)" || return 1
  [[ "$image" == "$user_binary" || "$image" == "$system_binary" ]] ||
    { [[ -r "/proc/$pid/environ" ]] &&
      grep -zFxq -- "APPIMAGE=$user_binary" "/proc/$pid/environ" 2>/dev/null; }
}

is_installer_ancestor() {
  local pid="$$" stat
  while (( pid > 1 )); do
    [[ "$pid" != "$1" ]] || return 0
    IFS= read -r stat < "/proc/$pid/stat" || return 1
    stat="${stat##*) }"
    stat="${stat#* }"
    pid="${stat%% *}"
  done
  return 1
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
  if is_installer_ancestor "$pid"; then continue; fi
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

if [[ "$extension" == AppImage ]]; then
  # The old install stays in place until the download, digest and icon pass.
  mv -Tf -- "$icon_stage" "$icon"
  icon_stage=''
  mv -Tf -- "$launcher_stage" "$launcher"
  launcher_stage=''
  mv -Tf -- "$app_stage" "$installed"
  app_stage=''
elif [[ "$extension" == deb ]]; then
  # Allow the apt sandbox user to read the verified local package.
  chmod 0755 -- "$work_dir"
  chmod 0644 -- "$download_path"
  "${elevate[@]}" bash -c '
    apt-get update || printf "[steelgum] WARNING: apt package lists could not be refreshed.\n" >&2
    exec apt-get install -y --reinstall --allow-downgrades "$1"
  ' steelgum-apt "$download_path" || fail 'apt could not install Steelgum.'
else
  rpm_operation=install
  if installed_rpm=$(rpm -q --qf '%{VERSION}-%{RELEASE}\n' steelgum 2>/dev/null); then
    target_rpm=$(rpm -qp --qf '%{VERSION}-%{RELEASE}\n' "$download_path") || fail 'Could not read the RPM version.'
    if [[ "$installed_rpm" == "$target_rpm" ]]; then
      rpm_operation=reinstall
    elif [[ $(printf '%s\n' "$target_rpm" "$installed_rpm" | sort -V | tail -n 1) == "$installed_rpm" ]]; then
      rpm_operation=downgrade
    fi
  fi
  "${elevate[@]}" dnf -y "$rpm_operation" "$download_path" || fail 'dnf could not install Steelgum.'
fi

if [[ "$extension" != AppImage ]]; then
  actual=$("$installed" --version) || fail 'The installed system executable did not run.'
  [[ "$actual" == "steelgum ${tag#v}" ]] || fail "Expected steelgum ${tag#v}, got $actual."
  # Keep the existing user install usable if the system install fails.
  rm -f -- "$user_binary" "$launcher" "$icon"
fi
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
