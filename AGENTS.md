# Agent rules — Ocy

## Reading ritual (start of every session)
1. `AGENTS.md`: this file
2. `docs/ai/START_HERE.md`: orientation and current state
3. `docs/ARCHITECTURE.md`: what to build and the frozen contracts
4. Your task file (workers): `coord/tasks/<TASK>.md`. It is the whole scope of your job.
5. Unio: the lead also reads and follows `MASTER.md`. A worker also reads and follows `WORKER.md` (local
   untracked role files).

## docs map
| File | Owns |
| --- | --- |
| docs/ai/START_HERE.md | orientation, current state, how to build |
| docs/ARCHITECTURE.md | projects, contracts, engine rules |
| docs/ai/DESIGN_SYSTEM.md | UI laws and tokens |
| docs/ai/DECISIONS.md | durable "why" decisions |
| docs/ai/GOTCHAS.md | build and run traps |
| docs/ai/AI-WORKFLOW.md | lead / worker / reviewer roles, the Fable plan |
| docs/research/ | product, tech-stack and naming research |

## Rules for every AI
- Stay inside your task's scope paths. Do not edit `src/Ocy.Abstractions/` (FROZEN, lead-owned) unless the
  task says so.
- Build with Linux `dotnet` (`~/.dotnet`). Never call Windows programs (`dotnet.exe`, `cmd.exe`, `powershell.exe`).
- Use `-nodeReuse:false -p:UseSharedCompilation=false` on `dotnet build/test` in agent runs.
- 0 warnings, 0 errors (warnings are errors). Add tests for the behaviour you build. Run them before committing.
- Package versions only in `Directory.Packages.props`.
- No italic UI text, no decorative icons, no focus stealing (see DESIGN_SYSTEM).
- Write original code and content. Never copy EyeLeo or other apps' code, strings or art.
- Versions stay in **0.1.x** (alpha). Never set 0.2+ unless the owner says so.
- Never explain the meaning or origin of the name "Ocy" in app UI, README or public text.
- Commit early and often on your worker branch, with clear messages. Do not merge, push, rebase `dev` or start other agents.
- Your final message: what you built, the files changed, the exact commands you ran with their results, and
  anything unfinished.

## Commit and push (lead)
- Branch `dev` is the work branch. `main` is for releases only, on the owner's word.
- Commit trailer for AI commits: `Co-Authored-By: <model> <noreply@...>` as appropriate.
- Push from WSL uses Windows credentials: `cmd.exe /c "git push origin dev"` (once a remote exists).

## Documentation upkeep
- `CHANGELOG.md` = what shipped. `DECISIONS.md` = durable whys. Keep START_HERE "Current state" true.
