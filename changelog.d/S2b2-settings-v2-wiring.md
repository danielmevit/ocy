# S2b2-settings-v2-wiring

- Switched `SlintSettingsWindow` (`ocy-app`) from `SettingsWindow` to `ocy_ui::SettingsWindowV2`, wiring presets, live header (`nextShort`/`nextLong`/paused/on-break), 2-hour timeline projection (on open, every 30 s while open, and immediately on edits), today counters, theme-aware 48 px `blink` mascot, hidden-row gates, all 5 value formatters, and `--demo-settings` + `--demo-language`.
- Fixed endonym sorting in `language_choice` to compare diacritic-folded, case-folded keys with Cyrillic and CJK names after Latin ones (so `Čeština` sorts with `C`), and wired `language-selected` to save and live-switch texts, dropdown model, slider value phrases, and `font-family`.
