#!/usr/bin/env python3
"""
tools/icons/pack_tray_ico.py
Packs tray sprout PNGs (16, 20, 24, 32) into multi-size .ico files for Ocy desktop
for both light and dark taskbar sets.
"""

import argparse
import struct
import sys
from pathlib import Path

SETS = ["light", "dark"]
STATES = ["active", "paused", "onbreak", "attention"]
SIZES = [16, 20, 24, 32]


def fail(msg: str):
    print(f"pack_tray_ico error: {msg}", file=sys.stderr)
    sys.exit(1)


def read_png_dimensions(data: bytes) -> tuple[int, int]:
    """Extract width and height from PNG IHDR chunk."""
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("Invalid PNG magic header")
    if len(data) < 24:
        raise ValueError("PNG data too short")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def pack_ico(png_data_list: list[bytes]) -> bytes:
    """
    Pack a list of PNG data into a single multi-size ICO binary.
    """
    count = len(png_data_list)
    header = struct.pack("<HHH", 0, 1, count)

    entries = []
    # Calculate offset where image data starts: 6 bytes header + 16 bytes per entry
    offset = 6 + 16 * count

    for png_data in png_data_list:
        w, h = read_png_dimensions(png_data)
        width_byte = w if w < 256 else 0
        height_byte = h if h < 256 else 0
        size = len(png_data)

        # ICONDIRENTRY:
        # BYTE bWidth, BYTE bHeight, BYTE bColorCount, BYTE bReserved,
        # WORD wPlanes, WORD wBitCount, DWORD dwBytesInRes, DWORD dwImageOffset
        entry = struct.pack(
            "<BBBBHHII",
            width_byte,
            height_byte,
            0,      # color count (0 if >= 256 or PNG)
            0,      # reserved
            1,      # planes
            32,     # bit count
            size,
            offset,
        )
        entries.append(entry)
        offset += size

    return header + b"".join(entries) + b"".join(png_data_list)


def generate_ico_for_state(source_dir: Path, state: str) -> bytes:
    png_data_list = []
    for sz in SIZES:
        png_path = source_dir / f"tray-{state}-{sz}.png"
        if not png_path.exists():
            fail(f"Source PNG not found: {png_path}")
        png_data = png_path.read_bytes()
        w, h = read_png_dimensions(png_data)
        if w != sz or h != sz:
            fail(f"Dimension mismatch in {png_path}: expected {sz}x{sz}, got {w}x{h}")
        png_data_list.append(png_data)

    return pack_ico(png_data_list)


def main():
    parser = argparse.ArgumentParser(description="Pack tray PNGs into multi-size ICO files for light and dark sets")
    parser.add_argument("--check", action="store_true", help="Check that committed files match fresh generation")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent.parent
    tray_assets_root = repo_root / "assets" / "icons" / "tray"
    dest_dir = repo_root / "src" / "Ocy.Desktop" / "Assets" / "Tray"

    for s in SETS:
        source_dir = tray_assets_root / s / "png"
        if not source_dir.exists():
            fail(f"Source directory does not exist: {source_dir}")

    generated: dict[tuple[str, str], bytes] = {}
    for s in SETS:
        source_dir = tray_assets_root / s / "png"
        for state in STATES:
            generated[(s, state)] = generate_ico_for_state(source_dir, state)

    if args.check:
        for s in SETS:
            for state in STATES:
                ico_path = dest_dir / s / f"tray-{state}.ico"
                if not ico_path.exists():
                    fail(f"{ico_path} does not exist. Run pack_tray_ico.py to generate it.")
                current_bytes = ico_path.read_bytes()
                if current_bytes != generated[(s, state)]:
                    fail(f"{ico_path} differs from fresh generation. Run pack_tray_ico.py to regenerate.")

        for state in STATES:
            old_ico_path = dest_dir / f"tray-{state}.ico"
            if old_ico_path.exists():
                fail(f"Old flat file still exists: {old_ico_path}. Delete it.")

        print("pack_tray_ico: --check passed. Generated files match fresh generation.")
        sys.exit(0)
    else:
        # Delete old generated .ico files if they exist
        for state in STATES:
            old_ico_path = dest_dir / f"tray-{state}.ico"
            if old_ico_path.exists():
                old_ico_path.unlink()
                print(f"Deleted old {old_ico_path}")

        for s in SETS:
            set_dest_dir = dest_dir / s
            set_dest_dir.mkdir(parents=True, exist_ok=True)
            for state in STATES:
                ico_path = set_dest_dir / f"tray-{state}.ico"
                ico_path.write_bytes(generated[(s, state)])
                print(f"Generated {ico_path}")


if __name__ == "__main__":
    main()
