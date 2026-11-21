# Ocy contrast audit (V1)

Scope: every colour pair the Rust UI draws, found by grepping `Theme.*` and literal hex in
`rust/crates/damt-ui/ui/kit.slint`, `rust/apps/ocy/ocy-ui/ui/ocy.slint` and the overlays in
`rust/apps/ocy/ocy-app/src/tokens.rs`. Values come from `rust/apps/ocy/theme/tokens.toml`.
Alpha tokens are composited over the surface they sit on (or over a desktop for overlays).

Ratios are WCAG 2.x (relative luminance, `(L1+0.05)/(L2+0.05)`), computed by
[`contrast.py`](contrast.py) — re-run with `python3 docs/design/contrast.py [tokens.toml]`.

Targets: normal text 4.5:1; large text and UI parts (input/toggle borders, focus ring, progress
fill, icons such as the close ×) 3:1.

## Classification notes

- **Exempt / decorative:** the SettingRow divider and Card border (a card is not a control; it
  is identified by its surface and shadow), the progress *track* (WCAG 1.4.11 asks the
  *fill* — the part that carries the value — to reach 3:1; it does: fill vs track and vs card),
  and disabled controls (opacity 0.4; WCAG 1.4.3 exempts inactive components).
- **Hover / pressed of LinkButton:** hover keeps `accent`, pressed uses `system-accent-dark-1`.
  CloseButton/ghost button go from `text-muted` to `text` on hover (both listed).
