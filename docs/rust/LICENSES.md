# Ocy Rust Licence Audit (`x86_64-pc-windows-msvc`)

Audit of the normal (non-dev, non-build) dependency closure of `ocy-app` for Windows (`x86_64-pc-windows-msvc`),
generated offline from `cargo tree --locked -p ocy-app -e normal --target x86_64-pc-windows-msvc --prefix none`
with crate metadata from `cargo metadata --locked --format-version 1`.

- **Project licence:** `AGPL-3.0-only` (see repo root [`LICENSE`](../../LICENSE)).
- **Total crates in `ocy-app` normal closure:** **261** (9 local workspace crates + 252 third-party crates: **194** linked into `Ocy.exe` + **58** proc-macro / compile-time host crates; on a Linux host, `slint-macros` -> `i-slint-compiler` -> `fontique` pulls 3 Linux host crates `dlib`, `libc`, `yeslogic-fontconfig-sys` that are absent on a Windows build host).
- **Test-only workspace crates (`skia-safe`, `skia-bindings`, `wgpu*`, `i-slint-backend-qt`, `i-slint-backend-testing`, `i-slint-renderer-skia`, `glutin*`, `ash`, `naga*`, `profiling`, `spirv`, etc. from `slint-interpreter` `[dev-dependencies]`):** not shipped (excluded from `Ocy.exe` and `THIRD-PARTY-NOTICES.txt`).

## 1. Flagged Findings & Resolutions

### Flag 1 (Resolved): 11 third-party crates omit licence text files in `.crate` archives — vendored upstream texts

The following crates in `ocy-app`'s normal closure declare a valid SPDX `license` expression in `Cargo.toml`, **but ship zero `LICENSE*`, `LICENCE*`, `COPYING*`, `NOTICE*`, or `LICENSES/*` files in their published crates.io `.crate` tarballs**. Their exact upstream licence files (fetched 2026-11-19 from each repository's default branch) are vendored under [`rust/tools/licenses-extra/`](../../rust/tools/licenses-extra/SOURCES.md):

| Crate | Version | Declared licence | Scope in `ocy-app` | Vendored file(s) | Upstream source |
| --- | --- | --- | --- | --- | --- |
| `accesskit` | `0.25.2` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `accesskit/LICENSE-APACHE`, `accesskit/LICENSE-MIT`, `accesskit/LICENSE.chromium`, `accesskit/AUTHORS` | https://github.com/AccessKit/accesskit (`main`) |
| `accesskit_consumer` | `0.39.2` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `accesskit/LICENSE-APACHE`, `accesskit/LICENSE-MIT`, `accesskit/LICENSE.chromium`, `accesskit/AUTHORS` | https://github.com/AccessKit/accesskit (`main`) |
| `accesskit_windows` | `0.35.2` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `accesskit/LICENSE-APACHE`, `accesskit/LICENSE-MIT`, `accesskit/LICENSE.chromium`, `accesskit/AUTHORS` | https://github.com/AccessKit/accesskit (`main`) |
| `accesskit_winit` | `0.34.2` | `Apache-2.0` | Linked into `Ocy.exe` | `accesskit/LICENSE-APACHE`, `accesskit/AUTHORS` | https://github.com/AccessKit/accesskit (`main`) |
| `clipboard-win` | `5.4.3` | `BSL-1.0` | Linked into `Ocy.exe` | `clipboard-win/LICENSE` | https://github.com/DoumanAsh/clipboard-win (`master`) |
| `lyon_algorithms` | `1.0.22` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `lyon/LICENSE-MIT` | https://github.com/nical/lyon (`main`) |
| `lyon_extra` | `1.1.0` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `lyon/LICENSE-MIT` | https://github.com/nical/lyon (`main`) |
| `lyon_geom` | `1.0.20` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `lyon/LICENSE-MIT` | https://github.com/nical/lyon (`main`) |
| `lyon_path` | `1.0.19` | `MIT OR Apache-2.0` | Linked into `Ocy.exe` | `lyon/LICENSE-MIT` | https://github.com/nical/lyon (`main`) |
| `rspolib` | `0.1.2` | `MIT` | Proc-macro/compile-time (`slint-macros`) | `rspolib/LICENSE` | https://github.com/mondeja/rspolib (`master`) |
| `taffy` | `0.15.1` | `MIT` | Linked into `Ocy.exe` | `taffy/LICENSE` | https://github.com/DioxusLabs/taffy (`main`) |

