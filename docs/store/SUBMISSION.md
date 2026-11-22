# Microsoft Store submission — Ocy

Paste-ready reference for every Partner Center field, what values you must paste from Partner Center, and how to build and upload the `.msix`.

> **Owner rule:** Do **not** include any GitHub repository URLs anywhere in the Microsoft Store listing.

The short version of what Ocy needs to run: **a 64-bit Windows 10 (1809+) or Windows 11 desktop and ~50 MB of disk.** It is a single self-contained native binary (`Ocy.exe` built with Rust + Slint) — no .NET, no WebView2, no VC++ redistributable, no account, and no internet connection.

---

## 1. Product reservation
| Field | Value |
|-------|-------|
| Product type | **MSIX or PWA app** (so the Store signs it — not "EXE or MSI app") |
| Name | Reserve your chosen Store name first (for example `Ocy` or `Ocy - Eye Break Reminder`). Set `OCY_STORE_DISPLAY_NAME` to match the reserved name exactly. |

## 2. Where to get & paste the Partner Center identity values
In **Partner Center → your product → Product management → Product identity**, copy these three values (plus your reserved display name) into your shell environment before running `packaging\windows\build_msix.ps1 -Store`:

| Environment variable | Partner Center source | Manifest element stamped |
|---|---|---|
| `OCY_STORE_IDENTITY_NAME` | Package/Identity/Name (e.g. `12345DanielMevit.Ocy`) | `<Identity Name="...">` (`__IDENTITY_NAME__`) |
| `OCY_STORE_PUBLISHER` | Package/Identity/Publisher (e.g. `CN=A1B2C3D4-...`) | `<Identity Publisher="...">` (`__PUBLISHER__`) |
| `OCY_STORE_PUBLISHER_DISPLAY` | Package/Properties/PublisherDisplayName | `<PublisherDisplayName>` (`__PUBLISHER_DISPLAY_NAME__`) |
| `OCY_STORE_DISPLAY_NAME` | Reserved Store product name (defaults to `Ocy` if unset) | `<Properties><DisplayName>` (`__DISPLAY_NAME__`) |

Do **not** commit Partner Center identity strings or certificates into the repository; `packaging/windows/AppxManifest.xml` stays a template with `__...__` placeholders.

## 3. Properties page
| Field | Value |
|-------|-------|
| Category | **Productivity** (alternative: **Health & fitness**) |
| Subcategory | (leave blank) |
| Secondary category | (optional; leave blank) |
| Privacy policy URL | `https://danielmevit.github.io/ocy/privacy/` |
| Website | `https://danielmevit.github.io/ocy/` |
| Support contact info | Use a support email address (or a contact page on `https://danielmevit.github.io/ocy/`) — **do not** use a GitHub repository link |

**Product declarations** (checkboxes):
- Uses the Microsoft commerce platform for purchases — **yes** for the paid + 15-day free trial Store listing (`StoreContext::RequestPurchaseAsync`)
- Depends on non-Microsoft drivers or NT services — **no**
- Accesses, collects, or transmits personal information — **no**. *Privacy note:* The Store build (`--features store`) asks the Microsoft Store for its licence and handles purchases through Windows (`Windows.Services.Store.StoreContext`); Ocy itself contains no network code, telemetry, or analytics and caches only the local licence state in `%APPDATA%\Ocy\license.json`.
- Is a screen reader / accessibility tool — **no**
- Uses a Bluetooth/USB peripheral — **no**

## 4. Age ratings (IARC questionnaire)
Answer **No** to everything — no violence, no user-to-user interaction, no data collection, no controlled substances/gambling. Result: **Everyone / 3+** in all regions.

## 5. Pricing and availability
| Field | Value |
|-------|-------|
| Base price | **Free** |
| Markets | **All markets** |
| Visibility | **Public** |
| Discoverability | Available in Store, discoverable |
| Release schedule | **As soon as it passes certification** |
| Device families | **Windows 10/11 Desktop** only |

## 6. System requirements
| Requirement | Minimum | Recommended |
|---|---|---|
| **OS** | Windows 10 version 1809 (build **17763**) | Windows 11 |
| **Architecture** | **x64 (64-bit)** | x64 |
| **Processor** | Any 64-bit x86-64 CPU | Dual-core 2 GHz+ |
| **Memory (RAM)** | **2 GB** | 4 GB |
| **Free disk space** | **50 MB** | 100 MB |
| **Graphics** | Desktop session (software-rendered Slint UI; no GPU driver requirement) | — |
| **Display resolution** | **1024 × 768** | 1920 × 1080 |
| **Internet** | **Not required** — fully offline | Not required |
| **Runtime prerequisites** | **None** — single self-contained `Ocy.exe` | None |
| **Touch** | Not required | — |

