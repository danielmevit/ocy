use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use jiff::SignedDuration;
use ocy_app::break_logic::{warning_heading, warning_postpone_label};
use ocy_app::strings::{
    PluralCategory, Strings, available_languages, check_table_coverage, embedded_language_tables,
    font_family_for, for_each_checkable_char, is_metadata_key, is_rtl_language,
    is_valid_translation_key, language_display_name, parse_demo_language, plural_category,
    resolve_language_tag, split_plural_suffix,
};

fn extract_placeholders(template: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('}') {
            let name = &after_start[..end];
            if !name.is_empty() && !name.contains('{') {
                out.insert(name.to_owned());
            }
            rest = &after_start[end + 1..];
        } else {
            break;
        }
    }
    out
}

#[test]
fn get_existing_key_returns_value() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(strings.get("app.name"), "Ocy");
    assert_eq!(strings.get("tray.menu.settings"), "Settings");
    assert_eq!(strings.get("warning.strictOn"), "Strict mode is on");
}

#[test]
fn get_missing_key_returns_key_itself() {
    let strings = Strings::from_embedded().unwrap();
    let missing_key = "this.key.does.not.exist";
    assert_eq!(strings.get(missing_key), missing_key);
}

#[test]
fn format_replaces_named_placeholders() {
    let strings = Strings::from_embedded().unwrap();
    let result = strings.format("longbreak.countdown", &[("time", &"23 min")]);
    assert_eq!(result, "Back to your screen in 23 min");

    let postpone_result = strings.format("warning.postpone", &[("minutes", &3)]);
    assert_eq!(postpone_result, "Postpone 3 min");
}

#[test]
fn format_missing_key_replaces_named_placeholders_in_key() {
    let strings = Strings::from_embedded().unwrap();
    let result = strings.format("custom {name} reminder", &[("name", &"water")]);
    assert_eq!(result, "custom water reminder");
}

#[test]
fn time_text_under_one_minute() {
    let cases = [
        (0, "0:00"),
        (-10, "0:00"),
        (5, "0:05"),
        (42, "0:42"),
        (59, "0:59"),
    ];
    for (secs, expected) in cases {
        let duration = SignedDuration::from_secs(secs);
        assert_eq!(Strings::time_text_signed(duration), expected);
    }
}

#[test]
fn time_text_one_minute_or_more_under_one_hour() {
    let cases = [
        (60, "1 min"),
        (119, "1 min"),
        (120, "2 min"),
        (23 * 60, "23 min"),
        (59 * 60 + 59, "59 min"),
    ];
    for (secs, expected) in cases {
        let duration = Duration::from_secs(secs);
        assert_eq!(Strings::time_text(duration), expected);
    }
}

#[test]
fn time_text_one_hour_or_more() {
    let cases = [
        (60 * 60, "1 h 00 min"),
        (65 * 60, "1 h 05 min"),
        (123 * 60, "2 h 03 min"),
        (10 * 3600 + 45 * 60, "10 h 45 min"),
    ];
    for (secs, expected) in cases {
        let duration = Duration::from_secs(secs);
        assert_eq!(Strings::time_text(duration), expected);
    }
}

#[test]
fn format_time_uses_strings_units_and_matches_time_text_except_exact_hours() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(strings.get("time.hours"), "h");
    assert_eq!(strings.get("time.minutes"), "min");
    assert_eq!(strings.get("time.seconds"), "s");
    assert_eq!(strings.get("time.underOneMinute"), "under 1 min");
    assert_eq!(strings.get("time.format.hours"), "{hours} {hoursUnit}");

    // Every case except exact hours stays byte-identical to the C#-parity
    // `time_text` formatter.
    let signed_cases = [
        -10,
        0,
        5,
        42,
        59,
        60,
        119,
        120,
        23 * 60,
        59 * 60 + 59,
        65 * 60,
        123 * 60,
        10 * 3600 + 45 * 60,
    ];
    for secs in signed_cases {
        let dur = SignedDuration::from_secs(secs);
        assert_eq!(
            strings.format_time_signed(dur),
            Strings::time_text_signed(dur),
            "secs={secs}"
        );
    }

    // Exact hours read naturally: "1 h" and "2 h", not "1 h 00 min".
    assert_eq!(strings.format_time(Duration::from_secs(60 * 60)), "1 h");
    assert_eq!(strings.format_time(Duration::from_secs(2 * 3600)), "2 h");
    assert_eq!(
        strings.format_time(Duration::from_secs(65 * 60)),
        "1 h 05 min"
    );
    assert_eq!(strings.format_time(Duration::from_secs(15 * 60)), "15 min");
    assert_eq!(strings.format_time(Duration::from_secs(42)), "0:42");
}

#[test]
fn settings_window_title_formats_with_app_name() {
    let strings = Strings::from_embedded().unwrap();
    let app_name = strings.get("app.name");
    let title = strings.format("settings.windowTitle", &[("app", &app_name)]);
    assert_eq!(title, "Ocy \u{2014} Settings");
}

#[test]
fn key_count_and_core_entries_match_en_json() {
    let strings = Strings::from_embedded().unwrap();
    // The expected count is computed from the embedded en.json (minus `_meta.*`), so adding a
    // key to en.json updates the expectation instead of failing an off-by-hand number.
    let en_raw = embedded_language_tables()
        .iter()
        .find(|(tag, _)| *tag == "en")
        .map(|(_, raw)| *raw)
        .expect("en.json embedded");
    let en_map: HashMap<String, String> = serde_json::from_str(en_raw).expect("en.json valid JSON");
    let expected = en_map.keys().filter(|k| !is_metadata_key(k)).count();
    assert_eq!(strings.key_count(), expected);
    assert_eq!(
        strings.get("tray.menu.pausedUntil"),
        "Paused until {clock} \u{00b7} {time} left"
    );
    assert_eq!(
        strings.get("tray.menu.pausedIndefinitely"),
        "Paused until you resume"
    );
    assert_eq!(strings.get("time.underOneMinute"), "under 1 min");
}

