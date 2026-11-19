"""Unit tests for rust/tools/dist.py."""

from __future__ import annotations

import hashlib
import os
import pathlib
import sys

sys.dont_write_bytecode = True
import tempfile
import unittest
import zipfile

TOOLS_DIR = pathlib.Path(__file__).resolve().parents[1]
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))

import dist  # noqa: E402


class DistTests(unittest.TestCase):
    def _create_inputs(self, root: pathlib.Path, *, workspace_version: bool = True) -> dict[str, pathlib.Path]:
        ws = root / "rust"
        app_dir = ws / "apps" / "ocy" / "ocy-app"
        app_dir.mkdir(parents=True)
        if workspace_version:
            (ws / "Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.2.0"\n', encoding="utf-8"
            )
            cargo_toml = app_dir / "Cargo.toml"
            cargo_toml.write_text(
                '[package]\nname = "ocy-app"\nversion.workspace = true\n',
                encoding="utf-8",
            )
        else:
            cargo_toml = app_dir / "Cargo.toml"
            cargo_toml.write_text(
                '[package]\nname = "ocy-app"\nversion = "0.2.9"\n',
                encoding="utf-8",
            )

        exe = root / "Ocy.exe"
        exe.write_bytes(b"MZ" + b"\x00" * 128)
        notices_file = root / "THIRD-PARTY-NOTICES.txt"
        notices_file.write_text("Notice text\n", encoding="utf-8")
        license_file = root / "LICENSE"
        license_file.write_text("AGPL-3.0-only\n", encoding="utf-8")
        return {
            "exe": exe,
            "notices": notices_file,
            "license": license_file,
            "cargo_toml": cargo_toml,
        }

    def test_zip_contents_permissions_and_sha256_format(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            inputs = self._create_inputs(root)
            out_dir = root / "dist"

            rc = dist.main(
                [
                    "--exe",
                    str(inputs["exe"]),
                    "--notices",
                    str(inputs["notices"]),
                    "--out",
                    str(out_dir),
                    "--license",
                    str(inputs["license"]),
                    "--cargo-toml",
                    str(inputs["cargo_toml"]),
                ]
            )
            self.assertEqual(rc, 0)

            zip_path = out_dir / "Ocy-0.2.0-win-x64.zip"
            sha_path = out_dir / "Ocy-0.2.0-win-x64.zip.sha256"
            self.assertTrue(zip_path.is_file())
            self.assertTrue(sha_path.is_file())

            with zipfile.ZipFile(zip_path, "r") as zf:
                infos = zf.infolist()
                names = [i.filename for i in infos]
                self.assertEqual(
                    names,
                    [
                        "Ocy-0.2.0-win-x64/LICENSE.txt",
                        "Ocy-0.2.0-win-x64/Ocy.exe",
                        "Ocy-0.2.0-win-x64/README.txt",
                        "Ocy-0.2.0-win-x64/THIRD-PARTY-NOTICES.txt",
                    ],
                )
                for info in infos:
                    self.assertEqual(info.date_time, (2026, 1, 1, 0, 0, 0))
                    expected_mode = 0o100755 if info.filename.endswith(".exe") else 0o100644
                    self.assertEqual(info.external_attr >> 16, expected_mode)
                readme = zf.read("Ocy-0.2.0-win-x64/README.txt").decode("utf-8")
                self.assertIn("%APPDATA%\\Ocy", readme)
                self.assertIn("Quit", readme)
                self.assertIn("Uninstall", readme)

            sha_line = sha_path.read_text(encoding="utf-8")
            expected_hex = hashlib.sha256(zip_path.read_bytes()).hexdigest()
            self.assertEqual(sha_line, f"{expected_hex}  Ocy-0.2.0-win-x64.zip\n")

    def test_determinism_across_runs_and_mtimes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            inputs = self._create_inputs(root, workspace_version=False)

            out1 = root / "out1"
            out2 = root / "out2"
            zip1, sha1 = dist.build_dist(
                inputs["exe"], inputs["notices"], out1, inputs["license"], inputs["cargo_toml"]
            )

            # Mutate input mtimes to ensure zip timestamps do not leak host mtime.
            for p in (inputs["exe"], inputs["notices"], inputs["license"]):
                os.utime(p, (1700000000, 1700000000))

            zip2, sha2 = dist.build_dist(
                inputs["exe"], inputs["notices"], out2, inputs["license"], inputs["cargo_toml"]
            )

            self.assertEqual(zip1.read_bytes(), zip2.read_bytes())
            self.assertEqual(sha1.read_bytes(), sha2.read_bytes())

    def test_reads_real_ocy_app_version(self) -> None:
        real_toml = (
            pathlib.Path(__file__).resolve().parents[2]
            / "apps"
            / "ocy"
            / "ocy-app"
            / "Cargo.toml"
        )
        version = dist.read_app_version(real_toml)
        self.assertTrue(version.startswith("0.2.0"))


if __name__ == "__main__":
    unittest.main()
