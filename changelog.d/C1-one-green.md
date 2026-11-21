# C1-one-green

- Dark mode now uses the same jade as light mode ("one green"), replacing the mint:
  - Fills (buttons, toggles, selected preset, progress, timeline marks): `#348267`, with a white label. Hover goes to `#2F7D62`, pressed to `#266951`.
  - Small accent text uses a new `accent-text` role: `#A3C7B8` in dark, the jade in light. This covers "Try a short break", the Today count, and the website's links, nav highlight, eyebrows and counters.
- The dark progress track is dimmer (text at 4 %), so the fill reaches 3:1 against it.
- The dark illustrations' mint (`#5FBF97`) is now the lighter jade `#63A189`. The C# illustration resources were regenerated.
- New token test `damt-theme/tests/contrast.rs` asserts the AA pairs in both themes.
- The drift test keeps checking the frozen C# 0.1.x reference, with an explicit list of the dark accent keys that now diverge on purpose.
- The website's light mode is pixel-identical to before.
