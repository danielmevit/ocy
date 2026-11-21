# Ocy palette proposal (V1)

Numbers come from [`contrast.py`](contrast.py); the full tables are in
[CONTRAST-AUDIT.md](CONTRAST-AUDIT.md).

## 1. Minimal AA fixes (implemented on `agent/claude-ui`)

| Failing pair | Before | Change | After |
|---|---|---|---|
| Light: white label on accent button **hover** (`#469377`) | 3.68 | `button-accent-hover` light → `system-accent-dark-1` `#24654F` (hover darkens instead of lightening) | 6.89 |
| Light: (consequence) pressed must stay distinct from hover | 6.89 | `button-accent-pressed` light → `system-accent-dark-2` `#1A4D3C` | 9.67 |
| Dark: on-accent label on accent button **pressed** (`#3B8769`) | 4.12 | `button-accent-pressed` dark → `system-accent-dark-1` `#4DA380` (one step instead of two) | 5.82 |
| Dark: (consequence) hover must stay distinct from pressed | 5.82 | `button-accent-hover` dark → `system-accent-light-1` `#78CBA9` (hover lifts slightly) | 9.26 |
| Both: NumberField border at rest (`border`, 1.84 / 1.80) | 1.84 / 1.80 | new token `input-border` = `text-muted` @ 70 % light / 60 % dark, used only by NumberField | 3.22 / 3.35 |

Why each choice:
- Hover darkening in light is the Fluent default for filled buttons and keeps the same green family.
- Dark pressed only needs one ramp step; dark-2 was simply too dark for the dark on-accent text.
- `input-border` is a separate token so cards keep their soft `border`; only the input that needs to
  be identified (WCAG 1.4.11) gets a firmer edge. Hover still steps up to full `text-muted`.

No change to the core palette (bg, surface, text, text-muted, accent, on-accent, error, attention),
so the C# drift test (`rust/crates/damt-theme/tests/drift.rs`) is unaffected: it compares only the
`*Color` keys and three literal brushes, none of which changed. The C# `App.axaml` accent-button
brushes and NumberBox border would differ from Rust; mirror them there only if the C# app is still
shipped.

Visible in the static gallery: only the Settings number fields (see
`builds/_design/V1/compare-{light,dark}.png`). Hover/pressed states are not rendered by the gallery,
so they are shown as swatches in `builds/_design/V1/swatches.png` (top four rows).

## 2. Optional refinement (swatches only, not implemented)

The palette already reads calm; these are taste options, both keep every pair at AA.

- **A — Warm paper.** Shift the neutrals a few degrees toward warm (light bg `#F7F5F0`, surface
  `#FFFDF9`, text `#1E1D1A`, muted `#5E5B55`; dark bg `#191816`, surface `#23221F`, text `#EEEBE5`,
  muted `#ABA69D`). Accent unchanged. Makes the hand-drawn frames feel more "cozy" next to the UI,
  like paper rather than screen grey. Measured: muted 6.66 / 6.57 on surface, accent link 4.89 light
  (tightest), on-accent unchanged.
- **B — Deeper moss.** Keep the neutrals, deepen the light accent to `#2A7258` and lift the dark
  accent to `#6CC7A1`. One-line reason: gives the link and focus ring real margin (light link 4.96 →
  5.76 on surface, 5.36 on bg) so thin Segoe strokes never look faint, while staying the same green.
  The `system-accent-*` ramp would need regenerating from the new base.

Swatches: `builds/_design/V1/swatches.png` (bottom rows).
