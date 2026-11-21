//! Preset definitions, application, and detection for Settings V2.

use ocy_core::settings::OcySettings;

/// Named break-rhythm presets shown in Settings V2 (matching `SettingsWindowV2.preset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Lighter cadence: 15 s short break every 40 min, 5 min long break every 90 min.
    Gentle,
    /// Default Ocy cadence: 15 s short break every 25 min, 5 min long break every 60 min.
    Balanced,
    /// Longer deep-work blocks: 20 s short break every 50 min, 10 min long break every 90 min.
    Focused,
    /// User-customized values that do not match any built-in preset.
    Custom,
}

/// The four interval/duration settings controlled by a built-in [`Preset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PresetValues {
    pub short_duration_seconds: i32,
    pub short_interval_minutes: i32,
    pub long_duration_minutes: i32,
    pub long_interval_minutes: i32,
}

/// Built-in presets in UI display order (left-to-right preset cards).
pub const DISPLAY_PRESETS: [Preset; 3] = [Preset::Gentle, Preset::Balanced, Preset::Focused];

/// Returns the built-in presets in display order.
#[must_use]
pub const fn list() -> &'static [Preset] {
    &DISPLAY_PRESETS
}

impl Preset {
    /// Integer index expected by `SettingsWindowV2.preset` (`0`..=`3`).
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Self::Gentle => 0,
            Self::Balanced => 1,
            Self::Focused => 2,
            Self::Custom => 3,
        }
    }

    /// Maps a `SettingsWindowV2.preset` index back to [`Preset`].
    #[must_use]
    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Gentle),
            1 => Some(Self::Balanced),
            2 => Some(Self::Focused),
            3 => Some(Self::Custom),
            _ => None,
        }
    }

    /// Returns the four interval/duration values for built-in presets, or `None` for [`Self::Custom`].
    #[must_use]
    pub const fn values(self) -> Option<PresetValues> {
        match self {
            Self::Gentle => Some(PresetValues {
                short_duration_seconds: 15,
                short_interval_minutes: 40,
                long_duration_minutes: 5,
                long_interval_minutes: 90,
            }),
            Self::Balanced => Some(PresetValues {
                short_duration_seconds: 15,
                short_interval_minutes: 25,
                long_duration_minutes: 5,
                long_interval_minutes: 60,
            }),
            Self::Focused => Some(PresetValues {
                short_duration_seconds: 20,
                short_interval_minutes: 50,
                long_duration_minutes: 10,
                long_interval_minutes: 90,
            }),
            Self::Custom => None,
        }
    }
}

/// Applies `preset` to a clone of `settings`, changing only the four preset fields
/// (`short_breaks.duration_seconds`, `short_breaks.interval_minutes`,
/// `long_breaks.duration_minutes`, `long_breaks.interval_minutes`).
///
/// Warning, postpone, strict mode, quiet-time settings, enable flags, and all other fields stay
/// as the user set them. Applying [`Preset::Custom`] returns a clone unchanged.
#[must_use]
pub fn apply(preset: Preset, settings: &OcySettings) -> OcySettings {
    let mut updated = settings.clone();
    if let Some(values) = preset.values() {
        updated.short_breaks.duration_seconds = values.short_duration_seconds;
        updated.short_breaks.interval_minutes = values.short_interval_minutes;
        updated.long_breaks.duration_minutes = values.long_duration_minutes;
        updated.long_breaks.interval_minutes = values.long_interval_minutes;
    }
    updated
}

