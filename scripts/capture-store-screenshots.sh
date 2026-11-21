#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/packages/raycast/metadata"
mkdir -p "${OUT_DIR}"

# 1. Si tu as pris une capture native Raycast (Cmd + Shift + M -> "Save Screenshot for Store")
#    dans les 10 dernières minutes sur ~/Desktop, on l'importe directement !
RECENT_NATIVE=$(find "${HOME}/Desktop" -maxdepth 1 -name "Raycast*.png" -mmin -10 2>/dev/null | sort | tail -n 1 || true)
if [[ -n "${RECENT_NATIVE}" ]]; then
  echo "Importation de la capture native Raycast Store : ${RECENT_NATIVE}"
  cp -f "${RECENT_NATIVE}" "${OUT_DIR}/01-project-cockpit.png"
  cd "${ROOT_DIR}/packages/raycast"
  npx ray lint
  exit 0
fi

# 2. Sinon, ouvre directement le deeplink de Open Project Cockpit et vérifie que la fenêtre est visible (alpha > 0.5)
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
  echo "Astuce : Ouvre 'Open Project Cockpit' dans Raycast, appuie sur Cmd + Shift + M (Save Screenshot for Store), puis relance 'just screenshots' !" >&2
  exit 1
fi

RAW_1="/tmp/cb_raycast_raw_1.png"
screencapture -l "${RAYCAST_WIN_ID}" -o -x "${RAW_1}"

python3 - "${RAW_1}" "${OUT_DIR}/01-project-cockpit.png" << 'PY'
import sys
from PIL import Image, ImageDraw, ImageFilter

_, src_path, dst_path = sys.argv
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
print(f"Saved {dst_path} ({W}x{H})")
PY

rm -f "${RAW_1}"
cd "${ROOT_DIR}/packages/raycast"
npx ray lint
