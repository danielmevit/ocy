#!/usr/bin/env python3
"""Generate THIRD-PARTY-NOTICES.txt for ocy-app from Cargo tree + metadata (stdlib only, offline)."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass

DEFAULT_PACKAGE = "ocy-app"
DEFAULT_TARGET = "x86_64-pc-windows-msvc"
DEFAULT_EXTRA_DIR = pathlib.Path(__file__).resolve().parent / "licenses-extra"
LICENSE_FILE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE")
LICENSE_DIR_NAMES = {"LICENSES", "LICENCES"}
SCOPE_LINKED = "linked"
SCOPE_PROC_MACRO = "proc-macro"

_TREE_LINE_RE = re.compile(r"^([^\s]+)\s+v([^\s]+)")

# Explicit vendored upstream licence texts used ONLY when a crate's own source
# directory contains no licence files. Any other crate missing licence files fails.
VENDORED_LICENSE_FILES: dict[str, tuple[str, ...]] = {
    "clipboard-win": ("clipboard-win/LICENSE",),
    "lyon_algorithms": ("lyon/LICENSE-MIT",),
    "lyon_extra": ("lyon/LICENSE-MIT",),
    "lyon_geom": ("lyon/LICENSE-MIT",),
    "lyon_path": ("lyon/LICENSE-MIT",),
    "taffy": ("taffy/LICENSE",),
    "rspolib": ("rspolib/LICENSE",),
    "accesskit": (
        "accesskit/LICENSE-APACHE",
        "accesskit/LICENSE-MIT",
        "accesskit/LICENSE.chromium",
        "accesskit/AUTHORS",
    ),
    "accesskit_consumer": (
        "accesskit/LICENSE-APACHE",
        "accesskit/LICENSE-MIT",
        "accesskit/LICENSE.chromium",
        "accesskit/AUTHORS",
    ),
    "accesskit_windows": (
        "accesskit/LICENSE-APACHE",
        "accesskit/LICENSE-MIT",
        "accesskit/LICENSE.chromium",
        "accesskit/AUTHORS",
    ),
    "accesskit_winit": (
        "accesskit/LICENSE-APACHE",
        "accesskit/AUTHORS",
    ),
}


@dataclass(frozen=True)
class CrateInfo:
    pkg_id: str
    name: str
    version: str
    license_expr: str | None
    license_file: str | None
    repository: str | None
    manifest_path: pathlib.Path
    is_local: bool
    scope: str = SCOPE_LINKED


@dataclass(frozen=True)
class CrateNotice:
    crate: CrateInfo
    texts: list[tuple[str, str]]  # (relative_filename, normalized_text)
    from_upstream: bool = False


def run_cargo_tree(rust_dir: pathlib.Path, package: str, target: str, edges: str) -> str:
    """Run cargo tree --locked for the given package, target, and edge filter."""
    cmd = [
        "cargo",
        "tree",
        "--locked",
        "-p",
        package,
        "-e",
        edges,
        "--target",
        target,
        "--prefix",
        "none",
    ]
    proc = subprocess.run(
        cmd,
        cwd=rust_dir,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"cargo tree ({edges}) failed (exit {proc.returncode}):\n{proc.stderr.strip()}"
        )
    return proc.stdout


def parse_cargo_tree(tree_output: str) -> set[tuple[str, str]]:
    """Extract unique (name, version) pairs from `cargo tree --prefix none` output."""
    crates: set[tuple[str, str]] = set()
    for raw_line in tree_output.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        match = _TREE_LINE_RE.match(line)
        if not match:
            raise ValueError(f"Unrecognized cargo tree line: {line!r}")
        crates.add((match.group(1), match.group(2)))
    return crates


def run_cargo_metadata(rust_dir: pathlib.Path) -> dict:
    """Run cargo metadata --locked (unfiltered so host proc-macro deps are included)."""
    cmd = [
        "cargo",
        "metadata",
        "--locked",
        "--format-version",
        "1",
    ]
    proc = subprocess.run(
        cmd,
        cwd=rust_dir,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"cargo metadata failed (exit {proc.returncode}):\n{proc.stderr.strip()}"
        )
    return json.loads(proc.stdout)


def _crate_info_from_pkg(pkg: dict, scope: str) -> CrateInfo:
    return CrateInfo(
        pkg_id=pkg["id"],
        name=pkg["name"],
        version=pkg["version"],
        license_expr=pkg.get("license"),
        license_file=pkg.get("license_file"),
        repository=pkg.get("repository"),
        manifest_path=pathlib.Path(pkg["manifest_path"]),
        is_local=pkg.get("source") is None,
        scope=scope,
    )


def closure_from_trees(
    metadata: dict,
    all_crates: set[tuple[str, str]],
    linked_crates: set[tuple[str, str]],
) -> list[CrateInfo]:
    """Build sorted CrateInfo closure from cargo tree sets using cargo metadata fields."""
    pkg_by_nv: dict[tuple[str, str], dict] = {}
    for pkg in metadata["packages"]:
        key = (pkg["name"], pkg["version"])
        pkg_by_nv.setdefault(key, pkg)

    closure: list[CrateInfo] = []
    for key in all_crates:
        pkg = pkg_by_nv.get(key)
        if pkg is None:
            raise ValueError(
                f"Crate {key[0]} {key[1]} from cargo tree not found in cargo metadata"
            )
        scope = SCOPE_LINKED if key in linked_crates else SCOPE_PROC_MACRO
        closure.append(_crate_info_from_pkg(pkg, scope))

    closure.sort(key=lambda c: (c.name, c.version, c.pkg_id))
    return closure


def normal_closure(metadata: dict, root_package: str) -> list[CrateInfo]:
    """Fallback closure from cargo metadata resolve (used by synthetic metadata-only tests)."""
    packages = {p["id"]: p for p in metadata["packages"]}
    resolve = metadata.get("resolve") or {}
    nodes = {n["id"]: n for n in resolve.get("nodes", [])}

    root_id = next(
        (p["id"] for p in metadata["packages"] if p["name"] == root_package),
        None,
    )
    if root_id is None:
        raise ValueError(f"Package '{root_package}' not found in cargo metadata")

    def _walk(skip_proc_macro: bool) -> set[str]:
        seen: set[str] = set()
        stack = [root_id]
        while stack:
            current = stack.pop()
            if current in seen:
                continue
            seen.add(current)
            node = nodes.get(current)
            if node is None:
                continue
            for dep in node.get("deps", []):
                dep_kinds = dep.get("dep_kinds", [])
                if not any(dk.get("kind") is None for dk in dep_kinds):
                    continue
                dep_pkg = packages[dep["pkg"]]
                is_pm = any(
                    "proc-macro" in t.get("kind", []) for t in dep_pkg.get("targets", [])
                )
                if skip_proc_macro and is_pm:
                    continue
                stack.append(dep["pkg"])
        return seen

    all_ids = _walk(skip_proc_macro=False)
    linked_ids = _walk(skip_proc_macro=True)

    closure = [
        _crate_info_from_pkg(
            packages[pkg_id],
            SCOPE_LINKED if pkg_id in linked_ids else SCOPE_PROC_MACRO,
        )
        for pkg_id in all_ids
    ]
    closure.sort(key=lambda c: (c.name, c.version, c.pkg_id))
    return closure


def normalize_text(raw: str) -> str:
    """Normalize line endings and trailing whitespace for deterministic deduplication."""
    return raw.replace("\r\n", "\n").replace("\r", "\n").rstrip() + "\n"


def _select_reuse_files(
    license_dir: pathlib.Path, license_expr: str | None
) -> list[pathlib.Path]:
    files = sorted((f for f in license_dir.iterdir() if f.is_file()), key=lambda p: p.name)
    expr = license_expr or ""
    if "GPL-3.0-only" in expr and "LicenseRef-Slint-" in expr:
        gpl_files = [f for f in files if f.name.upper().startswith("GPL-3.0")]
        if gpl_files:
            return gpl_files
    return files


def find_license_files(crate: CrateInfo) -> list[tuple[str, str]]:
    """Locate and read non-empty license/copying/notice files in the crate source dir."""
    crate_dir = crate.manifest_path.parent
    if not crate_dir.is_dir():
        return []

    candidates: list[pathlib.Path] = []
    seen_paths: set[pathlib.Path] = set()

    if crate.license_file:
        explicit = (crate_dir / crate.license_file).resolve()
        if explicit.is_file():
            seen_paths.add(explicit)
            candidates.append(explicit)

    entries = sorted(crate_dir.iterdir(), key=lambda p: p.name)
    for entry in entries:
        if entry.is_file() and entry.name.upper().startswith(LICENSE_FILE_PREFIXES):
            resolved = entry.resolve()
            if resolved not in seen_paths:
                seen_paths.add(resolved)
                candidates.append(entry)

    for entry in entries:
        if entry.is_dir() and entry.name.upper() in LICENSE_DIR_NAMES:
            for sub in _select_reuse_files(entry, crate.license_expr):
                resolved = sub.resolve()
                if resolved not in seen_paths:
                    seen_paths.add(resolved)
                    candidates.append(sub)

    texts: list[tuple[str, str]] = []
    for path in candidates:
        content = path.read_text(encoding="utf-8", errors="replace")
        if not content.strip():
            continue
        rel = str(path.relative_to(crate_dir)).replace("\\", "/")
        texts.append((rel, normalize_text(content)))
    return texts


def find_vendored_license_files(
    crate: CrateInfo, extra_dir: pathlib.Path
) -> list[tuple[str, str]]:
    """Return vendored upstream licence texts for explicitly mapped crates."""
    rel_paths = VENDORED_LICENSE_FILES.get(crate.name)
    if not rel_paths:
        return []

    texts: list[tuple[str, str]] = []
    for rel in rel_paths:
        path = extra_dir / rel
        if not path.is_file():
            raise FileNotFoundError(
                f"Vendored licence file missing for {crate.name}: {path}"
            )
        content = path.read_text(encoding="utf-8", errors="replace")
        if not content.strip():
            raise ValueError(
                f"Vendored licence file is empty for {crate.name}: {path}"
            )
        texts.append((rel, normalize_text(content)))
    return texts


def collect_notices(
    closure: list[CrateInfo],
    extra_dir: pathlib.Path = DEFAULT_EXTRA_DIR,
) -> tuple[list[CrateNotice], list[str]]:
    """Collect third-party license notices and return (notices, errors)."""
    notices: list[CrateNotice] = []
    missing_metadata: list[CrateInfo] = []
    missing_texts: list[CrateInfo] = []

    for crate in closure:
        has_meta = bool((crate.license_expr or "").strip()) or bool(
            (crate.license_file or "").strip()
        )
        if not has_meta:
            missing_metadata.append(crate)
            continue

        if crate.is_local:
            continue

        texts = find_license_files(crate)
        from_upstream = False
        if not texts:
            texts = find_vendored_license_files(crate, extra_dir)
            from_upstream = bool(texts)

        if not texts:
            missing_texts.append(crate)
            continue

        notices.append(CrateNotice(crate=crate, texts=texts, from_upstream=from_upstream))

    errors: list[str] = []
    if missing_metadata:
        errors.append("Crates missing licence metadata:")
        for c in missing_metadata:
            errors.append(
                f"  - {c.name} {c.version} (manifest: {c.manifest_path}, repo: {c.repository or '(none)'})"
            )
    if missing_texts:
        errors.append("Third-party crates with no licence text found in crate source:")
        for c in missing_texts:
            errors.append(
                f"  - {c.name} {c.version} [license: {c.license_expr}] "
                f"(source dir: {c.manifest_path.parent}, repo: {c.repository or '(none)'})"
            )

    return notices, errors


def render_notices(notices: list[CrateNotice], package: str, target: str) -> str:
    """Format THIRD-PARTY-NOTICES.txt with deduplicated licence text references."""
    hash_to_id: dict[str, str] = {}
    id_to_text: dict[str, str] = {}
    id_to_refs: dict[str, list[str]] = {}
    crate_entries: list[tuple[CrateNotice, list[tuple[str, str]]]] = []

    for entry in notices:
        refs: list[tuple[str, str]] = []
        for rel_name, text in entry.texts:
            digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
            lic_id = hash_to_id.get(digest)
            if lic_id is None:
                lic_id = f"LIC-{len(hash_to_id) + 1:03d}"
                hash_to_id[digest] = lic_id
                id_to_text[lic_id] = text
                id_to_refs[lic_id] = []
            origin = "; licence text from upstream repository" if entry.from_upstream else ""
            id_to_refs[lic_id].append(
                f"{entry.crate.name} {entry.crate.version} ({rel_name}{origin})"
            )
            refs.append((lic_id, rel_name))
        crate_entries.append((entry, refs))

    linked_count = sum(1 for entry in notices if entry.crate.scope == SCOPE_LINKED)
    pm_count = len(notices) - linked_count

    lines: list[str] = [
        f"THIRD-PARTY SOFTWARE NOTICES AND LICENSES ({package}, {target})",
        "=" * 78,
        "",
        "Ocy is licensed under AGPL-3.0-only (see LICENSE.txt).",
        "Slint crates (GPL-3.0-only OR LicenseRef-Slint-*) are used under GPL-3.0-only,",
        "which is compatible with AGPL-3.0-only per GPL-3.0 Section 13 and AGPL-3.0 Section 13.",
        "",
        (
            f"Third-party crates in normal dependency closure: {len(crate_entries)} "
            f"({linked_count} linked into Ocy.exe, {pm_count} proc-macro/build-time)"
        ),
        f"Unique license/notice texts: {len(id_to_text)}",
        "",
        "CRATES",
        "-" * 78,
    ]

    for entry, refs in crate_entries:
        crate = entry.crate
        scope_label = (
            "linked into Ocy.exe" if crate.scope == SCOPE_LINKED else "proc-macro/build-time"
        )
        lic_display = crate.license_expr or f"file:{crate.license_file}"
        if "GPL-3.0-only" in lic_display and "LicenseRef-Slint-" in lic_display:
            lic_display = f"{lic_display} [chosen: GPL-3.0-only]"
        suffix = "; licence text from upstream repository" if entry.from_upstream else ""
        ref_str = ", ".join(f"[{lic_id}] ({fname}{suffix})" for lic_id, fname in refs)
        lines.extend(
            [
                f"{crate.name} {crate.version}",
                f"  Scope:      {scope_label}",
                f"  License:    {lic_display}",
                f"  Repository: {crate.repository or '(none)'}",
                f"  Notices:    {ref_str}",
            ]
        )

    lines.extend(["", "LICENSE AND NOTICE TEXTS", "-" * 78])
    for lic_id, text in id_to_text.items():
        lines.extend(
            [
                "",
                "=" * 78,
                f"[{lic_id}]",
                "Referenced by:",
                *(f"  - {r}" for r in id_to_refs[lic_id]),
                "-" * 78,
                text.rstrip("\n"),
            ]
        )

    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Generate THIRD-PARTY-NOTICES.txt for ocy-app."
    )
    parser.add_argument(
        "--out",
        required=True,
        type=pathlib.Path,
        help="Path to write THIRD-PARTY-NOTICES.txt",
    )
    parser.add_argument(
        "--rust-dir",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parents[1],
        help="Path to the Cargo workspace (default: rust/)",
    )
    parser.add_argument(
        "--package",
        default=DEFAULT_PACKAGE,
        help=f"Root Cargo package name (default: {DEFAULT_PACKAGE})",
    )
    parser.add_argument(
        "--target",
        default=DEFAULT_TARGET,
        help=f"Target triple for cargo tree (default: {DEFAULT_TARGET})",
    )
    parser.add_argument(
        "--extra-dir",
        type=pathlib.Path,
        default=DEFAULT_EXTRA_DIR,
        help="Directory containing vendored fallback upstream licence texts",
    )
    parser.add_argument(
        "--metadata-json",
        type=pathlib.Path,
        default=None,
        help="Optional pre-generated cargo metadata JSON file (for offline tests)",
    )
    parser.add_argument(
        "--tree-file",
        type=pathlib.Path,
        default=None,
        help="Optional pre-generated `cargo tree -e normal --prefix none` output (for offline tests)",
    )
    parser.add_argument(
        "--linked-tree-file",
        type=pathlib.Path,
        default=None,
        help="Optional pre-generated `cargo tree -e normal,no-proc-macro --prefix none` output (for offline tests)",
    )
    args = parser.parse_args(argv)

    if args.metadata_json is not None:
        metadata = json.loads(args.metadata_json.read_text(encoding="utf-8"))
    else:
        metadata = run_cargo_metadata(args.rust_dir)

    if args.tree_file is not None:
        all_crates = parse_cargo_tree(args.tree_file.read_text(encoding="utf-8"))
        if args.linked_tree_file is not None:
            linked_crates = parse_cargo_tree(args.linked_tree_file.read_text(encoding="utf-8"))
        else:
            linked_crates = all_crates
        closure = closure_from_trees(metadata, all_crates, linked_crates)
    elif args.metadata_json is not None:
        closure = normal_closure(metadata, args.package)
    else:
        all_tree = run_cargo_tree(args.rust_dir, args.package, args.target, "normal")
        linked_tree = run_cargo_tree(
            args.rust_dir, args.package, args.target, "normal,no-proc-macro"
        )
        closure = closure_from_trees(
            metadata,
            parse_cargo_tree(all_tree),
            parse_cargo_tree(linked_tree),
        )

    notices, errors = collect_notices(closure, extra_dir=args.extra_dir)
    if errors:
        print(
            "ERROR: third-party licence audit failed:\n" + "\n".join(errors),
            file=sys.stderr,
        )
        return 1

    rendered = render_notices(notices, args.package, args.target)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(rendered, encoding="utf-8", newline="\n")
    print(f"notices: wrote {len(notices)} crates to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
