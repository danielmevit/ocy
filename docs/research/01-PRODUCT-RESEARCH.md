# Product research — eye-break reminder

Research date: 2026-11-17. Lead: Claude Code (Opus 5.5). Sources are listed at the end.
Statements marked **(verified)** were checked against local files or the live
source on that date; everything else is a product judgement.

## 1. What EyeLeo is (the reference product)

Installed copy inspected read-only at `C:\Program Files (x86)\EyeLeo`.

| Fact | Value |
| --- | --- |
| Version | 1.3.6, December 2020 **(verified, readme.txt)** |
| Platforms | Windows 7/8/10 only **(verified)** |
| Price | Freeware, donation-supported |
| Source | Opened at github.com/Yamatao/eyeleoapp in 1.3.4; last push 2021-02-21; GitHub cannot detect a standard licence (NOASSERTION) **(verified)**. Treat its code as **not reusable**. |
| Runtime | Native C++ (MSVC runtime DLLs + `activity-monitor.dll`) |
| Mascot | A leopard character ("Personages/leopard"). It is EyeLeo's brand and must not be copied. |

### Feature inventory (from the English language pack and changelog)

| Area | EyeLeo behaviour **(verified from langpack.en.xml / readme)** |
| --- | --- |
| Long break | Every 20–120 min, lasting 2–20 min. The display is dimmed. |
| Pre-break warning | 20 s – 1.5 min before a long break, offering "No, thanks", "Give me 3 minutes" and "Start break (N)". |
| Short break | Every 5–30 min, lasting 8–20 s. An eye exercise in a small window, or optionally full screen with a semi-transparent background. |
| Exercises | Roll eyes, look up/down, look left/right, close eyes, blink, look at distant objects through a window and stretch your body, plus one more added in 1.3.5. Never the same exercise twice in a row. |
| Strict mode | A long break cannot be ignored or postponed, and the tray menu is disabled during it. |
| Window nearby | If off, the "look through the window" exercise is never shown. |
| Inactivity tracking | Keyboard and mouse idle time counts as rest, so a long absence counts as a long break. |
| Fullscreen apps | "Please minimize or close the fullscreen application to start a break". |
| Session handling | No long break straight after unlocking Windows. RDP fix. Inactivity tracking is off while suspended. |
| Tray menu | Time left until the long break, pause for 1 h or 3 h, resume, take a long break now, settings, quit. A left click also opens the menu. |
| Notifications | Status toasts (which can be turned off), plus closing notifications with the right mouse button. |
| Settings | "Try short break" and "Try long break" preview buttons, plus sounds on/off. |
| Tips | Posture tips, plus donate and share nags. |
| Focus | Since 1.1 the short-break popup does **not** steal focus (important while drawing or typing). |

### Why build a successor

- There have been no releases since December 2020 and no upstream commits since February 2021 **(verified)**. It is Windows-only.
- Its licence is unclear, so it can't be forked.
- Opportunities beyond parity:
  - pause during calls and meetings (microphone or camera in use)
  - statistics you can look at
  - dark and light themes and HiDPI by design
  - macOS, Linux and mobile
  - Microsoft Store distribution
  - modern packaging and updates
  - wide localisation, with Balkan languages and Czech/Slovak in the first wave, matching the owner's other apps

## 2. The GitHub port the owner suggested — `OmidArdestani/cross_eyeleo_app`

**(verified via GitHub API, 2026-11-17)**

| Fact | Value |
| --- | --- |
| Stack | Qt 6.6+ Widgets, C++17, CMake; Objective-C++ for macOS |
| Licence | MIT |
| Age / activity | Created 2026-05-13, last push 2026-05-21; 1 star, 0 forks |
| Size | ~96 KB C++, 2 test files (settings, state machine) |
| CI | GitHub Actions on Linux, Windows and macOS; DEB, TGZ, ZIP and DMG packages |
| Idle detection | Polls every 30 s. Windows `GetLastInputInfo`, macOS `CGEventSourceSecondsSinceLastEventType`, Linux X11 XScreenSaver only (no Wayland path) |

**Verdict: use it as a feature checklist, not as a base.**
- Qt Widgets has no credible mobile path. Mobile would need a QML rewrite.
- Qt is not installed on this machine, and the Windows Qt toolchain needs the online installer and an account. Our agent workers build in WSL; this is friction for every task.
- It is C++. The owner's shipped desktop apps are C#/.NET, as are their Store packaging know-how and existing tooling.
- It is a young one-week codebase with a basic state machine and two tests. Its value is the feature list, which matches EyeLeo's and is already captured above.
- Its MIT licence would allow reuse with attribution, but we write original code. Nothing is copied.

