# How Ocy is built with AI (Fable, Opus and Unio workers)

The owner does not always have Claude Fable 5.2. The workflow is designed so that any lead model can pick up
where the last one stopped, and so that Fable time goes where it adds the most.

## 1. Roles

| Role | Who | Does | Never does |
| --- | --- | --- | --- |
| **Lead** (main AI) | **Claude Fable 5.2** when available, otherwise **Claude Opus 5.5** (Claude Code) | research, architecture, frozen contracts, task files, dispatch, waiting, reviewing diffs, running checks, merging to `dev`, docs/CHANGELOG | write product code (it delegates it), approve its own unreviewed work, push `main` without the owner |
| **Implementer** | Unio workers: **Antigravity `agy` / Gemini 3.8 Flash High** (main), **Mistral Vibe / GLM 5.3 high** | one frozen task in its own worktree, with build + tests + commit | edit outside its task scope, start other agents, merge |
| **Routine helper** | OpenCode Zen **free** models (verified zero-price, currently GLM-5.3-Flash Free / DeepSeek V4.1 Flash Free) | mechanical code against an exact spec, docs, formatting, predefined checks | own core features or decide acceptance |
| **Text proposals** | **GitHub Copilot Free** (Auto model, lab Unknown) via Unio's proposal-only adapter | copy and wording drafts, inventories | edit the repo (the adapter cannot) |
| **Reviewer** (owner rule for Ocy, 2026-11-17) | **the lead** for small or mechanical changes; a **separate Claude Opus session below xhigh** (normally medium) for UI and larger changes | read the exact diff (and, for UI, rendered screenshots against DESIGN_SYSTEM); return a verdict plus findings | fix code during review |
| **UI rewrite** (only when a review says so) | an Opus worker at medium or lower effort | rewrite the flagged UI pieces against DESIGN_SYSTEM | change behaviour beyond the flagged scope |
| **Owner** | Daniel | product calls, design taste, releases, `main`, Store, money/auth | |

The owner decided on 2026-11-17 that Ocy does **not** use reviewers from other AI labs (GLM, Gemini, Mistral). The
lead always does the final assessment. This project rule replaces the generic OWNER-WORKFLOW reviewer routing for
Ocy.

## 2. When to spend Fable

Fable is the scarcest resource. Use it for the work where depth pays off most:

1. **Research and architecture passes.** Examples: a new platform (macOS/Linux/mobile), the Wayland overlay
   spike, packaging and updates. Output: an ADR in `docs/research/` and contract changes in `ARCHITECTURE.md`.
2. **Hard debugging** that a worker failed twice (no blind third retry).
3. **Milestone final assessment** (end of each phase): the whole diff, behaviour on the real Windows machine and
   the roadmap.
4. **Product/UX critique** of screenshots against `DESIGN_SYSTEM.md`.

Run Fable in **bounded, module-sized passes**. Write findings to a file early and checkpoint every few
minutes, because Fable access can hit account limits mid-session. Saved files survive the limit; chat does not.

When Fable is unavailable, Opus 5.5 leads with the same files. It uses **xhigh** effort for planning, difficult
reasoning and final review, and **low** for routine coordination. Unio's managed AUTO lead effort currently
drives only a Codex app-server lead, not Claude Code. In Claude Code the owner sets effort with `/effort`.

## 3. The loop (per task)

```
lead: write coord/tasks/<TASK>.md  (scope paths, acceptance, "## Validate" commands)
lead: unio run -b <worker> <TASK>      (background; the lead waits WITHOUT polling)
worker: implement -> dotnet build/test -> commit in wt/<worker>
lead: unio verify <worker> <TASK>      (scope + Validate commands; machine gate)
reviewer: lead (small changes) or a separate Opus session below xhigh (UI or larger changes)
lead: read diff + findings, reproduce material issues, accept or send a correction task
lead: merge worker branch -> dev, update CHANGELOG + START_HERE "Current state", unio experience record
```

Rules that keep it cheap and safe:
- **One workflow per shared subscription** (Unio tier `low`), counting the lead. Mode `fast`: focused checks,
  a real smoke check and a brief lead review. The full gate runs at release.
- Independent tasks run **concurrently** only when their file scopes don't overlap and different subscriptions
  are free.
- Start workers at a supported effort suited to the task. **Raise effort only for reasoning difficulty**, never for
  auth, network or quota failures.
- Preserve failures and partial work (`unio save`, receipts). Never fake an approval.
- A frozen contract (`src/Ocy.Abstractions`, ARCHITECTURE "FROZEN") changes only through the lead.

## 4. Handoff — so any model can continue

Every lead session ends by updating:
1. `docs/ai/START_HERE.md` → **Current state** (what is merged, what is running, next task)
2. `CHANGELOG.md` (what shipped)
3. `<workspace>/coord/LOG.md` (dated coordination log: route, model, effort, exits, verdicts)

**Kickoff prompt for a new lead session** (paste into Claude Code with Fable or Opus):

> You are the Unio lead for Ocy at `D:\Vibe Coding\Ocy` (WSL `/mnt/d/Vibe Coding/Ocy`).
> Read `~/.config/unio/OWNER-WORKFLOW.md`, then `repo/AGENTS.md`, `repo/docs/ai/START_HERE.md` and
> `coord/LOG.md`. Continue from "Current state". Research and decide as the lead; implementation goes to Unio
> workers; reviews are done by the lead or a separate Opus session below xhigh (UI/larger changes), then you do
> the final assessment.

## 5. Workspace layout

```
D:\Vibe Coding\Ocy\
  repo\     this git repository (branch dev; main = releases only)
  coord\    Unio coordination: tasks, reports, receipts, LOG.md, agents.conf
  wt\       one git worktree per worker (wt\antigravity-core, ...)
```
