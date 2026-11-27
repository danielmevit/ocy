# Microsoft Store submission — Ocy

Paste-ready reference for every Partner Center field, what values you must paste from Partner Center, and how to build and upload the `.msix`.

> **Owner rule:** Do **not** include any GitHub repository URLs anywhere in the Microsoft Store listing.

The short version of what Ocy needs to run: **a 64-bit Windows 10 (1809+) or Windows 11 desktop and ~50 MB of disk.** It is a single self-contained native binary (`Ocy.exe` built with Rust + Slint) — no .NET, no WebView2, no VC++ redistributable, no account, and no internet connection.

---

## 1. Product reservation
| Field | Value |
|-------|-------|
| Product type | **MSIX or PWA app** (so the Store signs it — not "EXE or MSI app") |
| Name | Launch as **Ocy** (reserved); switch to **Ocy - Screen Break Reminder** once it can be reserved in the same product (see LISTING.md). `OCY_STORE_DISPLAY_NAME` must match the reserved name the listing uses, exactly. |

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
| Privacy policy URL | `https://ocy.damt.xyz/privacy/` |
| Website | `https://ocy.damt.xyz/` |
| Support contact info | Use a support email address (or a contact page on `https://ocy.damt.xyz/`) — **do not** use a GitHub repository link |

**Product declarations** (checkboxes):
- Uses the Microsoft commerce platform for purchases — **yes** for the paid + 15-day free trial Store listing (`StoreContext::RequestPurchaseAsync`)
- Depends on non-Microsoft drivers or NT services — **no**
- Accesses, collects, or transmits personal information — **yes**. Privacy policy URL: `https://ocy.damt.xyz/privacy/`. *Privacy note:* Since 0.3.0, only if the user opts in (Settings), Ocy sends one anonymous daily summary of break counts and settings, with a random install ID, to PostHog (EU region); it is off by default. The Store build (`--features store`) also asks the Microsoft Store for its licence and handles purchases through Windows (`Windows.Services.Store.StoreContext`) and caches the local licence state in `%APPDATA%\Ocy\license.json`.
- What's new in 0.3.0 (privacy-relevant): optional anonymous usage stats, off unless the user turns them on (see `docs/store/LISTING.md`).
- Is a screen reader / accessibility tool — **no**
- Uses a Bluetooth/USB peripheral — **no**

## 4. Age ratings (IARC questionnaire)
Answer **No** to everything — no violence, no user-to-user interaction, no data collection, no controlled substances/gambling. Result: **Everyone / 3+** in all regions.

## 5. Pricing and availability
| Field | Value |
|-------|-------|
| Base price | **OWNER DECISION** — a paid price tier the owner chooses in Partner Center. It must **not** be Free: the listing promises "Try it free for 15 days, then buy it once", and the app has a Buy button (`StoreContext::RequestPurchaseAsync`), so the base price is the paid tier the owner picks |
| Free trial | **15 days** |
| Markets | **All markets** |
| Visibility | **Public** |
| Discoverability | Available in Store, discoverable |
| Release schedule | **As soon as it passes certification** |
| Device families | **Windows 10/11 Desktop** only |

Field names above follow Microsoft's "Pricing and availability" documentation for Partner Center
("Base price" = the price tier you pick per market or for all markets; "Free trial" = the trial
length offered on top of the paid base price):
<https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/price-and-availability>

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
`docs/store/LISTING.md` is the **single source** for the listing text: product name, description,
short description, product features, "What's new in this version", keywords, copyright, category,
privacy questionnaire, screenshots, and notes for certification. Paste from there; do not keep a
second copy here (the copies had drifted — for example, the copy that used to live here claimed
Ocy "defers breaks while you are in fullscreen video, games", while since 0.2.2 games get gentle
cards at a calm moment instead).

Still specific to this submission:
- **Owner rule:** no GitHub repository URLs anywhere in the listing.
- Enter the LISTING.md text once per language on the Store listing tab; the screenshots planned
  in LISTING.md upload on the same tab.

## 9. Submission Options — `runFullTrust` justification (REQUIRED)
Because `Ocy` declares `runFullTrust`, Partner Center will show a restricted-capability notice after upload. Paste this on the **Submission Options** page:

> Ocy is a full-trust Win32 desktop application (written in Rust using the Slint UI toolkit, packaged as MSIX). It declares `runFullTrust` because that capability is required for a packaged Win32 executable to run outside the UWP AppContainer sandbox. Ocy runs in the Windows system tray, monitors local keyboard/mouse idle time (`GetLastInputInfo`) and fullscreen presentation state (`SHQueryUserNotificationState`) to time eye-rest breaks, renders non-activating break reminder windows, and stores user preferences, daily break counts, and the cached Store licence state locally in the user's AppData folder. The Store build queries the Microsoft Store for its licence through the built-in Windows `StoreContext` API. Ocy installs no drivers or background services, and sends nothing by default: only if the user opts in to the optional anonymous usage stats in Settings does it send one anonymous daily summary to PostHog (EU region).

## 10. Notes for certification (reviewer instructions)
> Ocy is a tray app: after install it starts in the notification area with no window. Left-click the Ocy sprout icon to open Settings. Right-click it for the menu: "Take a short break now", "Take a long break now", "Hold breaks until I close this game" (only shown while a game is running), "Pause for 1 hour", "Pause for 3 hours", "Pause until resumed", "Resume reminders", "Settings", "Quit Ocy". All functionality works offline with no account or configuration required. Optional anonymous usage stats are off by default and can be turned on or off in Settings. It has a 15-day free trial managed by the Microsoft Store licence.

## 11. Checking the trial UI before submitting
The Store build's trial UI (trial days left, "Your trial has ended.", the Buy button) can be opened
on Windows without a real Store licence, using isolated demo runs. Build `Ocy.exe` with
`--features store`, then run each state from a terminal:

```powershell
.\Ocy.exe --demo-settings --demo-license full            # paid: no trial UI in Settings
.\Ocy.exe --demo-settings --demo-license trial          # "5 days left in your trial"
.\Ocy.exe --demo-settings --demo-license trial-last-day # "1 day left in your trial"
.\Ocy.exe --demo-settings --demo-license trial-ended    # "Your trial has ended." + "Buy Ocy"
.\Ocy.exe --demo-settings --demo-license unknown        # unknown licence (behaves as an active trial)
```

- Both `--demo-license <state>` and `--demo-license=<state>` work; state names are
  case-insensitive.
- The flag is only honoured when the run is isolated (`--demo-settings` above makes it isolated)
  and the binary is a Store build; otherwise it is ignored with one line on stderr.
- Each run uses a fresh throwaway data dir: it never queries the Microsoft Store, never writes
  `license.json`, and clicking Buy only writes "demo: purchase skipped" to the error log — the
  purchase dialog never opens.
- Each command opens the Settings window straight away; crop the window for review.

## 12. Build and upload steps (Windows)
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
