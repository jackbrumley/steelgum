#!/usr/bin/env bash
set -euo pipefail

confirm=true
while (( $# )); do
  case "$1" in
    --yes) confirm=false ;;
    -h|--help)
      printf '%s\n' 'Usage: uninstall.sh [--yes]' 'Removes the Steelgum AppImage and launcher. Keeps settings and logs.'
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

rm -f -- "$HOME/.local/bin/steelgum" "$desktop_dir/info.brumley.steelgum.desktop" \
  "$data_home/icons/hicolor/scalable/apps/info.brumley.steelgum.svg"
if [[ -d "$desktop_dir" ]] && command -v update-desktop-database >/dev/null; then
  update-desktop-database "$desktop_dir" || printf '[steelgum] WARNING: desktop menu cache could not be refreshed.\n' >&2
fi
if [[ -d "$data_home/icons/hicolor" ]] && command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache --quiet --ignore-theme-index "$data_home/icons/hicolor" ||
    printf '[steelgum] WARNING: icon cache could not be refreshed.\n' >&2
fi
printf '[steelgum] Removed Steelgum. Settings and logs were kept.\n'
