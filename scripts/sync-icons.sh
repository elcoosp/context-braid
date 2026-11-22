#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
ICONS_DIR="$ROOT_DIR/docs/icons"

for name in main cockpit cycle alert; do
  if [ ! -f "$ICONS_DIR/${name}.png" ]; then
    echo "❌ Missing master icon: $ICONS_DIR/${name}.png" >&2
    exit 1
  fi
done

echo "🎨 Generating macOS squircle icons (with coral->cyan specular rim) from docs/icons/{main,cockpit,cycle,alert}.png"

python3 - "$ROOT_DIR" "$ICONS_DIR" << 'PY'
import sys
from pathlib import Path
from PIL import Image

root = Path(sys.argv[1])
icons_dir = Path(sys.argv[2])

SIZE = 1024
SCALE = 2
W = SIZE * SCALE

def squircle_mask(w: int, inset: int = 0, power: float = 4.6) -> Image.Image:
    """Generates a smooth Apple-style continuous superellipse mask."""
    mask = Image.new("L", (w, w), 0)
    px = mask.load()
    cx = (w - 1) / 2.0
    r = (w / 2.0) - inset
    aa = 1.6
    for y in range(w):
        dy = abs(y - cx) / r
        dy_p = dy ** power if dy < 1.2 else 10.0
        for x in range(w):
            dx = abs(x - cx) / r
            if dx >= 1.15 or dy >= 1.15:
                continue
            val = (dx ** power + dy_p) ** (1.0 / power)
            if val <= 1.0 - (aa / r):
                px[x, y] = 255
            elif val < 1.0 + (aa / r):
                t = 1.0 - (val - (1.0 - aa / r)) / (2.0 * aa / r)
                px[x, y] = max(0, min(255, int(round(t * 255))))
    return mask

OUTER_INSET = int(34 * SCALE)
BEVEL_INSET = OUTER_INSET + int(22 * SCALE)
INNER_INSET = OUTER_INSET + int(26 * SCALE)

BASE_MASK = squircle_mask(W, inset=OUTER_INSET)
BEVEL_MASK = squircle_mask(W, inset=BEVEL_INSET)
INNER_MASK = squircle_mask(W, inset=INNER_INSET)

def make_dual_tone_rim(w: int) -> Image.Image:
    rim = Image.new("RGBA", (w, w), (0, 0, 0, 255))
    rpx = rim.load()
    for y in range(w):
        ty = y / (w - 1)
        for x in range(w):
            tx = x / (w - 1)
            # Bottom-left coral (245, 120, 140) -> top-right sky-cyan (115, 215, 255)
            t = 0.5 * (tx + (1.0 - ty))
            r = int(245 * (1.0 - t) + 115 * t)
            g = int(120 * (1.0 - t) + 215 * t)
            b = int(140 * (1.0 - t) + 255 * t)
            rpx[x, y] = (min(255, r + 15), min(255, g + 15), min(255, b + 15), 250)
    return rim

RIM_IMG = make_dual_tone_rim(W)
BEVEL_DARK = Image.new("RGBA", (W, W), (6, 10, 20, 255))

def to_macos_squircle(src_path: Path) -> Image.Image:
    src = Image.open(src_path).convert("RGBA")
    if src.width != src.height:
        side = min(src.width, src.height)
        left = (src.width - side) // 2
        top = (src.height - side) // 2
        src = src.crop((left, top, left + side, top + side))

    art_size = W - (INNER_INSET * 2) + int(16 * SCALE)
    art = src.resize((art_size, art_size), Image.Resampling.LANCZOS)

    art_canvas = Image.new("RGBA", (W, W), (13, 22, 40, 255))
    offset = (W - art_size) // 2
    art_canvas.paste(art, (offset, offset), art)

    canvas = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    canvas.paste(RIM_IMG, (0, 0), BASE_MASK)
    canvas.paste(BEVEL_DARK, (0, 0), BEVEL_MASK)
    canvas.paste(art_canvas, (0, 0), INNER_MASK)

    return canvas.resize((SIZE, SIZE), Image.Resampling.LANCZOS)

rendered = {
    name: to_macos_squircle(icons_dir / f"{name}.png")
    for name in ("main", "cockpit", "cycle", "alert")
}

def save_resized(img: Image.Image, size: int, rel_path: str):
    dest = root / rel_path
    dest.parent.mkdir(parents=True, exist_ok=True)
    img.resize((size, size), Image.Resampling.LANCZOS).save(dest, format="PNG", optimize=True)
    print(f"  ✓ {rel_path} ({size}x{size})")

save_resized(rendered["main"], 1024, "docs/icon-squircle.png")

save_resized(rendered["main"],    512, "packages/raycast/assets/icon.png")
save_resized(rendered["cockpit"], 512, "packages/raycast/assets/cockpit.png")
save_resized(rendered["cycle"],   512, "packages/raycast/assets/cycle.png")
save_resized(rendered["alert"],   512, "packages/raycast/assets/alert.png")

for sz in (16, 32, 48, 128):
    save_resized(rendered["main"], sz, f"packages/extension/public/icon/{sz}.png")

save_resized(rendered["main"], 300, "packages/extension/store-assets/edge-icon-300x300.png")
save_resized(rendered["main"], 512, "packages/extension/store-assets/store-icon-512x512.png")
PY

APP_SUPPORT_BASE="$HOME/Library/Application Support/ContextBraid"
APP_BUNDLE="$APP_SUPPORT_BASE/ContextBraid.app"
if [ -d "$APP_BUNDLE/Contents/Resources" ]; then
  cp -f "$ROOT_DIR/docs/icon-squircle.png" "$APP_SUPPORT_BASE/icon.png"
  ICONSET_DIR="$(mktemp -d)/ContextBraid.iconset"
  mkdir -p "$ICONSET_DIR"
  for spec in \
    "16:icon_16x16.png" \
    "32:icon_16x16@2x.png" \
    "32:icon_32x32.png" \
    "64:icon_32x32@2x.png" \
    "128:icon_128x128.png" \
    "256:icon_128x128@2x.png" \
    "256:icon_256x256.png" \
    "512:icon_256x256@2x.png" \
    "512:icon_512x512.png" \
    "1024:icon_512x512@2x.png"; do
    sz="${spec%%:*}"
    nm="${spec##*:}"
    sips -z "$sz" "$sz" "$ROOT_DIR/docs/icon-squircle.png" --out "$ICONSET_DIR/$nm" >/dev/null
  done
  iconutil -c icns "$ICONSET_DIR" -o "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
  rm -rf "$(dirname "$ICONSET_DIR")"
  touch "$APP_BUNDLE"
  LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
  if [ -x "$LSREG" ]; then
    "$LSREG" -f "$APP_BUNDLE" >/dev/null 2>&1 || true
  fi
  echo "  ✓ ~/Library/Application Support/ContextBraid/ContextBraid.app/Contents/Resources/AppIcon.icns"
fi

echo "✅ All squircle icons synchronized from docs/icons/"
