#!/bin/sh
# Puts the program, its menu entry and its icon under ~/.local (or the prefix given as the first argument).
# Run it from the unpacked release folder, or from the repository after `cargo build --release`.
set -e
here=$(cd "$(dirname "$0")" && pwd)
prefix=${1:-$HOME/.local}
bin=$here/../breakfast-cereal
[ -f "$bin" ] || bin=$here/../target/release/breakfast-cereal
[ -f "$bin" ] || { echo "breakfast-cereal was not found; build it first with: cargo build --release" >&2; exit 1; }
install -Dm755 "$bin" "$prefix/bin/breakfast-cereal"
install -Dm644 "$here/breakfast-cereal.desktop" "$prefix/share/applications/breakfast-cereal.desktop"
install -Dm644 "$here/breakfast-cereal.png" "$prefix/share/icons/hicolor/256x256/apps/breakfast-cereal.png"
install -Dm644 "$here/breakfast-cereal.svg" "$prefix/share/icons/hicolor/scalable/apps/breakfast-cereal.svg"
install -Dm644 "$here/breakfast-cereal.xml" "$prefix/share/mime/packages/breakfast-cereal.xml"
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$prefix/share/mime" || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" || true
echo "Installed to $prefix. Make sure $prefix/bin is on your PATH."
