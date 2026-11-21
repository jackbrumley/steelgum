#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

version=$(node scripts/release-assets.mjs version)
asset=$(node scripts/release-assets.mjs asset linux)
work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT
app_dir="$work_dir/Steelgum.AppDir"
app_id=info.brumley.steelgum

manifest=apps/door-game-bevy/Cargo.toml
lock_args=()
if [[ -f apps/door-game-bevy/Cargo.lock ]]; then
    lock_args=(--locked)
fi
cargo build --release --manifest-path "$manifest" --bin steelgum "${lock_args[@]}"
install -Dm755 apps/door-game-bevy/target/release/steelgum "$app_dir/usr/bin/steelgum"
install -m755 packaging/linux/AppRun "$app_dir/AppRun"
install -Dm644 "packaging/linux/$app_id.desktop" "$app_dir/usr/share/applications/$app_id.desktop"
install -Dm644 docs/assets/steelgum.svg "$app_dir/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
ln -s "usr/share/applications/$app_id.desktop" "$app_dir/$app_id.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$app_id.svg" "$app_dir/$app_id.svg"
ln -s "$app_id.svg" "$app_dir/.DirIcon"
desktop-file-validate "$app_dir/$app_id.desktop"

download() {
    local url=$1 destination=$2 digest=$3
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location --retry 3         --output "$destination" "$url"
    printf '%s  %s\n' "$digest" "$destination" | sha256sum --check
}

appimagetool="$work_dir/appimagetool.AppImage"
runtime="$work_dir/runtime-x86_64"
download https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage     "$appimagetool" ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
download https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64     "$runtime" 2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
chmod +x "$appimagetool"

mkdir -p release-artifacts
output="release-artifacts/$asset"
ARCH=x86_64 "$appimagetool" --appimage-extract-and-run --runtime-file "$runtime" "$app_dir" "$output"
chmod +x "$output"
actual=$("$output" --appimage-extract-and-run --version)
if [[ $actual != "steelgum $version" ]]; then
    printf 'Expected steelgum %s, got %s\n' "$version" "$actual" >&2
    exit 1
fi