## 3. Landscape (alternatives people use today)

| App | Stack / licence | Platforms | Notes |
| --- | --- | --- | --- |
| Stretchly | Electron, BSD-2-Clause | Win, macOS, Linux | The most recommended OSS option. It has idle detection, Do Not Disturb detection, pre-break notices and one-click postpone. 1.21.0 (Apr 2026). |
| Workrave | C++ (GTK) | Win, Linux | The classic RSI tool: micro-pauses, rest breaks, a daily limit and statistics. Its UI looks dated. 1.10.54 (Oct 2025), 1.11 RC. |
| Safe Eyes | Python | Linux | Overlay on every monitor, optional keyboard lock, guided exercises. 3.5.0 (PPA). |
| BreakTimer | Open source (Electron) | Win, macOS, Linux | A simple break scheduler. |
| LookAway | Commercial | macOS | Polished. It warns a minute ahead and lets you delay by 1 or 5 minutes. |
| Pausa | Wails (Go) | macOS | Lives in the menu bar and pauses automatically during meetings and calls. |
| Mobile 20-20-20 apps | various | iOS/Android | Notification-only reminders. Phones cannot overlay other apps. |

**Positioning for Ocy:**
- EyeLeo's friendly guided short and long breaks, with modern craft.
- Native-feeling and light. A tray app that runs all day must be cheap, so no Electron.
- Respectful: no focus stealing, pauses for fullscreen apps and calls, and strict mode only when the user asks for it.
- Private: local-only data, with no account and no telemetry.
- One codebase for Windows, macOS and Linux, plus a shared core that mobile apps reuse.

## 4. Product scope

### Phase 1 — Windows MVP (parity core)
1. Tray app: status tooltip ("Long break in 23 min"), menu (take a long break now, pause 1 h / 3 h / until resumed, resume, settings, quit) and an icon that shows its state.
2. Short breaks: interval, duration and exercise text, shown in a small topmost window that does not take focus. Optional fullscreen mode.
3. Long breaks: a dimmed overlay on **every monitor**, with a countdown, a suggestion and Skip (hidden in strict mode).
4. Pre-break warning with countdown: Start now / Postpone (N min) / Skip. Postpone and Skip are hidden in strict mode.
5. Idle (away) detection. Away time does not count as screen time, and a long enough absence counts as the break.
6. Deferral while a fullscreen app or presentation is active.
7. Session lock and sleep handled as away time.
8. Settings window with "Try short break" and "Try long break".
9. Settings and timer state persisted. A restart does not reset the counters.
10. Start with Windows (opt-in), soft sounds (opt-in) and a single instance.
11. Statistics counted in the core. The UI view can arrive in phase 2.

### Phase 2 — polish and distribution (Windows)
- Statistics view, call/meeting detection, localisation (en + mk + cs/sk + Balkan languages) and themes.
- Packaging: MSIX for the Microsoft Store, plus a direct-download installer with updates.
- NativeAOT or trimming to cut memory.

### Phase 3 — macOS and Linux
Platform services per OS (see 02-TECH-STACK.md §4). Signing and notarization on macOS need the Apple Developer Program, which the owner does not have yet.

### Phase 4 — mobile companion
A reminder and exercise app built on the same `Ocy.Core`: notifications instead of overlays, and the same schedules and exercise catalogue.

## Sources

- Local: `C:\Program Files (x86)\EyeLeo\readme.txt`, `config.xml`, `Langpacks/langpack.en.xml`
- GitHub API: `repos/OmidArdestani/cross_eyeleo_app`, `repos/Yamatao/eyeleoapp`
- [Take a Break: Linux Break Reminder Apps to Prevent RSI in 2026](https://www.starryhope.com/linux/take-a-break-workrave/)
- [AlternativeTo — Stretchly alternatives](https://alternativeto.net/software/stretchly/), [BreakTimer](https://alternativeto.net/software/breaktimer/)
- [Wails showcase — Pausa](https://wails.io/docs/community/showcase/pausa)
- [LookAway — how I made break reminders less annoying](https://lookaway.app/blog/2025/04/07/how-i-made-break-reminders-less-annoying)
