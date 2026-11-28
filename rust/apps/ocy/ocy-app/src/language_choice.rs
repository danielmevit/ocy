//! Language dropdown model and tag/index mapping for Settings V2.

use damt_app::ErrorLog;

use crate::strings::{available_languages, language_display_name};

/// One embedded language entry in the Settings V2 language dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanguageOption {
    /// Embedded BCP-47 language tag (e.g. `"en"`, `"pt-BR"`).
    pub tag: &'static str,
    /// Endonym shown in the dropdown (e.g. `"English"`, `"Português (Brasil)"`).
    pub display_name: &'static str,
}

/// Script bucket for endonym sorting: Latin scripts sort first, then Cyrillic, then CJK/other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ScriptRank {
    Latin = 0,
    Cyrillic = 1,
    Other = 2,
}

fn is_latin_char(ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || ('\u{00C0}'..='\u{024F}').contains(&ch)
        || ('\u{1E00}'..='\u{1EFF}').contains(&ch)
}

fn is_cyrillic_char(ch: char) -> bool {
    ('\u{0400}'..='\u{052F}').contains(&ch)
}

fn script_rank(name: &str) -> ScriptRank {
    for ch in name.chars() {
        if ch.is_alphabetic() {
            if is_latin_char(ch) {
                return ScriptRank::Latin;
            }
            if is_cyrillic_char(ch) {
                return ScriptRank::Cyrillic;
            }
            return ScriptRank::Other;
        }
    }
    ScriptRank::Other
}

/// Folds Latin diacritics to base ASCII letters and lowercases every character for sorting.
fn fold_latin_char(ch: char, out: &mut String) {
    match ch {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å'
        | 'ā' | 'ă' | 'ą' => out.push('a'),
        'Æ' | 'æ' => out.push_str("ae"),
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => out.push('c'),
        'Ď' | 'Đ' | 'ď' | 'đ' => out.push('d'),
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ'
        | 'ė' | 'ę' | 'ě' => out.push('e'),
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' => out.push('g'),
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => out.push('h'),
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī'
        | 'ĭ' | 'į' | 'ı' => out.push('i'),
        'Ĳ' | 'ĳ' => out.push_str("ij"),
        'Ĵ' | 'ĵ' => out.push('j'),
        'Ķ' | 'ķ' => out.push('k'),
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => out.push('l'),
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' => out.push('n'),
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø'
        | 'ō' | 'ŏ' | 'ő' => out.push('o'),
        'Œ' | 'œ' => out.push_str("oe"),
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => out.push('r'),
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ś' | 'ŝ' | 'ş' | 'š' => out.push('s'),
        'ß' => out.push_str("ss"),
        'Ţ' | 'Ť' | 'Ŧ' | 'ţ' | 'ť' | 'ŧ' => out.push('t'),
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ'
        | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
        'Ŵ' | 'ŵ' => out.push('w'),
        'Ý' | 'Ÿ' | 'Ŷ' | 'ý' | 'ÿ' | 'ŷ' => out.push('y'),
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => out.push('z'),
        _ => {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        }
    }
}

/// Computes a diacritic-folded, case-folded sort key placing Latin names before Cyrillic and CJK.
fn endonym_sort_key(display_name: &str) -> (ScriptRank, String) {
    let rank = script_rank(display_name);
    let mut folded = String::with_capacity(display_name.len());
    for ch in display_name.chars() {
        fold_latin_char(ch, &mut folded);
    }
    (rank, folded)
}

/// Returns every embedded language sorted by its endonym (`language_display_name`), comparing
/// diacritic-folded, case-folded keys with Cyrillic and CJK names after Latin ones.
#[must_use]
pub fn language_options() -> Vec<LanguageOption> {
    let mut options: Vec<LanguageOption> = available_languages()
        .into_iter()
        .map(|tag| LanguageOption {
            tag,
            display_name: language_display_name(tag),
        })
        .collect();
    options.sort_by(|a, b| {
        endonym_sort_key(a.display_name)
            .cmp(&endonym_sort_key(b.display_name))
            .then_with(|| a.display_name.cmp(b.display_name))
            .then_with(|| a.tag.cmp(b.tag))
    });
    options
}

/// String key for the `"Automatic (...)"` first entry of the Settings V2 language dropdown.
#[must_use]
pub const fn automatic_language_key_for_platform(is_windows: bool) -> &'static str {
    if is_windows {
        "settings.v2.language.automatic"
    } else {
        "settings.v2.language.automaticSystem"
    }
}

/// String key for the `"Automatic (...)"` dropdown entry on the current compile target.
#[must_use]
pub const fn automatic_language_key() -> &'static str {
    automatic_language_key_for_platform(cfg!(windows))
}

/// Builds the full dropdown model: index 0 = `automatic_label`, followed by every embedded
/// language's endonym in sorted order.
#[must_use]
pub fn dropdown_model(automatic_label: &str) -> Vec<String> {
    let options = language_options();
    let mut model = Vec::with_capacity(options.len() + 1);
    model.push(automatic_label.to_owned());
    for option in options {
        model.push(option.display_name.to_owned());
    }
    model
}

