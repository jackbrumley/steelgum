#!/usr/bin/env bash
set -euo pipefail

confirm=true
mode=auto
while (( $# )); do
  case "$1" in
    --yes) confirm=false ;;
    --appimage|--system)
      if [[ "$mode" != auto && "$mode" != "${1#--}" ]]; then
        printf '[steelgum] ERROR: Choose either --system or --appimage.\n' >&2
        exit 1
      fi
      mode="${1#--}" ;;
    -h|--help)
      printf '%s\n' 'Usage: uninstall.sh [--system | --appimage] [--yes]' 'Removes installed Steelgum packages and launchers. Keeps settings, projects and logs.'
      exit 0 ;;
    *) printf '[steelgum] ERROR: Unknown option: %s\n' "$1" >&2; exit 1 ;;
  esac
  shift
done

config_home="${XDG_CONFIG_HOME:-}"
[[ "$config_home" == /* ]] || config_home="$HOME/.config"
data_home="${XDG_DATA_HOME:-}"
[[ "$data_home" == /* ]] || data_home="$HOME/.local/share"
debug_dir="$config_home/steelgum/debug"
desktop_dir="$data_home/applications"
mkdir -p -- "$debug_dir"
log_file="$debug_dir/update.log"
touch -- "$log_file"
exec > >(tee -a -- "$log_file") 2>&1
exec 9>"$debug_dir/install.lock"
flock -n 9 || { printf '[steelgum] ERROR: Another installation or removal is running.\n' >&2; exit 1; }
printf '[steelgum] Uninstaller started at %s\n' "$(date --iso-8601=seconds)"
if [[ "$confirm" == true && -t 0 ]]; then
  read -r -p 'Remove Steelgum? [y/N] ' answer
  [[ "$answer" == [yY] ]] || { printf '[steelgum] Uninstall cancelled.\n'; exit 1; }
fi

fail() { printf '[steelgum] ERROR: %s\n' "$*" >&2; exit 1; }

if [[ "$mode" != appimage ]]; then
  remove=()
  if command -v dpkg-query >/dev/null &&
    [[ $(dpkg-query -W -f='${Status}' steelgum 2>/dev/null) == 'install ok installed' ]]; then
    remove=(apt-get remove -y steelgum)
  elif command -v rpm >/dev/null && rpm -q steelgum >/dev/null 2>&1; then
    remove=(dnf --setopt=clean_requirements_on_remove=False remove -y steelgum)
  fi
  if (( ${#remove[@]} )); then
    if ! command -v "${remove[0]}" >/dev/null; then
      fail "System removal requires ${remove[0]}."
    fi
    elevate=()
    if (( EUID != 0 )); then
      if command -v sudo >/dev/null && sudo -n true 2>/dev/null; then
        elevate=(sudo -n)
      elif command -v sudo >/dev/null && ( : </dev/tty ) 2>/dev/null; then
        elevate=(sudo)
      elif command -v pkexec >/dev/null && [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]]; then
        elevate=(pkexec)
      else
        fail 'System removal requires administrator authentication. Use --appimage to remove only the user install.'
      fi
    fi
    "${elevate[@]}" "${remove[@]}" || fail 'The package manager could not remove Steelgum.'
  fi
fi

if [[ "$mode" != system ]]; then
  rm -f -- "$HOME/.local/bin/steelgum" "$desktop_dir/info.brumley.steelgum.desktop" \
    "$data_home/icons/hicolor/scalable/apps/info.brumley.steelgum.svg"
fi
if [[ -d "$desktop_dir" ]] && command -v update-desktop-database >/dev/null; then
  update-desktop-database "$desktop_dir" || printf '[steelgum] WARNING: desktop menu cache could not be refreshed.\n' >&2
fi
if [[ -d "$data_home/icons/hicolor" ]] && command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache --quiet --ignore-theme-index "$data_home/icons/hicolor" ||
    printf '[steelgum] WARNING: icon cache could not be refreshed.\n' >&2
fi
printf '[steelgum] Removed Steelgum. Settings and logs were kept.\n'
