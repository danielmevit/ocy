//! Integration tests for the anonymous usage-stats sender, all against a local mock HTTP
//! server on 127.0.0.1 (nothing here touches the real internet).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use damt_platform::{Clock, SystemClock};
use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use ocy_app::usage_stats::{
    AttemptOutcome, RECORD_FILE_NAME, UsageStats, UsageStatsRecord, daily_event_uuid,
};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::{OcySettings, UsageStatsConsent};
use ocy_core::statistics::{DailyStatistics, StatisticsData};
use serde_json::{Value, json};

/// One request as seen by the mock server.
#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    body: String,
}

impl RecordedRequest {
    fn json_body(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// A tiny single-purpose HTTP/1.1 mock: every POST gets `status` (after `delay_ms`), every
/// request is recorded. Runs on 127.0.0.1:0 until the process ends.
struct MockServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    status: Arc<AtomicU16>,
    delay_ms: Arc<AtomicU64>,
}

impl MockServer {
    fn spawn() -> Self {
        Self::spawn_with_status(200)
    }

    fn spawn_with_status(status: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(AtomicU16::new(status));
        let delay_ms = Arc::new(AtomicU64::new(0));
        {
            let requests = Arc::clone(&requests);
            let status = Arc::clone(&status);
            let delay_ms = Arc::clone(&delay_ms);
            thread::Builder::new()
                .name("mock-capture".to_owned())
                .spawn(move || serve(listener, requests, status, delay_ms))
                .expect("spawn mock server thread");
        }
        Self {
            endpoint: format!("http://{addr}"),
            requests,
            status,
            delay_ms,
        }
    }

    fn set_status(&self, status: u16) {
        self.status.store(status, Ordering::Relaxed);
    }