- `rust/tools/notices.py` uses these vendored texts **only** when the crate's own source directory contains no licence files, labels each resulting notice entry with `licence text from upstream repository`, and still **fails** for any other crate missing licence texts.
- For `lyon_*` (`MIT OR Apache-2.0`), upstream `nical/lyon`'s `LICENSE-APACHE` is only a short notice rather than the full Apache-2.0 text, so Ocy chooses `MIT` (`lyon/LICENSE-MIT`).
- For `accesskit`, `accesskit_consumer`, and `accesskit_windows`, Chromium-derived source headers reference `LICENSE.chromium` (`BSD-3-Clause`, FSF-listed GPLv3/AGPLv3-compatible), and `AUTHORS` is included alongside `LICENSE-APACHE` and `LICENSE-MIT` because AccessKit's `LICENSE-MIT` omits an inline copyright line.

### Flag 2 (Verify): `crossbeam-channel 0.5.17` `LICENSE-THIRD-PARTY` includes `CC-BY-3.0` example attribution

`crossbeam-channel 0.5.17` (`MIT OR Apache-2.0`) ships `LICENSE-THIRD-PARTY`, which contains a `CC-BY-3.0` notice for `matching.go` (used only in `examples/matching.rs`, not compiled into the library or `Ocy.exe`) alongside `Apache-2.0` Go notices. `CC-BY-3.0` is not GPL/AGPL-compatible for distributed code, so this is acceptable only because `examples/matching.rs` is excluded from library compilation.

### Flag 3 (Metadata note): `spin_on 0.1.1` has no `repository` field

`spin_on 0.1.1` (`Apache-2.0 OR MIT`) has `repository = null` in Cargo metadata; its `LICENSE-APACHE` and `LICENSE-MIT` files are present in `~/.cargo/registry/src/`.

### Flag 4: Slint `GPL-3.0-only` selection & `AGPL-3.0-only` compatibility

All 9 Slint crates (`1.19.0`) in `ocy-app`'s normal closure declare `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` and ship all three texts under `LICENSES/`. Ocy selects **`GPL-3.0-only`** (never the royalty-free badge/AboutSlint option or paid commercial licence). Under **GPL-3.0 §13** and **AGPL-3.0 §13**, code under GPL-3.0 may be combined with code under AGPL-3.0 into a single combined work licensed under AGPL-3.0, while preserving the GPL-3.0 notice for the Slint components. `notices.py` selects `LICENSES/GPL-3.0-only.txt` and excludes `LicenseRef-Slint-*`.

## 2. Summary by Licence Expression

