//! Local crash-report capture: consent mirror, panic hook, (Windows) native-crash raw
//! record, scrubbing, report/event format, storage limits, and the API for CR2a/CR3.
//!
//! Nothing is ever sent from here; sending is CR2a. A crash writes a small local report
//! into `<data>/crashes/<report_id>.json` (atomic tmp + rename). On the next start the
//! host shows a card (CR3) whose "See what's in it" text is exactly [`preview_text`].
//!
//! Consent has one source of truth, the host's in-memory `OcySettings`, applied through
//! [`apply_crash_consent_from_settings`]. This module never reads or writes `settings.json`,
//! and its consent is completely separate from the usage-stats consent.
//!
//! Scrubbing is applied to the message, the location and every frame string: known
//! runtime values (user, host, home, data folder, silent-game names) and generic path
//! patterns are replaced before anything is written to disk, so a report never contains
//! personal data in the first place.

use std::any::Any;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use ocy_core::settings::{CrashReportsConsent, OcySettings};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::usage_stats::{channel_tag, os_version, platform_tag};

/// JSON schema version of the report file.
pub const SCHEMA_VERSION: i32 = 1;

/// Directory (under the data dir) holding pending reports and the native raw record.
pub const CRASHES_DIR_NAME: &str = "crashes";

/// Name of the raw native-crash record file the exception handler appends to.
pub const NATIVE_RAW_FILE_NAME: &str = "native-raw.log";

/// At most this many pending reports are kept; the oldest are dropped.
pub const MAX_PENDING_REPORTS: usize = 5;

/// One report file is at most this many bytes; frames are truncated to fit.
pub const MAX_REPORT_BYTES: usize = 32 * 1024;

/// Reports older than this are deleted (30 days).
pub const MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// Crash-loop guard window (10 minutes).
pub const LOOP_WINDOW_SECS: i64 = 10 * 60;

/// More reports than this inside [`LOOP_WINDOW_SECS`] is a crash loop.
pub const LOOP_MAX_REPORTS: usize = 3;

/// At most this many native raw records one run appends (the loop guard trims the rest).
pub const MAX_NATIVE_RECORDS_PER_RUN: u32 = 4;

/// The `type` of a panic exception.
pub const PANIC_EVENT_TYPE: &str = "panic";

// --- Consent mirror (like usage stats: the host's in-memory settings own it) ---

const CONSENT_ASK: u8 = 0;
const CONSENT_ALWAYS: u8 = 1;
const CONSENT_NEVER: u8 = 2;

static CONSENT: AtomicU8 = AtomicU8::new(CONSENT_ASK);

const fn consent_to_u8(consent: CrashReportsConsent) -> u8 {
    match consent {
        CrashReportsConsent::Ask => CONSENT_ASK,
        CrashReportsConsent::Always => CONSENT_ALWAYS,
        CrashReportsConsent::Never => CONSENT_NEVER,
    }
}

/// Current consent, from the in-memory mirror the host last applied.
#[must_use]
pub fn consent() -> CrashReportsConsent {
    match CONSENT.load(Ordering::Acquire) {
        CONSENT_ALWAYS => CrashReportsConsent::Always,
        CONSENT_NEVER => CrashReportsConsent::Never,
        _ => CrashReportsConsent::Ask,
    }
}

/// `true` when the mirror says `Never` (checked by the panic hook and the native handler).
fn consent_is_never() -> bool {
    CONSENT.load(Ordering::Acquire) == CONSENT_NEVER
}

// --- Scrubbing ---

/// Placeholder tokens replacing personal values.
const PLACEHOLDER_USER: &str = "<user>";
const PLACEHOLDER_HOST: &str = "<host>";
const PLACEHOLDER_HOME: &str = "<home>";
const PLACEHOLDER_DATA: &str = "<data>";
const PLACEHOLDER_GAME: &str = "<game>";
const PLACEHOLDER_PATH: &str = "<path>";

