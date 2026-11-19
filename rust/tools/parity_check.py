#!/usr/bin/env python3
"""Parity gate for the Rust port: every C# test in tests/Ocy.Core.Tests must be mapped in
rust/apps/ocy/ocy-core/tests/PARITY.md to an existing Rust #[test] fn (or 'N/A: <reason>').
usage: python3 rust/tools/parity_check.py   (run from the repo root)"""
import pathlib, re, sys

root = pathlib.Path(__file__).resolve().parents[2]
cs_dir = root / "tests" / "Ocy.Core.Tests"
parity = root / "rust" / "apps" / "ocy" / "ocy-core" / "tests" / "PARITY.md"
rust_dir = root / "rust" / "apps" / "ocy" / "ocy-core"

cs_tests = []
for f in sorted(cs_dir.rglob("*.cs")):
    text = f.read_text(encoding="utf-8")
    for m in re.finditer(r"\[(?:Fact|Theory)[^\]]*\](?:\s*\[[^\]]*\])*\s*public\s+(?:async\s+Task|void)\s+(\w+)\s*\(", text):
        cs_tests.append(f"{f.stem}.{m.group(1)}")

rust_fns = set()
for f in rust_dir.rglob("*.rs"):
    text = f.read_text(encoding="utf-8")
    for m in re.finditer(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+(\w+)", text):
        rust_fns.add(m.group(1))
    for m in re.finditer(r"proptest!\s*\{(.*?)\n\}", text, re.S):
        rust_fns.update(re.findall(r"fn\s+(\w+)\s*\(", m.group(1)))

if not parity.exists():
    sys.exit(f"parity: {parity} missing ({len(cs_tests)} C# tests to map)")
mapping = {}
for line in parity.read_text(encoding="utf-8").splitlines():
    cells = [c.strip().strip("`") for c in line.strip().strip("|").split("|")]
    if len(cells) >= 2 and "." in cells[0] and not cells[0].startswith("-"):
        mapping[cells[0]] = cells[1]

errors = []
for t in cs_tests:
    target = mapping.get(t)
    if target is None:
        errors.append(f"unmapped C# test: {t}")
    elif target.upper().startswith("N/A"):
        if len(target) < 8:
            errors.append(f"N/A without a reason: {t}")
    else:
        for fn in [x.strip() for x in target.split(",")]:
            if fn not in rust_fns:
                errors.append(f"{t} -> Rust test '{fn}' not found")
extra = sorted(set(mapping) - set(cs_tests))
if extra:
    errors.append(f"PARITY.md lists tests that don't exist in C#: {extra[:5]}")
print(f"parity: {len(cs_tests)} C# tests, {len(mapping)} mapped, {len(rust_fns)} Rust tests found")
if errors:
    print("\n".join(errors[:40]))
    sys.exit(1)
print("parity: OK")
