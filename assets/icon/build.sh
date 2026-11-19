#!/bin/sh
# Rebuild every generated icon file from qtable-icon.svg. macOS only (AppKit renders the SVG,
# iconutil builds the .icns). Run from anywhere:  assets/icon/build.sh
#
# Writes: assets/icon/qtable.icns            the macOS app icon, all standard sizes
#         assets/icon/qtable-512.png         the Linux desktop icon
#         crates/qtable-gui/assets/window-icon.rgba   128x128 raw RGBA for the window icon
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/qtable-icon.XXXXXX")
trap 'rm -rf "${work:?}"' EXIT

swift "$here/render.swift" "$here/qtable-icon.svg" "$work" 16 32 64 128 256 512 1024

set_dir="$work/qtable.iconset"
mkdir "$set_dir"
cp "$work/16.png"   "$set_dir/icon_16x16.png"
cp "$work/32.png"   "$set_dir/icon_16x16@2x.png"
cp "$work/32.png"   "$set_dir/icon_32x32.png"
cp "$work/64.png"   "$set_dir/icon_32x32@2x.png"
cp "$work/128.png"  "$set_dir/icon_128x128.png"
cp "$work/256.png"  "$set_dir/icon_128x128@2x.png"
cp "$work/256.png"  "$set_dir/icon_256x256.png"
cp "$work/512.png"  "$set_dir/icon_256x256@2x.png"
cp "$work/512.png"  "$set_dir/icon_512x512.png"
cp "$work/1024.png" "$set_dir/icon_512x512@2x.png"
iconutil -c icns "$set_dir" -o "$here/qtable.icns"

cp "$work/512.png" "$here/qtable-512.png"
mkdir -p "$root/crates/qtable-gui/assets"
cp "$work/128.rgba" "$root/crates/qtable-gui/assets/window-icon.rgba"
echo "icon files rebuilt from $here/qtable-icon.svg"
