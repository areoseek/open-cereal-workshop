#!/bin/sh
# Builds a single-file AppImage from a release build. Needs appimagetool on the PATH
# (https://github.com/AppImage/appimagetool); without it the AppDir is left ready in target/AppDir.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/..
bin=$root/target/release/open-cereal-workshop
[ -f "$bin" ] || { echo "build first: cargo build --release -p open-cereal-workshop" >&2; exit 1; }
dir=$root/target/AppDir
rm -rf "$dir"
install -Dm755 "$bin" "$dir/usr/bin/open-cereal-workshop"
install -Dm644 "$here/open-cereal-workshop.desktop" "$dir/usr/share/applications/open-cereal-workshop.desktop"
install -Dm644 "$here/open-cereal-workshop.png" "$dir/usr/share/icons/hicolor/256x256/apps/open-cereal-workshop.png"
install -Dm644 "$here/open-cereal-workshop.svg" "$dir/usr/share/icons/hicolor/scalable/apps/open-cereal-workshop.svg"
install -Dm644 "$here/open-cereal-workshop.xml" "$dir/usr/share/mime/packages/open-cereal-workshop.xml"
install -Dm644 "$root/LICENSE" "$dir/usr/share/licenses/open-cereal-workshop/LICENSE"
cp "$here/open-cereal-workshop.desktop" "$here/open-cereal-workshop.png" "$dir/"
ln -s open-cereal-workshop.png "$dir/.DirIcon"
cat > "$dir/AppRun" <<'RUN'
#!/bin/sh
exec "$(dirname "$(readlink -f "$0")")/usr/bin/open-cereal-workshop" "$@"
RUN
chmod +x "$dir/AppRun"
if command -v appimagetool >/dev/null 2>&1; then
    ARCH=$(uname -m) appimagetool "$dir" "$root/target/Open-Cereal-Workshop-$(uname -m).AppImage"
    echo "Made target/Open-Cereal-Workshop-$(uname -m).AppImage"
else
    echo "appimagetool was not found; the folder is ready at $dir"
fi
