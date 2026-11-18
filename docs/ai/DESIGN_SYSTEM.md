# Design system (Phase 1 baseline)

The owner is a designer. These are the rules they judge quality by. Check every window against them before
asking for review.

## Laws
- **Calm, not alarming.** Breaks are an invitation, not an alert. There are no red banners and no "!!!", and
  copy contains at most one exclamation mark.
- **Never steal focus** for short breaks and warnings (`ShowActivated=false`, topmost). Only the long-break
  overlay takes input.
- **No italic text anywhere.** Set roman explicitly. On the owner's PC, requesting bold "Inter" can resolve to an
  italic instance, so use the OS UI font (Segoe UI Variable / Segoe UI on Windows) and never synthesized bold.
- **No decorative icons** on labels or headings. Functional icons (tray, close) are fine. Close is a plain X with
  no plate behind it.
- **No coloured left-edge bars** on cards or rows. Show status as a small coloured square.
- **AA contrast or better** in both themes, including selected states of nested controls. Toggle knobs match the
  accent family.
- **Compact rhythm:** title → subtitle ~2 px, subtitle → content small. Denser rather than airier.
- **Update in place:** no full-view rebuild flashes when state changes.
- **Sounds are soft:** low register, quiet, slow attack (swelling), short. No bright rising blips. Off by default.
- Follow the system light/dark theme. Asset names state the mode they are **shown in** (`tray_dark.png` = used in
  dark mode).

## Tokens (proposal — the owner decides final colours)

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `Bg` | #F6F7F5 | #17191A | windows |
| `Surface` | #FFFFFF | #212426 | cards |
| `Text` | #1B1E1F | #ECEEEC | primary text |
| `TextMuted` | #5A6064 | #A4ABAF | secondary text (AA on Surface) |
| `Accent` | #2F7D62 | #5FBF97 | buttons, progress, focus ring |
| `Overlay` | #000000 @ 78% | #000000 @ 82% | long-break dim |
| `Divider` | `Text` @ 8% | `Text` @ 8% | row dividers (never the Fluent `Separator`) |
| `OnAccent` | #FFFFFF | #0F1A15 | text on accent-filled buttons (AA) |
| `Error` | #B3261E | #FF8A80 | inline error text (AA) |

Spacing scale: 2 / 4 / 6 / 8 / 10 / 16 px. Corner radius: 8 px for cards, 6 px for buttons.

## Windows
- **Short break:** 360 wide, sized to its content, bottom-right of the **active monitor's** work area with a 16 px
  margin. It shows the exercise
  title, one line of body text, a thin progress bar and a plain X (skip).
- **Warning:** same size and spot (active monitor), with "Long break in 0:42", Start now (accent), Postpone N min, and Skip (text
  button). Postpone and Skip are hidden in strict mode.
- **Edge indicator** (other monitors during a short break or warning): one transparent, click-through, full-screen
  frame window per monitor with a 3 DIP border in `Accent` at 55% **brush alpha**. Calm, no animation, no text.
  (Thin strip windows are impossible on Windows: the OS enforces a minimum size of 32–39 px.)
- **Long break:** a full-screen dim on every monitor. The **active** screen has a centred card, 480 wide, padding 24,
  radius 8, soft shadow:
  - a small "Back to your screen in" label
  - the countdown as numerals only (56 Light, tabular, `Text` colour)
  - title 20 SemiBold and body 14 muted
  - a divider, then two suggestion columns
  - a footer with a thin accent progress bar and "End break" as `Button.outline` (hidden in strict mode)
  Accent appears only on the progress bar and the focus ring.
- **Settings:** 560×640 (min 480). Sections General / Short breaks / Long breaks / Behaviour / About are
  separate cards.
  - Rows use the `SettingRow` control: label 13 + help 12 muted, then a right column 132 wide for the control and
    its unit; `Divider` lines; about 46 px per row.
  - The enable toggles sit in the Short/Long section headers. Rows of a disabled section are disabled, not hidden.
  - "Try a short/long break" are text links inside their sections.
  - Changes **apply live** (debounced 400 ms). There is no Save button.
- **Buttons:** use the app classes `Button.accent` / `Button.outline` / `Button.text`, which have hover, pressed and
  disabled states. Never style a button locally.
- **Every break window** sets `TransparencyLevelHint=Transparent`; without it, Windows draws the transparent
  areas black.