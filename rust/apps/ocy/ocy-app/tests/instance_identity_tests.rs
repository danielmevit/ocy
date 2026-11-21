use ocy_app::break_logic::{CliArgs, DemoBreak, parse_cli_args};
use ocy_app::instance::{
    APP_ID, InstanceScopeError, is_probe_mode, is_valid_instance_scope, parse_instance_scope,
    resolve_instance_id, validate_instance_scope, windows_mutex_name, windows_signal_event_name,
};

#[test]
fn real_launch_identity_is_unchanged() {
    assert_eq!(APP_ID, "xyz.damt.ocy");

    let cli = parse_cli_args(Vec::<String>::new());
    let scope = parse_instance_scope(Vec::<String>::new()).unwrap();
    assert_eq!(scope, None);
    assert!(!is_probe_mode(&cli));

    let id = resolve_instance_id(&cli, scope.as_deref(), 4242);
    assert_eq!(id, "xyz.damt.ocy");
    assert_eq!(windows_mutex_name(&id), r"Local\xyz.damt.ocy");
    assert_eq!(windows_signal_event_name(&id), r"Local\xyz.damt.ocy.signal");
}

#[test]
fn demo_in_alone_is_not_a_probe_launch() {
    let cli = parse_cli_args(["--demo-in", "5"].map(str::to_owned));
    assert!(!is_probe_mode(&cli));
    let id = resolve_instance_id(&cli, None, 4242);
    assert_eq!(windows_mutex_name(&id), r"Local\xyz.damt.ocy");
}

#[test]
fn probe_modes_use_per_pid_identity_without_scope() {
    let cases: [&[&str]; 9] = [
        &["--demo-break", "short"],
        &["--demo-break=warning"],
        &["--demo-break", "long", "--demo-in", "3"],
        &["--demo-settings"],
        &["--report-memory"],
        &["--report-memory", "--demo-break", "short"],
        &["--report-memory", "--demo-settings", "--demo-break", "long"],
        &["--soak", "30"],
        &["--soak=5"],
    ];

    for args in cases {
        let cli = parse_cli_args(args.iter().copied().map(str::to_owned));
        assert!(is_probe_mode(&cli), "expected probe mode for {args:?}");

        let id = resolve_instance_id(&cli, None, 9876);
        assert_eq!(id, "xyz.damt.ocy.probe-9876", "args={args:?}");
        assert_eq!(
            windows_mutex_name(&id),
            r"Local\xyz.damt.ocy.probe-9876",
            "args={args:?}"
        );
        assert_eq!(
            windows_signal_event_name(&id),
            r"Local\xyz.damt.ocy.probe-9876.signal",
            "args={args:?}"
        );

        let other_pid = resolve_instance_id(&cli, None, 9877);
        assert_ne!(id, other_pid);
        assert_ne!(windows_mutex_name(&id), r"Local\xyz.damt.ocy");
    }
}

#[test]
fn instance_scope_overrides_real_and_probe_identities() {
    let flag_sets: [&[&str]; 6] = [
        &[],
        &["--demo-break", "short"],
        &["--demo-break=long", "--demo-in=2"],
        &["--demo-settings"],
        &["--report-memory"],
        &[
            "--report-memory",
            "--demo-settings",
            "--demo-break",
            "warning",
        ],
    ];

    for args in flag_sets {
        let cli = parse_cli_args(args.iter().copied().map(str::to_owned));
        let id = resolve_instance_id(&cli, Some("lead-smoke-1"), 4242);
        assert_eq!(id, "xyz.damt.ocy.scope-lead-smoke-1", "args={args:?}");
        assert_eq!(
            windows_mutex_name(&id),
            r"Local\xyz.damt.ocy.scope-lead-smoke-1",
            "args={args:?}"
        );
        assert_eq!(
            windows_signal_event_name(&id),
            r"Local\xyz.damt.ocy.scope-lead-smoke-1.signal",
            "args={args:?}"
        );
    }
}

#[test]
fn resolve_instance_id_covers_explicit_cli_structs() {
    let empty = CliArgs::default();
    assert_eq!(
        windows_mutex_name(&resolve_instance_id(&empty, None, 1)),
        r"Local\xyz.damt.ocy"
    );

    let demo_short = CliArgs {
        demo_break: Some(DemoBreak::Short),
        ..CliArgs::default()
    };
    assert_eq!(
        windows_mutex_name(&resolve_instance_id(&demo_short, None, 17)),
        r"Local\xyz.damt.ocy.probe-17"
    );
    assert_eq!(
        windows_mutex_name(&resolve_instance_id(&demo_short, Some("S1"), 17)),
        r"Local\xyz.damt.ocy.scope-S1"
    );
}

