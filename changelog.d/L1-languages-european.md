# L1-languages-european

- Added machine translations (marked "needs native review") for 24 languages: es, fr, it, pt-BR, pt-PT, nl, pl, cs, sk, sl, hr, bs, sr-Latn, hu, ro, sv, da, nb, fi, et, lv, lt, sq, tr — every en.json key, placeholders preserved.
- CLDR cardinal plural rules in `rust/apps/ocy/ocy-app/src/strings.rs` for the new batch: es/it/fr/pt/pt-PT `many` (exact millions), ro (one/few/other), lt (one/few/many/other), lv (zero/one/other), sq (one/other); sr-Latn reuses the Serbo-Croatian rule; unit tests follow the CLDR plural rules chart examples.
- New `language_display_name(tag)` endonym table (including en/de/mk/ja) for the upcoming Settings language picker (S2).
