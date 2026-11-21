#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/packages/raycast/metadata"
mkdir -p "${OUT_DIR}"

echo "1/4 Lancement de Raycast en mode dev pour ouvrir Open Project Cockpit..."
cd "${ROOT_DIR}/packages/raycast"
npx ray develop --non-interactive >/dev/null 2>&1 &
DEV_PID=$!
trap 'kill ${DEV_PID} 2>/dev/null || true' EXIT

sleep 2.2
open "raycast://extensions/elcoosp/context-braid-raycast/index"
sleep 1.6

RAYCAST_WIN_ID=$(swift -e '
import AppKit
import CoreGraphics

guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.raycast.macos").first else {
    exit(1)
}
let pid = app.processIdentifier
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
if let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] {
    var bestId: Int?
    var bestArea: Double = 0
    for w in list {
        guard let ownerPid = w[kCGWindowOwnerPID as String] as? pid_t, ownerPid == pid,
              let alpha = w[kCGWindowAlpha as String] as? Double, alpha > 0.5,
              let bounds = w[kCGWindowBounds as String] as? [String: Any],
              let width = bounds["Width"] as? Double,
              let height = bounds["Height"] as? Double,
              width > 500 && height > 300,
              let id = w[kCGWindowNumber as String] as? Int else { continue }
        let area = width * height
        if area > bestArea {
            bestArea = area
            bestId = id
        }
    }
    if let id = bestId {
        print(id)
    }
}
')

if [[ -z "${RAYCAST_WIN_ID}" ]]; then
  echo "❌ Fenêtre Raycast introuvable." >&2
  exit 1
fi

RAW_1="/tmp/cb_raycast_raw_1.png"
RAW_2="/tmp/cb_raycast_raw_2.png"

echo "2/4 Capture Split Pane (01-project-cockpit.png)..."
screencapture -l "${RAYCAST_WIN_ID}" -o -x "${RAW_1}"

echo "3/4 Ouverture du panneau d'actions (Cmd + K) & capture (02-quick-actions.png)..."
osascript -e 'tell application "System Events" to keystroke "k" using {command down}' 2>/dev/null || true
sleep 0.6
screencapture -l "${RAYCAST_WIN_ID}" -o -x "${RAW_2}"
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true

echo "4/4 Composition Store 2000x1250 px (16:10) & validation ray lint..."
python3 - "${RAW_1}" "${RAW_2}" "${OUT_DIR}" << 'PY'
import sys, os
from PIL import Image, ImageDraw, ImageFilter, ImageChops

_, raw1, raw2, out_dir = sys.argv

def compose_screenshot(src_path: str, dst_path: str):
    W, H = 2000, 1250
    base = Image.new("RGBA", (W, H), (11, 16, 32, 255))
    draw = ImageDraw.Draw(base)
    for y in range(H):
        t = y / max(1, H - 1)
        draw.line([(0, y), (W, y)], fill=(int(11 + t * 10), int(16 + t * 21), int(32 + t * 34), 255))

    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(glow)
    gdraw.ellipse((W * 0.45, -H * 0.25, W * 1.15, H * 0.65), fill=(45, 140, 255, 42))
    gdraw.ellipse((-W * 0.2, H * 0.45, W * 0.55, H * 1.25), fill=(94, 92, 230, 34))
    glow = glow.filter(ImageFilter.GaussianBlur(radius=140))
    bg = Image.alpha_composite(base, glow)

    win = Image.open(src_path).convert("RGBA")
    max_w, max_h = int(W * 0.84), int(H * 0.82)
    scale = min(max_w / win.width, max_h / win.height)
    nw, nh = int(win.width * scale), int(win.height * scale)
    win = win.resize((nw, nh), Image.Resampling.LANCZOS)

    radius = 24
    mask = Image.new("L", (nw, nh), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, nw, nh), radius=radius, fill=255)
    rounded_win = Image.new("RGBA", (nw, nh), (0, 0, 0, 0))
    rounded_win.paste(win, (0, 0), mask)

    pad = 90
    shadow = Image.new("RGBA", (nw + pad * 2, nh + pad * 2), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (pad, pad + 18, pad + nw, pad + nh + 18),
        radius=radius,
        fill=(0, 0, 0, 165),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(radius=34))

    x = (W - nw) // 2
    y = (H - nh) // 2
    bg.alpha_composite(shadow, (x - pad, y - pad))
    bg.alpha_composite(rounded_win, (x, y))

    border = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(border).rounded_rectangle(
        (x, y, x + nw - 1, y + nh - 1),
        radius=radius,
        outline=(255, 255, 255, 36),
        width=2,
    )
    bg = Image.alpha_composite(bg, border)
    bg.convert("RGB").save(dst_path, "PNG", optimize=True)
    print(f"✅ Sauvegardé : {dst_path} ({W}x{H})")

compose_screenshot(raw1, os.path.join(out_dir, "01-project-cockpit.png"))

# N'enregistre le 2e screenshot que s'il est réellement différent du 1er (éviter tout doublon)
im1 = Image.open(raw1).convert("RGB")
im2 = Image.open(raw2).convert("RGB")
if im1.size == im2.size and ImageChops.difference(im1, im2).getbbox() is not None:
    compose_screenshot(raw2, os.path.join(out_dir, "02-quick-actions.png"))
else:
    dst2 = os.path.join(out_dir, "02-quick-actions.png")
    if os.path.exists(dst2):
        os.remove(dst2)
PY

rm -f "${RAW_1}" "${RAW_2}"
npx ray lint