/// Maps `settings.language` to its dropdown index (`0` for `"auto"` or empty).
///
/// Unknown tags log a diagnostic message (to `error_log` when provided and stderr) and return `0`.
#[must_use]
pub fn index_for(language: &str, error_log: Option<&ErrorLog>) -> usize {
    let trimmed = language.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return 0;
    }

    let normalized = trimmed.replace('_', "-");
    let options = language_options();
    if let Some(pos) = options
        .iter()
        .position(|opt| opt.tag.eq_ignore_ascii_case(&normalized))
    {
        return pos + 1;
    }

    let msg = format!("unknown language tag {trimmed:?}; falling back to Automatic");
    if let Some(log) = error_log {
        log.log(Some("i18n"), &msg);
    }
    eprintln!("Ocy i18n: {msg}");
    0
}

/// Maps a dropdown index back to the setting string (`"auto"` for `0` or out-of-range indices).
#[must_use]
pub fn language_for(index: usize) -> &'static str {
    if index == 0 {
        return "auto";
    }
    language_options()
        .get(index - 1)
        .map_or("auto", |opt| opt.tag)
}

/// Convenience wrapper of [`index_for`] returning `i32` for `SettingsWindowV2.language-index`.
#[must_use]
pub fn index_for_i32(language: &str, error_log: Option<&ErrorLog>) -> i32 {
    i32::try_from(index_for(language, error_log)).unwrap_or(0)
}

/// Convenience wrapper of [`language_for`] accepting `i32` from `SettingsWindowV2.language-selected`.
#[must_use]
pub fn language_for_i32(index: i32) -> &'static str {
    usize::try_from(index).map_or("auto", language_for)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ocy-lang-choice-{prefix}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn automatic_language_key_selects_windows_or_system_by_platform() {
        assert_eq!(
            automatic_language_key_for_platform(true),
            "settings.v2.language.automatic"
        );
        assert_eq!(
            automatic_language_key_for_platform(false),
            "settings.v2.language.automaticSystem"
        );
        assert_eq!(
            automatic_language_key(),
            automatic_language_key_for_platform(cfg!(windows))
        );
    }

    #[test]
    fn dropdown_model_starts_with_automatic_then_sorted_endonyms() {
        let model = dropdown_model("Automatic (Windows language)");
        let embedded = available_languages();
        assert_eq!(model.len(), embedded.len() + 1);
        assert_eq!(model[0], "Automatic (Windows language)");

        let endonyms = &model[1..];
        let mut expected_sorted = endonyms.to_vec();
        expected_sorted.sort_by(|a, b| {
            endonym_sort_key(a)
                .cmp(&endonym_sort_key(b))
                .then_with(|| a.cmp(b))
        });
        assert_eq!(endonyms, expected_sorted.as_slice());

        // Every embedded language appears in the model via its endonym.
        for tag in embedded {
            let name = language_display_name(tag);
            assert!(
                endonyms.iter().any(|item| item == name),
                "missing endonym {name:?} for tag {tag:?}"
            );
        }
    }

    #[test]
    fn endonym_sort_folds_diacritics_and_puts_cyrillic_and_cjk_after_latin() {
        let model = dropdown_model("Automatic");
        let endonyms = &model[1..];
        let pos = |name: &str| {
            endonyms
                .iter()
                .position(|entry| entry == name)
                .unwrap_or_else(|| panic!("missing {name:?} in {endonyms:?}"))
        };

        // "Čeština" sorts with "C", between Bosanski and Dansk.
        assert!(pos("Bosanski") < pos("Čeština"));
        assert!(pos("Čeština") < pos("Dansk"));

        // "Slovenčina" sorts before "Slovenščina" (c < s after diacritic folding).
        assert!(pos("Slovenčina") < pos("Slovenščina"));

        // "Türkçe" stays the last Latin-script entry, before Cyrillic and CJK.
        let last_latin = pos("Türkçe");
        let mk_pos = pos("Македонски");
        let ja_pos = pos("日本語");
        assert!(last_latin < mk_pos);
        assert!(last_latin < ja_pos);
        assert!(mk_pos < ja_pos);

        // Case- and diacritic-folding behaves consistently on mixed-case Latin strings.
        assert!(endonym_sort_key("česky") < endonym_sort_key("Dansk"));
        assert!(endonym_sort_key("ábc") < endonym_sort_key("Bcd"));
    }

    #[test]
    fn auto_and_empty_map_to_zero_and_back() {
        assert_eq!(index_for("auto", None), 0);
        assert_eq!(index_for(" AUTO ", None), 0);
        assert_eq!(index_for("", None), 0);
        assert_eq!(index_for("   ", None), 0);
        assert_eq!(language_for(0), "auto");
        assert_eq!(language_for_i32(-1), "auto");
        assert_eq!(language_for(9999), "auto");
    }

    #[test]
    fn every_embedded_tag_round_trips() {
        for tag in available_languages() {
            let idx = index_for(tag, None);
            assert!(idx > 0, "expected non-zero index for {tag:?}");
            assert_eq!(language_for(idx), tag);
            assert_eq!(language_for_i32(index_for_i32(tag, None)), tag);
        }

        // Case-insensitive and underscore-tolerant normalization.
        assert_eq!(language_for(index_for("PT_br", None)), "pt-BR");
        assert_eq!(language_for(index_for(" sr_Latn ", None)), "sr-Latn");
    }

    #[test]
    fn unknown_tag_returns_zero_and_writes_error_log() {
        let dir = temp_dir("unknown");
        let log = ErrorLog::new(&dir, "ocy");

        let idx = index_for("xx-UNKNOWN", Some(&log));
        assert_eq!(idx, 0);

        let contents = fs::read_to_string(log.path()).expect("error log should be written");
        assert!(contents.contains("i18n"));
        assert!(contents.contains("xx-UNKNOWN"));

        let _ = fs::remove_dir_all(&dir);
    }
}