| Licence expression | Crates (linked / proc-macro) | Chosen option | AGPL-3.0-only verdict | Notes |
| --- | --- | --- | --- | --- |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 (0 / 1) | `MIT AND Unicode-3.0` | Compatible | Conjunction (`AND`): both `MIT` (chosen from `MIT OR Apache-2.0`) and `Unicode-3.0` apply and are retained (`unicode-ident`). |
| `0BSD OR MIT OR Apache-2.0` | 1 (1 / 0) | `0BSD` | Compatible | Zero-Clause BSD (public-domain-equivalent permissive). |
| `AGPL-3.0-only` | 9 (9 / 0) | `AGPL-3.0-only` | Compatible (first-party) | Ocy's own licence (`LICENSE`). |
| `Apache-2.0` | 3 (3 / 0) | `Apache-2.0` | Compatible | Permissive; FSF-listed GPLv3/AGPLv3-compatible. Retain licence and any `NOTICE` files (`accesskit_winit` uses vendored upstream `accesskit/LICENSE-APACHE`). |
| `Apache-2.0 AND MIT` | 1 (1 / 0) | `Apache-2.0 AND MIT` | Compatible | Conjunction (`AND`): both Apache-2.0 and MIT notices must be retained (`dpi` also includes `LICENSE-LIBM-MIT`). |
| `Apache-2.0 OR MIT` | 28 (25 / 3) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `Apache-2.0/MIT` | 1 (0 / 1) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `BSD-2-Clause` | 1 (1 / 0) | `BSD-2-Clause` | Compatible | Permissive; retain copyright notice and disclaimer. |
| `BSD-3-Clause` | 2 (2 / 0) | `BSD-3-Clause` | Compatible | Permissive; retain copyright notice, disclaimer, and non-endorsement clause. |
| `BSD-3-Clause OR Apache-2.0` | 2 (2 / 0) | `Apache-2.0` | Compatible | Permissive; FSF-listed GPLv3/AGPLv3-compatible. Retain licence and any `NOTICE` files. |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 (0 / 2) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `BSL-1.0` | 2 (2 / 0) | `BSL-1.0` | Compatible | Boost Software License 1.0; FSF-listed GPL-compatible permissive licence (`clipboard-win` uses vendored upstream `clipboard-win/LICENSE`). |
| `CC0-1.0 OR Apache-2.0` | 1 (1 / 0) | `Apache-2.0` | Compatible | Permissive; FSF-listed GPLv3/AGPLv3-compatible. Retain licence and any `NOTICE` files. |
| `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | 9 (6 / 3) | `GPL-3.0-only` | Compatible (GPL-3.0 §13 / AGPL-3.0 §13) | Slint is used under `GPL-3.0-only` (explicitly NOT `LicenseRef-Slint-Royalty-free-2.0` or `LicenseRef-Slint-Software-3.0`). Both GPL-3.0 §13 and AGPL-3.0 §13 explicitly permit linking/combining GPL-3.0 and AGPL-3.0 code. |
| `ISC` | 1 (0 / 1) | `ISC` | Compatible | Permissive; FSF-listed GPL-compatible. |
| `MIT` | 32 (24 / 8) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt` (`rspolib` and `taffy` use vendored upstream `LICENSE`). |
| `MIT OR Apache-2.0` | 120 (85 / 35) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt` (`lyon_*` use vendored `lyon/LICENSE-MIT`; `accesskit`, `accesskit_consumer`, `accesskit_windows` include upstream `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE.chromium` BSD-3-Clause, and `AUTHORS`). |
| `MIT OR Apache-2.0 OR Zlib` | 4 (4 / 0) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `MIT OR Zlib OR Apache-2.0` | 2 (2 / 0) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `MIT/Apache-2.0` | 3 (3 / 0) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `Unicode-3.0` | 25 (22 / 3) | `Unicode-3.0` | Compatible | Unicode License v3 (`Unicode-3.0`); FSF-listed GPL-compatible permissive data/software licence. |
| `Unlicense OR MIT` | 6 (6 / 0) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |
| `Zlib` | 2 (2 / 0) | `Zlib` | Compatible | Permissive; FSF-listed GPL-compatible. |
| `Zlib OR Apache-2.0 OR MIT` | 3 (2 / 1) | `MIT` | Compatible | Permissive; retain copyright and permission notice in `THIRD-PARTY-NOTICES.txt`. |

## 3. Complete Crate Table (`cargo tree` normal closure: 261 crates)

| Crate | Version | Licence expression | Chosen | Scope | Licence file(s) | Repository |
| --- | --- | --- | --- | --- | --- | --- |
| `accesskit` | `0.25.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `accesskit/LICENSE-APACHE` (vendored upstream), `accesskit/LICENSE-MIT` (vendored upstream), `accesskit/LICENSE.chromium` (vendored upstream), `accesskit/AUTHORS` (vendored upstream) | https://github.com/AccessKit/accesskit |
| `accesskit_consumer` | `0.39.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `accesskit/LICENSE-APACHE` (vendored upstream), `accesskit/LICENSE-MIT` (vendored upstream), `accesskit/LICENSE.chromium` (vendored upstream), `accesskit/AUTHORS` (vendored upstream) | https://github.com/AccessKit/accesskit |
| `accesskit_windows` | `0.35.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `accesskit/LICENSE-APACHE` (vendored upstream), `accesskit/LICENSE-MIT` (vendored upstream), `accesskit/LICENSE.chromium` (vendored upstream), `accesskit/AUTHORS` (vendored upstream) | https://github.com/AccessKit/accesskit |
| `accesskit_winit` | `0.34.2` | `Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `accesskit/LICENSE-APACHE` (vendored upstream), `accesskit/AUTHORS` (vendored upstream) | https://github.com/AccessKit/accesskit |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | `0BSD` | Target (`Ocy.exe`) | `LICENSE-0BSD`, `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/oyvindln/adler2 |
| `allocator-api2` | `0.2.21` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/zakarumych/allocator-api2 |
| `annotate-snippets` | `0.12.18` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/annotate-snippets-rs |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-cli/anstyle.git |
| `arboard` | `3.7.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE.txt`, `LICENSE-MIT.txt` | https://github.com/1Password/arboard |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | `BSD-2-Clause` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/droundy/arrayref |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/bluss/arrayvec |
| `base64` | `0.23.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/marshallpierce/rust-base64 |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/bitflags/bitflags |
| `by_address` | `1.2.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/mbrubeck/by_address |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` | https://github.com/Lokathor/bytemuck |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` | https://github.com/Lokathor/bytemuck |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-MIT` | https://github.com/image-rs/byteorder-lite |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/cfg-if |
| `chrono` | `0.4.45` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE.txt` | https://github.com/chronotope/chrono |
| `clipboard-win` | `5.4.3` | `BSL-1.0` | `BSL-1.0` | Target (`Ocy.exe`) | `clipboard-win/LICENSE` (vendored upstream) | https://github.com/DoumanAsh/clipboard-win |
| `clru` | `0.6.3` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/marmeladema/clru-rs |
| `color_quant` | `1.1.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/image-rs/color_quant.git |
| `const-field-offset` | `0.2.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSES/Apache-2.0.txt`, `LICENSES/MIT.txt` | https://github.com/slint-ui/slint |
| `const-field-offset-macro` | `0.2.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSES/Apache-2.0.txt`, `LICENSES/MIT.txt` | https://github.com/slint-ui/slint |
| `convert_case` | `0.10.0` | `MIT` | `MIT` | Proc-macro/host | `LICENSE` | https://github.com/rutrum/convert-case |
| `countme` | `3.0.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/matklad/countme |
| `crc32fast` | `1.5.5` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/srijs/rust-crc32fast |
| `critical-section` | `1.2.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-embedded/critical-section |
| `crossbeam-channel` | `0.5.17` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-THIRD-PARTY` | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/crossbeam-rs/crossbeam |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` | https://github.com/rust-windowing/cursor-icon |
| `damt-app` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `damt-art` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `damt-overlay` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `damt-platform` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `damt-platform-win` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `damt-theme` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `data-url` | `0.3.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/rust-url |
| `derive_more` | `2.1.1` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/JelteF/derive_more |
| `derive_more-impl` | `2.1.1` | `MIT` | `MIT` | Proc-macro/host | `LICENSE` | https://github.com/JelteF/derive_more |
| `displaydoc` | `0.2.7` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/yaahc/displaydoc |
| `dlib` | `0.5.3` | `MIT` | `MIT` | Proc-macro/host | `LICENSE.txt` | https://github.com/elinorbgr/dlib |
| `dpi` | `0.1.2` | `Apache-2.0 AND MIT` | `Apache-2.0 AND MIT` | Target (`Ocy.exe`) | `LICENSE`, `LICENSE-LIBM-MIT` | https://github.com/rust-windowing/winit |
| `either` | `1.19.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rayon-rs/either |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/indexmap-rs/equivalent |
| `error-code` | `3.4.0` | `BSL-1.0` | `BSL-1.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/DoumanAsh/error-code |
| `euclid` | `0.22.14` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/euclid |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/fdeflate |
| `field-offset` | `0.3.6` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/Diggsey/rust-field-offset |
| `fixed_decimal` | `0.7.3` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `flate2` | `1.1.11` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/flate2-rs |
| `float-cmp` | `0.9.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/mikedilger/float-cmp |
| `foldhash` | `0.2.0` | `Zlib` | `Zlib` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/orlp/foldhash |
| `font-types` | `0.12.7` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/googlefonts/fontations |
| `fontdb` | `0.24.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/RazrFalcon/fontdb |
| `fontique` | `0.12.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/parley |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/rust-url |
| `gif` | `0.14.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/image-gif |
| `harfrust` | `0.12.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/harfbuzz/harfrust |
| `hashbrown` | `0.14.5` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/hashbrown |
| `hashbrown` | `0.16.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/hashbrown |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/hashbrown |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/withoutboats/heck |
| `htmlparser` | `0.2.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/jdrouet/htmlparser.git |
| `i-slint-backend-selector` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-backend-winit` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-common` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-compiler` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Proc-macro/host | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-core` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-core-macros` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Proc-macro/host | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `i-slint-renderer-software` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `icu_collections` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_decimal` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_decimal_data` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_locale_core` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_locale_fallback` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_locale_fallback_data` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_normalizer` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_normalizer_data` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_properties` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_properties_data` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_provider` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_segmenter` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `icu_segmenter_data` | `2.4.0` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/rust-url/ |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/hsivonen/idna_adapter |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/image |
| `image-webp` | `0.2.4` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/image-webp |
| `imagesize` | `0.15.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/Roughsketch/imagesize |
| `imgref` | `1.12.3` | `CC0-1.0 OR Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-CC0` | https://github.com/kornelski/imgref |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/indexmap-rs/indexmap |
| `itertools` | `0.15.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-itertools/itertools |
| `itoa` | `1.0.19` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/itoa |
| `jiff` | `1.0.1` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-MIT` | https://github.com/BurntSushi/jiff |
| `jiff-core` | `1.0.1` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-MIT` | https://github.com/BurntSushi/jiff |
| `jiff-tzdb` | `1.0.0` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-MIT` | https://github.com/BurntSushi/jiff |
| `jiff-tzdb-platform` | `1.0.0` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-MIT` | https://github.com/BurntSushi/jiff |
| `keyboard-types` | `0.8.3` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-windowing/keyboard-types |
| `kurbo` | `0.13.1` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/kurbo |
| `lazy_static` | `1.5.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang-nursery/lazy-static.rs |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/libc |
| `libloading` | `0.8.9` | `ISC` | `ISC` | Proc-macro/host | `LICENSE` | https://github.com/nagisa/rust_libloading/ |
| `libm` | `0.2.16` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE.txt` | https://github.com/rust-lang/compiler-builtins |
| `linebender_resource_handle` | `0.1.1` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/raw_resource_handle |
| `litemap` | `0.8.3` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `log` | `0.4.35` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/log |
| `lyon_algorithms` | `1.0.22` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `lyon/LICENSE-MIT` (vendored upstream) | https://github.com/nical/lyon |
| `lyon_extra` | `1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `lyon/LICENSE-MIT` (vendored upstream) | https://github.com/nical/lyon |
| `lyon_geom` | `1.0.20` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `lyon/LICENSE-MIT` (vendored upstream) | https://github.com/nical/lyon |
| `lyon_path` | `1.0.19` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `lyon/LICENSE-MIT` (vendored upstream) | https://github.com/nical/lyon |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-MIT` | https://github.com/BurntSushi/memchr |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/RazrFalcon/memmap2-rs |
| `memoffset` | `0.9.1` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/Gilnaa/memoffset |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE`, `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE`, `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `LICENSE-APACHE.md`, `LICENSE.md` | https://github.com/awxkee/moxcms.git |
| `muda` | `0.21.2` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE.spdx` | https://github.com/tauri-apps/muda |
| `natord` | `1.0.9` | `MIT` | `MIT` | Proc-macro/host | `LICENSE.txt` | https://github.com/lifthrasiir/rust-natord |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-num/num-traits |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` | https://github.com/illicitonion/num_enum |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-BSD`, `LICENSE-MIT` | https://github.com/illicitonion/num_enum |
| `ocy-app` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `ocy-core` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `ocy-ui` | `0.2.2-dev` | `AGPL-3.0-only` | `AGPL-3.0-only` | Workspace (`Ocy.exe`) | repo `LICENSE` | (none) |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/matklad/once_cell |
| `parlance` | `0.1.1` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/parley |
| `parley` | `0.12.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/parley |
| `parley_data` | `0.12.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/parley |
| `parley_emoji` | `0.12.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-MIT` | https://github.com/linebender/parley |
| `parley_engine` | `0.12.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/parley |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/rust-url/ |
| `pico-args` | `0.5.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/RazrFalcon/pico-args |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/taiki-e/pin-project |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/taiki-e/pin-project |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/taiki-e/pin-project-lite |
| `pin-utils` | `0.1.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/pin-utils |
| `pin-weak` | `1.1.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/sixtyfpsui/pin-weak |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/image-png |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/kurbo |
| `portable-atomic` | `1.15.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/taiki-e/portable-atomic |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/bkchr/proc-macro-crate |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/proc-macro2 |
| `pulldown-cmark` | `0.13.6` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/raphlinus/pulldown-cmark |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `LICENSE-APACHE.md`, `LICENSE.md` | https://github.com/awxkee/pxfm |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | http://github.com/tailhook/quick-error |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/quote |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` | https://github.com/rust-windowing/raw-window-handle |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rayon-rs/rayon |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rayon-rs/rayon |
| `read-fonts` | `0.41.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/googlefonts/fontations |
| `resvg` | `0.49.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/resvg |
| `resvg` | `0.50.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/resvg |
| `rgb` | `0.8.53` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/kornelski/rust-rgb |
| `rowan` | `0.17.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-analyzer/rowan |
| `roxmltree` | `0.21.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/RazrFalcon/roxmltree |
| `rspolib` | `0.1.2` | `MIT` | `MIT` | Proc-macro/host | `rspolib/LICENSE` (vendored upstream) | https://github.com/mondeja/rspolib |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang-nursery/rustc-hash |
| `rustversion` | `1.0.23` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/rustversion |
| `scoped-tls-hkt` | `0.1.5` | `MIT/Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/Diggsey/scoped-tls-hkt |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/bluss/scopeguard |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/serde-rs/serde |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/serde-rs/serde |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/serde-rs/serde |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/serde-rs/json |
| `serde_spanned` | `1.1.3` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `simd-adler32` | `0.3.10` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE.md` | https://github.com/mcountryman/simd-adler32 |
| `simplecss` | `0.2.2` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/simplecss |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `COPYING`, `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/jedisct1/rust-siphash |
| `skrifa` | `0.44.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/googlefonts/fontations |
| `slab` | `0.4.12` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/tokio-rs/slab |
| `slint` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Target (`Ocy.exe`) | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `slint-macros` | `1.19.0` | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `GPL-3.0-only` | Proc-macro/host | `LICENSES/GPL-3.0-only.txt` | https://github.com/slint-ui/slint |
| `slotmap` | `1.1.1` | `Zlib` | `Zlib` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/orlp/slotmap |
| `smallvec` | `1.16.6` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/smallvec/smallvec |
| `smol_str` | `0.2.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-analyzer/smol_str |
| `smol_str` | `0.3.7` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-lang/rust-analyzer/tree/master/lib/smol_str |
| `snafu` | `0.8.9` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/shepmaster/snafu |
| `snafu-derive` | `0.8.9` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/shepmaster/snafu |
| `softbuffer` | `0.4.8` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-windowing/softbuffer |
| `spin_on` | `0.1.1` | `Apache-2.0 OR MIT` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | (none) |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/storyyeller/stable_deref_trait |
| `static_assertions` | `1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/nvzqz/static-assertions-rs |
| `strict-num` | `0.1.1` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/RazrFalcon/strict-num |
| `strum` | `0.28.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/Peternator7/strum |
| `strum_macros` | `0.28.0` | `MIT` | `MIT` | Proc-macro/host | `LICENSE` | https://github.com/Peternator7/strum |
| `svgtypes` | `0.16.2` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/svgtypes |
| `swash` | `0.2.10` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dfrg/swash |
| `syn` | `2.0.119` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/syn |
| `syn` | `3.0.10` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/syn |
| `synstructure` | `0.14.0` | `MIT` | `MIT` | Proc-macro/host | `LICENSE` | https://github.com/mystor/synstructure |
| `sys-locale` | `0.3.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/1Password/sys-locale |
| `taffy` | `0.15.1` | `MIT` | `MIT` | Target (`Ocy.exe`) | `taffy/LICENSE` (vendored upstream) | https://github.com/DioxusLabs/taffy |
| `text-size` | `1.1.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/rust-analyzer/text-size |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/thiserror |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dtolnay/thiserror |
| `tiny-skia` | `0.12.0` | `BSD-3-Clause` | `BSD-3-Clause` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/linebender/tiny-skia |
| `tiny-skia-path` | `0.12.0` | `BSD-3-Clause` | `BSD-3-Clause` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/linebender/tiny-skia/tree/master/path |
| `tinystr` | `0.8.5` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `tinyvec` | `1.13.3` | `Zlib OR Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE.md`, `LICENSE-MIT.md`, `LICENSE-ZLIB.md` | https://github.com/Lokathor/tinyvec |
| `toml` | `1.2.1+spec-1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `toml_datetime` | `1.1.3+spec-1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `toml_edit` | `0.25.19+spec-1.1.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `toml_parser` | `1.1.5+spec-1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `toml_writer` | `1.2.0+spec-1.1.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/toml-rs/toml |
| `tracing` | `0.1.44` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/tokio-rs/tracing |
| `tracing-core` | `0.1.36` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/tokio-rs/tracing |
| `typed-index-collections` | `3.5.0` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/zheland/typed-index-collections |
| `unicase` | `2.10.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/seanmonstar/unicase |
| `unicode-bidi` | `0.3.18` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/unicode-bidi |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | `MIT AND Unicode-3.0` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-UNICODE` | https://github.com/dtolnay/unicode-ident |
| `unicode-linebreak` | `0.1.5` | `Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/axelf4/unicode-linebreak |
| `unicode-script` | `0.5.9` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/unicode-rs/unicode-script |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/unicode-rs/unicode-segmentation |
| `unicode-vo` | `0.1.0` | `MIT/Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/RazrFalcon/unicode-vo |
| `unicode-width` | `0.2.3` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/unicode-rs/unicode-width |
| `unicode-xid` | `0.2.6` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/unicode-rs/unicode-xid |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/servo/rust-url |
| `usvg` | `0.49.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/resvg |
| `usvg` | `0.50.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/linebender/resvg |
| `utf8_iter` | `1.1.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/hsivonen/utf8_iter |
| `utf_types` | `1.0.0` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/hsivonen/utf_types |
| `uuid` | `1.29.3` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/uuid-rs/uuid |
| `vtable` | `0.5.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSES/Apache-2.0.txt`, `LICENSES/MIT.txt` | https://github.com/slint-ui/slint |
| `vtable-macro` | `0.5.1` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `LICENSES/Apache-2.0.txt`, `LICENSES/MIT.txt` | https://github.com/slint-ui/slint |
| `webbrowser` | `1.2.7` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/amodm/webbrowser-rs |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/image-rs/weezl |
| `windows` | `0.62.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-collections` | `0.3.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-core` | `0.62.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-future` | `0.3.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-implement` | `0.60.2` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-interface` | `0.59.3` | `MIT OR Apache-2.0` | `MIT` | Proc-macro/host | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-link` | `0.2.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-numerics` | `0.3.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-result` | `0.4.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-strings` | `0.5.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-sys` | `0.52.0` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-sys` | `0.61.2` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-targets` | `0.52.6` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows-threading` | `0.2.1` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | `MIT` | Target (`Ocy.exe`) | `license-apache-2.0`, `license-mit` | https://github.com/microsoft/windows-rs |
| `winit` | `0.30.13` | `Apache-2.0` | `Apache-2.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/rust-windowing/winit |
| `winnow` | `1.0.5` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-MIT` | https://github.com/winnow-rs/winnow |
| `writeable` | `0.6.4` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `xmlwriter` | `0.1.0` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/RazrFalcon/xmlwriter |
| `yazi` | `0.2.2` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dfrg/yazi |
| `yeslogic-fontconfig-sys` | `6.0.1` | `MIT` | `MIT` | Proc-macro/host | `LICENSE` | https://github.com/yeslogic/fontconfig-rs |
| `yoke` | `0.8.4` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `yoke-derive` | `0.8.4` | `Unicode-3.0` | `Unicode-3.0` | Proc-macro/host | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zeno` | `0.3.3` | `Apache-2.0 OR MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT` | https://github.com/dfrg/zeno |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zerofrom-derive` | `0.1.8` | `Unicode-3.0` | `Unicode-3.0` | Proc-macro/host | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zerotrie` | `0.2.5` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zerovec` | `0.11.9` | `Unicode-3.0` | `Unicode-3.0` | Target (`Ocy.exe`) | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zerovec-derive` | `0.11.7` | `Unicode-3.0` | `Unicode-3.0` | Proc-macro/host | `LICENSE` | https://github.com/unicode-org/icu4x |
| `zmij` | `1.0.23` | `MIT` | `MIT` | Target (`Ocy.exe`) | `LICENSE-MIT` | https://github.com/dtolnay/zmij |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` | https://github.com/etemesi254/zune-image |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | `MIT` | Target (`Ocy.exe`) | `LICENSE-APACHE`, `LICENSE-MIT`, `LICENSE-ZLIB` | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |

## 4. Linux Platform Additions (`damt-platform-linux`, `x86_64-unknown-linux-gnu`)

`damt-platform-linux` (`0.2.2-dev`, `AGPL-3.0-only`) is compiled only on `target_os = "linux"` (empty on Windows and other targets, leaving the `x86_64-pc-windows-msvc` dependency closure unchanged). Its direct Linux-only dependencies were already present in `Cargo.lock` (pulled by `slint` / `i-slint-backend-winit` / `accesskit_unix` on Linux):

| Crate | Version | Declared licence | Chosen | AGPL-3.0-only verdict | Repository |
| --- | --- | --- | --- | --- | --- |
| `async-channel` | `2.5.0` | `Apache-2.0 OR MIT` | `MIT` | Compatible | https://github.com/smol-rs/async-channel |
| `async-io` | `2.6.0` | `Apache-2.0 OR MIT` | `MIT` | Compatible | https://github.com/smol-rs/async-io |
| `futures-lite` | `2.6.1` | `Apache-2.0 OR MIT` | `MIT` | Compatible | https://github.com/smol-rs/futures-lite |
| `ksni` | `0.3.7` | `Unlicense` | `Unlicense` | Compatible (public-domain-equivalent; ships `UNLICENSE`) | https://github.com/iovxw/ksni |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | `MIT` | Compatible | https://github.com/image-rs/image-png |
| `x11rb` | `0.13.2` | `MIT OR Apache-2.0` | `MIT` | Compatible (`screensaver` and `randr` features) | https://github.com/psychon/x11rb |
| `zbus` | `5.19.0` | `MIT` | `MIT` | Compatible (`async-io` and `blocking-api` features) | https://github.com/z-galaxy/zbus/ |
| `wayland-client` | `0.31.15` | `MIT` | `MIT` | Compatible (Linux target only) | https://github.com/smithay/wayland-rs |
| `wayland-protocols` | `0.32.13` | `MIT` | `MIT` | Compatible (`client` and `staging` features; `ext-idle-notify-v1`) | https://github.com/smithay/wayland-rs |