/// Replaces known runtime values and generic path patterns in report strings.
///
/// Known values are longest-first, so a data folder inside the home folder is replaced
/// with `<data>` (not a half-eaten `<home>...`).
#[derive(Debug, Clone)]
pub struct Scrubber {
    known: Vec<(String, &'static str)>,
}

impl Scrubber {
    /// Builds the scrubber from the runtime values: user and computer name, home and data
    /// folders, and every `games_silent` entry.
    #[must_use]
    pub fn new(home: &str, data_dir: &str, games: &[String]) -> Self {
        let mut env_values: Vec<(String, &'static str)> = Vec::new();
        for (key, placeholder) in [
            ("USERNAME", PLACEHOLDER_USER),
            ("USER", PLACEHOLDER_USER),
            ("LOGNAME", PLACEHOLDER_USER),
            ("COMPUTERNAME", PLACEHOLDER_HOST),
            ("HOSTNAME", PLACEHOLDER_HOST),
        ] {
            if let Ok(value) = std::env::var(key) {
                env_values.push((value, placeholder));
            }
        }
        Self::build(home, data_dir, games, &env_values)
    }

    /// Test seam: the user and computer names are injected directly, so tests do not
    /// depend on the environment they run in.
    #[cfg(test)]
    pub(crate) fn with_values(
        home: &str,
        data_dir: &str,
        games: &[String],
        user: &str,
        host: &str,
    ) -> Self {
        Self::build(
            home,
            data_dir,
            games,
            &[
                (user.to_owned(), PLACEHOLDER_USER),
                (host.to_owned(), PLACEHOLDER_HOST),
            ],
        )
    }

    fn build(
        home: &str,
        data_dir: &str,
        games: &[String],
        env_values: &[(String, &'static str)],
    ) -> Self {
        let mut known: Vec<(String, &'static str)> = Vec::new();
        let mut push = |value: String, placeholder: &'static str| {
            let value = value.trim();
            if value.len() >= 2 && !known.iter().any(|(v, _)| v == value) {
                known.push((value.to_owned(), placeholder));
            }
        };
        // Longest/most specific first: data folder, home, then the bare names.
        push(data_dir.to_owned(), PLACEHOLDER_DATA);
        push(home.to_owned(), PLACEHOLDER_HOME);
        for game in games {
            push(game.clone(), PLACEHOLDER_GAME);
        }
        for (value, placeholder) in env_values {
            push(value.clone(), placeholder);
        }
        known.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        Self { known }
    }

    /// Applies every scrub rule to `text`: known literals, cargo-registry paths, workspace
    /// paths, then any remaining absolute path.
    #[must_use]
    pub fn scrub(&self, text: &str) -> String {
        let text = strip_extended_prefixes(text);
        let text = self.replace_known(&text);
        let text = rewrite_cargo_registry(&text);
        let text = rewrite_workspace_paths(&text);
        rewrite_absolute_paths(&text)
    }

    fn replace_known(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (value, placeholder) in &self.known {
            if out.eq_ignore_ascii_case(value) {
                return (*placeholder).to_owned();
            }
            out = replace_ignore_case(&out, value, placeholder);
        }
        out
    }
}

/// Case-insensitive substring replacement (ASCII case folding, char-based).
fn replace_ignore_case(text: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if hay.len() < needle.len() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < hay.len() {
        if i + needle.len() <= hay.len()
            && hay[i..i + needle.len()]
                .iter()
                .zip(&needle)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            out.push_str(replacement);
            i += needle.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}

/// Strips `\\?\` and `\\.\` path prefixes so the later passes see a plain path.
fn strip_extended_prefixes(text: &str) -> String {
    text.replace(r"\\?\", "").replace(r"\\.\", "")
}

/// `true` for the two path separators.
const fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Walks back from `from` to the start of the containing token (whitespace/quote boundary).
fn token_start(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut start = from;
    while start > 0 {
        let prev = bytes[start - 1];
        if prev.is_ascii_whitespace() || prev == b'"' || prev == b'\'' {
            break;
        }
        start -= 1;
    }
    start
}

/// Rewrites cargo registry paths to the crate-relative form:
/// `.../.cargo/registry/src/<index>/crate-1.2.3/src/x.rs` -> `crate-1.2.3/src/x.rs`.
fn rewrite_cargo_registry(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(found) = find_cargo_registry_marker(rest) else {
            out.push_str(rest);
            return out;
        };
        let cut_from = token_start(rest, found);
        let keep_from = found + found_registry_prefix_len(&rest[found..]);
        out.push_str(&rest[..cut_from]);
        out.push_str(&normalize_separators(&rest[keep_from..]));
        // Skip past the rewritten path (cut at the next whitespace/quote).
        let after = &rest[keep_from..];
        let end = after
            .char_indices()
            .find(|(i, c)| *i > 0 && (c.is_whitespace() || *c == '"' || *c == '\''))
            .map_or(after.len(), |(i, _)| i);
        rest = &after[end..];
    }
}

/// Finds the offset of the `.cargo` component of a registry path, case-insensitively.
fn find_cargo_registry_marker(text: &str) -> Option<usize> {
    let hay = text.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(pos) = hay[from..].find(".cargo") {
        let at = from + pos;
        // Segment start: preceded by start, whitespace, or a separator.
        if (at == 0
            || hay.as_bytes()[at - 1].is_ascii_whitespace()
            || is_sep(text[..at].chars().next_back().unwrap_or('/')))
            && found_registry_prefix_len(&text[at..]) > 0
        {
            return Some(at);
        }
        from = at + 1;
        if from >= hay.len() {
            return None;
        }
    }
    None
}

/// Length of `.cargo[/\]registry[/\]src[/\]<index>[/\]` starting at `s`, or 0 when `s` does
/// not start with that shape. The crate name begins right after it.
fn found_registry_prefix_len(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    for component in [".cargo", "registry", "src"] {
        let lower: String = chars[i..]
            .iter()
            .take(component.len())
            .collect::<String>()
            .to_ascii_lowercase();
        if lower != component {
            return 0;
        }
        i += component.len();
        if i >= chars.len() || !is_sep(chars[i]) {
            return 0;
        }
        i += 1;
    }
    // Skip exactly one segment: the registry index hash.
    while i < chars.len() && !is_sep(chars[i]) {
        i += 1;
    }
    if i < chars.len() {
        i += 1;
    }
    i
}

/// Rewrites build-machine workspace paths to repo-relative ones:
/// `.../rust/apps/ocy/...` -> `apps/ocy/...`, `.../rust/crates/damt-app/...` ->
/// `crates/damt-app/...` (either separator).
fn rewrite_workspace_paths(text: &str) -> String {
    let hay = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(at) = find_workspace_marker(text, &hay, cursor) {
        let keep_from = at + "rust/".len();
        let token_end = token_end_at(text, keep_from);
        out.push_str(&text[cursor..token_start(text, at)]);
        out.push_str(&normalize_separators(&text[keep_from..token_end]));
        cursor = token_end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// End of the path-like token starting at or before `from`: the first whitespace or quote.
fn token_end_at(text: &str, from: usize) -> usize {
    text[from..]
        .char_indices()
        .find(|(i, c)| *i > 0 && (c.is_whitespace() || *c == '"' || *c == '\''))
        .map_or(text.len(), |(i, _)| from + i)
}

/// Finds `rust[/\](apps|crates)[/\]` at a token start, case-insensitively. Returns the
/// marker offset.
fn find_workspace_marker(text: &str, hay_lower: &str, from: usize) -> Option<usize> {
    let mut search = from;
    loop {
        let rel = hay_lower[search..].find("rust")?;
        let at = search + rel;
        let boundary_ok = at == 0
            || hay_lower.as_bytes()[at - 1].is_ascii_whitespace()
            || is_sep(text[..at].chars().next_back().unwrap_or('/'))
            || hay_lower.as_bytes()[at - 1] == b'"'
            || hay_lower.as_bytes()[at - 1] == b'\'';
        if boundary_ok {
            let after: Vec<char> = text[at + 4..].chars().collect();
            if !after.is_empty() && is_sep(after[0]) {
                for project in ["apps", "crates"] {
                    let candidate: String = after[1..]
                        .iter()
                        .take(project.len())
                        .collect::<String>()
                        .to_ascii_lowercase();
                    if candidate == project
                        && after.get(1 + project.len()).is_some_and(|c| is_sep(*c))
                    {
                        return Some(at);
                    }
                }
            }
        }
        search = at + 1;
    }
}

/// Normalizes backslashes to forward slashes in a relative path fragment.
fn normalize_separators(text: &str) -> String {
    text.replace('\\', "/")
}

/// Replaces any remaining absolute path with `<path>`: drive-letter paths, UNC paths,
/// and unix-absolute paths. A path token may contain spaces while it continues after a
/// separator (`C:\Program Files\Ocy\...`).
fn rewrite_absolute_paths(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if let Some(end) = absolute_path_end(&chars, i) {
            out.push_str(PLACEHOLDER_PATH);
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// When `chars[start..]` begins an absolute path, returns the exclusive end of the path
/// token (position of the next non-path char).
fn absolute_path_end(chars: &[char], start: usize) -> Option<usize> {
    let prev_is_boundary = start == 0
        || chars[start - 1].is_whitespace()
        || matches!(
            chars[start - 1],
            '"' | '\'' | '(' | '[' | '{' | ',' | ';' | '>' | '<' | '=' | '`'
        );
    if !prev_is_boundary {
        return None;
    }
    let c = chars[start];
    let windows_drive = chars.len() > start + 2
        && c.is_ascii_alphabetic()
        && chars[start + 1] == ':'
        && is_sep(chars[start + 2]);
    let unc = chars.len() > start + 2 && c == '\\' && chars[start + 1] == '\\';
    let unix_abs = c == '/' && chars.get(start + 1).is_some_and(|n| *n != '/');
    if !windows_drive && !unc && !unix_abs {
        return None;
    }
    let mut i = start;
    let mut last_was_sep = is_sep(c);
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            break;
        }
        if ch.is_whitespace() {
            if !last_was_sep && !space_continues_path(chars, i) {
                break;
            }
            // A space only continues the path right after a separator or when the next
            // segment still looks like path content (`C:\Program Files\Ocy\Ocy.exe`).
            i += 1;
            continue;
        }
        last_was_sep = is_sep(ch);
        i += 1;
    }
    Some(i)
}

/// `true` when the path-like content after the whitespace run at `space_at` contains a
/// path separator, so the whitespace sits inside a path token, not after it.
fn space_continues_path(chars: &[char], space_at: usize) -> bool {
    let mut i = space_at;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() && !chars[i].is_whitespace() {
        if is_sep(chars[i]) {
            return true;
        }
        i += 1;
    }
    false
}

// --- Report and event format ---

/// What crashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrashKind {
    /// A Rust panic (the payload and location are compiled in).
    Panic,
    /// A native crash (Windows: access violation, stack overflow, and similar).
    Native,
}

/// One stack frame. `function` is always present; unresolved frames use
/// `<module>+0x<offset>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashFrame {
    /// The resolved function name, or `<module>+0x<offset>` when symbols are missing.
    pub function: String,
    /// Source file name (scrubbed, repo- or crate-relative), when resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineno: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colno: Option<u32>,
    /// Module file name, like `Ocy.exe` (Windows native frames).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Whether the frame is Ocy's own code (not a dependency or the OS).
    pub in_app: bool,
}

impl CrashFrame {
    #[must_use]
    pub fn new(function: String, in_app: bool) -> Self {
        Self {
            function,
            filename: None,
            lineno: None,
            colno: None,
            module: None,
            in_app,
        }
    }
}

/// A local crash report: `<data>/crashes/<report_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    /// JSON schema version (currently `1`).
    pub schema_version: i32,
    /// Random UUID v4 report ID; never the usage-stats install ID.
    pub report_id: String,
    /// UTC unix seconds.
    pub crashed_at: i64,
    pub kind: CrashKind,
    /// Scrubbed panic payload or native exception description.
    pub message: String,
    /// Scrubbed panic location (file:line:col) or `unknown`.
    pub location: String,
    /// Scrubbed frames, innermost first.
    pub frames: Vec<CrashFrame>,
    pub app_version: String,
    pub platform: String,
    pub os_version: String,
    pub channel: String,
}

impl CrashReport {
    /// Builds a report with a fresh random UUID v4 and the environment tags gathered now.
    #[must_use]
    pub fn new(kind: CrashKind, crashed_at: i64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_id: Uuid::new_v4().to_string(),
            crashed_at,
            kind,
            message: String::new(),
            location: String::new(),
            frames: Vec::new(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            platform: platform_tag().to_owned(),
            os_version: os_version(),
            channel: channel_tag().to_owned(),
        }
    }

    /// Serializes the report, truncated to [`MAX_REPORT_BYTES`]: bottom (oldest) frames
    /// are dropped first; a pathological message is then cut at a char boundary.
    #[must_use]
    pub fn to_bounded_json(&self) -> String {
        let mut candidate = self.clone();
        loop {
            let json = serde_json::to_string(&candidate).unwrap_or_default();
            if json.len() <= MAX_REPORT_BYTES || candidate.frames.is_empty() {
                if json.len() <= MAX_REPORT_BYTES {
                    return json;
                }
                break;
            }
            candidate.frames.pop();
        }
        // Still too big: trim the message.
        loop {
            let json = serde_json::to_string(&candidate).unwrap_or_default();
            if json.len() <= MAX_REPORT_BYTES || candidate.message.is_empty() {
                return json;
            }
            let cut = candidate.message.len() / 2;
            let mut boundary = cut;
            while boundary > 0 && !candidate.message.is_char_boundary(boundary) {
                boundary -= 1;
            }
            candidate.message.truncate(boundary);
        }
    }
}

/// Names for the common native exception codes (anything else keeps its hex code).
#[must_use]
pub fn exception_code_name(code: u32) -> String {
    let name = match code {
        0x8000_0001 => "EXCEPTION_GUARD_PAGE",
        0x8000_0002 => "EXCEPTION_DATATYPE_MISALIGNMENT",
        0x8000_0003 => "EXCEPTION_BREAKPOINT",
        0x8000_0004 => "EXCEPTION_SINGLE_STEP",
        0xC000_0005 => "EXCEPTION_ACCESS_VIOLATION",
        0xC000_0006 => "EXCEPTION_IN_PAGE_ERROR",
        0xC000_001D => "EXCEPTION_ILLEGAL_INSTRUCTION",
        0xC000_0025 => "EXCEPTION_NONCONTINUABLE_EXCEPTION",
        0xC000_0026 => "EXCEPTION_INVALID_DISPOSITION",
        0xC000_008C => "EXCEPTION_ARRAY_BOUNDS_EXCEEDED",
        0xC000_008D => "EXCEPTION_FLT_DENORMAL_OPERAND",
        0xC000_008E => "EXCEPTION_FLT_DIVIDE_BY_ZERO",
        0xC000_0090 => "EXCEPTION_FLT_INVALID_OPERATION",
        0xC000_0091 => "EXCEPTION_FLT_OVERFLOW",
        0xC000_0093 => "EXCEPTION_FLT_UNDERFLOW",
        0xC000_0094 => "EXCEPTION_INT_DIVIDE_BY_ZERO",
        0xC000_0095 => "EXCEPTION_INT_OVERFLOW",
        0xC000_0096 => "EXCEPTION_PRIVILEGED_INSTRUCTION",
        0xC000_00FD => "EXCEPTION_STACK_OVERFLOW",
        0xC000_0374 => "STATUS_HEAP_CORRUPTION",
        0xC000_0409 => "STATUS_STACK_BUFFER_OVERRUN",
        _ => return format!("EXCEPTION_code_0x{code:08x}"),
    };
    name.to_owned()
}

/// The `$exception` event type: `panic`, the exception code name, or `native`.
#[must_use]
pub fn exception_type(report: &CrashReport) -> String {
    match report.kind {
        CrashKind::Panic => PANIC_EVENT_TYPE.to_owned(),
        CrashKind::Native => {
            if report.message.starts_with("EXCEPTION_") {
                report
                    .message
                    .split_whitespace()
                    .next()
                    .unwrap_or("native")
                    .to_owned()
            } else {
                "native".to_owned()
            }
        }
    }
}

/// The exact PostHog `$exception` event for this report (CR2a posts it inside the batch
/// wrapper only). Also the exact text CR3's "See what's in it" shows: see [`preview_text`].
#[must_use]
pub fn exception_event_json(report: &CrashReport) -> serde_json::Value {
    let frames: Vec<serde_json::Value> = report
        .frames
        .iter()
        .map(|frame| {
            let mut value = serde_json::Map::new();
            value.insert("platform".into(), "custom".into());
            value.insert("lang".into(), "rust".into());
            value.insert(
                "function".into(),
                serde_json::Value::String(frame.function.clone()),
            );
            if let Some(filename) = &frame.filename {
                value.insert(
                    "filename".into(),
                    serde_json::Value::String(filename.clone()),
                );
            }
            if let Some(lineno) = frame.lineno {
                value.insert("lineno".into(), serde_json::Value::from(lineno));
            }
            if let Some(colno) = frame.colno {
                value.insert("colno".into(), serde_json::Value::from(colno));
            }
            if let Some(module) = &frame.module {
                value.insert("module".into(), serde_json::Value::String(module.clone()));
            }
            value.insert("in_app".into(), serde_json::Value::Bool(frame.in_app));
            serde_json::Value::Object(value)
        })
        .collect();
    serde_json::json!({
        "event": "$exception",
        "distinct_id": report.report_id,
        "timestamp": event_timestamp(report.crashed_at),
        "properties": {
            "$process_person_profile": false,
            "$geoip_disable": true,
            "$lib": "ocy",
            "platform": report.platform,
            "os_version": report.os_version,
            "app_version": report.app_version,
            "channel": report.channel,
            "$exception_list": [{
                "type": exception_type(report),
                "value": report.message,
                "mechanism": {
                    "handled": false,
                    "synthetic": false,
                },
                "stacktrace": {
                    "type": "raw",
                    "frames": frames,
                },
            }],
        },
    })
}

/// The exact text that would be sent (pretty-printed), byte for byte what CR2a posts
/// (the sender adds only the outer `api_key`/batch wrapper).
#[must_use]
pub fn preview_text(report: &CrashReport) -> String {
    serde_json::to_string_pretty(&exception_event_json(report)).unwrap_or_default()
}

/// The event timestamp for unix seconds, PostHog's ISO-8601 form.
#[must_use]
pub fn event_timestamp(crashed_at: i64) -> String {
    jiff::Timestamp::from_second(crashed_at)
        .map(|ts| ts.to_string())
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

// --- Storage ---

/// Report storage under `<data_dir>/crashes/`. Pure enough to drive directly in tests.
#[derive(Debug, Clone)]
pub struct CrashStore {
    data_dir: PathBuf,
}

impl CrashStore {
    #[must_use]
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// The crashes directory.
    #[must_use]
    pub fn crashes_dir(&self) -> PathBuf {
        self.data_dir.join(CRASHES_DIR_NAME)
    }

    fn report_path(&self, report_id: &str) -> PathBuf {
        self.crashes_dir().join(format!("{report_id}.json"))
    }

    /// Writes the report atomically (tmp + rename) and enforces the limits: the 5-report
    /// cap and the crash-loop guard. Returns `false` when consent is `Never` (nothing is
    /// written) or the write fails.
    pub fn save_report(&self, report: &CrashReport) -> bool {
        if consent_is_never() {
            return false;
        }
        let dir = self.crashes_dir();
        if fs::create_dir_all(&dir).is_err() {
            return false;
        }
        let json = report.to_bounded_json();
        let path = self.report_path(&report.report_id);
        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp_path = PathBuf::from(tmp);
        if fs::write(&tmp_path, json.as_bytes()).is_err() {
            return false;
        }
        if fs::rename(&tmp_path, &path).is_err() {
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
        self.enforce_limits();
        true
    }

    /// Enforces the pending cap and the loop guard on the current on-disk reports.
    fn enforce_limits(&self) {
        let mut entries: Vec<(String, i64)> = self
            .ids_with_times()
            .into_iter()
            .map(|(id, _, at)| (id, at))
            .collect();
        entries.sort_unstable_by_key(|(_, at)| *at);
        let mut to_delete = drop_beyond_pending_cap(&entries);
        to_delete.extend(drop_loop_guard(&entries));
        for id in to_delete {
            let _ = fs::remove_file(self.report_path(&id));
        }
    }

    /// Lists pending reports, oldest first: expired and corrupt files are deleted, never
    /// propagated as errors.
    #[must_use]
    pub fn pending_reports(&self) -> Vec<CrashReport> {
        self.pending_reports_at(now_unix())
    }

    /// [`CrashStore::pending_reports`] with an injectable `now` (tests).
    #[must_use]
    pub fn pending_reports_at(&self, now: i64) -> Vec<CrashReport> {
        let mut reports = Vec::new();
        for (id, report, at) in self.ids_with_times() {
            if now - at > MAX_AGE_SECS {
                let _ = fs::remove_file(self.report_path(&id));
                continue;
            }
            reports.push(report);
        }
        reports.sort_unstable_by_key(|report| report.crashed_at);
        reports
    }

    /// Deletes one pending report by ID.
    pub fn delete(&self, report_id: &str) -> bool {
        if Uuid::parse_str(report_id).is_err() {
            return false; // ID-shaped paths only; never traverse arbitrary input.
        }
        fs::remove_file(self.report_path(report_id)).is_ok()
    }

    /// Deletes every pending report. Returns how many were deleted.
    pub fn delete_all(&self) -> usize {
        let mut deleted = 0usize;
        if let Ok(entries) = fs::read_dir(self.crashes_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "json")
                    && fs::remove_file(&path).is_ok()
                {
                    deleted += 1;
                }
            }
        }
        deleted
    }

    /// (file id, parsed report, crashed_at) of every parseable report file; corrupt files
    /// are deleted.
    fn ids_with_times(&self) -> Vec<(String, CrashReport, i64)> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(self.crashes_dir()) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            match fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<CrashReport>(&raw).ok())
            {
                Some(report) => {
                    let crashed_at = report.crashed_at;
                    out.push((id, report, crashed_at));
                }
                None => {
                    // Corrupt report: delete, never crash.
                    let _ = fs::remove_file(&path);
                }
            }
        }
        out
    }
}

/// Returns the IDs beyond the [`MAX_PENDING_REPORTS`] cap (the oldest first-sorted ones).
fn drop_beyond_pending_cap(entries: &[(String, i64)]) -> Vec<String> {
    if entries.len() > MAX_PENDING_REPORTS {
        entries[..entries.len() - MAX_PENDING_REPORTS]
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    } else {
        Vec::new()
    }
}

/// Crash-loop guard: a burst of more than [`LOOP_MAX_REPORTS`] reports inside
/// [`LOOP_WINDOW_SECS`] keeps only its first report; the IDs of the dropped reports are
/// returned. A burst extends while each report is within the window of the burst's first.
fn drop_loop_guard(entries: &[(String, i64)]) -> Vec<String> {
    let mut dropped = Vec::new();
    let mut i = 0usize;
    while i < entries.len() {
        let burst_at = entries[i].1;
        let mut j = i + 1;
        while j < entries.len() && entries[j].1 - burst_at <= LOOP_WINDOW_SECS {
            j += 1;
        }
        let burst_len = j - i;
        let keep = if burst_len > LOOP_MAX_REPORTS {
            1
        } else {
            burst_len
        };
        dropped.extend(entries[i + keep..j].iter().map(|(id, _)| id.clone()));
        i = j;
    }
    dropped
}

// --- Backtrace parsing ---

/// Parses the `Debug` text of a captured backtrace into frames. Unresolved frames (bare
/// addresses) are mapped to `<module>+0x<offset>` through `module_for_address` when
/// available, otherwise the address is kept.
#[must_use]
pub fn parse_backtrace_text(
    text: &str,
    module_for_address: impl Fn(u64) -> Option<(String, u64)>,
) -> Vec<CrashFrame> {
    let lines: Vec<&str> = text.lines().collect();
    let mut frames = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(function) = split_frame_line(lines[i]) else {
            i += 1;
            continue;
        };
        let mut filename = None;
        let mut lineno = None;
        let mut colno = None;
        // The optional following "at file:line:col" line.
        if let Some(next) = lines.get(i + 1)
            && let Some(at) = next.trim_start().strip_prefix("at ")
            && let Some((file, line, col)) = split_file_line_col(at.trim())
        {
            filename = Some(file);
            lineno = Some(line);
            colno = Some(col);
            i += 1;
        }
        let is_address = function
            .strip_prefix("0x")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit()));
        let frame = if is_address {
            let address = u64::from_str_radix(&function[2..], 16).unwrap_or(0);
            match module_for_address(address) {
                Some((module, base)) => {
                    let mut frame =
                        CrashFrame::new(format!("{module}+0x{:x}", address - base), false);
                    frame.module = Some(module);
                    frame
                }
                None => CrashFrame::new(function, false),
            }
        } else {
            let in_app = filename.as_deref().is_some_and(looks_in_app);
            let mut frame = CrashFrame::new(function, in_app);
            frame.filename = filename;
            frame.lineno = lineno;
            frame.colno = colno;
            frame
        };
        frames.push(frame);
        i += 1;
    }
    frames
}

