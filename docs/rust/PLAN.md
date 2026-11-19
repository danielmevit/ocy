# Ocy in Rust — plan (draft for owner approval, 2026-11-19)

Owner decision (2026-11-19): rebuild the tools in Rust, starting with Ocy, and build a **shared package** that every later
app reuses. Priorities: app efficiency and speed, high tech, even if it takes longer.

This plan is the single reference for the lead and the workers. Numbers marked *budget* are targets to be proven by
measurement; nothing here is a measured result yet, except where stated.

## 1. Principles
1. **Measure, don't guess.** Phase R0 measures the C# 0.1.2 baseline and a minimal Rust spike on the owner's PC
   before any stack is locked.
2. **Idle means idle.**
   - With no break on screen, Ocy holds no windows, no renderer and no decoded artwork: just the tray and the
     engine.
   - UI is created on demand and dropped afterwards.
3. **Parity before switch.** The C# app keeps shipping as 0.1.x until the Rust build passes the parity checklist (§9). The
   Rust build ships as **0.2.0** (owner decision 2026-11-19: the Rust line is 0.2.x). **Everything after the
   parity release also ships as 0.2.x** (0.2.1, 0.2.2 …; owner, 2026-11-19: no 0.3). See §12.
4. **The kit grows from real needs.**
   - Shared crates (`damt-*`) start inside the Ocy repo, with narrow APIs that Ocy actually uses.
   - They are generalised and extracted into their own repo only when the second app (Vecto) starts.
5. **Same quality rules as today:**
   - runtime checks on Windows, not verify alone
   - frame strips for anything animated
   - the scorecard, recording experience before every merge
   - LOG stamped with `date +%H:%M`

## 2. Budgets (to be confirmed by R0)
| Metric | C# 0.1.2 (**measured 2026-11-19, owner's PC**) | Rust *budget* |
| --- | --- | --- |
| Download | 69.9 MB zip; 204.8 MB unpacked, 230 files | ≤ 12 MB, single `Ocy.exe`, no runtime |
| Idle RAM (tray only) | 100 MB working set / 129 MB private | ≤ 15 MB private |
| RAM with Settings open | 144 MB working set / 167 MB private | ≤ 40 MB private |
| Launch → initialised | ~0.75 s (start + smoke + exit: 1.0 s warm, 2.2 s cold) | ≤ 150 ms to the tray icon |
| CPU while idle | ~330 ms of CPU per minute (~0.55 % of a core, constantly) | ≈ 0 (≤ 10 ms/min after R4b) |
| Timer wake-ups while working | 60/min (1 s tick) | ≤ 6/min (after R4b, see §5) |
| Break due → card visible | ? (R0) | ≤ 50 ms |

CI enforces size and startup budgets once they are set from R0's numbers.


## R0 results (measured 2026-11-19 on the owner's PC; spike in `Ocy/rust-spike`, data in `Ocy/builds/_spike/`)
| Metric | C# 0.1.2 | Rust spike (Slint 1.19, **software renderer only**) | Budget |
| --- | --- | --- | --- |
| Exe | 69.9 MB zip, 230 files | **10.5 MB** single exe, static CRT (no VCRUNTIME140) | ≤ 12 MB ✅ |
| Process start → tray | ~750 ms | **53 ms** | ≤ 150 ms ✅ |
| Idle private memory (tray only) | 129 MB | **3.2–3.4 MB** | ≤ 15 MB ✅ |
| Private memory with the card shown | — (Settings: 167 MB) | **4.8 MB** | ≤ 40 MB ✅ |
| After the break (windows dropped) | — | **4.55 MB** (memory comes back) | ✅ |
| Idle CPU | ~330 ms/min | **0 ms in 30 s** | ≈ 0 ✅ |
| Break due → card visible | ? | **~120 ms** (`show()` returns in 0.8 ms; the HWND is visible 120 ms later) | ≤ 50 ms ❌ → R3 |

**Decisions after R0:**
- **D1 Slint:** confirmed.
- **D2 software renderer:** confirmed, and femtovg is **excluded from the build**. A memory bisect showed that merely
  compiling in `renderer-femtovg` commits ~54 MB of private memory at backend start (GPU/OpenGL initialisation),
  even when the software renderer runs. The femtovg run also took 1.0–1.8 s to first frame and ~190 MB with the card.