`rust/tools/notices.py` also recognizes `UNLICENSE` files (covering `ksni 0.3.7`) and maps `accesskit_atspi_common 0.21.1` / `accesskit_unix 0.24.1` (`MIT OR Apache-2.0`) to the already-vendored `accesskit/*` upstream licence texts, so `python3 tools/notices.py --target x86_64-unknown-linux-gnu` completes cleanly (342 third-party crates).

## 5. macOS Platform Additions (`damt-platform-mac`, `aarch64-apple-darwin`)

`damt-platform-mac` (`0.2.2-dev`, `AGPL-3.0-only`) is compiled only on `target_os = "macos"` (empty on Windows, Linux, and other targets, leaving the `x86_64-pc-windows-msvc` dependency closure unchanged). Its macOS-only dependencies are pinned exactly and are pure-Rust `extern "C"` bindings to Apple system frameworks (no build script, no Apple SDK needed for `cargo check --target aarch64-apple-darwin`):

| Crate | Version | Declared licence | Chosen | AGPL-3.0-only verdict | Repository |
| --- | --- | --- | --- | --- | --- |
| `block2` | `0.6.2` | `MIT` | `MIT` | Compatible | https://github.com/madsmtm/objc2 |
| `objc2` | `0.6.5` | `MIT` | `MIT` | Compatible | https://github.com/madsmtm/objc2 |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | `MIT` | Compatible | https://github.com/madsmtm/objc2 |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | `MIT` | Compatible | https://github.com/madsmtm/objc2 |
| `objc2-foundation` | `0.3.2` | `MIT` | `MIT` | Compatible | https://github.com/madsmtm/objc2 |

