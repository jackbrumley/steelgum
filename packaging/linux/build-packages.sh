#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

version=$(node scripts/release-assets.mjs version)
deb=$(node scripts/release-assets.mjs asset deb)
rpm=$(node scripts/release-assets.mjs asset rpm)
appimage=$(node scripts/release-assets.mjs asset linux)
work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT
stage="$work_dir/stage"
app_dir="$work_dir/Steelgum.AppDir"
app_id=info.brumley.steelgum
output_dir="$root/release-artifacts"

build_binary() {
    local manifest=$1 name=$2 installed_name=$3 target_dir
    local -a lock_args=()
    target_dir="${manifest%/*}/target"
    if [[ -f "${manifest%.toml}.lock" ]]; then
        lock_args=(--locked)
    fi
    cargo build --release --manifest-path "$manifest" --target-dir "$target_dir" --bin "$name" "${lock_args[@]}"
    install -Dm755 "$target_dir/release/$name" "$stage/usr/bin/$installed_name"
}
build_binary "$(node scripts/release-assets.mjs desktop-manifest)" steelgum-gpui steelgum
build_binary "$(node scripts/release-assets.mjs player-manifest)" steelgum steelgum-player
install -Dm644 "packaging/linux/$app_id.desktop" "$stage/usr/share/applications/$app_id.desktop"
install -Dm644 docs/assets/steelgum.svg "$stage/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
desktop-file-validate "$stage/usr/share/applications/$app_id.desktop"

check_version() {
    local actual
    actual=$("$@")
    if [[ $actual != "steelgum $version" ]]; then
        printf 'Expected steelgum %s, got %s\n' "$version" "$actual" >&2
        exit 1
    fi
}
check_version "$stage/usr/bin/steelgum" --version
check_version "$stage/usr/bin/steelgum-player" --version
mkdir -p -- "$output_dir"

# Bevy's window and graphics backends load some libraries at runtime. The
# package metadata includes those libraries as well as the linked dependencies.
deb_root="$work_dir/deb"
cp -a -- "$stage" "$deb_root"
mkdir -- "$deb_root/DEBIAN"
cat > "$deb_root/DEBIAN/control" <<EOF
Package: steelgum
Version: $version
Architecture: amd64
Maintainer: Jack Brumley <82296582+jackbrumley@users.noreply.github.com>
Depends: libc6 (>= 2.35), libgcc-s1, libxcb1, libfreetype6, libfontconfig1,
 libwayland-client0, libwayland-cursor0,
 libxkbcommon0, libxkbcommon-x11-0, libx11-6, libx11-xcb1, libxcursor1,
 libxi6, libxrandr2, libvulkan1, libegl1, curl, git, python3, procps, util-linux
Section: devel
Priority: optional
Homepage: https://brumley.info/steelgum/
Description: Build and preview Steelgum games
 Steelgum is a desktop toolkit for developing and previewing games.
EOF
dpkg-deb --root-owner-group --build "$deb_root" "$output_dir/$deb"

rpm_root="$work_dir/rpm"
mkdir -p -- "$rpm_root/SPECS"
cat > "$rpm_root/SPECS/steelgum.spec" <<EOF
Name: steelgum
Version: $version
Release: 1
Summary: Build and preview Steelgum games
License: All rights reserved
URL: https://brumley.info/steelgum/
Requires: libwayland-client.so.0()(64bit), libwayland-cursor.so.0()(64bit)
Requires: libxcb.so.1()(64bit), libfreetype.so.6()(64bit), libfontconfig.so.1()(64bit)
Requires: libxkbcommon.so.0()(64bit), libxkbcommon-x11.so.0()(64bit)
Requires: libX11.so.6()(64bit), libX11-xcb.so.1()(64bit), libXcursor.so.1()(64bit)
Requires: libXi.so.6()(64bit), libXrandr.so.2()(64bit), libvulkan.so.1()(64bit), libEGL.so.1()(64bit)
Requires: /usr/bin/curl, /usr/bin/git, /usr/bin/python3, /usr/bin/pgrep, /usr/bin/setsid, /usr/bin/flock
%global debug_package %{nil}
%global __os_install_post %{nil}

%description
Steelgum is a desktop toolkit for developing and previewing games.

%install
mkdir -p %{buildroot}
cp -a "$stage/." %{buildroot}/

%files
/usr/bin/steelgum
/usr/bin/steelgum-player
/usr/share/applications/$app_id.desktop
/usr/share/icons/hicolor/scalable/apps/$app_id.svg
EOF
rpmbuild --define "_topdir $rpm_root" --target x86_64 -bb "$rpm_root/SPECS/steelgum.spec"
mv -- "$rpm_root/RPMS/x86_64/steelgum-$version-1.x86_64.rpm" "$output_dir/$rpm"

cp -a -- "$stage" "$app_dir"
install -m755 packaging/linux/AppRun "$app_dir/AppRun"
ln -s "usr/share/applications/$app_id.desktop" "$app_dir/$app_id.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$app_id.svg" "$app_dir/$app_id.svg"
ln -s "$app_id.svg" "$app_dir/.DirIcon"

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

output="$output_dir/$appimage"
ARCH=x86_64 "$appimagetool" --appimage-extract-and-run --runtime-file "$runtime" "$app_dir" "$output"
chmod +x "$output"
check_version "$output" --appimage-extract-and-run --version