/// Splits `   12: <function>` into the function text. Numbered backtrace frames only.
fn split_frame_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let colon = trimmed.find(':')?;
    let number = &trimmed[..colon];
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(trimmed[colon + 1..].trim().to_owned())
}

/// `true` when the file lives in the workspace's `apps` or `crates` trees (either
/// separator, absolute or repo-relative): Ocy's own code, not a dependency's.
fn looks_in_app(file: &str) -> bool {
    file.to_ascii_lowercase()
        .split(['/', '\\'])
        .any(|segment| segment == "apps" || segment == "crates")
}

/// Splits `path/file.rs:12:34` into (file, line, col).
fn split_file_line_col(text: &str) -> Option<(String, u32, u32)> {
    let mut parts = text.rsplit(':');
    let col: u32 = parts.next()?.parse().ok()?;
    let line: u32 = parts.next()?.parse().ok()?;
    let file = parts.collect::<Vec<&str>>().join(":");
    if file.is_empty() {
        return None;
    }
    Some((file, line, col))
}

// --- Panic capture ---

/// The panic payload as text (same shape as damt-app's private helper).
fn panic_payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        String::from("non-string panic payload")
    }
}

/// Builds the report for a panic, scrubbed, without writing it.
#[must_use]
fn panic_report(
    info: &std::panic::PanicHookInfo<'_>,
    scrubber: &Scrubber,
    now: i64,
) -> CrashReport {
    let mut report = CrashReport::new(CrashKind::Panic, now);
    report.message = scrubber.scrub(&panic_payload_text(info.payload()));
    report.location = match info.location() {
        Some(location) => scrubber.scrub(&format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )),
        None => "unknown".to_owned(),
    };
    let backtrace = std::backtrace::Backtrace::force_capture();
    let raw = format!("{backtrace:?}");
    let frames = parse_backtrace_text(&raw, module_for_address);
    report.frames = frames
        .into_iter()
        .map(|mut frame| {
            frame.function = scrubber.scrub(&frame.function);
            if let Some(filename) = &frame.filename {
                frame.filename = Some(scrubber.scrub(filename));
            }
            if let Some(module) = &frame.module {
                frame.module = Some(scrubber.scrub(module));
            }
            frame
        })
        .collect();
    report
}