Transitive additions from the same repository (all pulled in by the `objc2-app-kit` `NSWorkspace`/`NSRunningApplication` features; all `Zlib OR Apache-2.0 OR MIT`, **MIT** chosen, AGPL-compatible): `dispatch2 0.3.1`, `objc2-cloud-kit 0.3.2`, `objc2-core-data 0.3.2`, `objc2-core-foundation 0.3.2`, `objc2-core-image 0.3.2`, `objc2-core-text 0.3.2`, `objc2-core-video 0.3.2`, `objc2-metal 0.3.2`, `objc2-quartz-core 0.3.2`, plus `objc2-encode 4.1.0` (`MIT`). `bitflags`, `libc`, `jiff`, `thiserror`, and the `syn`/`proc-macro2`/`quote`/`unicode-ident`/`serde_core` host crates were already in the workspace closure.

The older `objc2 0.5.3` / `objc2-app-kit 0.2.2` / `objc2-foundation 0.2.2` / `objc2-core-data 0.2.2` / `objc2-core-image 0.2.2` / `objc2-metal 0.2.2` / `objc2-quartz-core 0.2.2` / `block2 0.5.1` versions already in `Cargo.lock` come from `slint`/`winit` macOS dependencies (present since M0); they are `MIT` and unchanged.

M2 (`M2-macos-shell`) adds no new third-party crates to `Cargo.lock`: `damt-platform-mac` reuses `png 0.18.1` (`MIT OR Apache-2.0`, **MIT** chosen, already audited in §3 and §4) for template menu-bar icon generation and enables additional features on the already-pinned `objc2-app-kit 0.3.2` and `objc2-foundation 0.3.2` (`MIT`) crates (also used by `ocy-app` under `cfg(target_os = "macos")`).
