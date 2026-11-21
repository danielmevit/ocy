# S2a-settings-v2-strings

- Moved the 25 parked Settings V2 keys (`settings.v2.*`: header, today, timeline, presets, customize section, tabs, `language.automatic`, value formats incl. the plural `value.times.*`) from `rust/apps/ocy/ocy-ui/strings/settings-v2.en.json` into `content/strings/en.json`, and translated them into all 27 other languages (de, mk, ja plus the L1 batch), with CLDR plural forms where the language needs them (sk few; lt few/many; lv zero).
- `strings_tests.rs` now computes the expected key count from the embedded `en.json` instead of a hand-maintained number; the completeness and placeholder checks stay strict.
- `ocy-ui/tests/support/mod.rs` reads the V2 keys from the real `content/strings/en.json` / `de.json`; the parked `settings-v2.en.json` and the `settings-v2.de.test.json` fixture were deleted (the real German translation is byte-identical to the fixture, so the 560 × 640 no-scroll check is unchanged).