    fn set_delay_ms(&self, delay_ms: u64) {
        self.delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    fn recorded(&self) -> Vec<RecordedRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    fn count(&self) -> usize {
        self.requests.lock().map(|r| r.len()).unwrap_or(0)
    }
}

fn serve(
    listener: TcpListener,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    status: Arc<AtomicU16>,
    delay_ms: Arc<AtomicU64>,
) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let Some(request) = read_request(&mut stream) else {
            continue;
        };
        if let Ok(mut requests) = requests.lock() {
            requests.push(request);
        }
        let delay = delay_ms.load(Ordering::Relaxed);
        if delay > 0 {
            thread::sleep(Duration::from_millis(delay));
        }
        let code = status.load(Ordering::Relaxed);
        let reason = if code == 200 { "OK" } else { "Error" };
        let body = "{}";
        let response = format!(
            "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }
}

fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // Headers first.
    let header_end = loop {
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break buf
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|p| p + 4)?;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let headers = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut method = String::new();
    let mut path = String::new();
    let mut content_length = 0usize;
    for line in headers.split("\r\n") {
        if let Some(rest) = line.strip_prefix("POST ")
            && let Some(target) = rest.split(' ').next()
        {
            path = target.to_owned();
            method = "POST".to_owned();
        }
        if let Some(value) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(str::trim)
        {
            content_length = value.parse().unwrap_or(0);
        }
    }
    let body_start = header_end;
    while buf.len() < body_start + content_length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body_end = (body_start + content_length).min(buf.len());
    Some(RecordedRequest {
        method,
        path,
        body: String::from_utf8_lossy(&buf[body_start..body_end]).into_owned(),
    })
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-usage-stats-test-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn manager_for(dir: &Path, server: &MockServer) -> UsageStats {
    let config = ocy_app::usage_stats::SenderConfig::for_tests(server.endpoint.clone());
    UsageStats::with_config(dir.to_path_buf(), Arc::new(SystemClock), config)
}

fn today() -> Date {
    SystemClock
        .now()
        .timestamp()
        .to_zoned_datetime(TimeZone::system())
        .date()
}

fn store_for(dir: &Path) -> AppDataStore {
    AppDataStore::new(dir.to_path_buf(), Arc::new(SystemClock))
}

fn write_stats(dir: &Path, stats: &StatisticsData) {
    store_for(dir).save_stats(stats).expect("save stats");
}

fn record_path(dir: &Path) -> PathBuf {
    dir.join(RECORD_FILE_NAME)
}

fn load_record(dir: &Path) -> Option<UsageStatsRecord> {
    let raw = std::fs::read_to_string(record_path(dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_record(dir: &Path, record: &UsageStatsRecord) {
    let json = serde_json::to_string_pretty(record).expect("serialize record");
    std::fs::write(record_path(dir), json.as_bytes()).expect("write record");
}

/// A granted record with an install ID fixed in the test and an opt-in `days_back` days ago.
fn granted_record(opted_in: Date, enabled_sent: bool) -> UsageStatsRecord {
    UsageStatsRecord {
        schema_version: 1,
        install_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        opted_in_date: opted_in,
        last_sent_date: None,
        enabled_event_sent: enabled_sent,
    }
}

fn stats_with_days(days: &[(Date, u32)]) -> StatisticsData {
    let mut stats = StatisticsData::default();
    for (date, active_minutes) in days {
        let mut day = DailyStatistics::new(*date);
        day.short_completed = 3;
        day.long_completed = 1;
        day.active_minutes = *active_minutes;
        stats.days.push(day);
    }
    stats
}

#[test]
fn granted_consent_sends_enabled_plus_due_days_and_advances_record() {
    let dir = temp_dir("send-days");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);

    // The user said yes three days ago; two full days have passed since.
    let today = today();
    let opted_in = today
        .checked_sub(3.days())
        .expect("opt-in date three days back");
    let day_before_yesterday = today.checked_sub(2.days()).expect("day two days back");
    let yesterday = today.checked_sub(1.days()).expect("yesterday");
    manager.apply_consent(UsageStatsConsent::Granted);
    // set_consent creates a record for today; the test backdates the opt-in to exercise
    // day selection (a real opt-in three days old behaves the same).
    write_record(&dir, &granted_record(opted_in, false));
    write_stats(
        &dir,
        &stats_with_days(&[
            (day_before_yesterday, 100),
            (yesterday, 200),
            // today itself and the opt-in day must never be sent
            (opted_in, 999),
            (today, 999),
        ]),
    );

    assert_eq!(manager.attempt(), AttemptOutcome::Sent);

    let recorded = server.recorded();
    assert_eq!(recorded.len(), 1, "exactly one batch POST");
    let request = &recorded[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/batch/");
    let body = request.json_body();
    assert_eq!(body["api_key"], json!("phc_test"));
    let batch = body["batch"].as_array().cloned().unwrap_or_default();
    assert_eq!(batch.len(), 3, "enabled event plus two daily summaries");

    assert_eq!(batch[0]["event"], json!("usage_stats_enabled"));
    assert_eq!(
        batch[0]["uuid"],
        json!("37cb5470-d009-5402-9b7b-30dfd4912e14")
    );
    assert_eq!(batch[0]["timestamp"], format!("{opted_in}T12:00:00Z"));
    assert_eq!(
        batch[0]["distinct_id"],
        json!("11111111-1111-4111-8111-111111111111")
    );

    assert_eq!(batch[1]["event"], json!("daily_summary"));
    assert_eq!(
        batch[1]["timestamp"],
        format!("{day_before_yesterday}T12:00:00Z")
    );
    // Deterministic v5 from (install ID, date): computed independently of the sender.
    assert_eq!(
        batch[1]["uuid"],
        json!(daily_event_uuid(
            "11111111-1111-4111-8111-111111111111",
            day_before_yesterday
        ))
    );
    assert_eq!(batch[2]["timestamp"], format!("{yesterday}T12:00:00Z"));
    assert_eq!(
        batch[2]["uuid"],
        json!(daily_event_uuid(
            "11111111-1111-4111-8111-111111111111",
            yesterday
        ))
    );
    // Day counts flow through; day zero (today) and the opt-in day never appear.
    assert_eq!(batch[1]["properties"]["active_minutes"], json!(100));
    assert_eq!(batch[2]["properties"]["active_minutes"], json!(200));
    assert_eq!(batch[1]["properties"]["days_since_opt_in"], json!(1));
    assert_eq!(batch[2]["properties"]["days_since_opt_in"], json!(2));
    let body_text = request.body.to_ascii_lowercase();
    assert!(!body_text.contains("games_silent"));

    // The record advanced: enabled sent, last day accepted.
    let record = load_record(&dir).expect("record persisted");
    assert!(record.enabled_event_sent);
    assert_eq!(record.last_sent_date, Some(yesterday));

    // Nothing more is due: a second attempt makes no request.
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 1);
}

#[test]
fn server_error_500_retries_later_without_advancing() {
    let dir = temp_dir("retry-500");
    let server = MockServer::spawn_with_status(500);
    let manager = manager_for(&dir, &server);
    manager.apply_consent(UsageStatsConsent::Granted);

    assert_eq!(manager.attempt(), AttemptOutcome::RetryLater);
    assert_eq!(server.count(), 1);
    // The enabled event stays pending: nothing was accepted.
    let record = load_record(&dir).expect("record exists");
    assert!(!record.enabled_event_sent);
    assert_eq!(record.last_sent_date, None);

    // The server recovers: the same batch is accepted and the record advances.
    server.set_status(200);
    assert_eq!(manager.attempt(), AttemptOutcome::Sent);
    assert_eq!(server.count(), 2);
    let record = load_record(&dir).expect("record exists");
    assert!(record.enabled_event_sent);
}

#[test]
fn client_error_400_stops_the_sender_for_the_run() {
    let dir = temp_dir("stop-400");
    let server = MockServer::spawn_with_status(400);
    let manager = manager_for(&dir, &server);
    manager.apply_consent(UsageStatsConsent::Granted);

    // The classification: a 4xx stops this run.
    assert_eq!(manager.attempt(), AttemptOutcome::RejectedClient);
    assert_eq!(server.count(), 1);
    let record = load_record(&dir).expect("record exists");
    assert!(!record.enabled_event_sent);

    // The thread-level behavior: after a 4xx the background thread exits and never
    // posts again (no retries in a tight loop, no hourly hammering of a rejected batch).
    let shared = Arc::new(manager);
    let log = damt_app::ErrorLog::new(dir.clone(), "ocy-test");
    let _handle = shared.start_sender(log.clone());
    let start = Instant::now();
    while server.count() < 2 && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(50));
    }
    let reached = server.count();
    thread::sleep(Duration::from_millis(600));
    assert_eq!(reached, 2, "exactly one thread attempt was made");
    assert_eq!(server.count(), 2, "the sender stopped after the 4xx");

    // The 4xx was logged once, to disk, with no event content in the line (R4 finding 6).
    let logged = std::fs::read_to_string(log.path()).expect("error log written");
    assert_eq!(logged.matches("HTTP 4xx").count(), 1, "logged exactly once");
    assert!(logged.contains("usage_stats"));
    assert!(logged.contains("sending disabled for this run"));
    assert!(!logged.contains("phc_"), "no key in the log");
    assert!(
        !logged.contains("distinct_id"),
        "no event content in the log"
    );
}

#[test]
fn request_times_out_as_retry_later() {
    let dir = temp_dir("timeout");
    let server = MockServer::spawn_with_status(200);
    server.set_delay_ms(3000);
    let mut config = ocy_app::usage_stats::SenderConfig::for_tests(server.endpoint.clone());
    config.request_timeout = Duration::from_millis(500);
    let manager = UsageStats::with_config(dir.clone(), Arc::new(SystemClock), config);
    manager.apply_consent(UsageStatsConsent::Granted);

    let started = Instant::now();
    assert_eq!(manager.attempt(), AttemptOutcome::RetryLater);
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "the client gave up at its timeout, not at the server delay ({elapsed:?})"
    );
    // Nothing advanced.
    let record = load_record(&dir).expect("record exists");
    assert!(!record.enabled_event_sent);
}

#[test]
fn granted_creates_the_id_and_declined_deletes_the_record() {
    let dir = temp_dir("consent-file");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);