/// Installs the panic hook that chains with the previous one (the damt-app error-log hook):
/// this hook writes the local crash report first, then calls the previous hook, so the
/// damt-app error-log line is still written.
pub fn install_panic_hook(store: CrashStore) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let scrubber = global_scrubber();
        let now = now_unix();
        let report = panic_report(info, &scrubber, now);
        store.save_report(&report);
        previous(info);
    }));
}

// --- Native raw records (written by the Windows exception handler; converted on start) ---

/// Appends one native raw record line: `C:0x<code> A:0x<addr> F:<frames>`.
#[must_use]
pub fn native_record_line(code: u32, address: u64, frames: &[String]) -> String {
    let mut line = format!("C:0x{code:x} A:0x{address:x} F:");
    for (i, frame) in frames.iter().enumerate() {
        if i > 0 {
            line.push(';');
        }
        line.push_str(frame);
    }
    line
}

/// Converts pending native raw records into reports and deletes the raw file. Returns how
/// many reports were stored. A malformed line is skipped, never a crash.
#[must_use]
pub fn convert_native_raw(store: &CrashStore, scrubber: &Scrubber, now: i64) -> usize {
    let path = store.crashes_dir().join(NATIVE_RAW_FILE_NAME);
    let Ok(raw) = fs::read_to_string(&path) else {
        return 0;
    };
    let _ = fs::remove_file(&path);
    let mut stored = 0usize;
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let Some((code, address, frames)) = parse_native_record(line) else {
            continue;
        };
        let mut report = CrashReport::new(CrashKind::Native, now);
        report.message = scrubber.scrub(&format!(
            "{} (0x{code:x}) at address 0x{address:x}",
            exception_code_name(code)
        ));
        report.location = "unknown".to_owned();
        report.frames = frames
            .iter()
            .map(|frame| {
                let (module, _) = frame
                    .split_once("+0x")
                    .map(|(module, _)| (module, ()))
                    .unwrap_or((frame.as_str(), ()));
                let mut parsed = CrashFrame::new(frame.clone(), false);
                parsed.module = Some(scrubber.scrub(module));
                parsed
            })
            .collect();
        if store.save_report(&report) {
            stored += 1;
        }
    }
    stored
}

