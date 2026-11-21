# L2-languages-world

Added the second batch of languages (machine translations, marked for native review): sr (Cyrillic), bg, ru, uk, el, zh-Hans, zh-Hant, ko, hi, th, vi, id — each translating every key of `content/strings/en.json` including the 25 `settings.v2.*` keys, with CLDR-correct plural forms where the language needs them. Extended `language_display_name` with the 12 endonyms, made the `other`-only plural rule explicit for th/vi/id, and added CLDR-chart unit tests for the new rules.
