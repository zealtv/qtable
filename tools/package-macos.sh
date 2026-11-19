#!/bin/sh
# Build qtable's macOS release zip from this source tree.
#
#   tools/package-macos.sh <out-dir>
#
# Runs cargo build --release for the GUI and the command-line client (no cargo features), refuses a
# build that carries development hooks, and writes qtable-vX.Y.Z-macos-<arch>.zip into <out-dir>
# (created if missing). The zip holds qtable.app (the GUI with its icon), the bare qtable-gui and
# qtable, the example sets in examples/, LICENSE, THIRD_PARTY_NOTICES.md, CHANGELOG.md and a README.txt.
#
# The version comes from RELEASE_VERSION (X.Y.Z), else from [workspace.package] in Cargo.toml.
# CARGO_TARGET_DIR may point at a build cache outside the checkout; the binaries still build from
# this tree's source. Run on a Mac: the zip holds macOS builds.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

die() {
    echo "package-macos: $*" >&2
    exit 1
}

[ $# -eq 1 ] || { echo "usage: tools/package-macos.sh <out-dir>" >&2; exit 2; }
out=$1

[ "$(uname -s)" = Darwin ] || die "the release zip holds macOS builds; run on a Mac"
arch=$(uname -m)
case "$arch" in arm64 | x86_64) ;; *) die "unsupported architecture $arch" ;; esac

version=${RELEASE_VERSION:-$(awk '
    /^\[/ { in_section = ($0 == "[workspace.package]"); next }
    in_section && /^version[ \t]*=/ { split($0, a, "\""); print a[2]; exit }' Cargo.toml)}
printf '%s\n' "$version" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' >/dev/null ||
    die "no version found (RELEASE_VERSION or Cargo.toml's [workspace.package] version)"
tag=v$version

for f in LICENSE THIRD_PARTY_NOTICES.md CHANGELOG.md assets/icon/qtable.icns; do
    [ -f "$f" ] || die "no $f to put in the zip"
done
[ -d examples ] && ls examples/*.ron >/dev/null 2>&1 || die "no example sets in examples/ to put in the zip"

cargo build --quiet --release -p qtable-gui -p qtable-cli

target=${CARGO_TARGET_DIR:-$root/target}
gui=$target/release/qtable-gui
cli=$target/release/qtable
[ -x "$gui" ] && [ -x "$cli" ] || die "the build left no qtable-gui and qtable in $target/release"

# A release binary carries no capture or test hooks. Two checks, because the hooks reach a binary two
# ways: the GUI's capture switches (QTABLE_CAPTURE_* strings in the binary), and the engine's own
# dev-hooks feature, which leaves no string behind, so ask cargo whether this build turns it on.
for binary in "$gui" "$cli"; do
    if grep -aq 'QTABLE_CAPTURE' "$binary"; then
        die "$binary contains capture hooks; a release build must not enable the dev-hooks feature"
    fi
done
features=$(cargo tree -p qtable-gui -p qtable-cli -e normal,build,features -i qtable-engine) ||
    die "cargo tree could not report the engine's features"
if printf '%s\n' "$features" | grep -F 'feature "dev-hooks"' >/dev/null; then
    die "this build enables the engine's dev-hooks feature; a release build must not"
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/qtable-package.XXXXXX")
trap 'rm -rf "${work:?}"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

name="qtable-$tag-macos-$arch"
dir=$work/$name
mkdir -p "$dir"
cp "$gui" "$cli" "$dir/"
cp LICENSE THIRD_PARTY_NOTICES.md CHANGELOG.md "$dir/"
cp -R examples "$dir/examples"

# qtable.app holds the GUI with its icon (the Dock and Finder take it from the bundle). assets/icon/build.sh
# makes the icon.
app=$dir/qtable.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$gui" "$app/Contents/MacOS/qtable-gui"
cp assets/icon/qtable.icns "$app/Contents/Resources/qtable.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>qtable</string>
    <key>CFBundleDisplayName</key><string>qtable</string>
    <key>CFBundleIdentifier</key><string>io.github.zealtv.qtable</string>
    <key>CFBundleExecutable</key><string>qtable-gui</string>
    <key>CFBundleIconFile</key><string>qtable</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

format=$(sed -n 's/^pub const FORMAT_VERSION: u32 = \([0-9]*\);/\1/p' crates/qtable-engine/src/file.rs)
cat > "$dir/README.txt" <<EOF
qtable $tag (macOS $arch), project file format $format
Built from the release source by tools/package-macos.sh.

qtable.app   the session-view application, as a macOS app with its icon
qtable-gui   the same application as a bare binary
qtable       the command-line client
examples/    example sets to open and take apart, for example
             ./qtable-gui examples/grid-demo.ron

qtable is free software under the GNU GPL, version 3 or any later version
(LICENSE). THIRD_PARTY_NOTICES.md lists the libraries built into the binaries.

Open qtable.app, or run ./qtable-gui. The app and the binaries are not
code-signed or notarised: if macOS refuses to open them, run
xattr -dr com.apple.quarantine .  in this folder.
A release reads every older project format; sets saved by this release may
not open in older ones (see CHANGELOG.md).
EOF

mkdir -p "$out"
out=$(cd "$out" && pwd)
rm -f "${out:?}/${name:?}.zip"
(cd "$work" && zip -qr -X "$out/$name.zip" "$name")
echo "package-macos: wrote $out/$name.zip"
