//! The crash-report sender (CR2a): the send gate, the batch transport, the retry
//! lifecycle, and the API the crash card (CR3) calls. Capture and storage are CR1's
//! ([`crate::crash_reports`]); this module only decides **when a stored report may leave
//! the machine** and posts it through the shared PostHog transport ([`crate::posthog`],
//! the same TLS stack, endpoint and key the usage-stats sender uses).
//!
//! ## The send gate (one source of truth)
//!
//! A report may be sent only when **both** hold:
//! 1. the build has a PostHog key ([`usage_stats::build_key`] — the same Ocy project; a
//!    keyless build never even constructs a sender), **and**
//! 2. either the crash-report consent is `Always`, **or** the user pressed **Send** for
//!    exactly that report (`send_now`, the card's button).
//!
//! The usage-stats consent plays **no** role: it is not a parameter of the gate, and no
//! code path here reads it. `Always` + usage `Declined` sends; usage `Granted` + crash
//! `Never` or `Ask` (without Send) sends nothing.
//!
//! ## Lifecycle
//!
//! A background thread (never the UI thread, about a minute after start, then hourly)
//! snapshots the pending reports, builds the batch from CR1's `exception_event_json`
//! (byte-identical to `preview_text`), and POSTs `{endpoint}/batch/`:
//! - **2xx:** the local report file is deleted.
//! - **network error / 5xx:** the file stays, the thread retries next hour.
//! - **4xx:** the thread logs once (no event content) and stops for the run.
//! - **`Always` mode:** at most [`MAX_REPORTS_PER_DAY`] reports per day are sent; extras
//!   wait for the next day (the daily counter lives in `crash-send-state.json` next to
//!   the other data files, so a restart cannot reset it).
//!
//! Offline is fine: reports simply wait (CR1's 30-day and 5-report storage limits still
//! apply, enforced by CR1). `discard` deletes report files without sending anything.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::park_timeout;
use std::time::SystemTime;

use jiff::tz::TimeZone;
use ocy_core::settings::CrashReportsConsent;
use serde::{Deserialize, Serialize};

use crate::crash_reports::{CrashReport, CrashStore, exception_event_json};
use crate::posthog::{Batch, SenderConfig, SenderHandle, SubmitOutcome, post_batch};
use crate::usage_stats::AttemptOutcome;

/// At most this many reports are sent per day while consent is `Always`; extras wait.
pub const MAX_REPORTS_PER_DAY: u32 = 5;

/// File name of the daily send counter (`crash-send-state.json`), next to the other data
/// files (outside `crashes/`, which CR1 owns and cleans).
pub const STATE_FILE_NAME: &str = "crash-send-state.json";

/// The one send gate: a report may be sent only if the build has a PostHog key **and**
/// either the crash-report consent is `Always` or the user pressed Send for that report.
/// The usage-stats consent is deliberately not a parameter: it plays no role.
///
/// (The key half is checked once per sender construction in the real app — a keyless
/// build never constructs a sender or a thread — but it stays part of the gate so the
/// pure function states the whole rule.)
#[must_use]
pub fn report_may_send(has_key: bool, consent: CrashReportsConsent, user_approved: bool) -> bool {
    has_key
        && match consent {
            CrashReportsConsent::Always => true,
            CrashReportsConsent::Ask => user_approved,
            CrashReportsConsent::Never => false,
        }
}

/// The daily send counter (`crash-send-state.json`): how many reports were sent on `day`
/// while consent was `Always`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendState {
    schema_version: i32,
    /// UTC date, `YYYY-MM-DD`.
    day: String,
    sent_count: u32,
}

impl SendState {
    const SCHEMA_VERSION: i32 = 1;

    fn fresh(day: &str) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            day: day.to_owned(),
            sent_count: 0,
        }
    }
}