- **Overlays (advisory):** the edge frame and attention frame are 3 px ambient cues drawn over
  arbitrary desktop content; the break/warning card carries the actual information, so 1.4.11
  does not strictly apply. Evidence that a hard 3:1 target is the wrong requirement here: the
  desktop behind can be any colour, and even for the two reference desktops no 55 %-alpha tint
  of a mid green passes both (e.g. light-theme frame: 2.12 on #F3F3F3 and 1.87 on #202020).
  They are reported, not "fixed". The dim overlay has no contrast target; its effective colour
  is reported for reference (the card on top is opaque).
- Literal shadows (`#0000002A`, `#00000040`) are decorative.

## Before (pre-V1 tokens): 2 failures in light, 2 in dark


#### light

| fg | bg | fg hex | bg hex | ratio | target | result | where |
|---|---|---|---|---|---|---|---|
| text | surface | #1B1E1F | #FFFFFF | 16.77 | 4.5 | pass | card titles, labels, outline button text |
| text | bg | #1B1E1F | #F6F7F5 | 15.61 | 4.5 | pass | Settings window text outside cards |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 4.5 | pass | help/unit text, ghost button, muted body |
| text-muted | bg | #5A6064 | #F6F7F5 | 5.94 | 4.5 | pass | muted text directly on window bg |
| accent | surface | #2F7D62 | #FFFFFF | 4.96 | 4.5 | pass | LinkButton text (normal size) |
| accent | bg | #2F7D62 | #F6F7F5 | 4.62 | 4.5 | pass | LinkButton text on bg |
| system-accent-dark-1 | surface | #24654F | #FFFFFF | 6.89 | 4.5 | pass | LinkButton pressed |
| on-accent | accent | #FFFFFF | #2F7D62 | 4.96 | 4.5 | pass | accent button label (rest) |
| on-accent | button-accent-hover | #FFFFFF | #469377 | 3.68 | 4.5 | FAIL | accent button label (hover) |
| on-accent | button-accent-pressed | #FFFFFF | #24654F | 6.89 | 4.5 | pass | accent button label (pressed) |
| error | bg | #B3261E | #F6F7F5 | 6.08 | 4.5 | pass | save error text (Settings) |
| error | surface | #B3261E | #FFFFFF | 6.54 | 4.5 | pass | field error text in cards |
| attention | surface | #B04E2C | #FFFFFF | 5.29 | 4.5 | pass | WarningCard title + help text |
| text | button-outline-hover@surface | #1B1E1F | #EFEFEF | 14.59 | 4.5 | pass | outline button text on hover |
| text | button-outline-pressed@surface | #1B1E1F | #E4E4E4 | 13.19 | 4.5 | pass | outline button text pressed |
| accent | surface | #2F7D62 | #FFFFFF | 4.96 | 3.0 | pass | focus ring, toggle track/knob, progress fill, accent button shape |
| accent | bg | #2F7D62 | #F6F7F5 | 4.62 | 3.0 | pass | focus ring on bg |
| system-accent-light-1 | surface | #469377 | #FFFFFF | 3.68 | 3.0 | pass | toggle hover |
| system-accent-dark-1 | surface | #24654F | #FFFFFF | 6.89 | 3.0 | pass | toggle pressed |
| surface | accent | #FFFFFF | #2F7D62 | 4.96 | 3.0 | pass | toggle knob (checked) on track |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 3.0 | pass | outline button border, CloseButton x, stepper arrows |
| input-border@surface | surface | #BDBFC1 | #FFFFFF | 1.84 | 3.0 | FAIL | NumberField border at rest |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 3.0 | pass | NumberField border on hover |
| border@surface | bg | #BDBFC1 | #F6F7F5 | 1.72 | 3.0 | exempt | Card border vs window bg (decorative, exempt: card is not a control) |
| accent | progress-track@surface | #2F7D62 | #E8E8E9 | 4.05 | 3.0 | pass | progress fill vs track |
| progress-track@surface | surface | #E8E8E9 | #FFFFFF | 1.22 | 3.0 | exempt | progress track vs card (exempt: the fill carries the info) |
| divider@surface | surface | #EDEDED | #FFFFFF | 1.17 | - | pass | SettingRow divider (decorative, exempt) |
| on-accent | accent | #FFFFFF | #ACCBC0 | 1.74 | 4.5 | exempt | disabled accent button (opacity .4, exempt) |
| text | surface | #A4A5A5 | #FFFFFF | 2.47 | 4.5 | exempt | disabled outline button (opacity .4, exempt) |
| edge-frame over desktop | #F3F3F3 | #87B2A3 | #F3F3F3 | 2.12 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #F3F3F3 | #CE9886 | #F3F3F3 | 2.24 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #F3F3F3 | #353535 | #F3F3F3 | 11.05 | - | - | overlay effective colour |
| edge-frame over desktop | #202020 | #285344 | #202020 | 1.87 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #202020 | #6F3927 | #202020 | 1.78 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #202020 | #070707 | #202020 | 1.24 | - | - | overlay effective colour |

Failing (non-exempt): 2

#### dark

| fg | bg | fg hex | bg hex | ratio | target | result | where |
|---|---|---|---|---|---|---|---|
| text | surface | #ECEEEC | #212426 | 13.39 | 4.5 | pass | card titles, labels, outline button text |
| text | bg | #ECEEEC | #17191A | 15.13 | 4.5 | pass | Settings window text outside cards |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 4.5 | pass | help/unit text, ghost button, muted body |
| text-muted | bg | #A4ABAF | #17191A | 7.58 | 4.5 | pass | muted text directly on window bg |
| accent | surface | #5FBF97 | #212426 | 6.98 | 4.5 | pass | LinkButton text (normal size) |
| accent | bg | #5FBF97 | #17191A | 7.88 | 4.5 | pass | LinkButton text on bg |
| system-accent-dark-1 | surface | #4DA380 | #212426 | 5.10 | 4.5 | pass | LinkButton pressed |
| on-accent | accent | #0F1A15 | #5FBF97 | 7.96 | 4.5 | pass | accent button label (rest) |
| on-accent | button-accent-hover | #0F1A15 | #4DA380 | 5.82 | 4.5 | pass | accent button label (hover) |
| on-accent | button-accent-pressed | #0F1A15 | #3B8769 | 4.12 | 4.5 | FAIL | accent button label (pressed) |
| error | bg | #FF8A80 | #17191A | 7.73 | 4.5 | pass | save error text (Settings) |
| error | surface | #FF8A80 | #212426 | 6.84 | 4.5 | pass | field error text in cards |
| attention | surface | #E08A64 | #212426 | 5.94 | 4.5 | pass | WarningCard title + help text |
| text | button-outline-hover@surface | #ECEEEC | #2F3234 | 11.07 | 4.5 | pass | outline button text on hover |
| text | button-outline-pressed@surface | #ECEEEC | #393C3E | 9.53 | 4.5 | pass | outline button text pressed |
| accent | surface | #5FBF97 | #212426 | 6.98 | 3.0 | pass | focus ring, toggle track/knob, progress fill, accent button shape |
| accent | bg | #5FBF97 | #17191A | 7.88 | 3.0 | pass | focus ring on bg |
| system-accent-light-1 | surface | #78CBA9 | #212426 | 8.11 | 3.0 | pass | toggle hover |
| system-accent-dark-1 | surface | #4DA380 | #212426 | 5.10 | 3.0 | pass | toggle pressed |
| surface | accent | #212426 | #5FBF97 | 6.98 | 3.0 | pass | toggle knob (checked) on track |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 3.0 | pass | outline button border, CloseButton x, stepper arrows |
| input-border@surface | surface | #484C4F | #212426 | 1.80 | 3.0 | FAIL | NumberField border at rest |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 3.0 | pass | NumberField border on hover |
| border@surface | bg | #484C4F | #17191A | 2.03 | 3.0 | exempt | Card border vs window bg (decorative, exempt: card is not a control) |
| accent | progress-track@surface | #5FBF97 | #35383A | 5.28 | 3.0 | pass | progress fill vs track |
| progress-track@surface | surface | #35383A | #212426 | 1.32 | 3.0 | exempt | progress track vs card (exempt: the fill carries the info) |
| divider@surface | surface | #313436 | #212426 | 1.25 | - | pass | SettingRow divider (decorative, exempt) |
| on-accent | accent | #1A201F | #3A6253 | 2.40 | 4.5 | exempt | disabled accent button (opacity .4, exempt) |
| text | surface | #727575 | #212426 | 3.36 | 4.5 | exempt | disabled outline button (opacity .4, exempt) |
| edge-frame over desktop | #F3F3F3 | #A2D6C0 | #F3F3F3 | 1.47 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #F3F3F3 | #E9B9A4 | #F3F3F3 | 1.58 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #F3F3F3 | #2C2C2C | #F3F3F3 | 12.59 | - | - | overlay effective colour |
| edge-frame over desktop | #202020 | #437761 | #202020 | 3.14 | 3.0 (advisory) | pass | overlay effective colour |
| attention-frame over desktop | #202020 | #8A5A45 | #202020 | 2.82 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #202020 | #060606 | #202020 | 1.24 | - | - | overlay effective colour |

Failing (non-exempt): 2

## After (V1 minimal fix): 0 failures in light, 0 in dark


#### light

| fg | bg | fg hex | bg hex | ratio | target | result | where |
|---|---|---|---|---|---|---|---|
| text | surface | #1B1E1F | #FFFFFF | 16.77 | 4.5 | pass | card titles, labels, outline button text |
| text | bg | #1B1E1F | #F6F7F5 | 15.61 | 4.5 | pass | Settings window text outside cards |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 4.5 | pass | help/unit text, ghost button, muted body |
| text-muted | bg | #5A6064 | #F6F7F5 | 5.94 | 4.5 | pass | muted text directly on window bg |
| accent | surface | #2F7D62 | #FFFFFF | 4.96 | 4.5 | pass | LinkButton text (normal size) |
| accent | bg | #2F7D62 | #F6F7F5 | 4.62 | 4.5 | pass | LinkButton text on bg |
| system-accent-dark-1 | surface | #24654F | #FFFFFF | 6.89 | 4.5 | pass | LinkButton pressed |
| on-accent | accent | #FFFFFF | #2F7D62 | 4.96 | 4.5 | pass | accent button label (rest) |
| on-accent | button-accent-hover | #FFFFFF | #24654F | 6.89 | 4.5 | pass | accent button label (hover) |
| on-accent | button-accent-pressed | #FFFFFF | #1A4D3C | 9.67 | 4.5 | pass | accent button label (pressed) |
| error | bg | #B3261E | #F6F7F5 | 6.08 | 4.5 | pass | save error text (Settings) |
| error | surface | #B3261E | #FFFFFF | 6.54 | 4.5 | pass | field error text in cards |
| attention | surface | #B04E2C | #FFFFFF | 5.29 | 4.5 | pass | WarningCard title + help text |
| text | button-outline-hover@surface | #1B1E1F | #EFEFEF | 14.59 | 4.5 | pass | outline button text on hover |
| text | button-outline-pressed@surface | #1B1E1F | #E4E4E4 | 13.19 | 4.5 | pass | outline button text pressed |
| accent | surface | #2F7D62 | #FFFFFF | 4.96 | 3.0 | pass | focus ring, toggle track/knob, progress fill, accent button shape |
| accent | bg | #2F7D62 | #F6F7F5 | 4.62 | 3.0 | pass | focus ring on bg |
| system-accent-light-1 | surface | #469377 | #FFFFFF | 3.68 | 3.0 | pass | toggle hover |
| system-accent-dark-1 | surface | #24654F | #FFFFFF | 6.89 | 3.0 | pass | toggle pressed |
| surface | accent | #FFFFFF | #2F7D62 | 4.96 | 3.0 | pass | toggle knob (checked) on track |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 3.0 | pass | outline button border, CloseButton x, stepper arrows |
| input-border@surface | surface | #8C9092 | #FFFFFF | 3.22 | 3.0 | pass | NumberField border at rest |
| text-muted | surface | #5A6064 | #FFFFFF | 6.38 | 3.0 | pass | NumberField border on hover |
| border@surface | bg | #BDBFC1 | #F6F7F5 | 1.72 | 3.0 | exempt | Card border vs window bg (decorative, exempt: card is not a control) |
| accent | progress-track@surface | #2F7D62 | #E8E8E9 | 4.05 | 3.0 | pass | progress fill vs track |
| progress-track@surface | surface | #E8E8E9 | #FFFFFF | 1.22 | 3.0 | exempt | progress track vs card (exempt: the fill carries the info) |
| divider@surface | surface | #EDEDED | #FFFFFF | 1.17 | - | pass | SettingRow divider (decorative, exempt) |
| on-accent | accent | #FFFFFF | #ACCBC0 | 1.74 | 4.5 | exempt | disabled accent button (opacity .4, exempt) |
| text | surface | #A4A5A5 | #FFFFFF | 2.47 | 4.5 | exempt | disabled outline button (opacity .4, exempt) |
| edge-frame over desktop | #F3F3F3 | #87B2A3 | #F3F3F3 | 2.12 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #F3F3F3 | #CE9886 | #F3F3F3 | 2.24 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #F3F3F3 | #353535 | #F3F3F3 | 11.05 | - | - | overlay effective colour |
| edge-frame over desktop | #202020 | #285344 | #202020 | 1.87 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #202020 | #6F3927 | #202020 | 1.78 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #202020 | #070707 | #202020 | 1.24 | - | - | overlay effective colour |

Failing (non-exempt): 0

#### dark

| fg | bg | fg hex | bg hex | ratio | target | result | where |
|---|---|---|---|---|---|---|---|
| text | surface | #ECEEEC | #212426 | 13.39 | 4.5 | pass | card titles, labels, outline button text |
| text | bg | #ECEEEC | #17191A | 15.13 | 4.5 | pass | Settings window text outside cards |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 4.5 | pass | help/unit text, ghost button, muted body |
| text-muted | bg | #A4ABAF | #17191A | 7.58 | 4.5 | pass | muted text directly on window bg |
| accent | surface | #5FBF97 | #212426 | 6.98 | 4.5 | pass | LinkButton text (normal size) |
| accent | bg | #5FBF97 | #17191A | 7.88 | 4.5 | pass | LinkButton text on bg |
| system-accent-dark-1 | surface | #4DA380 | #212426 | 5.10 | 4.5 | pass | LinkButton pressed |
| on-accent | accent | #0F1A15 | #5FBF97 | 7.96 | 4.5 | pass | accent button label (rest) |
| on-accent | button-accent-hover | #0F1A15 | #78CBA9 | 9.26 | 4.5 | pass | accent button label (hover) |
| on-accent | button-accent-pressed | #0F1A15 | #4DA380 | 5.82 | 4.5 | pass | accent button label (pressed) |
| error | bg | #FF8A80 | #17191A | 7.73 | 4.5 | pass | save error text (Settings) |
| error | surface | #FF8A80 | #212426 | 6.84 | 4.5 | pass | field error text in cards |
| attention | surface | #E08A64 | #212426 | 5.94 | 4.5 | pass | WarningCard title + help text |
| text | button-outline-hover@surface | #ECEEEC | #2F3234 | 11.07 | 4.5 | pass | outline button text on hover |
| text | button-outline-pressed@surface | #ECEEEC | #393C3E | 9.53 | 4.5 | pass | outline button text pressed |
| accent | surface | #5FBF97 | #212426 | 6.98 | 3.0 | pass | focus ring, toggle track/knob, progress fill, accent button shape |
| accent | bg | #5FBF97 | #17191A | 7.88 | 3.0 | pass | focus ring on bg |
| system-accent-light-1 | surface | #78CBA9 | #212426 | 8.11 | 3.0 | pass | toggle hover |
| system-accent-dark-1 | surface | #4DA380 | #212426 | 5.10 | 3.0 | pass | toggle pressed |
| surface | accent | #212426 | #5FBF97 | 6.98 | 3.0 | pass | toggle knob (checked) on track |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 3.0 | pass | outline button border, CloseButton x, stepper arrows |
| input-border@surface | surface | #707578 | #212426 | 3.35 | 3.0 | pass | NumberField border at rest |
| text-muted | surface | #A4ABAF | #212426 | 6.71 | 3.0 | pass | NumberField border on hover |
| border@surface | bg | #484C4F | #17191A | 2.03 | 3.0 | exempt | Card border vs window bg (decorative, exempt: card is not a control) |
| accent | progress-track@surface | #5FBF97 | #35383A | 5.28 | 3.0 | pass | progress fill vs track |
| progress-track@surface | surface | #35383A | #212426 | 1.32 | 3.0 | exempt | progress track vs card (exempt: the fill carries the info) |
| divider@surface | surface | #313436 | #212426 | 1.25 | - | pass | SettingRow divider (decorative, exempt) |
| on-accent | accent | #1A201F | #3A6253 | 2.40 | 4.5 | exempt | disabled accent button (opacity .4, exempt) |
| text | surface | #727575 | #212426 | 3.36 | 4.5 | exempt | disabled outline button (opacity .4, exempt) |
| edge-frame over desktop | #F3F3F3 | #A2D6C0 | #F3F3F3 | 1.47 | 3.0 (advisory) | advisory | overlay effective colour |
| attention-frame over desktop | #F3F3F3 | #E9B9A4 | #F3F3F3 | 1.58 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #F3F3F3 | #2C2C2C | #F3F3F3 | 12.59 | - | - | overlay effective colour |
| edge-frame over desktop | #202020 | #437761 | #202020 | 3.14 | 3.0 (advisory) | pass | overlay effective colour |
| attention-frame over desktop | #202020 | #8A5A45 | #202020 | 2.82 | 3.0 (advisory) | advisory | overlay effective colour |
| overlay over desktop | #202020 | #060606 | #202020 | 1.24 | - | - | overlay effective colour |

Failing (non-exempt): 0

## Font-sensitive spots

The gallery renders on Linux with a fallback for Segoe UI, so glyph weights differ. Ratios do not
depend on the font, but the tightest text pairs are worth a look on Windows: LinkButton accent text
on `bg` in light (4.62:1, 13 px) and the 12 px muted help text (5.94:1 on bg). Thin Segoe strokes
render lighter than the formula assumes, so these are the first places to tighten if anything
looks faint.
