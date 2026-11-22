#!/usr/bin/env python3
"""
packaging/windows/generate_assets.py

Generates MSIX visual assets (scale and targetsize variants, including unplated
and light-theme unplated variants) from the source sprout PNGs in assets/icons/app/png/.
Keeps all source icons untouched.

Usage:
    python3 packaging/windows/generate_assets.py
    python3 packaging/windows/generate_assets.py --check
"""

from __future__ import annotations

import argparse
import io
import math
import sys
from pathlib import Path

from PIL import Image

SCALES: tuple[int, ...] = (100, 125, 150, 200, 400)
TARGET_SIZES: tuple[int, ...] = (16, 20, 24, 32, 40, 48, 64, 256)


def ceil_scale(base: int, scale_pct: int) -> int:
    return math.ceil(base * scale_pct / 100.0)


def load_source_pngs(src_dir: Path) -> dict[int, Image.Image]:
    sizes = (16, 20, 24, 32, 40, 48, 64, 128, 256)
    loaded: dict[int, Image.Image] = {}
    for sz in sizes:
        path = src_dir / f"ocy-app-{sz}.png"
        if not path.is_file():
            raise FileNotFoundError(f"Missing source icon: {path}")
        with Image.open(path) as im:
            loaded[sz] = im.convert("RGBA")
    return loaded


def render_square_icon(sources: dict[int, Image.Image], size: int) -> Image.Image:
    if size in sources:
        return sources[size].copy()
    master = sources[256]
    return master.resize((size, size), Image.Resampling.LANCZOS)


def render_padded_tile(
    sources: dict[int, Image.Image],
    width: int,
    height: int,
    icon_ratio: float = 0.66,
) -> Image.Image:
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    icon_size = max(1, round(min(width, height) * icon_ratio))
    icon = render_square_icon(sources, icon_size)
    offset_x = (width - icon_size) // 2
    offset_y = (height - icon_size) // 2
    canvas.paste(icon, (offset_x, offset_y), icon)
    return canvas


def encode_png(image: Image.Image) -> bytes:
    buf = io.BytesIO()
    image.save(buf, format="PNG", compress_level=9)
    return buf.getvalue()


def build_asset_map(sources: dict[int, Image.Image]) -> dict[str, bytes]:
    assets: dict[str, bytes] = {}

    # StoreLogo (50x50 base)
    assets["StoreLogo.png"] = encode_png(render_square_icon(sources, 50))
    for scale in SCALES:
        sz = ceil_scale(50, scale)
        assets[f"StoreLogo.scale-{scale}.png"] = encode_png(render_square_icon(sources, sz))

    # Square44x44Logo (44x44 base + scale + targetsize + unplated + lightunplated)
    assets["Square44x44Logo.png"] = encode_png(render_square_icon(sources, 44))
    for scale in SCALES:
        sz = ceil_scale(44, scale)
        assets[f"Square44x44Logo.scale-{scale}.png"] = encode_png(
            render_square_icon(sources, sz)
        )
    for sz in TARGET_SIZES:
        png = encode_png(render_square_icon(sources, sz))
        assets[f"Square44x44Logo.targetsize-{sz}.png"] = png
        assets[f"Square44x44Logo.targetsize-{sz}_altform-unplated.png"] = png
        assets[f"Square44x44Logo.targetsize-{sz}_altform-lightunplated.png"] = png

    # Square150x150Logo (150x150 base + scale)
    assets["Square150x150Logo.png"] = encode_png(render_padded_tile(sources, 150, 150))
    for scale in SCALES:
        sz = ceil_scale(150, scale)
        assets[f"Square150x150Logo.scale-{scale}.png"] = encode_png(
            render_padded_tile(sources, sz, sz)
        )

    # Wide310x150Logo (310x150 base + scale)
    assets["Wide310x150Logo.png"] = encode_png(render_padded_tile(sources, 310, 150))
    for scale in SCALES:
        w = ceil_scale(310, scale)
        h = ceil_scale(150, scale)
        assets[f"Wide310x150Logo.scale-{scale}.png"] = encode_png(
            render_padded_tile(sources, w, h)
        )

    return assets


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Generate Ocy MSIX logo assets from assets/icons/app/png/."
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify committed assets match fresh generation without writing.",
    )
    args = parser.parse_args(argv)

    repo_root = Path(__file__).resolve().parents[2]
    src_dir = repo_root / "assets" / "icons" / "app" / "png"
    out_dir = Path(__file__).resolve().parent / "assets"

    sources = load_source_pngs(src_dir)
    expected = build_asset_map(sources)

    if args.check:
        if not out_dir.is_dir():
            print(f"Missing MSIX assets directory: {out_dir}", file=sys.stderr)
            return 1
        actual_files = {p.name for p in out_dir.iterdir() if p.is_file()}
        expected_files = set(expected.keys())
        if actual_files != expected_files:
            missing = sorted(expected_files - actual_files)
            extra = sorted(actual_files - expected_files)
            print(
                f"MSIX assets mismatch: missing={missing}, extra={extra}",
                file=sys.stderr,
            )
            return 1
        for name, data in sorted(expected.items()):
            current = (out_dir / name).read_bytes()
            if current != data:
                print(
                    f"MSIX asset differs from fresh generation: {out_dir / name}",
                    file=sys.stderr,
                )
                return 1
        print(f"generate_assets: --check passed ({len(expected)} assets).")
        return 0

    out_dir.mkdir(parents=True, exist_ok=True)
    for existing in out_dir.iterdir():
        if existing.is_file() and existing.name not in expected:
            existing.unlink()
    for name, data in sorted(expected.items()):
        (out_dir / name).write_bytes(data)
    print(f"generate_assets: wrote {len(expected)} assets to {out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
