# Decisions (durable "why")

| Date | Decision | Why | Where |
| --- | --- | --- | --- |
| 2026-11-17 | Name **Ocy** (owner) | Short, easy to say in many languages, no eye-care or wellbeing collision. Replaced the first working name, Ocellus. Naming research is kept outside the repo. | — |
| 2026-11-17 | **.NET 10 LTS + Avalonia 12.1**, UI-free shared core | Desktop + mobile in one language. Matches the owner's C# craft. Workers build and test in WSL. Light tray app. | research/02-TECH-STACK.md |
| 2026-11-17 | Do **not** fork `cross_eyeleo_app` (Qt/C++, MIT) or EyeLeo's own source (unclear licence) | No mobile path, toolchain friction, young codebase, licence. Used only as a feature checklist. | research/01-PRODUCT-RESEARCH.md §2 |
| 2026-11-17 | Engine counts **active screen time**, driven by `Tick()` + injected `TimeProvider` | Deterministic tests. Sleep and lock are handled uniformly. Restarts don't reset counters. | ARCHITECTURE §3.2 |
| 2026-11-17 | Platform services behind **frozen interfaces** in `Ocy.Abstractions` | Lets Core and per-OS work proceed in parallel worktrees without conflicts. | ARCHITECTURE §2 |
| 2026-11-17 | Plain `net10.0` everywhere, Windows code behind `OperatingSystem.IsWindows()` | One build in WSL. A `win-x64` self-contained exe is published from Linux. | ARCHITECTURE §1 |
| 2026-11-17 | Original content and art only | EyeLeo's mascot, strings and art are its brand. Exercises are written fresh. | — |
| 2026-11-17 | Sounds **off** by default, strict mode **off** by default | Calm and respectful defaults. Owner taste: soft, non-urgent sounds. | DESIGN_SYSTEM |
| 2026-11-17 | **Versions stay in 0.1.x during the alpha** (owner) | This is the alpha. Bump the patch (0.1.1, 0.1.2…) per preview. Only the owner moves to 0.2. | Directory.Build.props |
| 2026-11-17 | **Never explain the name in the app or public texts** (owner) | The origin is told only when someone asks. | — |
| 2026-11-17 | **Break UI follows the active monitor** (owner) | The short break and warning open on the monitor the user is working on (foreground window, else cursor); other monitors get a subtle, click-through accent edge. | ARCHITECTURE §4 |