#[test]
fn parse_instance_scope_accepts_valid_names_and_flag_positions() {
    assert_eq!(
        parse_instance_scope(["--instance-scope", "abc"]).unwrap(),
        Some("abc".to_owned())
    );
    assert_eq!(
        parse_instance_scope(["--instance-scope=ABC-123"]).unwrap(),
        Some("ABC-123".to_owned())
    );
    assert_eq!(
        parse_instance_scope(["--demo-break", "short", "--instance-scope", "r4g-01"]).unwrap(),
        Some("r4g-01".to_owned())
    );
    assert_eq!(
        parse_instance_scope([
            "--instance-scope=first",
            "--demo-settings",
            "--instance-scope",
            "second"
        ])
        .unwrap(),
        Some("second".to_owned())
    );
    assert_eq!(
        parse_instance_scope(["--demo-in", "10", "--instance-scope=0-z"]).unwrap(),
        Some("0-z".to_owned())
    );
}

#[test]
fn parse_instance_scope_rejects_missing_or_invalid_values() {
    assert_eq!(
        parse_instance_scope(["--instance-scope"]),
        Err(InstanceScopeError::MissingValue)
    );
    assert_eq!(
        parse_instance_scope(["--demo-settings", "--instance-scope"]),
        Err(InstanceScopeError::MissingValue)
    );
    assert_eq!(
        parse_instance_scope(["--instance-scope="]),
        Err(InstanceScopeError::InvalidName(String::new()))
    );
    assert_eq!(
        parse_instance_scope(["--instance-scope", ""]),
        Err(InstanceScopeError::InvalidName(String::new()))
    );

    let invalid_names = [
        "bad_name",
        "bad.name",
        "bad/name",
        r"bad\name",
        "bad name",
        "bad:name",
        "bad@1",
        "café",
        "scope\t1",
    ];

    for bad in invalid_names {
        assert!(!is_valid_instance_scope(bad), "should reject {bad:?}");
        assert_eq!(
            validate_instance_scope(bad),
            Err(InstanceScopeError::InvalidName(bad.to_owned()))
        );
        assert_eq!(
            parse_instance_scope(["--instance-scope", bad]),
            Err(InstanceScopeError::InvalidName(bad.to_owned()))
        );
        let eq_flag = format!("--instance-scope={bad}");
        assert_eq!(
            parse_instance_scope([eq_flag.as_str()]),
            Err(InstanceScopeError::InvalidName(bad.to_owned()))
        );
    }

    assert_eq!(
        parse_instance_scope(["--instance-scope", "bad_1", "--instance-scope", "good"]),
        Err(InstanceScopeError::InvalidName("bad_1".to_owned()))
    );
}

#[cfg(not(windows))]
#[test]
fn single_instance_scopes_isolate_from_real_and_exclude_matching_scope() {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use damt_app::{Acquire, SingleInstance};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("ocy-r4g-test-{}-{nanos}", std::process::id()));
    let _ = fs::create_dir_all(&dir);

    let real_id = resolve_instance_id(&CliArgs::default(), None, 100);
    let probe_cli = parse_cli_args(["--demo-settings"].map(str::to_owned));
    let probe_id = resolve_instance_id(&probe_cli, None, 101);
    let scoped_id = resolve_instance_id(&probe_cli, Some("test-scope"), 102);

    let real_guard = SingleInstance::acquire_in_dir(&dir, &real_id);
    assert!(matches!(real_guard, Acquire::First(_)));

    let probe_guard = SingleInstance::acquire_in_dir(&dir, &probe_id);
    assert!(matches!(probe_guard, Acquire::First(_)));

    let scoped_first = SingleInstance::acquire_in_dir(&dir, &scoped_id);
    assert!(matches!(scoped_first, Acquire::First(_)));

    let scoped_second = SingleInstance::acquire_in_dir(&dir, &scoped_id);
    assert!(matches!(scoped_second, Acquire::AlreadyRunning));

    drop(scoped_first);
    drop(probe_guard);
    drop(real_guard);
    let _ = fs::remove_dir_all(&dir);
}