- **D3 Slint `SystemTrayIcon`:**
  - all 7 runtime icon switches (4 states × light/dark) were accepted, but the setter has no error channel
  - it can't be checked on screen in the spike, because Windows 11 hides new tray icons in the overflow
  - visual confirmation is moved to R2
  - the fallback `tray-icon` measured 2.4 MB private
- **D4 native layered overlays:** confirmed and pixel-verified on both secondary monitors (exactly 3 px #2F7D62 at
  55 %, transparent inside, rect = monitor rect, ex-style `0x080800A8`). Created in ~15 ms each.
- **Transparency:** a Slint window with `background: transparent` + `no-frame` was visibly transparent around its
  content (software renderer).

**New R3 items:**
- card latency (pre-create the window hidden, or keep one card window and reuse it)
- break windows must not get a taskbar button or take focus: Slint gives `WS_EX_APPWINDOW`, so set
  `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` through the HWND, and remove `WS_CAPTION` bits if they affect hit-testing
- mixed-DPI testing (all 3 of the owner's monitors are 96 DPI)

## 3. Architecture

### Repository layout (during migration, inside the Ocy repo)
```
rust/                       Cargo workspace (becomes the repo root at switchover)
  Cargo.toml  rust-toolchain.toml  .cargo/config.toml
  crates/                   the shared kit, MIT OR Apache-2.0 (apps stay AGPL-3.0)
    damt-app        single instance, data paths, logging (tracing, size-capped file), panic hook,
                    settings store (serde, atomic write, versioned migration, debounced save), hidden dev modes
    damt-platform   OS-service traits + Windows impl (windows crate) + "unsupported" fallbacks:
                    idle time, fullscreen/presentation, session lock/unlock/suspend/resume, autostart,
                    active monitor, reduced-motion preference, taskbar theme
    damt-overlay    native layered Win32 windows: click-through edge frames and the long-break dim, per monitor
    damt-tray       tray icon state machine + icon sets per taskbar theme
    damt-theme      design tokens: ONE tokens file → Slint globals + CSS for the websites
    damt-ui         Slint component library (buttons accent/outline/text, card, setting row, toggle,
                    number field, progress line) matching docs/ai/DESIGN_SYSTEM.md
    damt-art        SVG frame player: manifest + resvg rasterisation (cached per size/theme/DPI, freed after
                    use), stepped sequencer, reduced-motion freeze
    damt-i18n       strings + plural rules (see decision D5), export of the website JSON
    damt-update     auto-update via Velopack (phase R5)
  apps/ocy/
    ocy-core        the break engine, settings model, statistics, schedule: a pure port of Ocy.Core
    ocy-app         the binary: wiring, coordinator, .slint windows, tray, overlays
  xtask/            cargo xtask: dist (exe, zip, checksums), render (golden UI PNGs), budgets, art/icons
```

### Key decisions (with the alternative)
| # | Decision | Why | Alternative if R0 says no |
| --- | --- | --- | --- |
| D1 | **Slint** for cards and settings (declarative `.slint`, compiled to native code) | small binaries, low RAM, a declarative design close to Avalonia, Android support for the mobile plan; GPLv3 or royalty-free licence, both fine for an AGPL app | — |
| D2 | **Slint software renderer ONLY** (femtovg not compiled in; R0: +54 MB private just by being present) | no GPU context, smallest binary and RAM; the cards are small and only the countdown repaints (partial redraw) | femtovg (OpenGL), if the long-break overlay is too slow |
| D3 | **Tray via Slint `SystemTrayIcon`** (1.17+), with icons bound from code | one toolkit; Windows uses `Shell_NotifyIcon` | the `tray-icon` + `muda` crates |
| D4 | **Overlays (edge frames, dim) as native layered Win32 windows** in `damt-overlay` | exact click-through (`WS_EX_LAYERED \| WS_EX_TRANSPARENT \| WS_EX_NOACTIVATE`), no renderer needed, per-monitor sizes; avoids Windows' minimum window size and transparency traps (GOTCHAS) | Slint windows plus the HWND via `window_handle()` |
| D5 | **i18n: Slint's built-in `@tr()` (gettext, with Plural-Forms for Slavic plurals)**; `xtask` exports the website's `en.json` from the same source | integrated with the UI and its tooling; one source for the app and the site | Fluent (`fluent-bundle`) |
| D6 | **Art: resvg/tiny-skia** renders the existing SVG frames straight from `assets/` | no converter step and no XAML; the same files as the website | — |
| D7 | **Settings compatibility:** read the existing `%APPDATA%\Ocy\settings.json/state.json/stats.json` | users keep their settings and pause state when they upgrade | — |
| D8 | **Release profile:** `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`, `strip = true`, `panic = "abort"` (a panic hook logs first), static CRT, per-monitor-v2 DPI manifest, embedded icon | the smallest and fastest single exe with no VC++ redistributable | `opt-level = "s"` if size matters more |
| D9 | **Natural-moment timing in 0.2.0** (owner, 2026-11-19): when a break is due, wait up to ~60 s for a short lull in keyboard/mouse input before showing it. Built in R4 with the adaptive scheduler; the website then states it | never interrupts mid-sentence; a differentiator | showing breaks exactly when due (C# behaviour) |
| D10 | **"Take a short break now" tray item + clickable-looking "Try…" links in Settings** (owner, 2026-11-19), in the Rust build before 0.2.0 | a short break is otherwise hard to start on demand | keep the tray menu as in C# |
| D11 | **No C# 0.1.3** (owner, 2026-11-19): the small sprout icon ships with 0.2.0, which already embeds it | one fewer release | a C# release just for the icon |
| D12 | **Left-click on the tray icon opens Settings** (owner, 2026-11-19); right-click keeps the menu | the most direct way in; Settings becomes the home/dashboard after the redesign | C#: left click did nothing |
| D13 | **Tray pause state is visible** (owner, 2026-11-19): a checkmark on the active pause option, plus a status line "Paused until 15:40 · 1 h 15 min left" (or "until you resume") | you can see what you chose and when it ends | only the hover tooltip |

## 4. How the app runs (efficiency design)
- **Startup:**
  - single-instance check, load settings, create the tray and start the engine
  - nothing else is created; no windows exist until a break is due
- **Break:**
  - the coordinator creates the card window and rasterises only the needed frames at the monitor's DPI
  - it opens native overlays on the other monitors
  - when the break ends it destroys them and frees the frame cache
- **Session events** (lock, unlock, suspend, resume) arrive as events (`WTSRegisterSessionNotification`, power
  broadcast), not by polling.
- **Fullscreen checks** run lazily, only when a break is due.

## 5. Scheduling: two steps, never mixed
- **R1 (parity):** port the engine with its exact `Tick(now)` semantics and every test. The host keeps a 1 s tick.
  This makes the port verifiable 1:1 against the C# tests.
- **R4b (optimisation, a separate change):** an adaptive host scheduler sleeps until the next relevant moment (next
  due time, or an idle re-check every ~10 s while working).
  - Rule 1 ("dt > 15 s = sleep") is then redefined explicitly with a monotonic clock plus suspend/resume events, so
    a long sleep between ticks is not mistaken for suspend.
  - New tests cover it.

## 6. Testing
- **Engine:** every C# Core test ported (the parity oracle), plus `proptest` invariants (counters never negative, a
  pause survives everything, an absence is never credited twice).
- **Settings:** migration tests using real C#-written JSON files.
- **UI golden renders:** Slint's software renderer draws each window to PNG headless and deterministically; tests
  compare against approved images with a small tolerance. This replaces the C# `--render-screens` workaround.
- **Animation:** the frame-identity test (only original frames are ever shown) plus recorded frame strips.
- **Windows integration:** GitHub Actions `windows-latest` runs the platform crates' tests and a smoke run on every
  push. The lead still does hands-on checks on the owner's PC.
- **Benchmarks** (`criterion`) for the engine tick; budget checks in CI.

## 7. Build and toolchain
- **Rust:** stable (1.95 already installed in WSL), pinned in `rust-toolchain.toml`.
- **Windows builds (owner decision 2026-11-19): `cargo xwin` in WSL.** The owner accepts Microsoft's CRT/SDK licence
  for the downloaded build files. Release builds also run in CI on `windows-latest`. Rust on Windows itself is not
  needed (the exe has no runtime); install it later only for native debugging.
- **Speed and disk:**
  - every Unio route sets `CARGO_TARGET_DIR=$HOME/.cache/ocy-target/<worker>` on the Linux filesystem: the `/mnt/d`
    mount is slow for cargo, D: has ~17 GB free, and per-worker dirs avoid lock contention
  - optional `sccache`

## 8. Phases
| Phase | Content | Exit criteria | Estimate |
| --- | --- | --- | --- |
| **R0 Spike + baseline** | C# 0.1.2 numbers (idle working set + private bytes, Settings open, launch → tray, size, wake-ups) on the owner's PC. Rust spike: tray switching 4 icons × 2 themes; one click-through frame on each of the 3 monitors; one short-break card with a resvg frame; software vs femtovg renderer | a go/no-go table; D1–D4 confirmed or swapped; budgets set from real numbers | 1–2 days |
| **R1 Foundations + core** | workspace, xtask, CI, `ocy-core` port with every engine test, settings file compatibility | all ported tests green; settings round-trip with C# files | 3–4 days |
| **R2 Kit + shell** | `damt-app`, `damt-platform` (Windows), `damt-tray`, `damt-overlay`, single instance, autostart, logging | Windows integration tests green in CI; tray + overlays on the owner's PC | 3–4 days |
| **R3 UI** | `damt-theme`, `damt-ui`, `damt-art`, `damt-i18n`; Ocy's short break, warning, long break and settings; reduced motion; taskbar theme | golden renders approved by the owner; frame strips clean | 5–7 days |
| **R4 Parity + switch** | R4a owner hands-on fixes (first-click bug, theme-aware window icon); R4b adaptive scheduling + natural-moment timing (D9); R4c screen-reader parity (Slint accessibility, UIA check); R4d dist (zip) + CI budgets; R4e README/site (Opus); the parity checklist; C# kept under a tag | checklist 100 %; budgets met; owner hands-on OK → ship as 0.2.0 | 3–4 days |
| **R5 After the switch** | Velopack auto-update, MSIX/Store, code signing, macOS/Linux platform impls | — | later |

Total to switch: about **15–21 working days** (3–4 weeks with reviews). The kit makes Vecto, Laydown and Eqho
faster to start.

## 9. Parity checklist (from ARCHITECTURE rules 1–8, the 0.1.x features and GOTCHAS)
- **Engine:**
  - rules 1–8: gap = sleep, pause survives everything, breaks complete across a lock, away credit (short/long),
    deferral only during fullscreen, no double credit
  - postpone limits, strict mode
  - short/long schedules and ranges
  - statistics
- **Desktop:**
  - single instance (second launch opens Settings)
  - autostart
  - break on the active monitor with click-through frames on the others; warning in clay
  - long-break dim on every monitor, with the card on the active one
  - live-apply settings
  - tray: 4 states × light/dark taskbar sets; pause menu (1 h / 3 h / until resumed); take a long break now
  - illustrations with manifest timing and reduced motion
  - app icon (sprout at 16–32 px, face at 40+)
- **Windows traps carried over:**
  - minimum borderless window size (use full-screen layered frames, never thin strips)
  - real transparency
  - mixed-DPI monitors
  - Alt+F4 can't close break windows in strict mode
- **Files:** existing settings/state/stats are read and preserved; error log capped at 256 KB.

## 10. Process (Unio)
- **Worker routing (owner rule, 2026-11-19):**
  - **all visual work → Opus 5.5 low only** (run directly): app UI and redesigns, colours and contrast, icons,
    the website, and mobile UI. Other workers get visual work only as a minor fix.
  - Gemini 4 Hydrogen / GLM 5.3 high (Vibe) for kit, platform, engine and build work
  - free routes (DeepSeek, Copilot) only for small mechanical tasks, never on the critical path
  - the lead reviews everything at runtime on the owner's PC
- **Scorecard:** tracks Rust correction rounds separately (expected 1.3–2× more than for C#).
- **Before any dispatch:**
  - every route needs cargo on `PATH` plus its `CARGO_TARGET_DIR`
  - validate lines are dry-run first (lesson from OC11b)

## 11. Risks
| Risk | Mitigation |
| --- | --- |
| Slint gaps (tray icon updates, transparency, multi-window) | R0 proves each one; fallbacks D3/D4 |
| Working set doesn't shrink after dropping windows (allocator) | measure private bytes; try mimalloc; keep the frame cache bounded |
| Slow builds on `/mnt/d` | per-worker `CARGO_TARGET_DIR` on the Linux filesystem |
| AI workers struggle with fast-moving UI crate APIs | pin versions; Opus for UI; give docs links in tasks |
| The website reads `content/strings/en.json` and the manifest | D5 export keeps the site in sync; site and app change together |
| **Slint software renderer has no font fallback on Windows** (R3d: a ✕ glyph Segoe lacks rendered as nothing) | draw icons as paths (kit CloseButton); the i18n phase must render every language (mk Cyrillic, cs/sk diacritics, …) on Windows and check for missing glyphs; bundle a fallback font only if needed |
| Antivirus false positives on a small unsigned exe | code signing in R5; submit to Microsoft for analysis if flagged |

## 12. Roadmap after 0.2.0 (all 0.2.x; owner, 2026-11-19)
Things already built will get proper adjustments when their turn comes.
- **Colours and contrast (Opus):**
  - audit every text, surface, border, state and overlay pair in light and dark against WCAG AA (normal text
    4.5:1, large text and UI parts 3:1)
  - rethink the palette where it's weak
  - the owner approves before/after renders before anything changes
- **Settings redesign (Opus):**
  - 2–3 concepts rendered with the real Slint kit; the owner picks one, then it's built
  - direction so far:
    - presets first ("Gentle / Balanced / Focused") with details under "Customize"
    - friendly sliders instead of spin boxes
    - a live rhythm preview ("next short break in 7 min")
    - grouping by intent: how often / how long / how strict / when to stay quiet
    - Ocy in the header, and a small "today" card from `stats.json`
- **All languages (i18n, D5):**
  - the Slint `@tr()` / gettext pipeline, with `en.json` as the source
  - a language picker in Settings
  - per-script fonts: the software renderer has no fallback, so Cyrillic, Greek, CJK, Devanagari etc. need a
    font that covers them loaded explicitly; test every language's glyph coverage on Windows
  - plural rules, including the Slavic forms
  - an RTL decision (Arabic, Hebrew, Persian need mirrored layouts)
  - AI translation plus native review where possible (mk by the owner)
  - the website follows the same strings
- **Screen readers:** if not finished in R4c, completed here.
- **Features:**
  - sounds and status notifications (the C# phase-2 rows)
  - a stats view
  - Velopack auto-update + code signing (no SmartScreen warning)
  - MSIX / Microsoft Store and winget
- **Desktop platforms:** macOS and Linux implementations of `damt-platform` (the traits are already
  platform-neutral).

## 13. Mobile app (plan only; owner, 2026-11-19)
- **Goal:** Ocy on Android and iOS, sharing:
  - `ocy-core` (engine, settings schema, stats)
  - `damt-art` (the same SVG frames via resvg)
  - the theme tokens and the strings
- **What each OS allows (shapes the product):**
  - **Android:**
    - reminders via a foreground service / WorkManager / exact alarms, plus notifications
    - an optional on-screen break card over other apps with the "Display over other apps" permission
    - "active time" = screen on and unlocked; usage stats need special access
  - **iOS:**
    - no drawing over other apps, and background timers are limited
    - breaks are notification-led (scheduled local notifications), with the full break UI when the user taps in
    - a Live Activity / widget can show the countdown
    - Screen Time APIs need a special entitlement
- **UI technology (decide in spike M0):**
  - Slint on Android (Rust is supported) and iOS (since 1.12, a tech preview at launch; it runs through winit +
    **Skia**; safe areas and the on-screen keyboard since 1.15; Slint's own viewer has been on Google Play and the
    App Store since 1.17), reusing `damt-ui`. On mobile the GPU renderer is fine: desktop's software-only rule (D2)
    is a desktop memory decision.
    Sources: docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/, slint.dev/blog/slint-1.12-released,
    slint.dev/blog/slint-1.15-released, slint.dev/blog/slint-1.17-released
  - or thin native shells (Kotlin/Swift) around the Rust core via UniFFI
  - **The mobile UI is designed by Opus** as a mobile-first layout, not a shrunken desktop window.
- **Keep the desktop code mobile-ready now:**
  - `ocy-core` stays free of desktop assumptions (it already is: idle/focus/session come in through traits)
  - add notification and scheduling traits to `damt-platform` when mobile starts
  - no Windows-only types in shared crates
- **Phases:**
  - **M0 spike:** Slint vs native shell on both OSes; one notification, one break card; size, memory and battery
    measured
  - **M1:** shared core + mobile platform implementations
  - **M2:** the mobile UI (Opus)
  - **M3:** store releases (accounts, review, privacy labels: Ocy collects nothing)
  - Starts after desktop 0.2.0 is stable. Version line for mobile: owner to decide at M0.

