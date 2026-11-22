//! Single-instance identity resolution and `--instance-scope` flag validation.
//!
//! Keeps probe/demo launches and scoped test launches isolated from a running production
//! `Ocy` instance (`Local\xyz.damt.ocy`) while preserving C#/Rust mutual exclusion for
//! real launches.

use std::fmt;

use crate::break_logic::CliArgs;

/// Production application identifier passed to `damt_app::SingleInstance::acquire`.
///
/// On Windows this maps to the named mutex `Local\xyz.damt.ocy` and event
/// `Local\xyz.damt.ocy.signal`, matching the C# 0.1 app.
pub const APP_ID: &str = "xyz.damt.ocy";

/// `desktop:StartupTask` `TaskId` declared in `packaging/windows/AppxManifest.xml`.
pub const STARTUP_TASK_ID: &str = "OcyStartup";

/// Error returned when `--instance-scope` is missing its value or contains disallowed characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceScopeError {
    /// `--instance-scope` appeared at the end of the argument list with no following value.
    MissingValue,
    /// The scope name was empty or contained characters outside ASCII letters, digits, and `-`.
    InvalidName(String),
}

impl fmt::Display for InstanceScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue => f.write_str("missing value for --instance-scope"),
            Self::InvalidName(name) => write!(
                f,
                "invalid --instance-scope {name:?}: use 1 or more ASCII letters, digits, or '-'"
            ),
        }
    }
}

impl std::error::Error for InstanceScopeError {}

/// Returns `true` when `name` is a valid `--instance-scope` value:
/// non-empty and containing only ASCII letters, ASCII digits, and `-`.
#[must_use]
pub fn is_valid_instance_scope(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Validates a `--instance-scope` value, returning it on success or [`InstanceScopeError`] on
/// failure.
pub fn validate_instance_scope(name: &str) -> Result<&str, InstanceScopeError> {
    if is_valid_instance_scope(name) {
        Ok(name)
    } else {
        Err(InstanceScopeError::InvalidName(name.to_owned()))
    }
}

/// Parses and validates the hidden `--instance-scope <name>` / `--instance-scope=<name>` flag
/// from CLI arguments (excluding `argv[0]`).
///
/// Returns `Ok(None)` when the flag is absent, `Ok(Some(name))` when valid, or an error when
/// the value is missing or invalid.
pub fn parse_instance_scope<I, S>(args: I) -> Result<Option<String>, InstanceScopeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut scope = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--demo-break" || arg == "--demo-in" || arg == "--soak" {
            let _ = iter.next();
        } else if arg == "--instance-scope" {
            let Some(next) = iter.next() else {
                return Err(InstanceScopeError::MissingValue);
            };
            let valid = validate_instance_scope(next.as_ref())?;
            scope = Some(valid.to_owned());
        } else if let Some(rest) = arg.strip_prefix("--instance-scope=") {
            let valid = validate_instance_scope(rest)?;
            scope = Some(valid.to_owned());
        }
    }
    Ok(scope)
}

/// Returns `true` when any probe/demo flag (`--demo-break`, `--demo-settings`, `--report-memory`,
/// or `--soak`) is active.
#[must_use]
pub const fn is_probe_mode(cli: &CliArgs) -> bool {
    cli.demo_break.is_some() || cli.demo_settings || cli.report_memory || cli.soak_cycles.is_some()
}

/// Resolves the `app_id` passed to `damt_app::SingleInstance::acquire` for the given CLI flags,
/// optional validated `--instance-scope` name, and process id.
///
/// - `--instance-scope <name>` (with or without probe flags): `"xyz.damt.ocy.scope-<name>"`
///   (Windows lock `Local\xyz.damt.ocy.scope-<name>`)
/// - Probe modes without `--instance-scope`: `"xyz.damt.ocy.probe-<pid>"`
///   (Windows lock `Local\xyz.damt.ocy.probe-<pid>`)
/// - Real launch (no probe flags, no scope): `"xyz.damt.ocy"`
///   (Windows lock `Local\xyz.damt.ocy`)
#[must_use]
pub fn resolve_instance_id(cli: &CliArgs, instance_scope: Option<&str>, pid: u32) -> String {
    if let Some(scope) = instance_scope {
        return format!("{APP_ID}.scope-{scope}");
    }
    if is_probe_mode(cli) {
        return format!("{APP_ID}.probe-{pid}");
    }
    APP_ID.to_owned()
}

/// Returns the Windows named-mutex name (`Local\<id>`) that `damt-app` acquires for `instance_id`.
#[must_use]
pub fn windows_mutex_name(instance_id: &str) -> String {
    format!("Local\\{instance_id}")
}

/// Returns the Windows auto-reset signal event name (`Local\<id>.signal`) that `damt-app` uses
/// for `instance_id`.
#[must_use]
pub fn windows_signal_event_name(instance_id: &str) -> String {
    format!("Local\\{instance_id}.signal")
}
