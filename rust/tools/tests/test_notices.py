"""Unit tests for rust/tools/notices.py."""

from __future__ import annotations

import contextlib
import io
import json
import pathlib
import sys

sys.dont_write_bytecode = True

import tempfile
import unittest

TOOLS_DIR = pathlib.Path(__file__).resolve().parents[1]
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))

import notices  # noqa: E402


def _make_crate(
    root: pathlib.Path,
    name: str,
    version: str,
    license_expr: str | None,
    files: dict[str, str],
    *,
    is_local: bool = False,
    repo: str | None = None,
    is_proc_macro: bool = False,
) -> tuple[dict, str]:
    pkg_dir = root / f"{name}-{version}"
    pkg_dir.mkdir(parents=True, exist_ok=True)
    manifest = pkg_dir / "Cargo.toml"
    manifest.write_text(f'[package]\nname = "{name}"\nversion = "{version}"\n', encoding="utf-8")
    for rel, content in files.items():
        target = pkg_dir / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8", newline="")

    pkg_id = f"{name}#{version}"
    pkg = {
        "id": pkg_id,
        "name": name,
        "version": version,
        "license": license_expr,
        "license_file": None,
        "repository": repo,
        "manifest_path": str(manifest),
        "source": None if is_local else "registry+https://github.com/rust-lang/crates.io-index",
        "targets": [{"kind": ["proc-macro" if is_proc_macro else "lib"]}],
    }
    return pkg, pkg_id