    // NotAsked: nothing exists, nothing sends.
    assert_eq!(manager.consent(), UsageStatsConsent::NotAsked);
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0);

    // Granted creates the install ID and persists consent.
    manager.apply_consent(UsageStatsConsent::Granted);
    assert_eq!(manager.consent(), UsageStatsConsent::Granted);
    let record = load_record(&dir).expect("record created on consent");
    let parsed = uuid::Uuid::parse_str(&record.install_id).expect("install id is a UUID");
    assert_eq!(parsed.get_version_num(), 4);
    assert_eq!(record.opted_in_date, today());
    assert!(!record.enabled_event_sent);

    // Declined deletes the file (ID and queue gone) and persists the decline.
    manager.apply_consent(UsageStatsConsent::Declined);
    assert_eq!(manager.consent(), UsageStatsConsent::Declined);
    assert!(load_record(&dir).is_none(), "record deleted on decline");
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "declined never sends");

    // A later yes makes a new ID.
    manager.apply_consent(UsageStatsConsent::Granted);
    let new_record = load_record(&dir).expect("fresh record after re-opt-in");
    assert_ne!(new_record.install_id, record.install_id);
}

#[test]
fn fresh_opt_in_sends_only_the_enabled_event_on_day_zero() {
    let dir = temp_dir("day-zero");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);
    manager.apply_consent(UsageStatsConsent::Granted);
    // Today's stats exist but today is not a full day yet.
    write_stats(&dir, &stats_with_days(&[(today(), 400)]));

    assert_eq!(manager.attempt(), AttemptOutcome::Sent);
    let recorded = server.recorded();
    assert_eq!(recorded.len(), 1);
    let body = recorded[0].json_body();
    let batch = body["batch"].as_array().cloned().unwrap_or_default();
    assert_eq!(batch.len(), 1, "no daily summaries on day zero");
    assert_eq!(batch[0]["event"], json!("usage_stats_enabled"));

    // A second attempt has nothing queued at all.
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 1);
}

