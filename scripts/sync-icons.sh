#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
SRC_LOGO="$ROOT_DIR/docs/logo.png"

if [ ! -f "$SRC_LOGO" ]; then
  echo "❌ Source logo not found at $SRC_LOGO" >&2
  exit 1
fi

echo "🎨 Syncing all icons from single source of truth: docs/logo.png"

resize_png() {
  local size="$1"
  local dest="$2"
  mkdir -p "$(dirname "$dest")"
  sips -z "$size" "$size" "$SRC_LOGO" --out "$dest" >/dev/null
  echo "  ✓ ${dest#$ROOT_DIR/} (${size}x${size})"
}

# Raycast extension & command icons (512x512)
resize_png 512 "$ROOT_DIR/packages/raycast/assets/icon.png"
resize_png 512 "$ROOT_DIR/packages/raycast/assets/cockpit.png"
resize_png 512 "$ROOT_DIR/packages/raycast/assets/cycle.png"
resize_png 512 "$ROOT_DIR/packages/raycast/assets/alert.png"

# Browser extension icons (16, 32, 48, 128)
resize_png 16  "$ROOT_DIR/packages/extension/public/icon/16.png"
resize_png 32  "$ROOT_DIR/packages/extension/public/icon/32.png"
resize_png 48  "$ROOT_DIR/packages/extension/public/icon/48.png"
resize_png 128 "$ROOT_DIR/packages/extension/public/icon/128.png"

# Browser Store listing icons (300x300 Edge, 512x512 Chrome/Firefox)
resize_png 300 "$ROOT_DIR/packages/extension/store-assets/edge-icon-300x300.png"
resize_png 512 "$ROOT_DIR/packages/extension/store-assets/store-icon-512x512.png"

# Local macOS ContextBraid.app notification bundle icon (if installed)
APP_SUPPORT_BASE="$HOME/Library/Application Support/ContextBraid"
APP_BUNDLE="$APP_SUPPORT_BASE/ContextBraid.app"
if [ -d "$APP_BUNDLE/Contents/Resources" ]; then
  cp -f "$SRC_LOGO" "$APP_SUPPORT_BASE/icon.png"
  ICONSET_DIR="$(mktemp -d)/ContextBraid.iconset"
  mkdir -p "$ICONSET_DIR"
  sips -z 16   16   "$SRC_LOGO" --out "$ICONSET_DIR/icon_16x16.png" >/dev/null
  sips -z 32   32   "$SRC_LOGO" --out "$ICONSET_DIR/icon_16x16@2x.png" >/dev/null
  sips -z 32   32   "$SRC_LOGO" --out "$ICONSET_DIR/icon_32x32.png" >/dev/null
  sips -z 64   64   "$SRC_LOGO" --out "$ICONSET_DIR/icon_32x32@2x.png" >/dev/null
  sips -z 128  128  "$SRC_LOGO" --out "$ICONSET_DIR/icon_128x128.png" >/dev/null
  sips -z 256  256  "$SRC_LOGO" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
  sips -z 256  256  "$SRC_LOGO" --out "$ICONSET_DIR/icon_256x256.png" >/dev/null
  sips -z 512  512  "$SRC_LOGO" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
  sips -z 512  512  "$SRC_LOGO" --out "$ICONSET_DIR/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$SRC_LOGO" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null
  iconutil -c icns "$ICONSET_DIR" -o "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
  rm -rf "$(dirname "$ICONSET_DIR")"
  touch "$APP_BUNDLE"
  LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
  if [ -x "$LSREG" ]; then
    "$LSREG" -f "$APP_BUNDLE" >/dev/null 2>&1 || true
  fi
  echo "  ✓ ~/Library/Application Support/ContextBraid/ContextBraid.app/Contents/Resources/AppIcon.icns"
fi

echo "✅ All icons synchronized from docs/logo.png"