/// Parses `C:0x<code> A:0x<addr> F:<frame>;<frame>`.
fn parse_native_record(line: &str) -> Option<(u32, u64, Vec<String>)> {
    let code = parse_field(line, "C:")?;
    let address = parse_field(line, "A:")?;
    let frames_start = line.find("F:")?;
    let frames: Vec<String> = line[frames_start + 2..]
        .split(';')
        .filter(|frame| !frame.is_empty())
        .map(str::to_owned)
        .collect();
    Some((
        u32::from_str_radix(&code, 16).ok()?,
        u64::from_str_radix(&address, 16).ok()?,
        frames,
    ))
}

/// The hex value of the `PREFIX:0x...` field.
fn parse_field(line: &str, prefix: &str) -> Option<String> {
    let at = line.find(prefix)? + prefix.len();
    let rest = line[at..].trim_start_matches("0x");
    let end = rest
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(rest.len(), |(i, _)| i);
    Some(rest[..end].to_owned())
}

// --- Process-wide wiring (like usage_stats) ---

static STORE: OnceLock<CrashStore> = OnceLock::new();
static SCRUBBER: RwLock<Option<Arc<Scrubber>>> = RwLock::new(None);

/// Current unix seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

/// The current scrubber, built lazily from the environment plus the data dir.
fn global_scrubber() -> Arc<Scrubber> {
    if let Some(existing) = SCRUBBER.read().ok().and_then(|guard| guard.clone()) {
        return existing;
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let data_dir = STORE
        .get()
        .map(|store| store.data_dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fresh = Arc::new(Scrubber::new(&home, &data_dir, &[]));
    let mut guard = SCRUBBER
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get_or_insert_with(|| Arc::clone(&fresh)).clone()
}

/// Replaces the scrubber with one that also knows the silent-game names.
fn refresh_scrubber_from_settings(settings: &OcySettings) {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let data_dir = STORE
        .get()
        .map(|store| store.data_dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fresh = Arc::new(Scrubber::new(&home, &data_dir, &settings.games_silent));
    let mut guard = SCRUBBER
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(fresh);
}

/// App-start wiring: installs the process-wide store, the scrubber, and the panic hook
/// (chained after the damt-app hook the app installs first). On Windows also installs the
/// native-crash raw writer ([`crate::crash_reports`] native module).
pub fn install(data_dir: impl Into<PathBuf>) {
    let store = CrashStore::new(data_dir);
    let _ = STORE.set(store.clone());
    install_panic_hook(store.clone());
    crate::crash_reports::native::install(store);
}

/// Host hook: applies the crash-report consent from the in-memory `OcySettings` (the single
/// source of truth, persisted through `AppHost::save_settings`). On `Never`, all pending
/// reports are deleted and the hooks stop writing. Also refreshes the scrubber's known
/// values from the settings (silent-game names).
pub fn apply_crash_consent_from_settings(settings: &OcySettings) {
    CONSENT.store(consent_to_u8(settings.crash_reports), Ordering::Release);
    refresh_scrubber_from_settings(settings);
    if settings.crash_reports == CrashReportsConsent::Never
        && let Some(store) = STORE.get()
    {
        store.delete_all();
    }
}

/// Pending reports, oldest first. Corrupt and expired files are cleaned up.
#[must_use]
pub fn pending_reports() -> Vec<CrashReport> {
    match STORE.get() {
        Some(store) => store.pending_reports(),
        None => Vec::new(),
    }
}

/// Deletes one pending report by ID. Returns `false` when the store is not installed.
pub fn delete(report_id: &str) -> bool {
    STORE.get().is_some_and(|store| store.delete(report_id))
}

/// Deletes all pending reports. Returns how many were deleted.
pub fn delete_all() -> usize {
    STORE.get().map_or(0, CrashStore::delete_all)
}

/// Converts pending native raw records into reports (call once after [`install`]).
#[must_use]
pub fn convert_native_pending() -> usize {
    match STORE.get() {
        Some(store) => {
            let scrubber = global_scrubber();
            convert_native_raw(store, &scrubber, now_unix())
        }
        None => 0,
    }
}

// --- Windows native capture ---

#[cfg(target_os = "windows")]
pub mod native;

#[cfg(not(target_os = "windows"))]
pub mod native {
    //! Non-Windows: panics only; native crashes are WER's (Store) or the OS's business.

    use super::CrashStore;

    /// No-op on this platform.
    pub fn install(_store: CrashStore) {}

    /// No module table on this platform.
    #[must_use]
    pub fn module_for_address(_address: u64) -> Option<(String, u64)> {
        None
    }
}

/// Module name and base address for a code address (Windows module table), or `None`.
#[cfg(target_os = "windows")]
#[must_use]
pub fn module_for_address(address: u64) -> Option<(String, u64)> {
    native::module_for_address(address)
}

#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn module_for_address(_address: u64) -> Option<(String, u64)> {
    None
}

// --- Test trigger (cargo feature `crash-test`; off by default, never in release artifacts
// unless the build explicitly asks for it) ---
#[cfg(feature = "crash-test")]
/// The `--crash-test panic|native` trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashTestKind {
    Panic,
    Native,
}

#[cfg(feature = "crash-test")]
/// Parses `--crash-test panic` / `--crash-test=panic` (and `native`).
#[must_use]
pub fn parse_crash_test(args: &[String]) -> Option<CrashTestKind> {
    for (i, arg) in args.iter().enumerate() {
        let value = if arg == "--crash-test" {
            match args.get(i + 1) {
                Some(v) => v.as_str(),
                None => return None,
            }
        } else {
            match arg.strip_prefix("--crash-test=") {
                Some(v) => v,
                None => continue,
            }
        };
        return match value.trim() {
            "panic" => Some(CrashTestKind::Panic),
            "native" => Some(CrashTestKind::Native),
            _ => None,
        };
    }
    None
}

#[cfg(feature = "crash-test")]
/// Runs the requested crash. `panic` exercises the hook (then aborts in release); `native`
/// faults for the unhandled-exception filter (Windows; elsewhere it is a plain SIGSEGV).
///
/// # Panics
/// Deliberately panics for [`CrashTestKind::Panic`]; the native variant is unsafe code.
pub fn run_crash_test(kind: CrashTestKind) -> ! {
    match kind {
        CrashTestKind::Panic => panic!("crash-test: deliberate panic for the crash-report hook"),
        CrashTestKind::Native => {
            // A real access violation: a volatile read of null. Release or debug alike,
            // the OS faults; on Windows the unhandled-exception filter writes the raw
            // record before the process dies.
            unsafe {
                let null: *const u8 = std::ptr::null();
                std::ptr::read_volatile(null);
            }
            // If the read somehow did not fault, do not continue the run.
            std::process::abort();
        }
    }
}

/// Asserts that none of the `forbidden` tokens (case-insensitive) appear in `text`;
/// checked by tests on scrubbed output.
#[cfg(test)]
pub(crate) fn assert_no_forbidden(text: &str, forbidden: &[&str]) {
    let lower = text.to_ascii_lowercase();
    for token in forbidden {
        assert!(
            !lower.contains(&token.to_ascii_lowercase()),
            "forbidden token {token:?} still present in {text:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocy_core::settings::OcySettings;
    use std::sync::Mutex;

    /// Serializes tests that touch the global consent mirror (the mirror is process-wide).
    static CONSENT_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("ocy-crash-{name}-{}-{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The scrubber with sentinel runtime values: a fake user, computer name, home and
    /// data folder, and two silent-game names.
    fn sentinel_scrubber() -> Scrubber {
        Scrubber::with_values(
            r"C:\Users\alice",
            r"C:\Users\alice\AppData\Roaming\OcyCrashTest",
            &["Solitaire Deluxe".to_owned(), "Minesweeper X".to_owned()],
            "alice",
            "DESKTOP-FOX1",
        )
    }

    fn set_consent(consent: CrashReportsConsent) {
        CONSENT.store(consent_to_u8(consent), Ordering::Release);
    }

    fn fixed_report(scrubber: &Scrubber) -> CrashReport {
        let mut report = CrashReport {
            schema_version: SCHEMA_VERSION,
            report_id: "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0".to_owned(),
            crashed_at: 1_700_000_000,
            kind: CrashKind::Panic,
            message: String::new(),
            location: String::new(),
            frames: Vec::new(),
            app_version: "0.3.0-test".to_owned(),
            platform: "windows".to_owned(),
            os_version: "10.0.22621".to_owned(),
            channel: "direct".to_owned(),
        };
        report.message = scrubber.scrub(
            "crash while Minesweeper X was running for alice: \
             C:\\Users\\alice\\AppData\\Roaming\\OcyCrashTest\\games.json missing",
        );
        report.location = scrubber.scrub("apps/ocy/ocy-app/src/crash_reports.rs:42:7");
        let mut own = CrashFrame::new("ocy_app::crash_reports::panic_report".to_owned(), true);
        own.filename = Some("apps/ocy/ocy-app/src/crash_reports.rs".to_owned());
        own.lineno = Some(42);
        own.colno = Some(7);
        let dependency = CrashFrame::new(
            scrubber.scrub(
                "serde_json at C:\\Users\\alice\\.cargo\\registry\\src\\index-1b3f4e17c\\serde_json-1.0.100\\src\\value.rs",
            ),
            false,
        );
        let mut native_style = CrashFrame::new("Ocy.exe+0x1a2b3c".to_owned(), false);
        native_style.module = Some("Ocy.exe".to_owned());
        report.frames = vec![own, dependency, native_style];
        report
    }

    #[test]
    fn golden_event_json_scrubs_every_sentinel() {
        let scrubber = sentinel_scrubber();
        let report = fixed_report(&scrubber);
        let event = exception_event_json(&report);
        let expected = serde_json::json!({
            "event": "$exception",
            "distinct_id": "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0",
            "timestamp": event_timestamp(1_700_000_000),
            "properties": {
                "$process_person_profile": false,
                "$geoip_disable": true,
                "$lib": "ocy",
                "platform": "windows",
                "os_version": "10.0.22621",
                "app_version": "0.3.0-test",
                "channel": "direct",
                "$exception_list": [{
                    "type": "panic",
                    "value": "crash while <game> was running for <user>: <data>\\games.json missing",
                    "mechanism": { "handled": false, "synthetic": false },
                    "stacktrace": {
                        "type": "raw",
                        "frames": [
                            {
                                "platform": "custom",
                                "lang": "rust",
                                "function": "ocy_app::crash_reports::panic_report",
                                "filename": "apps/ocy/ocy-app/src/crash_reports.rs",
                                "lineno": 42,
                                "colno": 7,
                                "in_app": true,
                            },
                            {
                                "platform": "custom",
                                "lang": "rust",
                                "function": "serde_json at serde_json-1.0.100/src/value.rs",
                                "in_app": false,
                            },
                            {
                                "platform": "custom",
                                "lang": "rust",
                                "function": "Ocy.exe+0x1a2b3c",
                                "module": "Ocy.exe",
                                "in_app": false,
                            },
                        ],
                    },
                }],
            },
        });
        assert_eq!(event, expected);
        // preview_text is byte for byte the pretty-printed event.
        assert_eq!(
            preview_text(&report),
            serde_json::to_string_pretty(&expected).unwrap_or_default()
        );
        // None of the injected personal values survive anywhere in the preview.
        assert_no_forbidden(
            &preview_text(&report),
            &[
                "alice",
                "DESKTOP-FOX1",
                "Solitaire",
                "Minesweeper",
                "OcyCrashTest",
                "Users",
                "AppData",
            ],
        );
    }

    #[test]
    fn event_timestamp_is_iso8601_utc() {
        assert_eq!(event_timestamp(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn scrubber_replaces_known_runtime_values() {
        let scrubber = sentinel_scrubber();
        assert_eq!(scrubber.scrub("no state for alice"), "no state for <user>");
        assert_eq!(
            scrubber.scrub("cannot reach DESKTOP-FOX1"),
            "cannot reach <host>"
        );
        assert_eq!(scrubber.scrub("in C:\\Users\\alice"), "in <home>");
        // The data folder is more specific than the home folder and wins.
        assert_eq!(
            scrubber.scrub("C:\\Users\\alice\\AppData\\Roaming\\OcyCrashTest"),
            "<data>"
        );
        assert_eq!(scrubber.scrub("Solitaire Deluxe did it"), "<game> did it");
        assert_eq!(scrubber.scrub("Minesweeper X"), "<game>");
    }

    #[test]
    fn scrubber_replaces_generic_windows_paths() {
        let scrubber = sentinel_scrubber();
        assert_eq!(
            scrubber.scrub("cannot open C:\\Users\\bob\\notes.txt"),
            "cannot open <path>"
        );
        assert_eq!(
            scrubber.scrub("cannot open C:\\Program Files\\Ocy\\Ocy.exe"),
            "cannot open <path>"
        );
        assert_eq!(
            scrubber.scrub(r"failed at \\?\C:\Users\bob\data\settings.json"),
            "failed at <path>"
        );
        assert_eq!(
            scrubber.scrub("share \\\\fileserver\\public"),
            "share <path>"
        );
    }

    #[test]
    fn scrubber_replaces_generic_unix_paths() {
        let scrubber = sentinel_scrubber();
        assert_eq!(scrubber.scrub("read /home/bob/notes.txt"), "read <path>");
        assert_eq!(scrubber.scrub("read /Users/bob/notes.txt"), "read <path>");
        assert_eq!(scrubber.scrub("stat /var/log failed"), "stat <path> failed");
        assert_eq!(scrubber.scrub("gone to /home/bob"), "gone to <path>");
    }

    #[test]
    fn scrubber_rewrites_cargo_registry_paths() {
        let scrubber = sentinel_scrubber();
        assert_eq!(
            scrubber.scrub(
                "/home/bob/.cargo/registry/src/index.crates.io-6f3d5bb3/serde-1.2.3/src/de.rs"
            ),
            "serde-1.2.3/src/de.rs"
        );
        assert_eq!(
            scrubber.scrub(
                "C:\\Users\\bob\\.cargo\\registry\\src\\index-6f3d5bb3\\serde-1.2.3\\src\\de.rs"
            ),
            "serde-1.2.3/src/de.rs"
        );
    }

    #[test]
    fn scrubber_rewrites_workspace_paths_to_repo_relative() {
        let scrubber = sentinel_scrubber();
        assert_eq!(
            scrubber.scrub("at /build/agent/rust/apps/ocy/ocy-app/src/host.rs:9"),
            "at apps/ocy/ocy-app/src/host.rs:9"
        );
        assert_eq!(
            scrubber.scrub(r"in D:\code\ocy\rust\crates\damt-app\src\pure.rs"),
            "in crates/damt-app/src/pure.rs"
        );
    }

    #[test]
    fn scrubber_leaves_plain_text_alone() {
        let scrubber = sentinel_scrubber();
        assert_eq!(
            scrubber.scrub("index out of bounds: the len is 3"),
            "index out of bounds: the len is 3"
        );
        assert_eq!(scrubber.scrub(""), "");
    }

    // --- Consent ---

    #[test]
    fn consent_never_writes_nothing() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Never);
        let dir = temp_dir("consent-never");
        let store = CrashStore::new(&dir);
        let report = CrashReport::new(CrashKind::Panic, now_unix());
        assert!(!store.save_report(&report));
        assert!(!store.crashes_dir().exists());
        set_consent(CrashReportsConsent::Ask);
    }

    #[test]
    fn consent_ask_and_always_write() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = temp_dir("consent-ask-always");
        let store = CrashStore::new(&dir);
        for consent in [CrashReportsConsent::Ask, CrashReportsConsent::Always] {
            set_consent(consent);
            let report = CrashReport::new(CrashKind::Panic, now_unix());
            assert!(store.save_report(&report));
            assert!(store.report_path(&report.report_id).exists());
        }
        set_consent(CrashReportsConsent::Ask);
    }

    #[test]
    fn consent_never_deletes_pending_reports() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = temp_dir("consent-never-deletes");
        // Full wiring: store, scrubber, hooks (the panic hook is harmless in tests).
        install(&dir);
        let store = CrashStore::new(&dir);
        set_consent(CrashReportsConsent::Ask);
        assert!(store.save_report(&CrashReport::new(CrashKind::Panic, now_unix())));
        assert_eq!(store.pending_reports().len(), 1);
        let settings = OcySettings {
            crash_reports: CrashReportsConsent::Never,
            ..OcySettings::default()
        };
        apply_crash_consent_from_settings(&settings);
        assert_eq!(consent(), CrashReportsConsent::Never);
        assert!(store.pending_reports().is_empty());
        // The mirror blocks further writes too.
        assert!(!store.save_report(&CrashReport::new(CrashKind::Panic, now_unix())));
        set_consent(CrashReportsConsent::Ask);
    }

    // --- Storage limits ---

    fn spaced_reports(count: usize, start: i64, step_secs: i64) -> Vec<CrashReport> {
        (0..count)
            .map(|i| {
                let mut report = CrashReport::new(CrashKind::Panic, start + i as i64 * step_secs);
                report.message = format!("failure #{i}");
                report
            })
            .collect()
    }

    #[test]
    fn pending_cap_keeps_at_most_five_reports() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("cap-five");
        let store = CrashStore::new(&dir);
        // Spaced 20 minutes apart so the crash-loop guard stays out of the way, ending now.
        let now = now_unix();
        let reports = spaced_reports(7, now - 6 * 20 * 60, 20 * 60);
        for report in &reports {
            assert!(store.save_report(report));
        }
        let pending = store.pending_reports();
        assert_eq!(pending.len(), MAX_PENDING_REPORTS);
        // The oldest two are gone; the survivors are the newest five in order.
        for (i, report) in pending.iter().enumerate() {
            assert_eq!(report.message, format!("failure #{}", i + 2));
        }
    }

    #[test]
    fn report_is_truncated_to_32_kb() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("cap-32kb");
        let store = CrashStore::new(&dir);
        let mut report = CrashReport::new(CrashKind::Panic, 1_700_000_000);
        report.message = "x".repeat(64 * 1024);
        for i in 0..40 {
            report
                .frames
                .push(CrashFrame::new(format!("frame #{i}"), true));
        }
        assert!(store.save_report(&report));
        let path = store.report_path(&report.report_id);
        let size = fs::metadata(&path)
            .map(|meta| meta.len())
            .unwrap_or_default();
        assert!(
            (size as usize) <= MAX_REPORT_BYTES,
            "report is {size} bytes, limit is {MAX_REPORT_BYTES}"
        );
        // The truncated file is still valid JSON.
        let raw = fs::read_to_string(&path).unwrap_or_default();
        assert!(serde_json::from_str::<CrashReport>(&raw).is_ok());
        // The in-memory bounding agrees.
        assert!(report.to_bounded_json().len() <= MAX_REPORT_BYTES);
    }

    #[test]
    fn reports_older_than_30_days_expire() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("expiry-30d");
        let store = CrashStore::new(&dir);
        let now = now_unix();
        let old = CrashReport::new(CrashKind::Panic, now - MAX_AGE_SECS - 60);
        let fresh = CrashReport::new(CrashKind::Panic, now - 60);
        assert!(store.save_report(&old));
        assert!(store.save_report(&fresh));
        let pending = store.pending_reports_at(now);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].report_id, fresh.report_id);
        // The expired file is gone, not just hidden.
        assert!(!store.report_path(&old.report_id).exists());
    }

    #[test]
    fn crash_loop_guard_keeps_only_the_first_of_a_burst() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("loop-guard");
        let store = CrashStore::new(&dir);
        let now = now_unix();
        // Three reports inside 10 minutes are a rough patch, not a loop: all stay.
        for report in spaced_reports(3, now, 60) {
            assert!(store.save_report(&report));
        }
        assert_eq!(store.pending_reports().len(), 3);
        // The fourth inside the window turns it into a loop: only the first survives.
        for report in spaced_reports(1, now + 3 * 60, 60) {
            assert!(store.save_report(&report));
        }
        let pending = store.pending_reports();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].message, "failure #0");
    }

    #[test]
    fn corrupt_report_file_is_deleted_not_propagated() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("corrupt-file");
        let store = CrashStore::new(&dir);
        let good = CrashReport::new(CrashKind::Panic, now_unix());
        assert!(store.save_report(&good));
        fs::write(
            store
                .crashes_dir()
                .join("deadbeef-cafe-4bad-8bad-1feedfacebeef.json"),
            "not json at all",
        )
        .expect("write corrupt file");
        let pending = store.pending_reports();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].report_id, good.report_id);
        assert!(
            !store
                .crashes_dir()
                .join("deadbeef-cafe-4bad-8bad-1feedfacebeef.json")
                .exists()
        );
    }

    #[test]
    fn delete_removes_one_report_and_rejects_odd_ids() {
        let dir = temp_dir("delete-one");
        let store = CrashStore::new(&dir);
        fs::create_dir_all(store.crashes_dir()).expect("create crashes dir");
        let report = fixed_report(&sentinel_scrubber());
        fs::write(
            store.report_path(&report.report_id),
            serde_json::to_string(&report).unwrap_or_default(),
        )
        .expect("write report");
        // Path traversal and non-UUID names are refused.
        assert!(!store.delete("../../settings.json"));
        assert!(!store.delete("not-a-uuid"));
        assert!(store.delete(&report.report_id));
        assert!(!store.report_path(&report.report_id).exists());
        assert_eq!(store.delete_all(), 0);
    }

    // --- Backtrace parsing and native records ---

    #[test]
    fn parse_backtrace_resolves_locations_and_in_app() {
        let text = "\
   0: ocy_app::crash_reports::capture
             at /build/agent/rust/apps/ocy/ocy-app/src/crash_reports.rs:42:7
   1: serde_json::value::Value::serialize
             at /home/bob/.cargo/registry/src/index-6f3d5bb3/serde_json-1.0.100/src/value.rs:1:2
   2: 0x7ff6abc12340";
        let frames = parse_backtrace_text(text, |address| {
            assert_eq!(address, 0x7ff6abc12340);
            Some(("Ocy.exe".to_owned(), 0x7ff600000000))
        });
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].function, "ocy_app::crash_reports::capture");
        assert_eq!(
            frames[0].filename.as_deref(),
            Some("/build/agent/rust/apps/ocy/ocy-app/src/crash_reports.rs")
        );
        assert_eq!(frames[0].lineno, Some(42));
        assert!(frames[0].in_app);
        // A dependency's source is not in-app.
        assert!(!frames[1].in_app);
        // Unresolved addresses become module + offset.
        assert_eq!(frames[2].function, "Ocy.exe+0xabc12340");
        assert_eq!(frames[2].module.as_deref(), Some("Ocy.exe"));
        assert!(!frames[2].in_app);
    }

    #[test]
    fn native_record_round_trips_into_a_report() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("native-round-trip");
        let store = CrashStore::new(&dir);
        let line = native_record_line(
            0xC000_0005,
            0x0000_7ffd_e123_4567,
            &[
                "Ocy.exe+0x1a2b3c".to_owned(),
                "KERNELBASE.dll+0x2c1e".to_owned(),
            ],
        );
        fs::create_dir_all(store.crashes_dir()).expect("create crashes dir");
        fs::write(store.crashes_dir().join(NATIVE_RAW_FILE_NAME), line).expect("write raw");
        let stored = convert_native_raw(&store, &sentinel_scrubber(), now_unix());
        assert_eq!(stored, 1);
        // The raw file is consumed.
        assert!(!store.crashes_dir().join(NATIVE_RAW_FILE_NAME).exists());
        let pending = store.pending_reports();
        assert_eq!(pending.len(), 1);
        let report = &pending[0];
        assert_eq!(report.kind, CrashKind::Native);
        assert_eq!(exception_type(report), "EXCEPTION_ACCESS_VIOLATION");
        assert_eq!(report.location, "unknown");
        assert_eq!(report.frames.len(), 2);
        assert_eq!(report.frames[0].module.as_deref(), Some("Ocy.exe"));
        let event = exception_event_json(report);
        assert_eq!(
            event["properties"]["$exception_list"][0]["type"],
            "EXCEPTION_ACCESS_VIOLATION"
        );
        assert_eq!(
            event["properties"]["$exception_list"][0]["stacktrace"]["frames"][0]["function"],
            "Ocy.exe+0x1a2b3c"
        );
    }

    #[test]
    fn native_record_malformed_lines_are_skipped() {
        let dir = temp_dir("native-malformed");
        let store = CrashStore::new(&dir);
        fs::create_dir_all(store.crashes_dir()).expect("create crashes dir");
        fs::write(
            store.crashes_dir().join(NATIVE_RAW_FILE_NAME),
            "garbage without fields\n\nC:0xc0000005 A:0x1234 F:Ocy.exe+0x1\n",
        )
        .expect("write raw");
        let stored = convert_native_raw(&store, &sentinel_scrubber(), 1_700_000_000);
        assert_eq!(stored, 1);
    }

    #[test]
    fn exception_code_names() {
        assert_eq!(
            exception_code_name(0xC000_0005),
            "EXCEPTION_ACCESS_VIOLATION"
        );
        assert_eq!(exception_code_name(0xC000_00FD), "EXCEPTION_STACK_OVERFLOW");
        assert_eq!(
            exception_code_name(0x1234_5678),
            "EXCEPTION_code_0x12345678"
        );
    }

    // --- Panic hook chaining ---

    #[test]
    fn panic_hook_chains_with_the_damt_app_hook() {
        let _guard = CONSENT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        set_consent(CrashReportsConsent::Ask);
        let dir = temp_dir("hook-chain");
        let log = damt_app::ErrorLog::new(&dir, "ocy");
        damt_app::install_panic_hook(log);
        install_panic_hook(CrashStore::new(&dir));
        let result = std::panic::catch_unwind(|| {
            panic!("hook-chain: deliberate panic for the crash-report hook");
        });
        assert!(result.is_err());
        // The damt-app error-log line is still written (the hooks chain).
        let log_text = fs::read_to_string(dir.join("ocy-errors.log")).unwrap_or_default();
        assert!(log_text.contains("hook-chain: deliberate panic"));
        // The crash report was written too.
        let pending = CrashStore::new(&dir).pending_reports();
        assert_eq!(pending.len(), 1);
        let report = &pending[0];
        assert_eq!(report.kind, CrashKind::Panic);
        assert!(report.message.contains("hook-chain: deliberate panic"));
        assert!(report.location.contains("crash_reports.rs"));
        assert_eq!(report.schema_version, SCHEMA_VERSION);
    }

    #[cfg(feature = "crash-test")]
    mod crash_test_trigger {
        use super::super::*;

        fn args(values: &[&str]) -> Vec<String> {
            values.iter().map(|value| (*value).to_owned()).collect()
        }

        #[test]
        fn parses_both_forms() {
            assert_eq!(
                parse_crash_test(&args(&["ocy", "--crash-test", "panic"])),
                Some(CrashTestKind::Panic)
            );
            assert_eq!(
                parse_crash_test(&args(&["ocy", "--crash-test=native"])),
                Some(CrashTestKind::Native)
            );
        }

        #[test]
        fn finds_the_flag_after_other_args() {
            assert_eq!(
                parse_crash_test(&args(&["ocy", "--autostart", "--crash-test=panic"])),
                Some(CrashTestKind::Panic)
            );
            assert_eq!(parse_crash_test(&args(&["--crash-test"])), None);
        }

        #[test]
        fn unknown_kind_or_missing_value_is_none() {
            assert_eq!(parse_crash_test(&args(&["ocy", "--crash-test=what"])), None);
            assert_eq!(parse_crash_test(&args(&["ocy", "--crash-test"])), None);
            assert_eq!(
                parse_crash_test(&args(&["ocy", "--crash-test", "what"])),
                None
            );
            assert_eq!(parse_crash_test(&args(&["ocy", "--autostart"])), None);
        }
    }
}
