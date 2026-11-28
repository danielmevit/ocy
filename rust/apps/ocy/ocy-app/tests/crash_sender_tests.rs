//! Integration tests for the crash-report sender (CR2a), all against a local mock HTTP
//! server on 127.0.0.1 (nothing here touches the real internet).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use damt_platform::SystemClock;
use ocy_app::crash_reports::{CrashFrame, CrashKind, CrashReport, preview_text};
use ocy_app::crash_sender::CrashSender;
use ocy_app::posthog::SenderConfig;
use ocy_app::usage_stats::{AttemptOutcome, UsageStats};
use ocy_core::settings::{CrashReportsConsent, OcySettings, UsageStatsConsent};
use serde_json::Value;

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

/// A tiny single-purpose HTTP/1.1 mock: every POST gets `status`, every request is
/// recorded. Runs on 127.0.0.1:0 until the process ends.
struct MockServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    status: Arc<AtomicU16>,
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
        {
            let requests = Arc::clone(&requests);
            let status = Arc::clone(&status);
            thread::Builder::new()
                .name("mock-capture".to_owned())
                .spawn(move || serve(listener, requests, status))
                .expect("spawn mock server thread");
        }
        Self {
            endpoint: format!("http://{addr}"),
            requests,
            status,
        }
    }

    fn set_status(&self, status: u16) {
        self.status.store(status, Ordering::Relaxed);
    }

    fn recorded(&self) -> Vec<RecordedRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    fn count(&self) -> usize {
        self.requests.lock().map(|r| r.len()).unwrap_or_default()
    }
}

