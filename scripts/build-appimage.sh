# Builds and publishes the Kandang AppImage.
#
# Usage:
#   ./scripts/build-appimage.sh              # writes dist/kandang-<version>-x86_64.AppImage
#
# Requires: cargo, strip, curl, and FUSE (for appimagetool).
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
OUT="${OUT:-$ROOT/dist}"

(cd "$ROOT" && cargo build --release)
strip "$ROOT/target/release/kandang" -o "$WORK/kandang.stripped"

APPDIR="$WORK/kandang.AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/metainfo"
install -m755 "$WORK/kandang.stripped" "$APPDIR/usr/bin/kandang"
install -m644 "$ROOT/assets/brand/png/kandang-256.png" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps/kandang.png"
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/kandang.png" "$APPDIR/kandang.png"

printf '%s\n' \
  '[Desktop Entry]' \
  'Type=Application' \
  'Name=Kandang' \
  'Comment=Local WordPress authoring, static publishing' \
  'Exec=kandang applet' \
  'Icon=kandang' \
  'Categories=Development;' \
  'Terminal=false' \
  'StartupNotify=false' > "$APPDIR/kandang.desktop"
cp "$APPDIR/kandang.desktop" "$APPDIR/usr/share/applications/"

printf '%s\n' \
  '<?xml version="1.0" encoding="UTF-8"?>' \
  '<component type="desktop-application">' \
  '  <id>io.github.lksxyz.kandang</id>' \
  '  <metadata_license>FSFAP</metadata_license>' \
  '  <project_license>MIT</project_license>' \
  '  <launchable type="desktop-id">kandang.desktop</launchable>' \
  '  <name>Kandang</name>' \
  '  <summary>Local WordPress authoring, static publishing</summary>' \
  '  <url type="homepage">https://github.com/lksxyz/kandangwp</url>' \
  '  <developer_name>Fahmi Lukis</developer_name>' \
  "  <releases><release version=\"$VERSION\"/></releases>" \
  '  <content_rating type="oars-1.1"/>' \
  '</component>' > "$APPDIR/usr/share/metainfo/io.github.lksxyz.kandang.appdata.xml"

printf '%s\n' \
  '#!/bin/sh' \
  'HERE="$(dirname "$(readlink -f "$0")")"' \
  'exec "$HERE/usr/bin/kandang" "$@"' > "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
chmod -R a+rX "$APPDIR"

# appimagetool runs from a FUSE mount by default; CI runners have no libfuse2.
export APPIMAGE_EXTRACT_AND_RUN=1
if ! command -v appimagetool >/dev/null 2>&1; then
  curl -fsSL -o "$WORK/appimagetool.AppImage" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$WORK/appimagetool.AppImage"
  appimagetool="$WORK/appimagetool.AppImage"
else
  appimagetool=appimagetool
fi

mkdir -p "$OUT"
VERSION="$VERSION" "$appimagetool" "$APPDIR" "$OUT/kandang-$VERSION-x86_64.AppImage"
echo "built $OUT/kandang-$VERSION-x86_64.AppImage"
