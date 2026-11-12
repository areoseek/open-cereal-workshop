#!/bin/sh
# Puts the program, its menu entry and its icon under ~/.local (or the prefix given as the first argument).
# Run it from the unpacked release folder, or from the repository after `cargo build --release`.
set -e
here=$(cd "$(dirname "$0")" && pwd)
prefix=${1:-$HOME/.local}
bin=$here/../open-cereal-workshop
[ -f "$bin" ] || bin=$here/../target/release/open-cereal-workshop
[ -f "$bin" ] || { echo "open-cereal-workshop was not found; build it first with: cargo build --release" >&2; exit 1; }
install -Dm755 "$bin" "$prefix/bin/open-cereal-workshop"
install -Dm644 "$here/open-cereal-workshop.desktop" "$prefix/share/applications/open-cereal-workshop.desktop"
install -Dm644 "$here/open-cereal-workshop.png" "$prefix/share/icons/hicolor/256x256/apps/open-cereal-workshop.png"
install -Dm644 "$here/open-cereal-workshop.svg" "$prefix/share/icons/hicolor/scalable/apps/open-cereal-workshop.svg"
install -Dm644 "$here/open-cereal-workshop.xml" "$prefix/share/mime/packages/open-cereal-workshop.xml"
command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$prefix/share/mime" || true
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" || true
echo "Installed to $prefix. Make sure $prefix/bin is on your PATH."
