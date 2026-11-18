# START HERE — Ocy

**Ocy** is a calm, light, cross-platform break reminder for your eyes. It gives short eye
exercises often and a real long break every hour. It is a modern successor to the ideas in EyeLeo (Windows-only,
unmaintained since 2020). Windows comes first, then macOS and Linux, and mobile later through the shared core.

## Read in this order
1. `AGENTS.md`: rules for every AI
2. this file
3. `docs/ARCHITECTURE.md`: projects, frozen contracts, engine rules
4. as needed: `docs/research/*` (why), `docs/ai/DESIGN_SYSTEM.md` (UI), `docs/ai/GOTCHAS.md` (traps),
   `docs/ai/DECISIONS.md`, `docs/ai/AI-WORKFLOW.md` (how the lead and workers cooperate)

## Build and test (WSL/Linux or Windows)
```sh
dotnet build src/Ocy.Core/Ocy.Core.csproj -c Release
dotnet test  tests/Ocy.Core.Tests/Ocy.Core.Tests.csproj -c Release
# Windows exe from WSL (self-contained, run on Windows):
dotnet publish src/Ocy.Desktop/Ocy.Desktop.csproj -c Release -r win-x64 --self-contained -o publish/win-x64
```
WSL needs the .NET 10 SDK in `~/.dotnet` (see GOTCHAS). The build must pass with 0 warnings and 0 errors
(warnings are errors).

## Current state (2026-11-17, late)
- **0.1.1 alpha on `dev`.** Versions stay 0.1.x (owner rule).
  - Phase 1 Windows MVP plus the owner's 0.1.1 requests: active-monitor breaks, edge frame on the other monitors,
    no name-origin text.
  - Long-break card and settings rewritten by Opus from the Opus UI review (coord/reports/OPUS-UI-REVIEW-2026-11-17.md).
  - 215 tests.
- Preview builds: `D:\Vibe Coding\Ocy\builds\Ocy-0.1.1-win-x64\Ocy.exe`; the older 0.1.0-dev.1 folder is kept.
- **Review rule (owner):** no reviewers from other labs. Use the lead, or a separate Opus session below xhigh for UI
  and larger changes. See docs/ai/AI-WORKFLOW.md.
- **Open owner decisions:** product differentiators (proposal in coord/LOG.md); cross-device research.
- **Backlog (low):**
  - a settings change made less than 400 ms before quitting from the tray may be lost
  - a typed number without Enter may not apply on close
  - the focus ring on "End break" has not been checked on Windows
  - one-frame corner reposition
  - lock time after a pause expires while locked is not credited (stats only)
- Live coordination log: `D:\Vibe Coding\Ocy\coord\LOG.md`.
## Roadmap
See `ROADMAP.md`.
