"""Unit tests for rust/tools/path_leak_check.py."""

from __future__ import annotations

import pathlib
import sys

sys.dont_write_bytecode = True

import tempfile
import unittest

TOOLS_DIR = pathlib.Path(__file__).resolve().parents[1]
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))

import path_leak_check  # noqa: E402


class ByteCountsTests(unittest.TestCase):
    def test_clean_payload_has_zero_counts(self) -> None:
        data = b"cargo/registry/src/some-crate-1.0.0/src/lib.rs" + bytes(range(256))
        for fragment, _encoding, count in path_leak_check.byte_counts(data):
            self.assertEqual(count, 0, f"{fragment} should not match a clean payload")

    def test_counts_every_fragment_in_both_encodings(self) -> None:
        for fragment in path_leak_check.FORBIDDEN:
            data = fragment.encode("utf-8") + b"\x00" + fragment.encode("utf-16-le")
            counts = {
                (frag, enc): count
                for frag, enc, count in path_leak_check.byte_counts(data)
                if frag == fragment
            }
            self.assertEqual(
                counts[(fragment, "utf-8")], 1, f"utf-8 count wrong for {fragment}"
            )
            self.assertEqual(
                counts[(fragment, "utf-16-le")],
                1,
                f"utf-16-le count wrong for {fragment}",
            )

    def test_counts_overlapping_occurrences(self) -> None:
        data = b"C:\\Users\\runneradmin\\x" + b"C:\\Users\\runneradmin\\y"
        total = sum(
            count
            for fragment, _enc, count in path_leak_check.byte_counts(data)
            if fragment == "runneradmin"
        )
        self.assertEqual(total, 2)


class MainTests(unittest.TestCase):
    def _write(self, directory: pathlib.Path, name: str, payload: bytes) -> pathlib.Path:
        path = directory / name
        path.write_bytes(payload)
        return path

    def test_clean_binary_exits_zero(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            binary = self._write(pathlib.Path(tmp), "Ocy", b"clean payload")
            self.assertEqual(path_leak_check.main([str(binary)]), 0)

    def test_leaking_binary_exits_one(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            first = self._write(root, "first", b"/home/runner/.cargo/registry")
            second = self._write(root, "second", b"rustup/apps/ocy-app/src/main.rs")
            self.assertEqual(path_leak_check.main([str(first), str(second)]), 1)

    def test_missing_file_exits_nonzero(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            missing = pathlib.Path(tmp) / "missing"
            with self.assertRaises(SystemExit) as raised:
                path_leak_check.main([str(missing)])
            self.assertNotEqual(raised.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