## 7. What the package declares (auto-read from `AppxManifest.xml`)
| Manifest field | Value | Meaning |
|---|---|---|
| Identity Name | `__IDENTITY_NAME__` (stamped at build) | Package identity from Partner Center |
| Publisher | `__PUBLISHER__` (stamped at build) | Store publisher GUID |
| Version | `<major>.<minor>.<patch>.0` (stamped from `rust/Cargo.toml`) | Four-part MSIX version |
| ProcessorArchitecture | `x64` | 64-bit desktop |
| Min OS / Max tested | `10.0.17763.0` / `10.0.26100.0` | Windows 10 1809 through Windows 11 24H2 |
| Entry point | `Windows.FullTrustApplication` (`Ocy.exe`) | Packaged Win32 desktop app |
| Capability | `runFullTrust` | Only declared capability (no camera, mic, location, internet, etc.) |
| Startup task | `desktop:StartupTask` (`TaskId="OcyStartup"`, `Enabled="false"`, `DisplayName="Ocy"`) | Managed via `Windows.ApplicationModel.StartupTask` when packaged |

## 8. Store listing copy (no GitHub links)
- **Short description:**
  > Calm, offline break reminder for your eyes. Gives you brief eye exercises every few minutes and a full break every hour, without stealing focus or interrupting fullscreen apps.
- **Description:**
  > Ocy is a calm, light break reminder that helps you rest your eyes while working at your computer.
  >
  > It runs quietly in your system tray and nudges you with brief eye exercises every few minutes, plus a real break every hour so you can step away and stretch.
  >
  > What Ocy does:
  > • Short breaks with gentle eye exercises in a small corner card (or dimmed screen if you prefer)
  > • Hourly long breaks with a brief heads-up first, so you can finish a sentence or postpone
  > • Counts active screen time only — pauses automatically when you step away or lock your PC
  > • Defers breaks while you are in fullscreen video, games, or presentations
  > • Never steals keyboard focus during short breaks or warnings
  > • Tracks today's completed and skipped breaks with a 2-hour schedule timeline
  > • Optional strict mode and customizable presets (Gentle, Balanced, Focused)
  > • Optional launch at sign-in controlled cleanly through Windows Startup settings
  >
  > Good to know:
  > • Works completely offline — no network connections, no telemetry, no account
  > • Settings and daily statistics stay on your PC
- **Keywords (up to 7):**
  `eye break`, `break reminder`, `eye strain`, `20-20-20`, `posture`, `screen break`, `ergonomics`
- **Copyright / Developed by:**
  - Copyright: `© 2026 Daniel Mitev`
  - Developed by: `Daniel Mevit — damt.xyz`

## 9. Submission Options — `runFullTrust` justification (REQUIRED)
Because `Ocy` declares `runFullTrust`, Partner Center will show a restricted-capability notice after upload. Paste this on the **Submission Options** page:

> Ocy is a full-trust Win32 desktop application (written in Rust using the Slint UI toolkit, packaged as MSIX). It declares `runFullTrust` because that capability is required for a packaged Win32 executable to run outside the UWP AppContainer sandbox. Ocy runs in the Windows system tray, monitors local keyboard/mouse idle time (`GetLastInputInfo`) and fullscreen presentation state (`SHQueryUserNotificationState`) to time eye-rest breaks, renders non-activating break reminder windows, and stores user preferences, daily break counts, and the cached Store licence state locally in the user's AppData folder. Ocy itself contains no network code (the Store build queries the Microsoft Store for its licence through the built-in Windows `StoreContext` API), installs no drivers or background services, and collects or transmits no personal data.

## 10. Notes for certification (reviewer instructions)
> Ocy starts minimized in the system tray (notification area) with no initial window. Click or right-click the Ocy sprout icon in the tray to open Settings, start a short or long break immediately ("Take a short break" / "Take a long break"), or pause reminders. All functionality works offline with no account or configuration required.

## 11. Build and upload steps (Windows)
1. Ensure Windows 10 SDK (for `makeappx.exe`, `makepri.exe`), Rust toolchain, and Python 3 are installed on Windows.
2. Set the Partner Center identity environment variables in PowerShell:
   ```powershell
   $env:OCY_STORE_IDENTITY_NAME     = "<your Partner Center Identity Name>"
   $env:OCY_STORE_PUBLISHER         = "<your Partner Center Publisher, e.g. CN=...>"
   $env:OCY_STORE_PUBLISHER_DISPLAY = "<your Partner Center PublisherDisplayName>"
   $env:OCY_STORE_DISPLAY_NAME      = "<your reserved Store product name>"
   ```
3. Build the unsigned Store `.msix`:
   ```powershell
   powershell -ExecutionPolicy Bypass -File packaging\windows\build_msix.ps1 -Store
   ```
4. Upload `builds\store\Ocy-<version>-store.msix` on the **Packages** tab in Partner Center. (Keep it **unsigned** — Microsoft Store signs it during certification.)
5. Optional local sideload test before uploading (uses a local self-signed `.pfx` outside the repo; never commit `.pfx` or passwords):
   ```powershell
   powershell -ExecutionPolicy Bypass -File packaging\windows\build_msix.ps1 -SignForSideload C:\temp\ocy-test.pfx
   Add-AppxPackage builds\store\Ocy-<version>-sideload.msix
   ```