#[test]
fn no_key_build_path_is_inert() {
    let dir = temp_dir("no-key");
    let server = MockServer::spawn();
    let mut config = ocy_app::usage_stats::SenderConfig::for_tests(server.endpoint.clone());
    config.api_key = None;
    let manager = UsageStats::with_config(dir.clone(), Arc::new(SystemClock), config);

    assert!(!manager.is_available());
    // A keyless build: even a granted record produces no thread and no request.
    manager.apply_consent(UsageStatsConsent::Granted);
    let log = damt_app::ErrorLog::new(dir.clone(), "ocy-test");
    let handle = Arc::new(manager.clone()).start_sender(log);
    handle.wake();
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(server.count(), 0, "no network in a keyless build");

    // The real build-time wiring is only active with OCY_POSTHOG_KEY; this workspace never
    // sets it, so from_build is None here. A keyed build skips this assertion.
    if ocy_app::usage_stats::build_key().is_none() {
        assert!(
            UsageStats::from_build(dir.clone(), Arc::new(SystemClock)).is_none(),
            "keyless build must not create the manager"
        );
    }
}

#[test]
fn usage_stats_never_writes_settings_json() {
    let dir = temp_dir("no-settings-writes");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);

    // Consent flows only from the host's in-memory settings (apply_consent); the manager
    // must never persist usage_stats itself, or a later host save could resurrect or erase
    // the user's choice (R4 findings 2 and 3).
    manager.apply_consent(UsageStatsConsent::Granted);
    let settings: OcySettings = store_for(&dir).load_settings();
    assert_eq!(settings.usage_stats, UsageStatsConsent::NotAsked);

    manager.apply_consent(UsageStatsConsent::Declined);
    let settings: OcySettings = store_for(&dir).load_settings();
    assert_eq!(settings.usage_stats, UsageStatsConsent::NotAsked);
}

#[test]
fn settings_save_after_declined_never_resurrects_sending() {
    let dir = temp_dir("no-resurrect");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);
    let today = today();
    let yesterday = today.checked_sub(1.days()).expect("yesterday");
    manager.apply_consent(UsageStatsConsent::Granted);
    write_stats(&dir, &stats_with_days(&[(yesterday, 100)]));

    // The host applies the in-memory consent after every settings save. After a decline,
    // any number of saves (with Declined still in the in-memory settings, the only source
    // of truth) must never recreate the record or send anything.
    manager.apply_consent(UsageStatsConsent::Declined);
    assert_eq!(manager.consent(), UsageStatsConsent::Declined);
    for _ in 0..3 {
        manager.apply_consent(UsageStatsConsent::Declined);
    }
    assert!(load_record(&dir).is_none(), "record stays deleted");
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "nothing is ever sent after a decline");
}

#[test]
fn decline_during_inflight_post_does_not_advance_or_recreate_the_record() {
    let dir = temp_dir("inflight-decline");
    let server = MockServer::spawn();
    server.set_delay_ms(1200);
    let manager = manager_for(&dir, &server);
    manager.apply_consent(UsageStatsConsent::Granted);
    let snapshot_id = load_record(&dir).expect("record created").install_id;

    // One attempt blocks inside the HTTP call (the mock server delays its response), with
    // the record lock released for the call; the decline happens mid-flight.
    let sender = Arc::new(manager.clone());
    let attempt_sender = Arc::clone(&sender);
    let result = thread::spawn(move || attempt_sender.attempt());

    thread::sleep(Duration::from_millis(300));
    let declined = Arc::clone(&sender);
    let decline_time = Instant::now();
    declined.apply_consent(UsageStatsConsent::Declined);
    let decline_took = decline_time.elapsed();
    assert!(
        decline_took < Duration::from_millis(800),
        "apply_consent must not wait on the in-flight HTTP call (took {decline_took:?})"
    );

    // The server accepted the batch, but the declined record stays deleted and un-advanced.
    assert!(matches!(result.join(), Ok(AttemptOutcome::Sent)));
    assert_eq!(manager.consent(), UsageStatsConsent::Declined);
    assert!(
        load_record(&dir).is_none(),
        "no record is recreated after a decline"
    );
    assert_eq!(server.count(), 1);

    // A later re-grant makes a new install ID; the accepted-but-discarded old batch never
    // taints it (nothing sent, nothing advanced).
    manager.apply_consent(UsageStatsConsent::Granted);
    let fresh = load_record(&dir).expect("fresh record after re-grant");
    assert_ne!(fresh.install_id, snapshot_id);
    assert!(!fresh.enabled_event_sent);
    assert_eq!(fresh.last_sent_date, None);
}

