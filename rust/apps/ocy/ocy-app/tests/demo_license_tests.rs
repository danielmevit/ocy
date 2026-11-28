//! Tests for the hidden `--demo-license <state>` flag: parsing, the pure demo-value →
//! `LicenseState` mapping, and the honoured-or-ignored decision.

use jiff::Timestamp;
use ocy_app::break_logic::{
    CliArgs, DemoLicense, demo_license_is_honoured, demo_license_state, parse_cli_args,
};
use ocy_app::instance::is_probe_mode;
use ocy_app::license::{LicenseState, STORE_FEATURE_ENABLED};

fn parse(items: &[&str]) -> CliArgs {
    parse_cli_args(items.iter().copied().map(str::to_owned))
}

#[test]
fn demo_license_parses_both_forms_and_rejects_unknown_values() {
    assert_eq!(
        parse(&["--demo-license", "full"]).demo_license,
        Some(DemoLicense::Full)
    );
    assert_eq!(
        parse(&["--demo-license=trial"]).demo_license,
        Some(DemoLicense::Trial)
    );
    assert_eq!(
        parse(&["--demo-license", "TRIAL-LAST-DAY"]).demo_license,
        Some(DemoLicense::TrialLastDay)
    );
    assert_eq!(
        parse(&["--demo-license=trial-ended"]).demo_license,
        Some(DemoLicense::TrialEnded)
    );
    assert_eq!(
        parse(&["--demo-license", "unknown"]).demo_license,
        Some(DemoLicense::Unknown)
    );

    // Unknown value, missing value, and unknown flag text all leave the flag unset.
    assert_eq!(parse(&["--demo-license", "bogus"]).demo_license, None);
    assert_eq!(parse(&["--demo-license=bogus"]).demo_license, None);
    assert_eq!(parse(&["--demo-license"]).demo_license, None);

    // The flag combines with the other demo flags.
    let cli = parse(&["--demo-settings", "--demo-license=trial-ended"]);
    assert!(cli.demo_settings);
    assert_eq!(cli.demo_license, Some(DemoLicense::TrialEnded));
}

#[test]
fn demo_license_alone_does_not_make_the_run_isolated() {
    assert!(!is_probe_mode(&parse(&["--demo-license", "trial"])));
    assert!(is_probe_mode(&parse(&[
        "--demo-settings",
        "--demo-license",
        "trial"
    ])));
}

#[test]
fn demo_license_state_maps_each_demo_value() {
    let now = Timestamp::from_second(1_760_000_000).unwrap();

    assert_eq!(
        demo_license_state(DemoLicense::Full, now),
        LicenseState::Full
    );
    assert_eq!(
        demo_license_state(DemoLicense::Unknown, now),
        LicenseState::Unknown
    );

    let ended = demo_license_state(DemoLicense::TrialEnded, now);
    assert_eq!(ended, LicenseState::TrialExpired);
    assert!(!ended.allows_reminders());
    assert_eq!(ended.trial_days_left(now), Some(0));

    let trial = demo_license_state(DemoLicense::Trial, now);
    assert_eq!(
        trial,
        LicenseState::Trial {
            ends_at: Timestamp::from_second(now.as_second() + 5 * 86_400).unwrap()
        }
    );
    assert_eq!(trial.trial_days_left(now), Some(5));
    assert!(trial.allows_reminders());

    let last_day = demo_license_state(DemoLicense::TrialLastDay, now);
    assert_eq!(
        last_day,
        LicenseState::Trial {
            ends_at: Timestamp::from_second(now.as_second() + 12 * 3_600).unwrap()
        }
    );
    assert_eq!(last_day.trial_days_left(now), Some(1));
    assert!(last_day.allows_reminders());
}

#[test]
fn demo_license_is_only_honoured_for_isolated_runs_of_a_store_build() {
    assert!(!demo_license_is_honoured(false));
    assert_eq!(demo_license_is_honoured(true), STORE_FEATURE_ENABLED);
}
