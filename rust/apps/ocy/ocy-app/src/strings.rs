//! Embedded string table and `{name}` formatting (port of `Localization/Strings.cs`).

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use jiff::SignedDuration;

const EN_JSON: &str = include_str!("../../../../../content/strings/en.json");

/// Localized string lookup and placeholder formatting backed by `content/strings/en.json`.
#[derive(Debug, Clone)]
pub struct Strings {
    strings: HashMap<String, String>,
}

impl Default for Strings {
    fn default() -> Self {
        Self::from_embedded().unwrap_or_else(|_| Self {
            strings: HashMap::new(),
        })
    }
}

impl Strings {
    /// Parses the embedded `content/strings/en.json` resource.
    pub fn from_embedded() -> Result<Self, serde_json::Error> {
        let strings: HashMap<String, String> = serde_json::from_str(EN_JSON)?;
        Ok(Self { strings })
    }

    /// Constructs a `Strings` instance from an explicit key-value map.
    #[must_use]
    pub fn from_map(strings: HashMap<String, String>) -> Self {
        Self { strings }
    }

    /// Number of entries in the string table.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.strings.len()
    }

    /// Looks up `key`, returning `key` itself when missing.
    #[must_use]
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map_or(key, String::as_str)
    }

    /// Replaces `{name}` placeholders in the template looked up by `key`.
    #[must_use]
    pub fn format(&self, key: &str, args: &[(&str, &dyn fmt::Display)]) -> String {
        let template = self.get(key);
        if args.is_empty() {
            return template.to_owned();
        }

        let mut result = template.to_owned();
        for (name, value) in args {
            let placeholder = format!("{{{name}}}");
            let replacement = value.to_string();
            result = result.replace(&placeholder, &replacement);
        }
        result
    }

    /// Formats a break countdown as `m:ss`.
    #[must_use]
    pub fn format_countdown(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{minutes}:{seconds:02}")
    }

    /// Formats `duration` using the localized `"time.hours"` and `"time.minutes"` units.
    #[must_use]
    pub fn format_time(&self, duration: Duration) -> String {
        let signed = SignedDuration::try_from(duration).unwrap_or(SignedDuration::MAX);
        self.format_time_signed(signed)
    }

    /// Formats a signed `duration` using the localized `"time.hours"` and `"time.minutes"` units.
    #[must_use]
    pub fn format_time_signed(&self, duration: SignedDuration) -> String {
        let h = self.get("time.hours");
        let m = self.get("time.minutes");
        Self::format_time_with_units(duration, h, m)
    }

    /// Formats `duration` with default English `"h"` and `"min"` units.
    #[must_use]
    pub fn time_text(duration: Duration) -> String {
        let signed = SignedDuration::try_from(duration).unwrap_or(SignedDuration::MAX);
        Self::time_text_signed(signed)
    }

    /// Formats a signed `duration` with default English `"h"` and `"min"` units.
    #[must_use]
    pub fn time_text_signed(duration: SignedDuration) -> String {
        Self::format_time_with_units(duration, "h", "min")
    }

    /// Formats `duration` with explicit `hours_unit` and `minutes_unit` labels.
    #[must_use]
    pub fn format_time_with_units(
        duration: SignedDuration,
        hours_unit: &str,
        minutes_unit: &str,
    ) -> String {
        if duration < SignedDuration::from_mins(1) {
            let seconds = duration.as_secs().max(0);
            return format!("0:{seconds:02}");
        }

        if duration >= SignedDuration::from_hours(1) {
            let total_seconds = duration.as_secs();
            let hours = total_seconds / 3_600;
            let minutes = (total_seconds % 3_600) / 60;
            return format!("{hours} {hours_unit} {minutes:02} {minutes_unit}");
        }

        let total_minutes = duration.as_secs() / 60;
        format!("{total_minutes} {minutes_unit}")
    }
}
