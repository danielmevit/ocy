```markdown
<!-- R4b-rust-scheduler-natural-moments -->
### Added
- **Natural-moment timing (D9):** scheduled short breaks, long-break warnings, and long
  breaks now wait up to 60 s for a 2 s lull in input before starting, so Ocy never
  interrupts mid-sentence. Explicit starts (tray "Take a break now", warning "Start now",
  Settings "Try") are never held. New `naturalMoments` setting (default on, omitted from
  `settings.json` while on so files stay byte-identical to the C# app's).
- **Adaptive host scheduler:** the app now wakes only when something can happen (next due
  moment, 1 s while a break/warning window is visible or a hold is active, at most 10 s
  otherwise) instead of every second — target ≤ 6 wake-ups per working minute.
  `--report-memory` additionally prints `wakeups_per_min=<n>`.
```
