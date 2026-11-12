#!/bin/sh
# Builds a single-file AppImage from a release build. Needs appimagetool on the PATH
# (https://github.com/AppImage/appimagetool); without it the AppDir is left ready in target/AppDir.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/..
bin=$root/target/release/breakfast-cereal
[ -f "$bin" ] || { echo "build first: cargo build --release -p breakfast-cereal" >&2; exit 1; }
dir=$root/target/AppDir
rm -rf "$dir"
install -Dm755 "$bin" "$dir/usr/bin/breakfast-cereal"
install -Dm644 "$here/breakfast-cereal.desktop" "$dir/usr/share/applications/breakfast-cereal.desktop"
install -Dm644 "$here/breakfast-cereal.png" "$dir/usr/share/icons/hicolor/256x256/apps/breakfast-cereal.png"
install -Dm644 "$here/breakfast-cereal.svg" "$dir/usr/share/icons/hicolor/scalable/apps/breakfast-cereal.svg"
install -Dm644 "$here/breakfast-cereal.xml" "$dir/usr/share/mime/packages/breakfast-cereal.xml"
install -Dm644 "$root/LICENSE" "$dir/usr/share/licenses/breakfast-cereal/LICENSE"
cp "$here/breakfast-cereal.desktop" "$here/breakfast-cereal.png" "$dir/"
ln -s breakfast-cereal.png "$dir/.DirIcon"
cat > "$dir/AppRun" <<'RUN'
#!/bin/sh
exec "$(dirname "$(readlink -f "$0")")/usr/bin/breakfast-cereal" "$@"
RUN
chmod +x "$dir/AppRun"
if command -v appimagetool >/dev/null 2>&1; then
    ARCH=$(uname -m) appimagetool "$dir" "$root/target/Breakfast-Cereal-DAW-$(uname -m).AppImage"
    echo "Made target/Breakfast-Cereal-DAW-$(uname -m).AppImage"
else
    echo "appimagetool was not found; the folder is ready at $dir"
fi