#[test]
fn format_countdown_separate_checks() {
    let strings = Strings::from_embedded().unwrap();
    let cases = [(5, "0:05"), (263, "4:23"), (1800, "30:00")];
    for (secs, expected) in cases {
        let d = Duration::from_secs(secs);
        assert_eq!(Strings::format_countdown(d), expected);
        assert_eq!(strings.format_countdown_localized(d), expected);
    }
}

#[test]
fn plural_rules_match_cldr_published_examples_across_all_families() {
    // 1. English / German-like: one = 1; other = 0, 2..16, 100, 1000
    for lang in [
        "en", "en-US", "de", "de-AT", "nl", "sv", "da", "nb", "fi", "et", "el", "bg", "hu", "tr",
        "it", "es", "pt-PT",
    ] {
        assert_eq!(plural_category(lang, 1), PluralCategory::One, "{lang}: 1");
        for n in [0, 2, 3, 4, 5, 10, 11, 16, 21, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 2. French-like: one = 0, 1; other = 2..17, 100, 1000
    for lang in ["fr", "fr-CA", "pt", "pt-BR"] {
        for n in [0, 1] {
            assert_eq!(plural_category(lang, n), PluralCategory::One, "{lang}: {n}");
        }
        for n in [2, 3, 5, 11, 17, 21, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 3. Macedonian (CLDR: one = i%10=1 and i%100!=11; other = everything else; NO few/many!)
    for n in [1, 21, 31, 41, 51, 61, 71, 81, 101, 1001] {
        assert_eq!(plural_category("mk", n), PluralCategory::One, "mk: {n}");
    }
    for n in [
        0, 2, 3, 4, 5, 11, 12, 14, 17, 20, 22, 30, 100, 111, 211, 1000,
    ] {
        assert_eq!(plural_category("mk", n), PluralCategory::Other, "mk: {n}");
    }

    // 4. East-Slavic (ru, uk, be): one / few / many
    for lang in ["ru", "uk", "be"] {
        for n in [1, 21, 31, 41, 51, 61, 71, 81, 101, 1001] {
            assert_eq!(plural_category(lang, n), PluralCategory::One, "{lang}: {n}");
        }
        for n in [2, 3, 4, 22, 23, 24, 32, 42, 52, 62, 102, 1002] {
            assert_eq!(plural_category(lang, n), PluralCategory::Few, "{lang}: {n}");
        }
        for n in [
            0, 5, 6, 10, 11, 12, 14, 19, 20, 25, 100, 111, 112, 1000, 1_000_000,
        ] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Many,
                "{lang}: {n}"
            );
        }
    }

    // 5. Serbo-Croatian (sr, hr, bs): one / few / other
    for lang in ["sr", "hr", "bs"] {
        for n in [1, 21, 31, 41, 51, 61, 71, 81, 101, 1001] {
            assert_eq!(plural_category(lang, n), PluralCategory::One, "{lang}: {n}");
        }
        for n in [2, 3, 4, 22, 23, 24, 32, 42, 52, 62, 102, 1002] {
            assert_eq!(plural_category(lang, n), PluralCategory::Few, "{lang}: {n}");
        }
        for n in [0, 5, 10, 11, 12, 14, 19, 20, 25, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 6. Polish (pl): one = 1; few = 2..4, 22..24, ...; many = 0, 5..19, 20, 21, 25..31, 100, 1000
    assert_eq!(plural_category("pl", 1), PluralCategory::One);
    for n in [2, 3, 4, 22, 23, 24, 32, 33, 34, 42, 52, 62, 102, 1002] {
        assert_eq!(plural_category("pl", n), PluralCategory::Few, "pl: {n}");
    }
    for n in [
        0, 5, 6, 10, 11, 12, 13, 14, 19, 20, 21, 25, 30, 31, 100, 112, 1000, 1_000_000,
    ] {
        assert_eq!(plural_category("pl", n), PluralCategory::Many, "pl: {n}");
    }

    // 7. Czech / Slovak (cs, sk): one = 1; few = 2..4; other = 0, 5..19, 100, 1000
    for lang in ["cs", "sk"] {
        assert_eq!(plural_category(lang, 1), PluralCategory::One, "{lang}: 1");
        for n in [2, 3, 4] {
            assert_eq!(plural_category(lang, n), PluralCategory::Few, "{lang}: {n}");
        }
        for n in [0, 5, 11, 19, 21, 22, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 8. Slovenian (sl): one = %100==1; two = %100==2; few = %100 in 3..4; other = rest
    for n in [1, 101, 201, 301, 401, 501, 601, 701, 1001] {
        assert_eq!(plural_category("sl", n), PluralCategory::One, "sl: {n}");
    }
    for n in [2, 102, 202, 302, 402, 502, 602, 702, 1002] {
        assert_eq!(plural_category("sl", n), PluralCategory::Two, "sl: {n}");
    }
    for n in [
        3, 4, 103, 104, 203, 204, 303, 304, 403, 404, 503, 504, 603, 604, 703, 704, 1003,
    ] {
        assert_eq!(plural_category("sl", n), PluralCategory::Few, "sl: {n}");
    }
    for n in [0, 5, 11, 19, 21, 22, 100, 105, 1000] {
        assert_eq!(plural_category("sl", n), PluralCategory::Other, "sl: {n}");
    }

    // 9. Arabic (ar): all 6 forms
    assert_eq!(plural_category("ar", 0), PluralCategory::Zero);
    assert_eq!(plural_category("ar", 1), PluralCategory::One);
    assert_eq!(plural_category("ar", 2), PluralCategory::Two);
    for n in [3, 4, 5, 10, 103, 104, 110, 1003] {
        assert_eq!(plural_category("ar", n), PluralCategory::Few, "ar: {n}");
    }
    for n in [11, 12, 26, 99, 111, 199, 1011] {
        assert_eq!(plural_category("ar", n), PluralCategory::Many, "ar: {n}");
    }
    for n in [100, 101, 102, 200, 201, 202, 600, 1000] {
        assert_eq!(plural_category("ar", n), PluralCategory::Other, "ar: {n}");
    }

    // 10. Japanese / Chinese / Korean: other only
    for lang in ["ja", "zh", "zh-Hans", "zh-Hant", "ko"] {
        for n in [0, 1, 2, 3, 5, 11, 21, 100] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 11. Language with no explicit rule falls back to `other`
    for lang in ["xx", "tlh", "sw-KE"] {
        for n in [0, 1, 2, 3, 5, 11, 21, 100] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "unmapped language {lang} must fall back to PluralCategory::Other for n={n}"
            );
        }
    }
}

/// CLDR cardinal plural rules for the L1 European batch, checked against the published
/// examples in the CLDR plural rules chart (`common/supplemental/plurals.xml`,
/// unicode-org/cldr — see https://unicode-org.github.io/cldr-staging/charts/45/supplemental/language_plural_rules.html).
#[test]
fn plural_rules_match_cldr_examples_for_l1_european_languages() {
    // 1. Spanish / Italian (CLDR 42+): one = 1; many = exact multiples of 1,000,000;
    //    other = 0, 2..16, 100, 1000, 1000001, 1500000
    for lang in ["es", "es-MX", "es-419", "it", "it-CH"] {
        assert_eq!(plural_category(lang, 1), PluralCategory::One, "{lang}: 1");
        for n in [0, 2, 3, 5, 11, 16, 21, 100, 1000, 1_000_001, 1_500_000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
        for n in [1_000_000, 2_000_000, 5_000_000, 1_000_000_000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Many,
                "{lang}: {n} (exact millions)"
            );
        }
    }

    // 2. French / Brazilian Portuguese: one = 0, 1; many = exact millions; other = 2..17, 100, 1000
    for lang in ["fr", "fr-CA", "pt", "pt-BR"] {
        for n in [0, 1] {
            assert_eq!(plural_category(lang, n), PluralCategory::One, "{lang}: {n}");
        }
        for n in [2, 3, 5, 11, 17, 21, 100, 1000, 1_000_001] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
        for n in [1_000_000, 3_000_000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Many,
                "{lang}: {n} (exact millions)"
            );
        }
    }

    // 3. European Portuguese (pt-PT): one = 1 only (0 is other); many = exact millions
    assert_eq!(plural_category("pt-PT", 1), PluralCategory::One);
    for n in [0, 2, 3, 5, 11, 21, 100, 1000, 1_000_001] {
        assert_eq!(
            plural_category("pt-PT", n),
            PluralCategory::Other,
            "pt-PT: {n}"
        );
    }
    assert_eq!(plural_category("pt-PT", 1_000_000), PluralCategory::Many);

    // 4. Romanian: one = 1; few = exact hundreds that are not exact thousands
    //    (100, 200, ..., 900, 1100, 1200, 2100); other = 0, 2..19, 1000, 1001, 1000000.
    assert_eq!(plural_category("ro", 1), PluralCategory::One);
    for n in [100, 200, 300, 900, 1100, 1200, 2100, 10_100] {
        assert_eq!(plural_category("ro", n), PluralCategory::Few, "ro: {n}");
    }
    for n in [
        0, 2, 3, 5, 11, 19, 20, 21, 101, 111, 1000, 1001, 2000, 1_000_000,
    ] {
        assert_eq!(plural_category("ro", n), PluralCategory::Other, "ro: {n}");
    }

    // 5. Lithuanian: one = 1, 21, 31, ..., 101, 1001; few = 2..9, 22..29, ..., 102;
    //    many = 0, 10..19, 20, 30, 100, 1000, 111, 112; other = never for integers
    for n in [1, 21, 31, 41, 51, 61, 71, 81, 101, 1001] {
        assert_eq!(plural_category("lt", n), PluralCategory::One, "lt: {n}");
    }
    for n in [2, 3, 4, 7, 9, 22, 23, 24, 32, 42, 102, 1002] {
        assert_eq!(plural_category("lt", n), PluralCategory::Few, "lt: {n}");
    }
    for n in [
        0, 10, 11, 12, 13, 14, 15, 19, 20, 30, 100, 111, 112, 119, 1000, 1_000_000,
    ] {
        assert_eq!(plural_category("lt", n), PluralCategory::Many, "lt: {n}");
    }

    // 6. Latvian: zero = 0, 10..19, 100, 110, 1000, 111, 112; one = 1, 21, 101, 1001;
    //    other = 2..9, 22, 25, 102
    for n in [
        0, 10, 11, 12, 15, 19, 20, 100, 110, 111, 112, 1000, 1_000_000,
    ] {
        assert_eq!(plural_category("lv", n), PluralCategory::Zero, "lv: {n}");
    }
    for n in [1, 21, 31, 101, 201, 1001] {
        assert_eq!(plural_category("lv", n), PluralCategory::One, "lv: {n}");
    }
    for n in [2, 3, 4, 9, 22, 25, 102, 202] {
        assert_eq!(plural_category("lv", n), PluralCategory::Other, "lv: {n}");
    }

    // 7. Albanian: one = 1; other = 0, 2..16, 100, 1000
    assert_eq!(plural_category("sq", 1), PluralCategory::One);
    for n in [0, 2, 3, 5, 11, 16, 21, 100, 1000] {
        assert_eq!(plural_category("sq", n), PluralCategory::Other, "sq: {n}");
    }

    // 8. Hungarian / Turkish / Estonian / Finnish / Swedish / Danish / Norwegian Bokmål:
    //    one = 1; other = 0, 2..16, 100, 1000 (already in the German-like family; spot-check)
    for lang in ["hu", "tr", "et", "fi", "sv", "da", "nb"] {
        assert_eq!(plural_category(lang, 1), PluralCategory::One, "{lang}: 1");
        for n in [0, 2, 5, 11, 21, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 9. Bosnian / Croatian / Serbian Latin reuse the Serbo-Croatian rule (spot-check sr-Latn)
    for n in [1, 21, 31, 101] {
        assert_eq!(
            plural_category("sr-Latn", n),
            PluralCategory::One,
            "sr-Latn: {n}"
        );
    }
    for n in [2, 3, 4, 22, 102] {
        assert_eq!(
            plural_category("sr-Latn", n),
            PluralCategory::Few,
            "sr-Latn: {n}"
        );
    }
    for n in [0, 5, 11, 20, 100, 1000] {
        assert_eq!(
            plural_category("sr-Latn", n),
            PluralCategory::Other,
            "sr-Latn: {n}"
        );
    }
}

/// CLDR cardinal plural rules for the L2 world batch, checked against the published
/// examples in the CLDR plural rules chart (`common/supplemental/plurals.xml`,
/// unicode-org/cldr — see https://unicode-org.github.io/cldr-staging/charts/45/supplemental/language_plural_rules.html).
#[test]
fn plural_rules_match_cldr_examples_for_l2_world_languages() {
    // 1. Serbian Cyrillic (sr) reuses the Serbo-Croatian rule: one / few / other
    for n in [1, 21, 31, 101, 1001] {
        assert_eq!(plural_category("sr", n), PluralCategory::One, "sr: {n}");
    }
    for n in [2, 3, 4, 22, 23, 24, 102, 1002] {
        assert_eq!(plural_category("sr", n), PluralCategory::Few, "sr: {n}");
    }
    for n in [0, 5, 11, 12, 20, 100, 1000] {
        assert_eq!(plural_category("sr", n), PluralCategory::Other, "sr: {n}");
    }

    // 2. Bulgarian / Greek: one = 1; other = 0, 2..16, 100, 1000 (spot-check regional tags)
    for lang in ["bg", "bg-BG", "el", "el-GR"] {
        assert_eq!(plural_category(lang, 1), PluralCategory::One, "{lang}: 1");
        for n in [0, 2, 3, 5, 11, 16, 21, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }

    // 3. Russian / Ukrainian reuse the East-Slavic rule (spot-check regional tags)
    for lang in ["ru", "ru-RU", "uk", "uk-UA"] {
        for n in [1, 21, 31, 101, 1001] {
            assert_eq!(plural_category(lang, n), PluralCategory::One, "{lang}: {n}");
        }
        for n in [2, 3, 4, 22, 23, 24, 102, 1002] {
            assert_eq!(plural_category(lang, n), PluralCategory::Few, "{lang}: {n}");
        }
        for n in [0, 5, 10, 11, 14, 20, 100, 111, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Many,
                "{lang}: {n}"
            );
        }
    }

    // 4. Hindi (CLDR: one = 0 and 1; other = 2..16, 100, 1000)
    for n in [0, 1] {
        assert_eq!(plural_category("hi", n), PluralCategory::One, "hi: {n}");
    }
    for n in [2, 3, 5, 11, 16, 21, 100, 1000] {
        assert_eq!(plural_category("hi", n), PluralCategory::Other, "hi: {n}");
    }

    // 5. Thai / Vietnamese / Indonesian: other only (no plural forms)
    for lang in ["th", "th-TH", "vi", "vi-VN", "id", "id-ID"] {
        for n in [0, 1, 2, 3, 5, 11, 21, 100, 1000] {
            assert_eq!(
                plural_category(lang, n),
                PluralCategory::Other,
                "{lang}: {n}"
            );
        }
    }
}

#[test]
fn language_display_name_returns_endonyms_and_falls_back_to_the_tag() {
    let cases = [
        ("en", "English"),
        ("de", "Deutsch"),
        ("mk", "Македонски"),
        ("ja", "日本語"),
        ("es", "Español"),
        ("fr", "Français"),
        ("it", "Italiano"),
        ("pt-BR", "Português (Brasil)"),
        ("pt-PT", "Português (Portugal)"),
        ("nl", "Nederlands"),
        ("pl", "Polski"),
        ("cs", "Čeština"),
        ("sk", "Slovenčina"),
        ("sl", "Slovenščina"),
        ("hr", "Hrvatski"),
        ("bs", "Bosanski"),
        ("sr-Latn", "Srpski (latinica)"),
        ("hu", "Magyar"),
        ("ro", "Română"),
        ("sv", "Svenska"),
        ("da", "Dansk"),
        ("nb", "Norsk bokmål"),
        ("fi", "Suomi"),
        ("et", "Eesti"),
        ("lv", "Latviešu"),
        ("lt", "Lietuvių"),
        ("sq", "Shqip"),
        ("tr", "Türkçe"),
        ("sr", "Српски"),
        ("bg", "Български"),
        ("ru", "Русский"),
        ("uk", "Українська"),
        ("el", "Ελληνικά"),
        ("zh-Hans", "简体中文"),
        ("zh-Hant", "繁體中文"),
        ("ko", "한국어"),
        ("hi", "हिन्दी"),
        ("th", "ไทย"),
        ("vi", "Tiếng Việt"),
        ("id", "Bahasa Indonesia"),
    ];

    for (tag, expected) in cases {
        assert_eq!(language_display_name(tag), expected, "tag {tag:?}");
    }

    // Case-insensitive, underscore-tolerant, trimmed
    assert_eq!(language_display_name("PT-br"), "Português (Brasil)");
    assert_eq!(language_display_name("  sr_Latn "), "Srpski (latinica)");
    assert_eq!(language_display_name("NB"), "Norsk bokmål");

    // Unknown tags fall back to the trimmed tag itself
    assert_eq!(language_display_name(" xx-NOPE "), "xx-NOPE");
}

#[test]
fn every_embedded_language_has_an_endonym() {
    for tag in available_languages() {
        assert_ne!(
            language_display_name(tag),
            tag,
            "{tag}.json is embedded but has no endonym in language_display_name"
        );
    }
}

#[test]
fn fallback_chain_resolves_active_then_english_then_key_and_handles_plurals() {
    let mut en = HashMap::new();
    en.insert("only.en".to_owned(), "English fallback".to_owned());
    en.insert("shared".to_owned(), "English shared".to_owned());
    en.insert("items.one".to_owned(), "{n} item".to_owned());
    en.insert("items.other".to_owned(), "{n} items".to_owned());
    en.insert("unit.h".to_owned(), "h".to_owned());

    let mut active = HashMap::new();
    active.insert("shared".to_owned(), "French shared".to_owned());
    active.insert("unit.h.other".to_owned(), "h-fr".to_owned());

    let fr = Strings::with_fallback("fr", active, en);

    // Direct active override
    assert_eq!(fr.get("shared"), "French shared");
    // Missing key in active falls back to English
    assert_eq!(fr.get("only.en"), "English fallback");
    // Missing in both returns key itself
    assert_eq!(fr.get("totally.missing"), "totally.missing");
    // Active `.other` is preferred over English when active only defines `.other`
    assert_eq!(fr.plural("unit.h", 1), "h-fr");
    assert_eq!(fr.get("unit.h"), "h-fr");
    // Missing plural key falls back to English using English's plural rule (0 -> other in English, even in fr!)
    assert_eq!(fr.plural("items", 0), "{n} items");
    assert_eq!(fr.plural("items", 1), "{n} item");
    assert_eq!(fr.plural("items", 5), "{n} items");
    assert_eq!(fr.plural("no.such.plural", 2), "no.such.plural");

    // Missing language file falls back completely to English
    let missing_lang = Strings::from_language("zz-NOPE");
    assert_eq!(missing_lang.language(), "en");
    assert_eq!(missing_lang.get("tray.menu.settings"), "Settings");

    // `_meta.status` is stripped at load time
    let de = Strings::from_language("de");
    assert_eq!(de.language(), "de");
    assert_eq!(de.get("_meta.status"), "_meta.status");
    let mk = Strings::from_language("mk");
    assert_eq!(mk.language(), "mk");
    assert_eq!(mk.get("_meta.status"), "_meta.status");
}

#[test]
fn auto_language_resolution_matches_exact_then_language_else_english() {
    let available = ["de", "en", "mk", "pt-BR"];

    assert_eq!(
        resolve_language_tag("auto", Some("de-DE"), &available),
        "de"
    );
    assert_eq!(resolve_language_tag("auto", Some("de"), &available), "de");
    assert_eq!(
        resolve_language_tag("auto", Some("MK-mk"), &available),
        "mk"
    );
    assert_eq!(
        resolve_language_tag("auto", Some("pt-BR"), &available),
        "pt-BR"
    );
    assert_eq!(
        resolve_language_tag("auto", Some("pt_br"), &available),
        "pt-BR"
    );
    assert_eq!(
        resolve_language_tag("auto", Some("en-GB"), &available),
        "en"
    );
    assert_eq!(
        resolve_language_tag("auto", Some("fr-FR"), &available),
        "en"
    );
    assert_eq!(resolve_language_tag("auto", None, &available), "en");
    assert_eq!(resolve_language_tag("", Some("de-AT"), &available), "de");

    // Explicit language overrides OS language
    assert_eq!(resolve_language_tag("mk", Some("de-DE"), &available), "mk");
    assert_eq!(
        resolve_language_tag("en-US", Some("mk-MK"), &available),
        "en"
    );
    assert_eq!(resolve_language_tag("fr", Some("de-DE"), &available), "en");

    // End-to-end Strings::from_language_with_os
    let auto_mk = Strings::from_language_with_os("auto", Some("mk-MK"));
    assert_eq!(auto_mk.language(), "mk");
    assert_eq!(auto_mk.get("tray.menu.settings"), "Поставки");

    let auto_de = Strings::from_language_with_os("auto", Some("de-AT"));
    assert_eq!(auto_de.language(), "de");
    assert_eq!(auto_de.get("tray.menu.settings"), "Einstellungen");

    let auto_ja = Strings::from_language_with_os("auto", Some("ja-JP"));
    assert_eq!(auto_ja.language(), "ja");
    assert_eq!(auto_ja.get("tray.menu.settings"), "設定");

    let auto_ko = Strings::from_language_with_os("auto", Some("ko-KR"));
    assert_eq!(auto_ko.language(), "ko");
    assert_eq!(auto_ko.get("tray.menu.settings"), "설정");

    let auto_unknown = Strings::from_language_with_os("auto", Some("la-VA"));
    assert_eq!(auto_unknown.language(), "en");
    assert_eq!(auto_unknown.get("tray.menu.settings"), "Settings");

    let auto_none = Strings::from_language_with_os("auto", None);
    assert_eq!(auto_none.language(), "en");
}

#[test]
fn shipped_languages_include_de_ja_and_mk_with_valid_keys_and_matching_placeholders() {
    let available = available_languages();
    assert!(available.contains(&"en"), "en.json must be embedded");
    assert!(available.contains(&"de"), "de.json must be embedded");
    assert!(available.contains(&"ja"), "ja.json must be embedded");
    assert!(available.contains(&"mk"), "mk.json must be embedded");

    let tables = embedded_language_tables();
    let en_raw = tables
        .iter()
        .find(|(tag, _)| *tag == "en")
        .map(|(_, raw)| *raw)
        .expect("en.json embedded");
    let en_map: HashMap<String, String> = serde_json::from_str(en_raw).expect("en.json valid JSON");

    for (tag, raw) in tables {
        let map: HashMap<String, String> =
            serde_json::from_str(raw).unwrap_or_else(|e| panic!("failed to parse {tag}.json: {e}"));

        if *tag != "en" {
            assert!(
                map.contains_key("_meta.status"),
                "{tag}.json should record _meta.status"
            );
        }

        // 1. No extra keys outside en.json (other than _meta.* and valid plural suffixes)
        for (key, value) in &map {
            if is_metadata_key(key) {
                continue;
            }
            assert!(
                is_valid_translation_key(key, &en_map),
                "{tag}.json contains key '{key}' not present in en.json"
            );

            // 2. Placeholders must match the English counterpart
            let en_value = en_map
                .get(key)
                .or_else(|| {
                    split_plural_suffix(key).and_then(|(base, _)| {
                        en_map
                            .get(base)
                            .or_else(|| en_map.get(&format!("{base}.other")))
                    })
                })
                .unwrap_or_else(|| panic!("{tag}.json key '{key}' has no en.json counterpart"));

            assert_eq!(
                extract_placeholders(value),
                extract_placeholders(en_value),
                "placeholder mismatch in {tag}.json for key '{key}'"
            );
        }

        // 3. Every English key must be translated in each shipped language (directly or via
        //    `.other`).
        for en_key in en_map.keys() {
            if is_metadata_key(en_key) {
                continue;
            }
            let has_direct = map.contains_key(en_key);
            let has_plural = map.contains_key(&format!("{en_key}.other"));
            assert!(
                has_direct || has_plural,
                "{tag}.json is missing translation for en.json key '{en_key}'"
            );
        }
    }
}

#[test]
fn macedonian_and_german_formatting_exercise_plurals_and_units() {
    let mk = Strings::from_language("mk");
    // Macedonian CLDR: 1, 21 -> one ("час"); 2, 11 -> other ("часа")
    assert_eq!(mk.format_time(Duration::from_secs(65 * 60)), "1 час 05 мин");
    assert_eq!(
        mk.format_time(Duration::from_secs(2 * 3600 + 3 * 60)),
        "2 часа 03 мин"
    );
    assert_eq!(mk.format_time(Duration::from_secs(11 * 3600)), "11 часа");
    assert_eq!(mk.format_time(Duration::from_secs(21 * 3600)), "21 час");
    assert_eq!(mk.format_time(Duration::from_secs(42)), "0:42");
    assert_eq!(warning_postpone_label(&mk, 3), "Одложи 3 мин");
    assert_eq!(
        warning_heading(&mk, Duration::from_secs(42)),
        "Долга пауза за 0:42"
    );

    let de = Strings::from_language("de");
    assert_eq!(
        de.format_time(Duration::from_secs(65 * 60)),
        "1 Std. 05 Min."
    );
    assert_eq!(de.format_time(Duration::from_secs(15 * 60)), "15 Min.");
    assert_eq!(de.format_time(Duration::from_secs(3600)), "1 Std.");
    assert_eq!(warning_postpone_label(&de, 3), "3 Min. verschieben");
    assert_eq!(
        warning_heading(&de, Duration::from_secs(42)),
        "Lange Pause in 0:42"
    );

    let ja = Strings::from_language("ja");
    assert_eq!(ja.language(), "ja");
    assert_eq!(ja.format_time(Duration::from_secs(65 * 60)), "1 時間 05 分");
    assert_eq!(ja.format_time(Duration::from_secs(15 * 60)), "15 分");
    assert_eq!(ja.format_time(Duration::from_secs(3600)), "1 時間");
    assert_eq!(warning_postpone_label(&ja, 3), "3分延期");
    assert_eq!(
        warning_heading(&ja, Duration::from_secs(42)),
        "長い休憩まで 0:42"
    );
}

#[test]
fn font_family_for_maps_scripts_and_defaults_to_segoe_ui() {
    let cases = [
        // Latin, Greek, Cyrillic -> Segoe UI
        ("en", "Segoe UI"),
        ("en-US", "Segoe UI"),
        ("de", "Segoe UI"),
        ("de-AT", "Segoe UI"),
        ("mk", "Segoe UI"),
        ("sr", "Segoe UI"),
        ("bg", "Segoe UI"),
        ("ru", "Segoe UI"),
        ("uk", "Segoe UI"),
        ("el", "Segoe UI"),
        ("", "Segoe UI"),
        ("auto", "Segoe UI"),
        // Japanese -> Yu Gothic UI
        ("ja", "Yu Gothic UI"),
        ("ja-JP", "Yu Gothic UI"),
        ("JA_jp", "Yu Gothic UI"),
        // Simplified Chinese -> Microsoft YaHei UI
        ("zh", "Microsoft YaHei UI"),
        ("zh-Hans", "Microsoft YaHei UI"),
        ("zh-CN", "Microsoft YaHei UI"),
        ("zh_CN", "Microsoft YaHei UI"),
        ("zh-Hans-CN", "Microsoft YaHei UI"),
        ("zh-SG", "Microsoft YaHei UI"),
        ("zh-Hans-HK", "Microsoft YaHei UI"),
        // Traditional Chinese -> Microsoft JhengHei UI
        ("zh-Hant", "Microsoft JhengHei UI"),
        ("zh-TW", "Microsoft JhengHei UI"),
        ("zh-HK", "Microsoft JhengHei UI"),
        ("zh_tw", "Microsoft JhengHei UI"),
        ("zh-Hant-TW", "Microsoft JhengHei UI"),
        ("zh-Hant-CN", "Microsoft JhengHei UI"),
        // Korean -> Malgun Gothic
        ("ko", "Malgun Gothic"),
        ("ko-KR", "Malgun Gothic"),
        // Devanagari -> Nirmala UI
        ("hi", "Nirmala UI"),
        ("hi-IN", "Nirmala UI"),
        ("mr", "Nirmala UI"),
        ("mr-IN", "Nirmala UI"),
        ("ne", "Nirmala UI"),
        ("ne-NP", "Nirmala UI"),
        // Thai -> Leelawadee UI
        ("th", "Leelawadee UI"),
        ("th-TH", "Leelawadee UI"),
        // RTL tags (when passed directly) also stay on default Segoe UI
        ("ar", "Segoe UI"),
        ("he", "Segoe UI"),
        ("fa", "Segoe UI"),
    ];

    for (tag, expected_font) in cases {
        assert_eq!(
            font_family_for(tag),
            expected_font,
            "font_family_for({tag:?})"
        );
    }
}

#[test]
fn rtl_languages_fall_back_to_english() {
    // Even if ar/he/fa were in available tables, RTL must still resolve to English until RTL support ships.
    let available = ["ar", "de", "en", "fa", "he", "ja", "mk"];

    for rtl in ["ar", "ar-SA", "AR_eg", "he", "he-IL", "fa", "fa-IR"] {
        assert!(is_rtl_language(rtl), "expected RTL for {rtl:?}");
        assert_eq!(
            resolve_language_tag(rtl, None, &available),
            "en",
            "explicit {rtl:?} must resolve to English"
        );
        assert_eq!(
            resolve_language_tag("auto", Some(rtl), &available),
            "en",
            "auto with OS {rtl:?} must resolve to English"
        );

        let loaded_explicit = Strings::from_language_with_os(rtl, None);
        assert_eq!(loaded_explicit.language(), "en");
        assert_eq!(loaded_explicit.rtl_fallback_from(), Some(rtl));
        assert_eq!(loaded_explicit.get("tray.menu.settings"), "Settings");

        let loaded_auto = Strings::from_language_with_os("auto", Some(rtl));
        assert_eq!(loaded_auto.language(), "en");
        assert_eq!(loaded_auto.rtl_fallback_from(), Some(rtl));
        assert_eq!(loaded_auto.get("tray.menu.settings"), "Settings");
    }

    assert!(!is_rtl_language("en"));
    assert!(!is_rtl_language("ja"));
    assert_eq!(Strings::from_language("ja").rtl_fallback_from(), None);
}

#[test]
fn demo_language_flag_parsing_handles_space_equals_and_missing_values() {
    assert_eq!(
        parse_demo_language(["--demo-settings", "--demo-language", "ja"]),
        Some("ja".to_owned())
    );
    assert_eq!(
        parse_demo_language(["--demo-language=mk", "--demo-break", "short"]),
        Some("mk".to_owned())
    );
    assert_eq!(
        parse_demo_language(["--demo-break", "long", "--demo-language", " de "]),
        Some("de".to_owned())
    );
    assert_eq!(
        parse_demo_language(["--demo-language", "en", "--demo-language=zh-Hant"]),
        Some("zh-Hant".to_owned())
    );
    assert_eq!(parse_demo_language(["--demo-settings"]), None);
    assert_eq!(parse_demo_language(["--demo-language"]), None);
    assert_eq!(parse_demo_language(["--demo-language="]), None);
    assert_eq!(
        parse_demo_language(["--demo-language", "--demo-settings"]),
        None
    );
    assert_eq!(
        parse_demo_language(["--demo-break", "--demo-language", "--demo-settings"]),
        None
    );
}

#[test]
fn coverage_helper_exempts_controls_skips_placeholders_and_keeps_middot_and_emdash() {
    let mut chars = Vec::new();
    for_each_checkable_char("Paused until {clock} \u{00b7} {time}\n—\t!", |ch| {
        chars.push(ch)
    });
    let collected: String = chars.into_iter().collect();
    assert_eq!(collected, "Paused until  \u{00b7} \u{2014}!");

    let missing = check_table_coverage(
        "en",
        "Segoe UI",
        [
            ("_meta.status", "ignored\u{4e2d}"),
            ("ok.key", "Hi {name}\n\t\u{00b7}\u{2014}"),
            ("bad.key", "Wait \u{4f11}"),
        ],
        |ch| ch != '\u{4f11}',
    );
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].language, "en");
    assert_eq!(missing[0].key, "bad.key");
    assert_eq!(missing[0].ch, '\u{4f11}');
    assert_eq!(missing[0].font, "Segoe UI");
    let rendered = missing[0].to_string();
    assert!(rendered.contains("language=en"));
    assert!(rendered.contains("key=bad.key"));
    assert!(rendered.contains("U+4F11"));
    assert!(rendered.contains("font=Segoe UI"));
}

#[test]
fn format_collapses_double_dot_after_abbreviations_only() {
    assert_eq!(
        Strings::format_template("in {time}.", &[("time", &"1 Std.")]),
        "in 1 Std."
    );
    assert_eq!(
        Strings::format_template("in {time}.", &[("time", &"5 min")]),
        "in 5 min."
    );
    assert_eq!(
        Strings::format_template("{time} left", &[("time", &"1 Std.")]),
        "1 Std. left"
    );
    // Other punctuation following a dot-terminated value stays untouched.
    assert_eq!(
        Strings::format_template("in {time}!", &[("time", &"1 Std.")]),
        "in 1 Std.!"
    );

    let mut map = HashMap::new();
    map.insert("tpl".to_owned(), "in {time}.".to_owned());
    map.insert("tpl.one".to_owned(), "in {time}.".to_owned());
    map.insert("tpl.other".to_owned(), "in {time}.".to_owned());
    let strings = Strings::from_map(map);
    assert_eq!(strings.format("tpl", &[("time", &"1 Std.")]), "in 1 Std.");
    assert_eq!(
        strings.format_plural("tpl", 1, &[("time", &"1 Std.")]),
        "in 1 Std."
    );
}

#[test]
fn notify_absence_credited_has_no_double_dot_for_dot_abbreviations() {
    let de = Strings::from_language("de");
    for value in [
        de.format_time(Duration::from_secs(3600)),
        de.format_time(Duration::from_secs(15 * 60)),
        format!("30 {}", de.get("time.seconds")),
    ] {
        assert!(value.ends_with('.'), "de value {value:?} should end in '.'");
        let rendered = de.format("notify.absenceCredited", &[("time", &value)]);
        assert!(!rendered.contains(".."), "de rendered had '..': {rendered}");
    }

    let el = Strings::from_language("el");
    for unit_key in ["time.seconds", "unit.minutes", "unit.seconds"] {
        let value = format!("15 {}", el.get(unit_key));
        assert!(
            value.ends_with('.'),
            "el {unit_key} value {value:?} should end in '.'"
        );
        let rendered = el.format("notify.absenceCredited", &[("time", &value)]);
        assert!(!rendered.contains(".."), "el rendered had '..': {rendered}");
    }

    let lt = Strings::from_language("lt");
    let lt_hour = lt.format_time(Duration::from_secs(3600));
    assert!(
        lt_hour.ends_with('.'),
        "lt value {lt_hour:?} should end in '.'"
    );
    let lt_rendered = lt.format("notify.absenceCredited", &[("time", &lt_hour)]);
    assert!(
        !lt_rendered.contains(".."),
        "lt rendered had '..': {lt_rendered}"
    );

    let pl = Strings::from_language("pl");
    let pl_hour = pl.format_time(Duration::from_secs(3600));
    assert!(
        pl_hour.ends_with('.'),
        "pl value {pl_hour:?} should end in '.'"
    );
    let pl_rendered = pl.format("notify.absenceCredited", &[("time", &pl_hour)]);
    assert!(
        !pl_rendered.contains(".."),
        "pl rendered had '..': {pl_rendered}"
    );
}

#[cfg(windows)]
mod windows_glyph_coverage {
    use std::collections::HashMap;

    use ocy_app::strings::{
        MissingGlyph, check_table_coverage, embedded_language_tables, font_family_for,
    };
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory, IDWriteFont,
        IDWriteFontCollection,
    };
    use windows::core::{BOOL, PCWSTR};

    fn system_font_collection() -> Result<IDWriteFontCollection, String> {
        // SAFETY: `DWriteCreateFactory` with `DWRITE_FACTORY_TYPE_SHARED` has no memory
        // preconditions and returns an owning COM pointer wrapped by `windows::core::Interface`.
        let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|e| format!("DWriteCreateFactory failed: {e}"))?;
        let mut collection: Option<IDWriteFontCollection> = None;
        // SAFETY: `collection` is a valid out-pointer to `Option<IDWriteFontCollection>`.
        unsafe { factory.GetSystemFontCollection(&mut collection, false) }
            .map_err(|e| format!("GetSystemFontCollection failed: {e}"))?;
        collection.ok_or_else(|| "GetSystemFontCollection returned None".to_owned())
    }

    fn first_font_for_family(
        collection: &IDWriteFontCollection,
        family_name: &str,
    ) -> Result<IDWriteFont, String> {
        let wide: Vec<u16> = family_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut index = 0u32;
        let mut exists = BOOL(0);
        // SAFETY: `wide` is NUL-terminated UTF-16 and `index`/`exists` are valid out-pointers.
        unsafe { collection.FindFamilyName(PCWSTR(wide.as_ptr()), &mut index, &mut exists) }
            .map_err(|e| format!("FindFamilyName({family_name:?}) failed: {e}"))?;
        if !exists.as_bool() {
            return Err(format!(
                "DirectWrite system font collection does not contain family {family_name:?}"
            ));
        }

        // SAFETY: `index` came from `FindFamilyName` with `exists == true`.
        let family = unsafe { collection.GetFontFamily(index) }
            .map_err(|e| format!("GetFontFamily({family_name:?}, {index}) failed: {e}"))?;
        // SAFETY: `0` requests the first font face in the family.
        unsafe { family.GetFont(0) }.map_err(|e| format!("GetFont({family_name:?}, 0) failed: {e}"))
    }

    fn check_entries_with_dwrite<'a>(
        collection: &IDWriteFontCollection,
        language: &str,
        font_name: &str,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Vec<MissingGlyph>, String> {
        let font = first_font_for_family(collection, font_name)?;
        let mut dwrite_error: Option<String> = None;
        let missing = check_table_coverage(language, font_name, entries, |ch| {
            // SAFETY: `font` is a valid `IDWriteFont` COM object and `ch as u32` is a Unicode scalar.
            match unsafe { font.HasCharacter(ch as u32) } {
                Ok(has) => has.as_bool(),
                Err(err) => {
                    if dwrite_error.is_none() {
                        dwrite_error = Some(format!(
                            "HasCharacter(U+{:04X}) failed for {font_name:?}: {err}",
                            ch as u32
                        ));
                    }
                    false
                }
            }
        });
        if let Some(err) = dwrite_error {
            return Err(err);
        }
        Ok(missing)
    }

    #[test]
    fn embedded_languages_have_full_glyph_coverage() {
        let collection = system_font_collection().expect("DirectWrite system font collection");
        let mut all_missing = Vec::new();

        for (tag, raw) in embedded_language_tables() {
            let map: HashMap<String, String> = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("failed to parse {tag}.json: {e}"));
            let font_name = font_family_for(tag);
            let missing = check_entries_with_dwrite(
                &collection,
                tag,
                font_name,
                map.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            )
            .unwrap_or_else(|e| panic!("DirectWrite lookup failed for {tag} ({font_name}): {e}"));
            all_missing.extend(missing);
        }

        if !all_missing.is_empty() {
            for m in &all_missing {
                eprintln!("MISSING GLYPH: {m}");
            }
            let summary: Vec<String> = all_missing.iter().map(ToString::to_string).collect();
            panic!(
                "DirectWrite glyph coverage failed ({} missing):\n{}",
                all_missing.len(),
                summary.join("\n")
            );
        }
    }

    #[test]
    fn coverage_check_fails_when_font_lacks_character() {
        let collection = system_font_collection().expect("DirectWrite system font collection");
        let missing = check_entries_with_dwrite(
            &collection,
            "ja-test",
            "Segoe UI",
            [("test.cjk", "\u{4f11}")],
        )
        .expect("DirectWrite lookup for Segoe UI should succeed");

        assert!(
            !missing.is_empty(),
            "expected Segoe UI to lack CJK character U+4F11"
        );
        assert_eq!(missing[0].language, "ja-test");
        assert_eq!(missing[0].key, "test.cjk");
        assert_eq!(missing[0].ch, '\u{4f11}');
        assert_eq!(missing[0].font, "Segoe UI");
        let msg = missing[0].to_string();
        assert!(msg.contains("U+4F11"), "unexpected message: {msg}");
        assert!(msg.contains("Segoe UI"), "unexpected message: {msg}");
    }
}
