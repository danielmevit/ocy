#![deny(clippy::unwrap_used, clippy::expect_used)]
//! Embeds Ocy's localization tables (`content/strings/*.json`) for all targets, and
//! Windows resources (manifest, app icon, version info) into `Ocy.exe` as a pure-Rust COFF
//! `.rsrc` object via `damt-build`. No external resource compiler (`rc.exe`, `llvm-rc`,
//! `mt.exe`, `clang-cl`) is needed, so the same path is used for native Windows builds and
//! cross-compilation from Linux.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const PLURAL_SUFFIXES: &[&str] = &[".zero", ".one", ".two", ".few", ".many", ".other"];

fn strip_plural_suffix(key: &str) -> Option<&str> {
    for suffix in PLURAL_SUFFIXES {
        if let Some(base) = key.strip_suffix(suffix) {
            return Some(base);
        }
    }
    None
}

fn is_known_in_english(key: &str, en_keys: &BTreeSet<String>) -> bool {
    if en_keys.contains(key) {
        return true;
    }
    if let Some(base) = strip_plural_suffix(key) {
        if en_keys.contains(base) {
            return true;
        }
        if en_keys.contains(&format!("{base}.other")) || en_keys.contains(&format!("{base}.one")) {
            return true;
        }
    }
    false
}

fn generate_embedded_strings(manifest_dir: &Path, out_dir: &Path) {
    let strings_dir = manifest_dir.join("../../../../content/strings");
    println!("cargo:rerun-if-changed={}", strings_dir.display());

    let entries = fs::read_dir(&strings_dir).unwrap_or_else(|err| {
        panic!(
            "failed to list strings directory {}: {err}",
            strings_dir.display()
        )
    });

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry_res in entries {
        let entry = entry_res.unwrap_or_else(|err| {
            panic!("failed to read strings directory entry: {err}");
        });
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.is_empty() {
            continue;
        }
        files.push((stem.to_owned(), path));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut parsed_tables: Vec<(String, PathBuf, BTreeMap<String, String>)> = Vec::new();
    for (tag, path) in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let map: BTreeMap<String, String> = serde_json::from_str(&raw)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()));
        parsed_tables.push((tag, path, map));
    }

    let en_keys: BTreeSet<String> = parsed_tables
        .iter()
        .find(|(tag, _, _)| tag == "en")
        .map(|(_, _, map)| {
            map.keys()
                .filter(|k| !k.starts_with("_meta."))
                .cloned()
                .collect()
        })
        .unwrap_or_else(|| panic!("en.json not found in {}", strings_dir.display()));

    let mut generated = String::from(
        "/// Embedded `(bcp47_tag, raw_json)` pairs generated from `content/strings/*.json`.\n\
         pub const EMBEDDED_STRINGS: &[(&str, &str)] = &[\n",
    );

    for (tag, path, map) in &parsed_tables {
        if tag != "en" {
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(tag.as_str());
            for key in map.keys() {
                if key.starts_with("_meta.") {
                    continue;
                }
                if !is_known_in_english(key, &en_keys) {
                    println!(
                        "cargo:warning=content/strings/{file_name}: key '{key}' is not present in en.json"
                    );
                }
            }
        }

        let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        let path_str = abs_path
            .to_str()
            .unwrap_or_else(|| panic!("non-UTF-8 path: {}", abs_path.display()));
        let _ = writeln!(generated, "    ({tag:?}, include_str!({path_str:?})),");
    }
    generated.push_str("];\n");

    let dest = out_dir.join("embedded_strings.rs");
    let unchanged = fs::read_to_string(&dest)
        .map(|existing| existing == generated)
        .unwrap_or(false);
    if !unchanged {
        fs::write(&dest, generated)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", dest.display()));
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../../../assets/icons/app/ocy.ico");

    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| ".".into()));
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_else(|| ".".into()));

    generate_embedded_strings(&manifest_dir, &out_dir);

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // The manifest XML is generated by `embed-manifest` (pure Rust, no `mt.exe`); only
    // the resource *compilation* is done by `damt-build` instead of `mt.exe`/`llvm-rc`.
    let manifest = embed_manifest::new_manifest("Ocy")
        .dpi_awareness(embed_manifest::manifest::DpiAwareness::PerMonitorV2);

    let ico_path = manifest_dir.join("../../../../assets/icons/app/ocy.ico");
    if !ico_path.is_file() {
        panic!("ocy.ico not found at {}", ico_path.display());
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version_info = damt_build::VersionInfo {
        company_name: "damt.xyz".to_owned(),
        product_name: "Ocy".to_owned(),
        file_version: version.clone(),
        product_version: version,
        legal_copyright: "Copyright (C) 2026 Daniel Mitev (Daniel Mevit)".to_owned(),
    };

    let target = std::env::var("TARGET").unwrap_or_default();

    let resources = damt_build::ResourceBuilder::new()
        .manifest(&manifest.to_string())
        .icon(&ico_path)
        .version_info(version_info)
        .write_coff(&out_dir, "ocy-resources", &target)
        .unwrap_or_else(|err| panic!("failed to build the Windows resource object: {err}"));

    for arg in &resources.link_args {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