#[test]
fn https_url_returns_retry_later_instead_of_panicking() {
    // R4 finding 1: an https URL must never panic inside the TLS setup (the release
    // profile aborts on panic). A local listener accepts and immediately drops the
    // connection, so the TLS handshake fails at the transport level: that is a
    // retry-later error, not a crash. No test contacts the real internet.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind https mock");
    let addr = listener.local_addr().expect("https mock addr");
    thread::Builder::new()
        .name("https-dropper".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("spawn https dropper");

    let mut config = ocy_app::usage_stats::SenderConfig::for_tests(format!("https://{addr}"));
    config.request_timeout = Duration::from_secs(3);
    let manager = UsageStats::with_config(temp_dir("https-drop"), Arc::new(SystemClock), config);
    manager.apply_consent(UsageStatsConsent::Granted);

    assert_eq!(manager.attempt(), AttemptOutcome::RetryLater);
}

#[test]
fn sender_loop_retries_a_5xx_on_the_next_slot() {
    // Unlike a 4xx (which stops the run), a 5xx keeps the hourly cadence: the thread
    // must post again after retry_delay, without advancing the record.
    let dir = temp_dir("loop-500");
    let server = MockServer::spawn_with_status(500);
    let mut config = ocy_app::usage_stats::SenderConfig::for_tests(server.endpoint.clone());
    config.initial_delay = Duration::from_millis(150);
    config.retry_delay = Duration::from_millis(150);
    let manager = Arc::new(UsageStats::with_config(
        dir.clone(),
        Arc::new(SystemClock),
        config,
    ));
    manager.apply_consent(UsageStatsConsent::Granted);

    let log = damt_app::ErrorLog::new(dir.clone(), "ocy-test");
    let _handle = manager.start_sender(log);
    let start = Instant::now();
    while server.count() < 3 && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(server.count() >= 3, "the sender retried the 5xx");
    // Nothing was accepted, so the enabled event stays pending.
    let record = load_record(&dir).expect("record exists");
    assert!(!record.enabled_event_sent);
    assert_eq!(record.last_sent_date, None);
    // And a 5xx is never logged as a client rejection.
    let logged = std::fs::read_to_string(damt_app::ErrorLog::new(dir.clone(), "ocy-test").path())
        .unwrap_or_default();
    assert!(!logged.contains("HTTP 4xx"));
}

#[test]
fn waking_the_sender_beats_the_initial_delay() {
    let dir = temp_dir("wake-early");
    let server = MockServer::spawn();
    let mut config = ocy_app::usage_stats::SenderConfig::for_tests(server.endpoint.clone());
    config.initial_delay = Duration::from_secs(30);
    config.retry_delay = Duration::from_secs(30);
    let manager = Arc::new(UsageStats::with_config(
        dir.clone(),
        Arc::new(SystemClock),
        config,
    ));
    manager.apply_consent(UsageStatsConsent::Granted);

    let log = damt_app::ErrorLog::new(dir.clone(), "ocy-test");
    let sender = manager.start_sender(log);
    // The process-wide path AN2's one-time ask uses: install + wake_sender.
    ocy_app::usage_stats::install(Arc::clone(&manager), sender);
    let start = Instant::now();
    ocy_app::usage_stats::wake_sender();
    while server.count() < 1 && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        server.count() >= 1,
        "the sender woke long before the 30 s initial delay"
    );
}

#[test]
fn corrupt_record_is_recovered_by_the_next_consent_apply() {
    let dir = temp_dir("corrupt-record");
    let server = MockServer::spawn();
    let manager = manager_for(&dir, &server);
    manager.apply_consent(UsageStatsConsent::Granted);
    std::fs::write(record_path(&dir), b"{ this is not valid json").expect("corrupt the record");

    // A corrupt record never sends anything (and never crashes).
    assert_eq!(manager.attempt(), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0);

    // The next host hook (any settings load/save with Granted) replaces it with a fresh,
    // valid record, and sending works again.
    manager.apply_consent(UsageStatsConsent::Granted);
    let record = load_record(&dir).expect("record recovered");
    assert!(!record.enabled_event_sent);
    assert_eq!(manager.attempt(), AttemptOutcome::Sent);
    assert_eq!(server.count(), 1);
    let record = load_record(&dir).expect("record persisted");
    assert!(record.enabled_event_sent);
}
