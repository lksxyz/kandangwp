#!/bin/sh
# Installs the Kandang icons into the user's icon theme so the tray and launchers find them.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)/assets/brand"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"

install -Dm644 "$ROOT/kandang-mark.svg" "$DEST/scalable/apps/kandang.svg"
install -Dm644 "$ROOT/kandang-symbolic.svg" "$DEST/scalable/status/kandang-symbolic.svg"
install -Dm644 "$ROOT/kandang-stopped-symbolic.svg" "$DEST/scalable/status/kandang-stopped-symbolic.svg"
for size in 16 24 32 48 64 128 256 512; do
  install -Dm644 "$ROOT/png/kandang-$size.png" "$DEST/${size}x${size}/apps/kandang.png"
done
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q "$DEST" 2>/dev/null || true
fi
echo "installed Kandang icons into $DEST"
