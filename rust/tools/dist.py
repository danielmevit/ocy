#!/usr/bin/env python3
"""Build a deterministic Windows release zip and .sha256 file for Ocy."""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import tomllib
import zipfile

FIXED_ZIP_DATETIME = (2026, 1, 1, 0, 0, 0)
ZIP_CREATE_SYSTEM_UNIX = 3
FILE_MODE_EXEC = 0o100755
FILE_MODE_TEXT = 0o100644


def read_app_version(cargo_toml: pathlib.Path) -> str:
    """Read package version from ocy-app/Cargo.toml (supports workspace inheritance)."""
    data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    pkg = data.get("package", {})
    version = pkg.get("version")
    if isinstance(version, str) and version.strip():
        return version.strip()

    if isinstance(version, dict) and version.get("workspace") is True:
        for parent in cargo_toml.resolve().parents:
            candidate = parent / "Cargo.toml"
            if candidate == cargo_toml.resolve() or not candidate.is_file():
                continue
            ws_data = tomllib.loads(candidate.read_text(encoding="utf-8"))
            ws_version = ws_data.get("workspace", {}).get("package", {}).get("version")
            if isinstance(ws_version, str) and ws_version.strip():
                return ws_version.strip()

    raise ValueError(f"Could not determine package version from {cargo_toml}")


def build_readme(version: str) -> bytes:
    """Generate short Windows user instructions for the release folder."""
    text = (
        f"Ocy {version} (Windows x64)\n"
        "===========================\n"
        "\n"
        "Run\n"
        "  Extract this folder anywhere and run Ocy.exe.\n"
        "  Ocy starts in the system tray (notification area) with no initial window.\n"
        "  Click or right-click the tray icon to open Settings, pause breaks, or\n"
        "  start a break.\n"
        "\n"
        "Quit\n"
        "  Right-click the Ocy tray icon and choose Quit.\n"
        "\n"
        "Data location\n"
        "  Settings, break state, statistics, and logs are stored in:\n"
        "    %APPDATA%\\Ocy\n"
        "\n"
        "Uninstall\n"
        "  1. Quit Ocy from the tray icon.\n"
        "  2. If \"Start with Windows\" was enabled, turn it off in Settings first\n"
        "     (or remove the Ocy entry from\n"
        "     HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run).\n"
        "  3. Delete this folder and %APPDATA%\\Ocy.\n"
        "\n"
        "License\n"
        "  Ocy is licensed under the GNU Affero General Public License v3.0\n"
        "  (AGPL-3.0-only). See LICENSE.txt and THIRD-PARTY-NOTICES.txt.\n"
    )
    return text.replace("\n", "\r\n").encode("utf-8")


def write_deterministic_zip(
    zip_path: pathlib.Path,
    folder_name: str,
    entries: dict[str, bytes],
) -> None:
    """Write a byte-deterministic zip archive from sorted (relative_name -> bytes)."""
    with zipfile.ZipFile(
        zip_path,
        mode="w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as zf:
        for rel_name in sorted(entries):
            arcname = f"{folder_name}/{rel_name}"
            info = zipfile.ZipInfo(filename=arcname, date_time=FIXED_ZIP_DATETIME)
            info.create_system = ZIP_CREATE_SYSTEM_UNIX
            mode = FILE_MODE_EXEC if rel_name.lower().endswith(".exe") else FILE_MODE_TEXT
            info.external_attr = mode << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            zf.writestr(info, entries[rel_name], compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


def build_dist(
    exe_path: pathlib.Path,
    notices_path: pathlib.Path,
    out_dir: pathlib.Path,
    license_path: pathlib.Path,
    cargo_toml_path: pathlib.Path,
) -> tuple[pathlib.Path, pathlib.Path]:
    """Create the deterministic release zip and matching .sha256 file."""
    for label, path in (
        ("exe", exe_path),
        ("notices", notices_path),
        ("license", license_path),
        ("cargo_toml", cargo_toml_path),
    ):
        if not path.is_file() or path.stat().st_size == 0:
            raise FileNotFoundError(f"Missing or empty {label} file: {path}")

    version = read_app_version(cargo_toml_path)
    stem = f"Ocy-{version}-win-x64"
    out_dir.mkdir(parents=True, exist_ok=True)
    zip_path = out_dir / f"{stem}.zip"
    sha_path = out_dir / f"{stem}.zip.sha256"

    entries = {
        "Ocy.exe": exe_path.read_bytes(),
        "LICENSE.txt": license_path.read_bytes(),
        "THIRD-PARTY-NOTICES.txt": notices_path.read_bytes(),
        "README.txt": build_readme(version),
    }
    write_deterministic_zip(zip_path, stem, entries)

    digest = hashlib.sha256(zip_path.read_bytes()).hexdigest()
    sha_path.write_text(f"{digest}  {zip_path.name}\n", encoding="utf-8", newline="\n")
    return zip_path, sha_path


def main(argv: list[str] | None = None) -> int:
    repo_root = pathlib.Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(
        description="Build deterministic Ocy Windows release zip and .sha256."
    )
    parser.add_argument("--exe", required=True, type=pathlib.Path, help="Path to Ocy.exe")
    parser.add_argument(
        "--notices",
        required=True,
        type=pathlib.Path,
        help="Path to THIRD-PARTY-NOTICES.txt",
    )
    parser.add_argument(
        "--out",
        required=True,
        type=pathlib.Path,
        help="Output directory for .zip and .sha256",
    )
    parser.add_argument(
        "--license",
        type=pathlib.Path,
        default=repo_root / "LICENSE",
        help="Path to repo LICENSE file",
    )
    parser.add_argument(
        "--cargo-toml",
        type=pathlib.Path,
        default=repo_root / "rust" / "apps" / "ocy" / "ocy-app" / "Cargo.toml",
        help="Path to ocy-app Cargo.toml",
    )
    args = parser.parse_args(argv)

    zip_path, sha_path = build_dist(
        exe_path=args.exe,
        notices_path=args.notices,
        out_dir=args.out,
        license_path=args.license,
        cargo_toml_path=args.cargo_toml,
    )
    print(f"dist: {zip_path} ({zip_path.stat().st_size} bytes)")
    print(f"dist: {sha_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