fn serve(
    listener: TcpListener,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    status: Arc<AtomicU16>,
) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let Some(request) = read_request(&mut stream) else {
            continue;
        };
        if let Ok(mut requests) = requests.lock() {
            requests.push(request);
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

// The crash-report consent mirror in `crash_reports` is process-wide, so tests that set it
// serialize on this lock (the usage-stats manager instances below are per-test and do not
// share anything).
static CONSENT_LOCK: Mutex<()> = Mutex::new(());

fn consent_lock() -> std::sync::MutexGuard<'static, ()> {
    CONSENT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-crash-sender-test-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn set_crash_consent(consent: CrashReportsConsent) {
    let settings = OcySettings {
        crash_reports: consent,
        ..OcySettings::default()
    };
    ocy_app::crash_reports::apply_crash_consent_from_settings(&settings);
}

/// Creates a per-test usage-stats manager (never starts a thread, so it never posts) and
/// applies the given usage-stats consent to it, to prove the crash sender's behaviour does
/// not depend on the usage-stats state.
fn usage_stats_with_consent(dir: &Path, consent: UsageStatsConsent) -> UsageStats {
    let server = MockServer::spawn();
    let manager = UsageStats::with_config(
        dir.to_path_buf(),
        Arc::new(SystemClock),
        SenderConfig::for_tests(server.endpoint),
    );
    manager.apply_consent(consent);
    manager
}

fn fixed_report(id: &str, crashed_at: i64) -> CrashReport {
    CrashReport {
        schema_version: 1,
        report_id: id.to_owned(),
        crashed_at,
        kind: CrashKind::Panic,
        message: "index out of bounds: the len is 3 but the index is 7".to_owned(),
        location: "apps/ocy/ocy-app/src/break_logic.rs:42:7".to_owned(),
        frames: vec![CrashFrame {
            function: "ocy_app::break_logic::next_break".to_owned(),
            filename: Some("apps/ocy/ocy-app/src/break_logic.rs".to_owned()),
            lineno: Some(42),
            colno: Some(7),
            module: None,
            in_app: true,
        }],
        app_version: "0.3.0-test".to_owned(),
        platform: "linux".to_owned(),
        os_version: "6.8".to_owned(),
        channel: "direct".to_owned(),
        // CR2b's build ID: present here to prove it travels; the preview/byte-identity
        // assertions below then cover the property too.
        build_id: Some("12345678-90AB-CDEF-1122-334455667788-1".to_owned()),
    }
}

fn write_report(dir: &Path, report: &CrashReport) {
    let crashes = dir.join("crashes");
    std::fs::create_dir_all(&crashes).expect("create crashes dir");
    let json = serde_json::to_string(report).expect("serialize report");
    std::fs::write(
        crashes.join(format!("{}.json", report.report_id)),
        json.as_bytes(),
    )
    .expect("write report file");
}

fn report_file(dir: &Path, report_id: &str) -> PathBuf {
    dir.join("crashes").join(format!("{report_id}.json"))
}

fn sender_for(dir: &Path, server: &MockServer) -> CrashSender {
    CrashSender::with_config(
        dir.to_path_buf(),
        SenderConfig::for_tests(server.endpoint.clone()),
    )
}

/// A fixed "now" (2027-01-15) for deterministic daily-cap tests.
const NOW: i64 = 1_800_000_000;

#[test]
fn always_plus_usage_declined_sends_and_deletes_the_report_file() {
    let _guard = consent_lock();
    let dir = temp_dir("always-declined");
    let server = MockServer::spawn();
    let sender = sender_for(&dir, &server);

    // Usage stats Declined: the crash gate must not care.
    usage_stats_with_consent(&dir, UsageStatsConsent::Declined);
    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("11111111-1111-4111-8111-111111111111", NOW - 60);
    write_report(&dir, &report);

    assert_eq!(sender.attempt(NOW), AttemptOutcome::Sent);
    assert_eq!(server.count(), 1);
    // 2xx: the local report file is gone.
    assert!(!report_file(&dir, &report.report_id).exists());

    let recorded = server.recorded();
    assert_eq!(recorded[0].method, "POST");
    assert_eq!(recorded[0].path, "/batch/");
    let body = recorded[0].json_body();
    assert_eq!(body["api_key"], "phc_test");
    let batch = body["batch"].as_array().cloned().unwrap_or_default();
    assert_eq!(batch.len(), 1);
    // The posted event is byte-identical to the card's preview text.
    assert_eq!(
        serde_json::to_string_pretty(&batch[0]).unwrap_or_default(),
        preview_text(&report)
    );
    // The PDB build ID (CR2b) travels as a property; the sender adds nothing else.
    assert_eq!(
        batch[0]["properties"]["build_id"],
        "12345678-90AB-CDEF-1122-334455667788-1"
    );
}

#[test]
fn never_plus_usage_granted_sends_nothing() {
    let _guard = consent_lock();
    let dir = temp_dir("never-granted");
    let server = MockServer::spawn();
    let sender = sender_for(&dir, &server);

    usage_stats_with_consent(&dir, UsageStatsConsent::Granted);
    set_crash_consent(CrashReportsConsent::Never);
    let report = fixed_report("22222222-2222-4222-8222-222222222222", NOW - 60);
    write_report(&dir, &report);

    assert_eq!(sender.attempt(NOW), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "crash Never never sends");

    // Ask without a Send press is equally silent, whatever usage stats say.
    set_crash_consent(CrashReportsConsent::Ask);
    assert_eq!(sender.attempt(NOW), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "crash Ask without Send sends nothing");
}

#[test]
fn ask_plus_send_now_sends_exactly_the_approved_report() {
    let _guard = consent_lock();
    let dir = temp_dir("ask-send-now");
    let server = MockServer::spawn();
    let sender = sender_for(&dir, &server);

    usage_stats_with_consent(&dir, UsageStatsConsent::Declined);
    set_crash_consent(CrashReportsConsent::Ask);
    let approved = fixed_report("33333333-3333-4333-8333-333333333333", NOW - 60);
    let other = fixed_report("44444444-4444-4444-8444-444444444444", NOW - 30);
    write_report(&dir, &approved);
    write_report(&dir, &other);

    // The card's Send button: only the report the user saw and approved.
    assert_eq!(sender.approve(&[&approved.report_id]), 1);
    // An unknown ID is ignored, never approved.
    assert_eq!(sender.approve(&["99999999-9999-4999-8999-999999999999"]), 0);

    assert_eq!(sender.attempt(NOW), AttemptOutcome::Sent);
    assert_eq!(server.count(), 1);
    let body = server.recorded()[0].json_body();
    let batch = body["batch"].as_array().cloned().unwrap_or_default();
    assert_eq!(batch.len(), 1, "only the approved report is in the batch");
    assert_eq!(batch[0]["distinct_id"], approved.report_id);
    // The approved report's file is deleted; the unapproved one waits for the card.
    assert!(!report_file(&dir, &approved.report_id).exists());
    assert!(report_file(&dir, &other.report_id).exists());
}

#[test]
fn discard_deletes_files_without_sending() {
    let _guard = consent_lock();
    let dir = temp_dir("discard");
    let server = MockServer::spawn();
    let sender = sender_for(&dir, &server);

    set_crash_consent(CrashReportsConsent::Always);
    let first = fixed_report("55555555-5555-4555-8555-555555555555", NOW - 60);
    let second = fixed_report("66666666-6666-4666-8666-666666666666", NOW - 30);
    write_report(&dir, &first);
    write_report(&dir, &second);

    assert_eq!(sender.discard(&[&first.report_id]), 1);
    assert!(!report_file(&dir, &first.report_id).exists());
    assert_eq!(sender.discard(&[&second.report_id]), 1);
    assert!(!report_file(&dir, &second.report_id).exists());

    assert_eq!(sender.attempt(NOW), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "discarded reports are never sent");
}

#[test]
fn server_error_5xx_keeps_the_report_and_retries_later() {
    let _guard = consent_lock();
    let dir = temp_dir("retry-500");
    let server = MockServer::spawn_with_status(500);
    let sender = sender_for(&dir, &server);

    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("77777777-7777-4777-8777-777777777777", NOW - 60);
    write_report(&dir, &report);

    assert_eq!(sender.attempt(NOW), AttemptOutcome::RetryLater);
    assert_eq!(server.count(), 1);
    // The report file stays for the next hourly slot.
    assert!(report_file(&dir, &report.report_id).exists());

    // The server recovers: the same report is accepted and the file goes.
    server.set_status(200);
    assert_eq!(sender.attempt(NOW + 3600), AttemptOutcome::Sent);
    assert_eq!(server.count(), 2);
    assert!(!report_file(&dir, &report.report_id).exists());
}

#[test]
fn client_error_4xx_stops_the_sender_for_the_run() {
    let _guard = consent_lock();
    let dir = temp_dir("stop-400");
    let server = MockServer::spawn_with_status(400);
    let sender = sender_for(&dir, &server);

    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("88888888-8888-4888-8888-888888888888", NOW - 60);
    write_report(&dir, &report);

    // The classification: a 4xx stops this run.
    assert_eq!(sender.attempt(NOW), AttemptOutcome::RejectedClient);
    assert_eq!(server.count(), 1);
    // The report file stays (nothing was accepted).
    assert!(report_file(&dir, &report.report_id).exists());

    // The thread-level behavior: after a 4xx the background thread exits and never posts
    // again (no retries in a tight loop, no hourly hammering of a rejected batch).
    let shared = Arc::new(sender);
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

    // The 4xx was logged once, to disk, with no event content in the line.
    let logged = std::fs::read_to_string(log.path()).expect("error log written");
    assert_eq!(logged.matches("HTTP 4xx").count(), 1, "logged exactly once");
    assert!(logged.contains("crash_reports"));
    assert!(logged.contains("sending disabled for this run"));
    assert!(!logged.contains("phc_"), "no key in the log");
    assert!(
        !logged.contains("index out of bounds"),
        "no event content in the log"
    );
}

#[test]
fn always_mode_caps_at_five_reports_per_day() {
    let _guard = consent_lock();
    let dir = temp_dir("daily-cap");
    let server = MockServer::spawn();
    let sender = sender_for(&dir, &server);

    set_crash_consent(CrashReportsConsent::Always);
    let ids = [
        "a1111111-1111-4111-8111-111111111111",
        "a2222222-2222-4222-8222-222222222222",
        "a3333333-3333-4333-8333-333333333333",
        "a4444444-4444-4444-8444-444444444444",
        "a5555555-5555-4555-8555-555555555555",
        "a6666666-6666-4666-8666-666666666666",
    ];
    for (i, id) in ids.iter().enumerate() {
        write_report(&dir, &fixed_report(id, NOW - 600 + i as i64));
    }

    // Five reports fit the daily cap and the 5-report storage limit; one batch, five events.
    assert_eq!(sender.attempt(NOW), AttemptOutcome::Sent);
    let body = server.recorded()[0].json_body();
    let batch = body["batch"].as_array().cloned().unwrap_or_default();
    assert_eq!(batch.len(), 5);
    assert_eq!(server.count(), 1);

    // A new report the same day is over the cap: it waits, nothing is dropped or sent.
    let extra = fixed_report("a7777777-7777-4777-8777-777777777777", NOW - 10);
    write_report(&dir, &extra);
    assert_eq!(sender.attempt(NOW + 60), AttemptOutcome::NothingDue);
    assert_eq!(
        server.count(),
        1,
        "the daily cap holds back the extra report"
    );
    assert!(report_file(&dir, &extra.report_id).exists());

    // Next UTC day: the extra report goes.
    assert_eq!(sender.attempt(NOW + 24 * 60 * 60), AttemptOutcome::Sent);
    assert_eq!(server.count(), 2);
    assert!(!report_file(&dir, &extra.report_id).exists());
}

#[test]
fn keyless_sender_is_inert_even_with_consent_and_approval() {
    let _guard = consent_lock();
    let dir = temp_dir("no-key");
    let server = MockServer::spawn();
    let mut config = SenderConfig::for_tests(server.endpoint.clone());
    config.api_key = None;
    let sender = CrashSender::with_config(dir.clone(), config);

    assert!(!sender.is_available());
    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("b1111111-1111-4111-8111-111111111111", NOW - 60);
    write_report(&dir, &report);
    // Even an explicit Send press sends nothing without a key.
    assert_eq!(sender.approve(&[&report.report_id]), 1);
    assert_eq!(sender.attempt(NOW), AttemptOutcome::NothingDue);
    assert_eq!(server.count(), 0, "no network in a keyless build");
    assert!(report_file(&dir, &report.report_id).exists());

    // The real build-time wiring is only active with OCY_POSTHOG_KEY; this workspace never
    // sets it, so from_build is None here. A keyed build skips this assertion.
    if ocy_app::usage_stats::build_key().is_none() {
        assert!(
            CrashSender::from_build(dir.clone()).is_none(),
            "keyless build must not construct the crash sender"
        );
    }
}

#[test]
fn https_url_returns_retry_later_instead_of_panicking() {
    let _guard = consent_lock();
    // An https URL must never panic inside the TLS setup (the release profile aborts on
    // panic). A local listener accepts and immediately drops the connection, so the TLS
    // handshake fails at the transport level: that is a retry-later error, not a crash.
    // No test contacts the real internet.
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

    let mut config = SenderConfig::for_tests(format!("https://{addr}"));
    config.request_timeout = Duration::from_secs(3);
    let dir = temp_dir("https-drop");
    let sender = CrashSender::with_config(dir.clone(), config);
    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("b2222222-2222-4222-8222-222222222222", NOW - 60);
    write_report(&dir, &report);

    assert_eq!(sender.attempt(NOW), AttemptOutcome::RetryLater);
    assert!(
        report_file(&dir, &report.report_id).exists(),
        "the report waits for the next slot"
    );
}

#[test]
fn crash_sender_loop_retries_a_5xx_on_the_next_slot() {
    let _guard = consent_lock();
    // Unlike a 4xx (which stops the run), a 5xx keeps the hourly cadence: the thread must
    // post again after retry_delay, without a tight loop.
    let dir = temp_dir("loop-500");
    let server = MockServer::spawn_with_status(500);
    let mut config = SenderConfig::for_tests(server.endpoint.clone());
    config.initial_delay = Duration::from_millis(150);
    config.retry_delay = Duration::from_millis(150);
    let sender = Arc::new(CrashSender::with_config(dir.clone(), config));

    set_crash_consent(CrashReportsConsent::Always);
    let report = fixed_report("b3333333-3333-4333-8333-333333333333", NOW - 60);
    write_report(&dir, &report);

    let log = damt_app::ErrorLog::new(dir.clone(), "ocy-test");
    let _handle = sender.start_sender(log);
    let start = Instant::now();
    while server.count() < 3 && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(server.count() >= 3, "the sender retried the 5xx");
    // Nothing was accepted, so the report file stays.
    assert!(report_file(&dir, &report.report_id).exists());
    // And a 5xx is never logged as a client rejection.
    let logged = std::fs::read_to_string(damt_app::ErrorLog::new(dir.clone(), "ocy-test").path())
        .unwrap_or_default();
    assert!(!logged.contains("HTTP 4xx"));
}

#[test]
fn global_send_now_wakes_the_sender_thread() {
    let _guard = consent_lock();
    let dir = temp_dir("global-wake");
    let server = MockServer::spawn();
    let mut config = SenderConfig::for_tests(server.endpoint.clone());
    config.initial_delay = Duration::from_secs(30);
    config.retry_delay = Duration::from_secs(30);
    let sender = Arc::new(CrashSender::with_config(dir.clone(), config));

    set_crash_consent(CrashReportsConsent::Ask);
    let report = fixed_report("b4444444-4444-4444-8444-444444444444", NOW - 60);
    write_report(&dir, &report);

    // The process-wide path the card (CR3) uses: install + send_now (approve + wake).
    let handle = sender.start_sender(damt_app::ErrorLog::new(dir.clone(), "ocy-test"));
    ocy_app::crash_sender::install(Arc::clone(&sender), handle);
    let start = Instant::now();
    assert_eq!(
        ocy_app::crash_sender::send_now(&[&report.report_id]),
        1,
        "the pending report was approved"
    );
    while server.count() < 1 && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        server.count() >= 1,
        "send_now woke the sender long before the 30 s initial delay"
    );
    // The sent report's file is gone, and the global discard is a counted delete.
    assert_eq!(ocy_app::crash_sender::discard(&[&report.report_id]), 0);
    let other = fixed_report("b5555555-5555-4555-8555-555555555555", NOW - 30);
    write_report(&dir, &other);
    assert_eq!(ocy_app::crash_sender::discard(&[&other.report_id]), 1);
    assert!(!report_file(&dir, &other.report_id).exists());
}