class NoticesTests(unittest.TestCase):
    def test_closure_dedup_and_reuse_selection(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            mit_text = "MIT License\n\nCopyright (c) 2026 Example\n"

            app_pkg, app_id = _make_crate(
                root, "ocy-app", "0.2.0-dev", "AGPL-3.0-only", {}, is_local=True
            )
            a_pkg, a_id = _make_crate(
                root,
                "crate-alpha",
                "1.0.0",
                "MIT",
                {"LICENSE": mit_text},
                repo="https://example.com/alpha",
            )
            b_pkg, b_id = _make_crate(
                root,
                "crate-beta",
                "2.0.0",
                "MIT",
                {"LICENSE-MIT": mit_text.replace("\n", "\r\n") + "\n"},
                repo="https://example.com/beta",
            )
            slint_pkg, slint_id = _make_crate(
                root,
                "slint",
                "1.19.0",
                "GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0",
                {
                    "LICENSES/GPL-3.0-only.txt": "GNU GPL v3 text\n",
                    "LICENSES/LicenseRef-Slint-Royalty-free-2.0.md": "Commercial/Royalty-free text\n",
                },
                repo="https://github.com/slint-ui/slint",
            )
            dev_pkg, dev_id = _make_crate(
                root, "dev-only", "0.1.0", None, {}  # missing license, but dev-only -> ignored
            )
            build_pkg, build_id = _make_crate(
                root, "build-only", "0.1.0", None, {}  # missing license, but build-only -> ignored
            )

            metadata = {
                "packages": [app_pkg, a_pkg, b_pkg, slint_pkg, dev_pkg, build_pkg],
                "resolve": {
                    "nodes": [
                        {
                            "id": app_id,
                            "deps": [
                                {"pkg": a_id, "dep_kinds": [{"kind": None, "target": None}]},
                                {"pkg": slint_id, "dep_kinds": [{"kind": None, "target": None}]},
                                {"pkg": dev_id, "dep_kinds": [{"kind": "dev", "target": None}]},
                                {"pkg": build_id, "dep_kinds": [{"kind": "build", "target": None}]},
                            ],
                        },
                        {
                            "id": a_id,
                            "deps": [
                                {"pkg": b_id, "dep_kinds": [{"kind": None, "target": None}]},
                            ],
                        },
                        {"id": b_id, "deps": []},
                        {"id": slint_id, "deps": []},
                        {"id": dev_id, "deps": []},
                        {"id": build_id, "deps": []},
                    ]
                },
            }
            meta_file = root / "meta.json"
            meta_file.write_text(json.dumps(metadata), encoding="utf-8")
            out_file = root / "THIRD-PARTY-NOTICES.txt"

            rc = notices.main(["--metadata-json", str(meta_file), "--out", str(out_file)])
            self.assertEqual(rc, 0)
            text = out_file.read_text(encoding="utf-8")

            self.assertIn("crate-alpha 1.0.0", text)
            self.assertIn("crate-beta 2.0.0", text)
            self.assertIn("slint 1.19.0", text)
            self.assertNotIn("dev-only", text)
            self.assertNotIn("build-only", text)
            self.assertNotIn("Commercial/Royalty-free text", text)
            self.assertIn("GNU GPL v3 text", text)
            # MIT text deduplicated once under [LIC-001]
            self.assertEqual(text.count("Copyright (c) 2026 Example"), 1)
            self.assertIn("Unique license/notice texts: 2", text)

    def test_cargo_tree_closure_scope_and_vendored_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            app_pkg, _ = _make_crate(
                root, "ocy-app", "0.2.0-dev", "AGPL-3.0-only", {}, is_local=True
            )
            # Mapped crate WITHOUT source licence files -> uses vendored text
            taffy_pkg, _ = _make_crate(
                root,
                "taffy",
                "0.15.1",
                "MIT",
                {},
                repo="https://github.com/DioxusLabs/taffy",
            )
            # Mapped proc-macro-only crate WITHOUT source licence files -> uses vendored text
            rspolib_pkg, _ = _make_crate(
                root,
                "rspolib",
                "0.1.2",
                "MIT",
                {},
                repo="https://github.com/mondeja/rspolib",
            )
            # Mapped crate WITH its own source licence file -> source file wins over vendored
            clip_pkg, _ = _make_crate(
                root,
                "clipboard-win",
                "5.4.3",
                "BSL-1.0",
                {"LICENSE": "Own shipped BSL text\n"},
                repo="https://github.com/DoumanAsh/clipboard-win",
            )
            # Test-only crate present in metadata, absent from cargo tree -> excluded
            skia_pkg, _ = _make_crate(root, "skia-safe", "0.157.0", "MIT", {})

            extra_dir = root / "licenses-extra"
            (extra_dir / "taffy").mkdir(parents=True)
            (extra_dir / "taffy" / "LICENSE").write_text("Vendored Taffy MIT\n", encoding="utf-8")
            (extra_dir / "rspolib").mkdir(parents=True)
            (extra_dir / "rspolib" / "LICENSE").write_text("Vendored rspolib MIT\n", encoding="utf-8")
            (extra_dir / "clipboard-win").mkdir(parents=True)
            (extra_dir / "clipboard-win" / "LICENSE").write_text(
                "Vendored clipboard-win BSL\n", encoding="utf-8"
            )

            meta_file = root / "meta.json"
            meta_file.write_text(
                json.dumps(
                    {
                        "packages": [app_pkg, taffy_pkg, rspolib_pkg, clip_pkg, skia_pkg],
                    }
                ),
                encoding="utf-8",
            )
            tree_file = root / "tree-all.txt"
            tree_file.write_text(
                "\n".join(
                    [
                        f"ocy-app v0.2.0-dev ({root / 'ocy-app-0.2.0-dev'})",
                        "taffy v0.15.1",
                        "clipboard-win v5.4.3",
                        "rspolib v0.1.2 (proc-macro)",
                        "taffy v0.15.1 (*)",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            linked_file = root / "tree-linked.txt"
            linked_file.write_text(
                "\n".join(
                    [
                        f"ocy-app v0.2.0-dev ({root / 'ocy-app-0.2.0-dev'})",
                        "taffy v0.15.1",
                        "clipboard-win v5.4.3",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            out_file = root / "THIRD-PARTY-NOTICES.txt"

            rc = notices.main(
                [
                    "--metadata-json",
                    str(meta_file),
                    "--tree-file",
                    str(tree_file),
                    "--linked-tree-file",
                    str(linked_file),
                    "--extra-dir",
                    str(extra_dir),
                    "--out",
                    str(out_file),
                ]
            )
            self.assertEqual(rc, 0)
            text = out_file.read_text(encoding="utf-8")

            self.assertIn(
                "Third-party crates in normal dependency closure: 3 (2 linked into Ocy.exe, 1 proc-macro/build-time)",
                text,
            )
            self.assertNotIn("skia-safe", text)
            self.assertIn("Vendored Taffy MIT", text)
            self.assertIn("Vendored rspolib MIT", text)
            self.assertIn("Own shipped BSL text", text)
            self.assertNotIn("Vendored clipboard-win BSL", text)
            self.assertIn("(taffy/LICENSE; licence text from upstream repository)", text)
            self.assertIn("(rspolib/LICENSE; licence text from upstream repository)", text)
            self.assertIn("Scope:      linked into Ocy.exe", text)
            self.assertIn("Scope:      proc-macro/build-time", text)
            # clipboard-win used its own LICENSE, so no upstream-repository label on its entry
            self.assertIn("  Notices:    [LIC-001] (LICENSE)\n", text)

    def test_fails_on_missing_metadata_and_unmapped_missing_license_text(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            app_pkg, app_id = _make_crate(
                root, "ocy-app", "0.2.0-dev", "AGPL-3.0-only", {}, is_local=True
            )
            no_meta_pkg, no_meta_id = _make_crate(
                root, "no-meta", "0.1.0", None, {"LICENSE": "MIT\n"}
            )
            no_file_pkg, no_file_id = _make_crate(
                root, "no-file", "0.2.0", "MIT", {"README.md": "hi\n"}
            )
            empty_file_pkg, empty_file_id = _make_crate(
                root, "empty-file", "0.3.0", "MIT", {"LICENSE": "   \n"}
            )

            metadata = {
                "packages": [app_pkg, no_meta_pkg, no_file_pkg, empty_file_pkg],
                "resolve": {
                    "nodes": [
                        {
                            "id": app_id,
                            "deps": [
                                {"pkg": no_meta_id, "dep_kinds": [{"kind": None}]},
                                {"pkg": no_file_id, "dep_kinds": [{"kind": None}]},
                                {"pkg": empty_file_id, "dep_kinds": [{"kind": None}]},
                            ],
                        },
                        {"id": no_meta_id, "deps": []},
                        {"id": no_file_id, "deps": []},
                        {"id": empty_file_id, "deps": []},
                    ]
                },
            }
            meta_file = root / "meta.json"
            meta_file.write_text(json.dumps(metadata), encoding="utf-8")
            out_file = root / "THIRD-PARTY-NOTICES.txt"

            stderr_buf = io.StringIO()
            with contextlib.redirect_stderr(stderr_buf):
                rc = notices.main(["--metadata-json", str(meta_file), "--out", str(out_file)])

            self.assertEqual(rc, 1)
            self.assertFalse(out_file.exists())
            err = stderr_buf.getvalue()
            self.assertIn("no-meta 0.1.0", err)
            self.assertIn("no-file 0.2.0", err)
            self.assertIn("empty-file 0.3.0", err)


if __name__ == "__main__":
    unittest.main()
