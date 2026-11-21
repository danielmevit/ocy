#!/usr/bin/env python3
"""Release visuals: fits the `readme_screens` renders into the committed frames and builds the exercise strips.

  cargo run -p ocy-ui --example readme_screens -- <render_dir>      (from rust/)
  python3 site/scripts/readme-visuals.py <render_dir>               (from the repo root; needs Pillow)

Writes, keeping every committed filename and pixel size:
  site/public/assets/screens/{light,dark}/{settings,short-break,long-break,warning}.png
  assets/readme/{settings,short-break,long-break,warning}-{light,dark}.png
  assets/readme/exercises-{light,dark}.webp   (animated, 6 panels, 60 ms ticks)
"""
import sys, pathlib
from PIL import Image, ImageDraw

SRC = pathlib.Path(sys.argv[1])
ROOT = pathlib.Path(__file__).resolve().parents[2]
SIZES = {"settings": (1120, 1280), "short-break": (912, 338), "long-break": (1152, 876), "warning": (912, 304)}
STRIP = {"light": ((247, 248, 243), (255, 255, 255)), "dark": ((23, 25, 26), (33, 35, 37))}


def card_only(im):
    """The long-break render includes the dimmed overlay; keep the opaque card (plus its shadow margin)."""
    a = im.getchannel("A").point(lambda v: 255 if v == 255 else 0)
    l, t, r, b = a.getbbox()
    card = im.crop((l, t, r, b))
    # the card's own rounded corners (Theme radius-card 8 px at 2x), cut cleanly out of the overlay
    mask = Image.new("L", card.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, card.width - 1, card.height - 1), 16, fill=255)
    card.putalpha(mask)
    return card


def fit(im, size):
    """Centre on a transparent canvas of `size`, scaling down only if needed."""
    w, h = size
    s = min(1.0, w / im.width, h / im.height)
    if s < 1.0:
        im = im.resize((round(im.width * s), round(im.height * s)), Image.LANCZOS)
    out = Image.new("RGBA", size, (0, 0, 0, 0))
    out.alpha_composite(im, ((w - im.width) // 2, (h - im.height) // 2))
    return out


for theme in ("light", "dark"):
    for name, size in SIZES.items():
        im = Image.open(SRC / theme / f"{name}.png").convert("RGBA")
        if name == "long-break":
            im = card_only(im)
        out = im if im.size == size else fit(im, size)
        out.save(ROOT / "site/public/assets/screens" / theme / f"{name}.png", optimize=True)
        out.save(ROOT / "assets/readme" / f"{name}-{theme}.png", optimize=True)

    bg, panel = STRIP[theme]
    frames = []
    for f in sorted((SRC / "exercises" / theme).glob("*.png")):
        art = Image.open(f).convert("RGBA")
        n = art.width // 300
        im = Image.new("RGBA", (12 + n * 312, 324), bg + (255,))
        d = ImageDraw.Draw(im)
        for i in range(n):
            x = 12 + i * 312
            d.rounded_rectangle((x, 12, x + 299, 311), 14, fill=panel + (255,))
            im.alpha_composite(art.crop((i * 300, 0, i * 300 + 300, 300)), (x, 12))
        frames.append(im.convert("RGB"))
    frames[0].save(ROOT / "assets/readme" / f"exercises-{theme}.webp", save_all=True, append_images=frames[1:],
                   duration=60, loop=0, quality=80, method=6)
print("ok")
