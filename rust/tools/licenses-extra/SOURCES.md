# Vendored Upstream Licence Texts (`licenses-extra`)

These files are fallback licence and copyright texts for third-party crates in
`ocy-app`'s dependency closure whose published crates.io `.crate` tarballs
omit `LICENSE*` / `COPYING*` / `NOTICE*` files.

`rust/tools/notices.py` uses a vendored file **only** when a crate's own source
directory ships no licence files, and labels the resulting notice entry with
`licence text from upstream repository`.

All files below were fetched from each repository's **default branch** on
**2026-11-19**.

| Local path | Crates covered | Licence | Upstream URL (default branch, fetched 2026-11-19) |
| --- | --- | --- | --- |
| `clipboard-win/LICENSE` | `clipboard-win` (`5.4.3`) | `BSL-1.0` | https://raw.githubusercontent.com/DoumanAsh/clipboard-win/master/LICENSE (default branch: `master`) |
| `lyon/LICENSE-MIT` | `lyon_algorithms` (`1.0.22`), `lyon_extra` (`1.1.0`), `lyon_geom` (`1.0.20`), `lyon_path` (`1.0.19`) | `MIT` (chosen from `MIT OR Apache-2.0`) | https://raw.githubusercontent.com/nical/lyon/main/LICENSE-MIT (default branch: `main`) |
| `taffy/LICENSE` | `taffy` (`0.15.1`) | `MIT` | https://raw.githubusercontent.com/DioxusLabs/taffy/main/LICENSE (default branch: `main`) |
| `rspolib/LICENSE` | `rspolib` (`0.1.2`) | `MIT` | https://raw.githubusercontent.com/mondeja/rspolib/master/LICENSE (default branch: `master`) |
| `accesskit/LICENSE-APACHE` | `accesskit` (`0.25.2`), `accesskit_consumer` (`0.39.2`), `accesskit_windows` (`0.35.2`), `accesskit_winit` (`0.34.2`) | `Apache-2.0` | https://raw.githubusercontent.com/AccessKit/accesskit/main/LICENSE-APACHE (default branch: `main`; identical to crate VCS commit `f3026222559c7f5f46d8f25cb7404de68eb149f0`) |
| `accesskit/LICENSE-MIT` | `accesskit` (`0.25.2`), `accesskit_consumer` (`0.39.2`), `accesskit_windows` (`0.35.2`) | `MIT` | https://raw.githubusercontent.com/AccessKit/accesskit/main/LICENSE-MIT (default branch: `main`; identical to crate VCS commit `f3026222559c7f5f46d8f25cb7404de68eb149f0`) |
| `accesskit/LICENSE.chromium` | `accesskit` (`0.25.2`), `accesskit_consumer` (`0.39.2`), `accesskit_windows` (`0.35.2`) | `BSD-3-Clause` (Chromium-derived portions) | https://raw.githubusercontent.com/AccessKit/accesskit/main/LICENSE.chromium (default branch: `main`; identical to crate VCS commit `f3026222559c7f5f46d8f25cb7404de68eb149f0`) |
| `accesskit/AUTHORS` | `accesskit` (`0.25.2`), `accesskit_consumer` (`0.39.2`), `accesskit_windows` (`0.35.2`), `accesskit_winit` (`0.34.2`) | Copyright authors list | https://raw.githubusercontent.com/AccessKit/accesskit/main/AUTHORS (default branch: `main`; identical to crate VCS commit `f3026222559c7f5f46d8f25cb7404de68eb149f0`) |

## Notes
- **`lyon_*` (`MIT OR Apache-2.0`):** Upstream `nical/lyon`'s `LICENSE-APACHE` file is only a short pointer notice rather than the full Apache-2.0 licence text, so Ocy chooses `MIT` (`lyon/LICENSE-MIT`) for all `lyon_*` crates.
- **`accesskit*`:** `accesskit`, `accesskit_consumer`, and `accesskit_windows` are `MIT OR Apache-2.0` and also reference `LICENSE.chromium` (`BSD-3-Clause`) in Chromium-derived source headers; `accesskit_winit` is `Apache-2.0` only. AccessKit's `LICENSE-MIT` omits a copyright line because copyright holders are listed in `AUTHORS` and source file headers (`Copyright 2021 The AccessKit Authors`), so `AUTHORS` is included alongside the licence texts.
