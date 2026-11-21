#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/packages/raycast/metadata"
mkdir -p "${OUT_DIR}"

echo "1/4 Ouverture de ContextBraid dans Raycast..."
open -a Raycast
sleep 0.4
open "raycast://extensions/elcoosp/context-braid-raycast/index"
sleep 1.6

# Récupère le CGWindowID de la fenêtre principale de Raycast (tous layers >= 0, largeur > 400)
RAYCAST_WIN_ID=$(swift -e '
import CoreGraphics
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
if let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] {
    var bestId: Int?
    var bestArea: Double = 0
    for w in list {
        let owner = (w[kCGWindowOwnerName as String] as? String) ?? ""
        if owner.localizedCaseInsensitiveContains("Raycast"),
           let bounds = w[kCGWindowBounds as String] as? [String: Any],
           let width = bounds["Width"] as? Double,
           let height = bounds["Height"] as? Double,
           width > 400 && height > 250,
           let id = w[kCGWindowNumber as String] as? Int {
            let area = width * height
            if area > bestArea {
                bestArea = area
                bestId = id
            }
        }
    }
    if let id = bestId {
        print(id)
    }
}
')

if [[ -z "${RAYCAST_WIN_ID}" ]]; then
  echo "Erreur : Fenêtre Raycast introuvable à l'écran (vérifie la permission Enregistrement de l'écran pour ton terminal dans Réglages Système -> Confidentialité et sécurité)." >&2
  exit 1
fi

RAW_1="/tmp/cb_raycast_raw_1.png"
RAW_2="/tmp/cb_raycast_raw_2.png"

echo "2/4 Capture Retina de la fenêtre Raycast (ID=${RAYCAST_WIN_ID})..."
screencapture -l "${RAYCAST_WIN_ID}" -o -x "${RAW_1}"

osascript -e 'tell application "System Events" to keystroke "i" using {command down}' 2>/dev/null || true
sleep 0.8
screencapture -l "${RAYCAST_WIN_ID}" -o -x "${RAW_2}"
osascript -e 'tell application "System Events" to keystroke "i" using {command down}' 2>/dev/null || true

echo "3/4 Composition Store 2000x1250 px (16:10) avec fond dégradé & ombre portée..."
python3 - "${RAW_1}" "${RAW_2}" "${OUT_DIR}" << 'PY'
import sys, os
from PIL import Image, ImageDraw, ImageFilter

raw1, raw2, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]

def make_gradient_bg(w: int, h: int) -> Image.Image:
    base = Image.new("RGBA", (w, h), (11, 16, 32, 255))
    draw = ImageDraw.Draw(base)
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(11 + t * 10)
        g = int(16 + t * 21)
        b = int(32 + t * 34)
        draw.line([(0, y), (w, y)], fill=(r, g, b, 255))

    glow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(glow)
    gdraw.ellipse((w * 0.45, -h * 0.25, w * 1.15, h * 0.65), fill=(45, 140, 255, 42))
    gdraw.ellipse((-w * 0.2, h * 0.45, w * 0.55, h * 1.25), fill=(94, 92, 230, 34))
    glow = glow.filter(ImageFilter.GaussianBlur(radius=140))
    return Image.alpha_composite(base, glow)

def compose_screenshot(src_path: str, dst_path: str):
    W, H = 2000, 1250
    bg = make_gradient_bg(W, H)

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

compose_screenshot(raw1, os.path.join(out_dir, "01-project-cockpit.png"))
if os.path.exists(raw2):
    compose_screenshot(raw2, os.path.join(out_dir, "02-workspace-overview.png"))
PY

rm -f "${RAW_1}" "${RAW_2}"

echo "4/4 Validation officielle Raycast Store (npx ray lint)..."
cd "${ROOT_DIR}/packages/raycast"
npx ray lint

echo ">>> Captures Store générées avec succès dans ${OUT_DIR} !"
