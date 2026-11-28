#!/usr/bin/env python3
"""Gate release binaries on leaked build-machine paths (CR2c).

Counts the forbidden path fragments in the binary (UTF-8 and UTF-16LE bytes)
and exits 1 when any count is non-zero, so a CI release job fails loudly
instead of shipping paths like C:\\Users\\runneradmin or /home/runner.
"""

from __future__ import annotations

import argparse
import sys

FORBIDDEN: tuple[str, ...] = (
    "runneradmin",
    "/home/",
    "/Users/",
    "C:\\Users",
)


def byte_counts(data: bytes) -> list[tuple[str, str, int]]:
    """Per-fragment counts over UTF-8 and UTF-16LE encodings of the payload."""
    counts: list[tuple[str, str, int]] = []
    for fragment in FORBIDDEN:
        for encoding in ("utf-8", "utf-16-le"):
            needle = fragment.encode(encoding)
            counts.append((fragment, encoding, data.count(needle)))
    return counts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "binary",
        nargs="+",
        type=argparse.FileType("rb"),
        help="built release binary (or several, e.g. per-architecture slices)",
    )
    args = parser.parse_args(argv)

    leaked = False
    for binary in args.binary:
        data = binary.read()
        print(f"{binary.name}: {len(data)} bytes")
        for fragment, encoding, count in byte_counts(data):
            if count:
                leaked = True
            print(f"  {fragment} [{encoding}]: {count}")
    if leaked:
        print("ERROR: build-machine paths leaked into the binary", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
