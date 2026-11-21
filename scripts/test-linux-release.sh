#!/usr/bin/env bash
set -euo pipefail

format="${1:?Expected deb or rpm}"
test_dir=$(mktemp -d)
trap 'rm -rf -- "$test_dir"' EXIT
export HOME="$test_dir/Steelgum Test Home"
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share"
config="$XDG_CONFIG_HOME/steelgum"
user_binary="$HOME/.local/bin/steelgum"
launcher="$XDG_DATA_HOME/applications/info.brumley.steelgum.desktop"
icon="$XDG_DATA_HOME/icons/hicolor/scalable/apps/info.brumley.steelgum.svg"
mkdir -p -- "$config"
printf '%s\n' 'persistent settings' > "$config/settings-fixture"
args=(--version "$TAG" --yes --no-relaunch)

check_version() {
    local actual
    actual=$("$1" --version)
    if [[ "$actual" != "steelgum $VERSION" ]]; then
        printf 'Expected steelgum %s, got %s.\n' "$VERSION" "$actual" >&2
        exit 1
    fi
}

bash docs/install.sh --appimage "${args[@]}"
bash docs/install.sh --appimage "${args[@]}"
check_version "$user_binary"
test -f "$launcher"
test -f "$icon"

# Default install should choose the native format and leave a single launcher.
bash docs/install.sh "${args[@]}"
check_version /usr/bin/steelgum
for path in "$user_binary" "$launcher" "$icon"; do
    test ! -e "$path"
done
test -f /usr/share/applications/info.brumley.steelgum.desktop
test -f /usr/share/icons/hicolor/scalable/apps/info.brumley.steelgum.svg
case "$format" in
    deb) test "$(dpkg-query -W -f='${Version}' steelgum)" = "$VERSION" ;;
    rpm) test "$(rpm -q --qf '%{VERSION}' steelgum)" = "$VERSION" ;;
    *) exit 2 ;;
esac

bash docs/install.sh --system "${args[@]}"
check_version /usr/bin/steelgum
bash docs/uninstall.sh --yes
test ! -e /usr/bin/steelgum
test ! -e /usr/share/applications/info.brumley.steelgum.desktop
test "$(cat -- "$config/settings-fixture")" = 'persistent settings'
