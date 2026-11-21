#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/packages/raycast/metadata"
mkdir -p "${OUT_DIR}"

echo "1/4 Compilation 100% SWC de l'extension Raycast..."
cd "${ROOT_DIR}/packages/raycast"
pnpm build >/dev/null

# Helper Swift : trouve le CGWindowID visible de Raycast (alpha > 0.5)
find_raycast_win() {
  local min_w="$1"
  local min_h="$2"
  swift -e "
import AppKit
import CoreGraphics

guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: \"com.raycast.macos\").first else {
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
              let width = bounds[\"Width\"] as? Double,
              let height = bounds[\"Height\"] as? Double,
              width > ${min_w} && height > ${min_h},
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
"
}

echo "2/4 Capture du Cockpit principal (01-project-cockpit.png)..."
open "raycast://extensions/elcoosp/context-braid-raycast/index"
sleep 1.6

WIN_1=$(find_raycast_win 500 300)
if [[ -z "${WIN_1}" ]]; then
  echo "❌ Fenêtre principale Raycast introuvable." >&2
  exit 1
fi
RAW_1="/tmp/cb_raycast_raw_1.png"
screencapture -l "${WIN_1}" -o -x "${RAW_1}"

echo "3/4 Ouverture du Workspace Status Menu & capture (02-menu-bar.png)..."
# Ferme la fenêtre principale avec Échap puis active le menu barre Raycast
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 0.4
open "raycast://extensions/elcoosp/context-braid-raycast/menu-bar" 2>/dev/null || true
sleep 1.4

WIN_2=$(find_raycast_win 200 120 || true)
RAW_2="/tmp/cb_raycast_raw_2.png"
if [[ -n "${WIN_2}" ]]; then
  screencapture -l "${WIN_2}" -o -x "${RAW_2}"
fi

echo "4/4 Composition Store 2000x1250 px (16:10) & validation ray lint..."
python3 - "${RAW_1}" "${RAW_2}" "${OUT_DIR}" << 'PY'
import sys, os
from PIL import Image, ImageDraw, ImageFilter, ImageChops

_, raw1, raw2, out_dir = sys.argv

def compose_screenshot(src_path: str, dst_path: str, max_w_ratio: float = 0.84, max_h_ratio: float = 0.82):
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
    max_w, max_h = int(W * max_w_ratio), int(H * max_h_ratio)
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

dst2 = os.path.join(out_dir, "02-menu-bar.png")
if os.path.exists(raw2):
    im1 = Image.open(raw1).convert("RGB")
    im2 = Image.open(raw2).convert("RGB")
    if im1.size != im2.size or ImageChops.difference(im1, im2).getbbox() is not None:
        compose_screenshot(raw2, dst2, max_w_ratio=0.65, max_h_ratio=0.78)
    elif os.path.exists(dst2):
        os.remove(dst2)
PY

rm -f "${RAW_1}" "${RAW_2}"
npx ray lint