/// Detects which built-in preset matches all four fields of `settings`, or [`Preset::Custom`].
#[must_use]
pub fn detect(settings: &OcySettings) -> Preset {
    for &preset in &DISPLAY_PRESETS {
        if let Some(values) = preset.values()
            && settings.short_breaks.duration_seconds == values.short_duration_seconds
            && settings.short_breaks.interval_minutes == values.short_interval_minutes
            && settings.long_breaks.duration_minutes == values.long_duration_minutes
            && settings.long_breaks.interval_minutes == values.long_interval_minutes
        {
            return preset;
        }
    }
    Preset::Custom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_detect_as_balanced() {
        assert_eq!(detect(&OcySettings::default()), Preset::Balanced);
    }

    #[test]
    fn display_order_and_indices_match_settings_v2() {
        assert_eq!(list(), &[Preset::Gentle, Preset::Balanced, Preset::Focused]);
        assert_eq!(Preset::Gentle.index(), 0);
        assert_eq!(Preset::Balanced.index(), 1);
        assert_eq!(Preset::Focused.index(), 2);
        assert_eq!(Preset::Custom.index(), 3);
        for preset in [
            Preset::Gentle,
            Preset::Balanced,
            Preset::Focused,
            Preset::Custom,
        ] {
            assert_eq!(Preset::from_index(preset.index()), Some(preset));
        }
        assert_eq!(Preset::from_index(-1), None);
        assert_eq!(Preset::from_index(4), None);
    }

    #[test]
    fn apply_sets_exact_values_and_preserves_other_settings() {
        let mut base = OcySettings::default();
        base.short_breaks.enabled = false;
        base.short_breaks.fullscreen = true;
        base.long_breaks.enabled = false;
        base.long_breaks.warning_seconds = 180;
        base.long_breaks.postpone_minutes = 7;
        base.long_breaks.max_postpones = 5;
        base.strict_mode = true;
        base.inactivity_tracking = false;
        base.away_threshold_seconds = 300;
        base.pause_during_fullscreen = false;
        base.has_window_nearby = false;
        base.language = "de".to_owned();
        base.start_with_system = true;

        let gentle = apply(Preset::Gentle, &base);
        assert_eq!(detect(&gentle), Preset::Gentle);
        assert_eq!(gentle.short_breaks.duration_seconds, 15);
        assert_eq!(gentle.short_breaks.interval_minutes, 40);
        assert_eq!(gentle.long_breaks.duration_minutes, 5);
        assert_eq!(gentle.long_breaks.interval_minutes, 90);
        // Non-preset fields stay intact.
        assert!(!gentle.short_breaks.enabled);
        assert!(gentle.short_breaks.fullscreen);
        assert!(!gentle.long_breaks.enabled);
        assert_eq!(gentle.long_breaks.warning_seconds, 180);
        assert_eq!(gentle.long_breaks.postpone_minutes, 7);
        assert_eq!(gentle.long_breaks.max_postpones, 5);
        assert!(gentle.strict_mode);
        assert!(!gentle.inactivity_tracking);
        assert_eq!(gentle.away_threshold_seconds, 300);
        assert!(!gentle.pause_during_fullscreen);
        assert!(!gentle.has_window_nearby);
        assert_eq!(gentle.language, "de");
        assert!(gentle.start_with_system);

        let focused = apply(Preset::Focused, &base);
        assert_eq!(detect(&focused), Preset::Focused);
        assert_eq!(focused.short_breaks.duration_seconds, 20);
        assert_eq!(focused.short_breaks.interval_minutes, 50);
        assert_eq!(focused.long_breaks.duration_minutes, 10);
        assert_eq!(focused.long_breaks.interval_minutes, 90);
        assert_eq!(focused.long_breaks.warning_seconds, 180);
        assert!(focused.strict_mode);

        let balanced = apply(Preset::Balanced, &focused);
        assert_eq!(detect(&balanced), Preset::Balanced);
        assert_eq!(balanced.short_breaks.duration_seconds, 15);
        assert_eq!(balanced.short_breaks.interval_minutes, 25);
        assert_eq!(balanced.long_breaks.duration_minutes, 5);
        assert_eq!(balanced.long_breaks.interval_minutes, 60);
        assert_eq!(balanced.long_breaks.warning_seconds, 180);
        assert!(balanced.strict_mode);

        let custom = apply(Preset::Custom, &focused);
        assert_eq!(custom, focused);
    }

    #[test]
    fn detect_returns_custom_when_any_of_the_four_fields_differs() {
        let mut s = apply(Preset::Balanced, &OcySettings::default());
        s.short_breaks.interval_minutes = 11;
        assert_eq!(detect(&s), Preset::Custom);

        let mut s = apply(Preset::Gentle, &OcySettings::default());
        s.short_breaks.duration_seconds = 20;
        assert_eq!(detect(&s), Preset::Custom);

        let mut s = apply(Preset::Focused, &OcySettings::default());
        s.long_breaks.duration_minutes = 8;
        assert_eq!(detect(&s), Preset::Custom);

        let mut s = apply(Preset::Balanced, &OcySettings::default());
        s.long_breaks.interval_minutes = 45;
        assert_eq!(detect(&s), Preset::Custom);
    }
}