/// The crash-report sender. Cheap to clone; all clones share the approved-report set and
/// the state-file lock, so the UI thread (CR3's card) and the background thread agree on
/// what may be sent.
#[derive(Clone)]
pub struct CrashSender {
    store: CrashStore,
    data_dir: PathBuf,
    config: SenderConfig,
    approved: Arc<Mutex<HashSet<String>>>,
    state_lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for CrashSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrashSender")
            .field("data_dir", &self.data_dir)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CrashSender {
    /// Creates the sender with an explicit config (tests, ad-hoc probes).
    #[must_use]
    pub fn with_config(data_dir: impl Into<PathBuf>, config: SenderConfig) -> Self {
        let data_dir = data_dir.into();
        Self {
            store: CrashStore::new(data_dir.clone()),
            data_dir,
            config,
            approved: Arc::new(Mutex::new(HashSet::new())),
            state_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Creates the sender for the build's key and endpoint. `None` in a keyless build
    /// (the feature is inert: no thread, no network, nothing sent).
    #[must_use]
    pub fn from_build(data_dir: impl Into<PathBuf>) -> Option<Self> {
        let config = SenderConfig::default();
        config.api_key.as_ref()?;
        Some(Self::with_config(data_dir, config))
    }

    /// `true` when this sender can send (the build has a PostHog key).
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    /// The Send button on the card (CR3): approves exactly these reports and returns how
    /// many of them are pending right now (unknown IDs are ignored). The actual POST
    /// happens on the background thread; call the process-wide [`send_now`] wrapper, not
    /// this, from the UI.
    pub fn approve(&self, report_ids: &[&str]) -> usize {
        let pending: HashSet<String> = self
            .store
            .pending_reports()
            .into_iter()
            .map(|report| report.report_id)
            .collect();
        let mut approved = self.locked_approved();
        let mut accepted = 0usize;
        for id in report_ids {
            if pending.contains(*id) {
                approved.insert((*id).to_owned());
                accepted += 1;
            }
        }
        accepted
    }

    /// The Don't-send button on the card (CR3): deletes these report files, un-approves
    /// them, and never sends anything. Returns how many were deleted.
    pub fn discard(&self, report_ids: &[&str]) -> usize {
        let mut approved = self.locked_approved();
        let mut deleted = 0usize;
        for id in report_ids {
            approved.remove(*id);
            if self.store.delete(id) {
                deleted += 1;
            }
        }
        deleted
    }

    fn is_approved(&self, report_id: &str) -> bool {
        self.locked_approved().contains(report_id)
    }

    fn locked_approved(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.approved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs one send attempt. Also the body of one sender-thread iteration.
    ///
    /// The batch is CR1's `exception_event_json` per eligible report, wrapped only in the
    /// outer `api_key`/batch envelope. On 2xx the report files are deleted; a file that
    /// fails to delete stays (and loses its approval, so it is not resent in a loop) and
    /// the attempt reports [`AttemptOutcome::PersistFailed`].
    ///
    /// `now` (unix seconds) picks the pending snapshot and the daily-counter day; the
    /// thread passes the wall clock, tests pass fixed values.
    pub fn attempt(&self, now: i64) -> AttemptOutcome {
        if self.config.api_key.is_none() {
            return AttemptOutcome::NothingDue;
        }
        let consent = crate::crash_reports::consent();
        let day = utc_day(now);
        let mut eligible: Vec<CrashReport> = self
            .store
            .pending_reports_at(now)
            .into_iter()
            .filter(|report| report_may_send(true, consent, self.is_approved(&report.report_id)))
            .collect();
        if eligible.is_empty() {
            return AttemptOutcome::NothingDue;
        }
        if consent == CrashReportsConsent::Always {
            // The daily cap: extras wait for the next day, nothing is dropped.
            let sent_today = self.sent_count(&day);
            let room = MAX_REPORTS_PER_DAY.saturating_sub(sent_today);
            eligible.truncate(room as usize);
            if eligible.is_empty() {
                return AttemptOutcome::NothingDue;
            }
        }
        // The event bytes are CR1's `exception_event_json` (including the `build_id`
        // property CR2b's module adds on Windows); the sender adds only the outer
        // `api_key`/batch wrapper.
        let events: Vec<serde_json::Value> = eligible.iter().map(exception_event_json).collect();
        let batch = Batch {
            api_key: self.config.api_key.as_deref().unwrap_or_default(),
            batch: &events,
        };
        match post_batch(&self.config, &batch) {
            SubmitOutcome::Accepted => (),
            SubmitOutcome::RejectedClient => return AttemptOutcome::RejectedClient,
            SubmitOutcome::RetryLater => return AttemptOutcome::RetryLater,
        }
        let mut delete_failed = false;
        let mut approved = self.locked_approved();
        for report in &eligible {
            // Sent: the local report file goes; the approval does too (a resend would be
            // a duplicate event — the report has no dedup UUID by CR1's frozen format).
            approved.remove(&report.report_id);
            if !self.store.delete(&report.report_id) {
                delete_failed = true;
            }
        }
        drop(approved);
        if consent == CrashReportsConsent::Always {
            self.record_sent(&day, eligible.len() as u32);
        }
        if delete_failed {
            return AttemptOutcome::PersistFailed(io::Error::other(
                "a sent report file could not be deleted",
            ));
        }
        AttemptOutcome::Sent
    }

    /// Starts the background sender thread (about one minute after start, then hourly).
    /// Never the UI thread, never blocking startup. No thread is spawned without a key.
    #[must_use]
    pub fn start_sender(self: &Arc<Self>, error_log: damt_app::ErrorLog) -> SenderHandle {
        if self.config.api_key.is_none() {
            return SenderHandle::new(Arc::new(AtomicBool::new(false)), None);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let sender = Arc::clone(self);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("crash-reports".to_owned())
            .spawn(move || crash_sender_loop(sender, error_log, thread_stop))
            .ok();
        SenderHandle::new(stop, thread)
    }

    fn state_path(&self) -> PathBuf {
        self.data_dir.join(STATE_FILE_NAME)
    }

    /// Reports already sent on `day` while consent was `Always` (0 on a new day, a missing
    /// or corrupt file, or any error — never a crash).
    fn sent_count(&self, day: &str) -> u32 {
        let _guard = self
            .state_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match load_state(&self.state_path()) {
            Some(state) if state.day == day => state.sent_count,
            _ => 0,
        }
    }

    /// Adds `count` sent reports to the daily counter.
    fn record_sent(&self, day: &str, count: u32) {
        let _guard = self
            .state_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = self.state_path();
        let mut state = match load_state(&path) {
            Some(state) if state.day == day => state,
            _ => SendState::fresh(day),
        };
        state.sent_count = state.sent_count.saturating_add(count);
        save_state(&path, &state);
    }
}

fn load_state(path: &Path) -> Option<SendState> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<SendState>(&raw).ok()
}

fn save_state(path: &Path, state: &SendState) {
    let Ok(json) = serde_json::to_string(state) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut tmp = path.to_path_buf().into_os_string();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);
    if fs::write(&tmp_path, json.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, path);
    }
}

/// The UTC date (`YYYY-MM-DD`) of unix seconds; the daily cap counts UTC days.
fn utc_day(now: i64) -> String {
    jiff::Timestamp::from_second(now)
        .map(|timestamp| {
            timestamp
                .to_zoned_datetime(TimeZone::UTC)
                .date()
                .to_string()
        })
        .unwrap_or_else(|_| "1970-01-01".to_owned())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

fn crash_sender_loop(
    sender: Arc<CrashSender>,
    error_log: damt_app::ErrorLog,
    stop: Arc<AtomicBool>,
) {
    let mut delay = sender.config.initial_delay;
    loop {
        park_timeout(delay);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match sender.attempt(now_unix()) {
            AttemptOutcome::Sent
            | AttemptOutcome::NothingDue
            | AttemptOutcome::RetryLater
            | AttemptOutcome::PersistFailed(_) => {
                delay = sender.config.retry_delay;
            }
            AttemptOutcome::RejectedClient => {
                // Log once for the run, with no event content in the log.
                error_log.log(
                    Some("crash_reports"),
                    "server rejected the crash-report batch (HTTP 4xx); sending disabled for this run",
                );
                return;
            }
        }
    }
}

// --- Wiring: process-wide access for the app and the crash card (CR3) ---

static SENDER: OnceLock<Arc<CrashSender>> = OnceLock::new();
static HANDLE: OnceLock<SenderHandle> = OnceLock::new();

/// Installs the process-wide sender and handle (call once from the app start).
pub fn install(sender: Arc<CrashSender>, handle: SenderHandle) {
    let _ = SENDER.set(sender);
    let _ = HANDLE.set(handle);
}

/// The process-wide sender, for the crash card (CR3).
#[must_use]
pub fn global() -> Option<Arc<CrashSender>> {
    SENDER.get().cloned()
}

/// The card's Send button (CR3): approves these reports and sends them soon on the
/// background thread (never the UI thread). Returns how many of the IDs are pending.
/// Also the right call when the user approves a report shown by "See what's in it".
pub fn send_now(report_ids: &[&str]) -> usize {
    let Some(sender) = global() else {
        return 0;
    };
    let accepted = sender.approve(report_ids);
    wake();
    accepted
}

/// The card's Don't-send button (CR3): deletes these report files; nothing is sent.
/// Returns how many were deleted.
pub fn discard(report_ids: &[&str]) -> usize {
    let Some(sender) = global() else {
        return 0;
    };
    sender.discard(report_ids)
}

/// Wakes the sender thread early (a Send press, or consent just became `Always`).
pub fn wake() {
    if let Some(handle) = HANDLE.get() {
        handle.wake();
    }
}

/// App-start wiring: creates the build-time sender (when the build has a key), starts its
/// background thread, and installs both process-wide. Returns the sender for callers that
/// want a handle now; CR3 can also use [`global`], [`send_now`] and [`discard`].
///
/// Inert in keyless builds: no sender, no thread, nothing installed.
#[must_use]
pub fn start_from_build(
    data_dir: impl Into<PathBuf>,
    error_log: damt_app::ErrorLog,
) -> Option<Arc<CrashSender>> {
    let sender = Arc::new(CrashSender::from_build(data_dir)?);
    let handle = sender.start_sender(error_log);
    let stored = Arc::clone(&sender);
    install(sender, handle);
    Some(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_is_key_and_always_or_send() {
        use CrashReportsConsent::{Always, Ask, Never};
        // No key: nothing is ever sent, whatever the consent or the card says.
        assert!(!report_may_send(false, Always, true));
        assert!(!report_may_send(false, Always, false));
        // Key + Always: send, with or without a Send press.
        assert!(report_may_send(true, Always, false));
        assert!(report_may_send(true, Always, true));
        // Key + Ask: only with a Send press for exactly that report.
        assert!(!report_may_send(true, Ask, false));
        assert!(report_may_send(true, Ask, true));
        // Key + Never: never, even after a Send press.
        assert!(!report_may_send(true, Never, false));
        assert!(!report_may_send(true, Never, true));
    }

    #[test]
    fn utc_day_matches_jiff_formatting() {
        assert_eq!(utc_day(0), "1970-01-01");
        // The UTC day boundary: 23:30 and 00:30 UTC one hour apart fall on different days.
        let late = jiff::civil::datetime(2026, 11, 30, 23, 30, 0, 0)
            .to_zoned_datetime(TimeZone::UTC)
            .expect("late evening")
            .timestamp()
            .as_second();
        assert_eq!(utc_day(late), "2026-11-30");
        assert_eq!(utc_day(late + 1800), "2026-12-01");
    }

    #[test]
    fn state_round_trips_and_resets_on_a_new_day() {
        let dir =
            std::env::temp_dir().join(format!("ocy-crash-sender-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(STATE_FILE_NAME);

        save_state(&path, &SendState::fresh("2026-11-30"));
        let mut state = load_state(&path).expect("state file written");
        assert_eq!(state.schema_version, 1);
        assert_eq!(state.day, "2026-11-30");
        assert_eq!(state.sent_count, 0);
        state.sent_count = 5;
        save_state(&path, &state);
        assert_eq!(load_state(&path).expect("reloaded").sent_count, 5);

        // A corrupt file reads as absent: the cap restarts at zero, never crashes.
        fs::write(&path, b"{ not json").expect("corrupt the state");
        assert_eq!(load_state(&path), None);

        let _ = fs::remove_dir_all(&dir);
    }
}
