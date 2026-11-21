# R6a-remove-old-settings

- Removed the legacy `SettingsWindow` component from `ocy-ui` (`ocy.slint`), along with its exclusive `damt-ui` kit components (`NumberField`, `SettingRow`, `SectionHeader`).
- Ported old-window gallery, accessibility, first-click, and `font-family` tests to `SettingsWindowV2`, and removed the 17 old-settings-only string keys (`settings.section.{about,behaviour,longBreaks,shortBreaks}`, old `SettingRow` `.help` keys, `unit.minutes`, `unit.seconds`) across all 40 translation files.
