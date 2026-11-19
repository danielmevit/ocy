//! Time types and .NET JSON format adapters (`TimeSpan`, `DateTimeOffset`, `Paused`).

use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use damt_platform::Clock;
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration, Timestamp, ZonedDateTime};
use serde::{Deserialize, Deserializer, Serializer};
use thiserror::Error;

/// The .NET `DateTimeOffset.MaxValue` string representation used as the pause-forever sentinel.
pub const PAUSE_FOREVER_SENTINEL: &str = "9999-12-31T23:59:59.9999999+00:00";

/// Errors returned when parsing .NET `TimeSpan` or `DateTimeOffset` strings.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TimeParseError {
    /// The input string did not match the expected format or was out of range.
    #[error("invalid time format: {0}")]
    InvalidFormat(String),
}

/// An instant in time paired with a fixed UTC offset, mirroring .NET's `DateTimeOffset`.
///
/// Storing the offset explicitly preserves round-trip fidelity with C# `state.json` files
/// such as `"2026-11-19T08:00:00+01:00"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTimeOffset {
    timestamp: Timestamp,
    offset: Offset,
}

impl DateTimeOffset {
    /// Creates a `DateTimeOffset` at UTC (`+00:00`) from a [`Timestamp`].
    #[must_use]
    pub const fn from_utc(timestamp: Timestamp) -> Self {
        Self {
            timestamp,
            offset: Offset::UTC,
        }
    }

    /// Creates a `DateTimeOffset` from a [`Timestamp`] and a fixed [`Offset`].
    #[must_use]
    pub const fn new(timestamp: Timestamp, offset: Offset) -> Self {
        Self { timestamp, offset }
    }

    /// Creates a `DateTimeOffset` from a [`ZonedDateTime`], capturing its instant and offset.
    #[must_use]
    pub fn from_zoned(zdt: &ZonedDateTime) -> Self {
        Self {
            timestamp: zdt.timestamp(),
            offset: zdt.offset(),
        }
    }

    /// Returns the UTC instant.
    #[must_use]
    pub const fn timestamp(self) -> Timestamp {
        self.timestamp
    }

    /// Returns the fixed UTC offset.
    #[must_use]
    pub const fn offset(self) -> Offset {
        self.offset
    }

    /// Adds a [`Duration`] to this instant, preserving its offset.
    #[must_use]
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        let signed = SignedDuration::try_from(duration).ok()?;
        let ts = self.timestamp.checked_add(signed).ok()?;
        Some(Self {
            timestamp: ts,
            offset: self.offset,
        })
    }

    /// Subtracts a [`Duration`] from this instant, preserving its offset.
    #[must_use]
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        let signed = SignedDuration::try_from(duration).ok()?;
        let ts = self.timestamp.checked_sub(signed).ok()?;
        Some(Self {
            timestamp: ts,
            offset: self.offset,
        })
    }

    /// Returns the UTC timestamp formatted as `yyyyMMddTHHmmssZ` (used for corrupt file backups).
    #[must_use]
    pub fn format_backup_utc(self) -> String {
        let dt = Offset::UTC.to_datetime(self.timestamp);
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
        )
    }
}

impl fmt::Display for DateTimeOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt = self.offset.to_datetime(self.timestamp);
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
        )?;

        let ticks_100ns = (dt.subsec_nanosecond().max(0) as u32) / 100;
        if ticks_100ns > 0 {
            let frac_buf = format!("{ticks_100ns:07}");
            let trimmed = frac_buf.trim_end_matches('0');
            write!(f, ".{trimmed}")?;
        }

        let offset_secs = self.offset.seconds();
        let sign = if offset_secs < 0 { '-' } else { '+' };
        let abs_secs = offset_secs.unsigned_abs();
        let oh = abs_secs / 3600;
        let om = (abs_secs % 3600) / 60;
        write!(f, "{sign}{oh:02}:{om:02}")
    }
}

impl FromStr for DateTimeOffset {
    type Err = TimeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_dotnet_datetime_offset(s)
    }
}

/// Pause target for the engine, explicitly modelling the .NET `DateTimeOffset.MaxValue` sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paused {
    /// Paused until the given instant.
    Until(DateTimeOffset),
    /// Paused indefinitely until [`crate::engine::BreakEngine::resume`] is called.
    ///
    /// Serializes as `"9999-12-31T23:59:59.9999999+00:00"` for C# parity.
    UntilResumed,
}

impl Paused {
    /// Returns `true` if this pause has expired at `now`.
    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        match self {
            Self::Until(until) => now >= until.timestamp(),
            Self::UntilResumed => false,
        }
    }
}

impl fmt::Display for Paused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Until(dto) => dto.fmt(f),
            Self::UntilResumed => f.write_str(PAUSE_FOREVER_SENTINEL),
        }
    }
}

impl FromStr for Paused {
    type Err = TimeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if is_pause_forever_sentinel(s) {
            return Ok(Self::UntilResumed);
        }
        Ok(Self::Until(DateTimeOffset::from_str(s)?))
    }
}

fn is_pause_forever_sentinel(s: &str) -> bool {
    s == PAUSE_FOREVER_SENTINEL || s == "9999-12-31T23:59:59.9999999Z"
}

fn parse_digits_u32(s: &str, expected_len: Option<usize>) -> Option<u32> {
    if let Some(len) = expected_len
        && s.len() != len
    {
        return None;
    }
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok()
}

fn parse_dotnet_datetime_offset(s: &str) -> Result<DateTimeOffset, TimeParseError> {
    let err = || TimeParseError::InvalidFormat(s.to_owned());

    // Minimum length: "YYYY-MM-DDTHH:MM:SSZ" (20 chars)
    if s.len() < 20 {
        return Err(err());
    }

    let (date_part, rest) = s.split_at(10);
    let mut date_iter = date_part.split('-');
    let year: i16 = date_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(4)))
        .and_then(|v| i16::try_from(v).ok())
        .ok_or_else(err)?;
    let month: i8 = date_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(2)))
        .and_then(|v| i8::try_from(v).ok())
        .ok_or_else(err)?;
    let day: i8 = date_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(2)))
        .and_then(|v| i8::try_from(v).ok())
        .ok_or_else(err)?;
    if date_iter.next().is_some() {
        return Err(err());
    }

    let sep = rest.as_bytes().first().copied().ok_or_else(err)?;
    if sep != b'T' && sep != b't' && sep != b' ' {
        return Err(err());
    }
    let time_and_tz = &rest[1..];
    if time_and_tz.len() < 9 {
        return Err(err());
    }

    let (hms_part, after_hms) = time_and_tz.split_at(8);
    let mut hms_iter = hms_part.split(':');
    let hour: i8 = hms_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(2)))
        .and_then(|v| i8::try_from(v).ok())
        .ok_or_else(err)?;
    let minute: i8 = hms_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(2)))
        .and_then(|v| i8::try_from(v).ok())
        .ok_or_else(err)?;
    let second: i8 = hms_iter
        .next()
        .and_then(|p| parse_digits_u32(p, Some(2)))
        .and_then(|v| i8::try_from(v).ok())
        .ok_or_else(err)?;
    if hms_iter.next().is_some() {
        return Err(err());
    }

    let (nanosecond, tz_part) = if let Some(after_dot) = after_hms.strip_prefix('.') {
        let frac_len = after_dot.bytes().take_while(|b| b.is_ascii_digit()).count();
        if frac_len == 0 || frac_len > 9 {
            return Err(err());
        }
        let (frac_str, rem) = after_dot.split_at(frac_len);
        let mut nanos = frac_str.parse::<u32>().map_err(|_| err())?;
        for _ in frac_len..9 {
            nanos *= 10;
        }
        (i32::try_from(nanos).map_err(|_| err())?, rem)
    } else {
        (0, after_hms)
    };

    let offset = parse_offset(tz_part).ok_or_else(err)?;
    let dt = jiff::civil::DateTime::new(year, month, day, hour, minute, second, nanosecond)
        .map_err(|_| err())?;
    let ts = offset.to_timestamp(dt).map_err(|_| err())?;

    Ok(DateTimeOffset::new(ts, offset))
}

fn parse_offset(tz: &str) -> Option<Offset> {
    if tz == "Z" || tz == "z" {
        return Some(Offset::UTC);
    }

    let (sign, rest) = if let Some(r) = tz.strip_prefix('+') {
        (1i32, r)
    } else if let Some(r) = tz.strip_prefix('-') {
        (-1i32, r)
    } else {
        return None;
    };

    let (oh, om) = if let Some((h_str, m_str)) = rest.split_once(':') {
        let h = parse_digits_u32(h_str, Some(2))?;
        let m = parse_digits_u32(m_str, Some(2))?;
        (h, m)
    } else if rest.len() == 4 {
        let (h_str, m_str) = rest.split_at(2);
        let h = parse_digits_u32(h_str, Some(2))?;
        let m = parse_digits_u32(m_str, Some(2))?;
        (h, m)
    } else if rest.len() == 2 {
        let h = parse_digits_u32(rest, Some(2))?;
        (h, 0)
    } else {
        return None;
    };

    if oh > 23 || om > 59 {
        return None;
    }

    let total_secs = sign * (i32::try_from(oh).ok()? * 3600 + i32::try_from(om).ok()? * 60);
    Offset::from_seconds(total_secs).ok()
}

/// Formats a [`Duration`] in .NET `TimeSpan.ToString("c")` format: `[d.]hh:mm:ss[.fffffff]`.
#[must_use]
pub fn format_dotnet_timespan(duration: Duration) -> String {
    let total_100ns =
        duration.as_secs().saturating_mul(10_000_000) + u64::from(duration.subsec_nanos() / 100);
    let total_secs = total_100ns / 10_000_000;
    let frac_ticks = total_100ns % 10_000_000;

    let days = total_secs / 86_400;
    let rem = total_secs % 86_400;
    let hours = rem / 3_600;
    let minutes = (rem % 3_600) / 60;
    let seconds = rem % 60;

    let mut out = if days > 0 {
        format!("{days}.{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    };

    if frac_ticks > 0 {
        out.push_str(&format!(".{frac_ticks:07}"));
    }

    out
}

/// Parses a .NET `TimeSpan.ToString("c")` string (`[d.]hh:mm:ss[.fffffff]`) into a [`Duration`].
pub fn parse_dotnet_timespan(s: &str) -> Result<Duration, TimeParseError> {
    let err = || TimeParseError::InvalidFormat(s.to_owned());
    if s.is_empty() || s.starts_with('-') {
        return Err(err());
    }

    let mut parts = s.split(':');
    let d_h = parts.next().ok_or_else(err)?;
    let m_str = parts.next().ok_or_else(err)?;
    let s_frac = parts.next().ok_or_else(err)?;
    if parts.next().is_some() {
        return Err(err());
    }

    let (days, hours) = if let Some((d_str, h_str)) = d_h.split_once('.') {
        let d: u64 = if d_str.is_empty() || !d_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        } else {
            d_str.parse().map_err(|_| err())?
        };
        let h = u64::from(parse_digits_u32(h_str, Some(2)).ok_or_else(err)?);
        (d, h)
    } else {
        let h = u64::from(parse_digits_u32(d_h, Some(2)).ok_or_else(err)?);
        (0, h)
    };

    let minutes = u64::from(parse_digits_u32(m_str, Some(2)).ok_or_else(err)?);

    let (sec_str, nanos) = if let Some((ss, frac)) = s_frac.split_once('.') {
        if frac.is_empty() || frac.len() > 7 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let mut ticks_100ns = frac.parse::<u32>().map_err(|_| err())?;
        for _ in frac.len()..7 {
            ticks_100ns *= 10;
        }
        (ss, ticks_100ns * 100)
    } else {
        (s_frac, 0u32)
    };

    let seconds = u64::from(parse_digits_u32(sec_str, Some(2)).ok_or_else(err)?);
    if hours > 23 || minutes > 59 || seconds > 59 {
        return Err(err());
    }

    let total_secs = days
        .checked_mul(86_400)
        .and_then(|v| v.checked_add(hours * 3_600 + minutes * 60 + seconds))
        .ok_or_else(err)?;

    Ok(Duration::new(total_secs, nanos))
}

/// Serde helpers for .NET `TimeSpan` strings (`[d.]hh:mm:ss[.fffffff]`).
pub mod dotnet_timespan {
    use super::*;

    /// Serializes a [`Duration`] in .NET `TimeSpan` constant format.
    pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format_dotnet_timespan(*duration))
    }

    /// Deserializes a [`Duration`] from .NET `TimeSpan` constant format.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_dotnet_timespan(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde helpers for `Option<DateTimeOffset>` in .NET ISO 8601 format.
pub mod dotnet_datetime_offset_opt {
    use super::*;

    /// Serializes an `Option<DateTimeOffset>`.
    pub fn serialize<S: Serializer>(
        value: &Option<DateTimeOffset>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(dto) => serializer.serialize_str(&dto.to_string()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes an `Option<DateTimeOffset>`.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DateTimeOffset>, D::Error> {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => DateTimeOffset::from_str(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// Serde helpers for `Option<Paused>` in .NET `DateTimeOffset?` format.
pub mod dotnet_paused_opt {
    use super::*;

    /// Serializes an `Option<Paused>`.
    pub fn serialize<S: Serializer>(
        value: &Option<Paused>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(paused) => serializer.serialize_str(&paused.to_string()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes an `Option<Paused>`.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Paused>, D::Error> {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => Paused::from_str(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// An advanceable fake [`Clock`] with a fixed time zone, mirroring .NET's `FakeTimeProvider`.
#[derive(Debug)]
pub struct FakeClock {
    now: Mutex<ZonedDateTime>,
}

impl FakeClock {
    /// Creates a `FakeClock` initialized to `initial`.
    #[must_use]
    pub fn new(initial: ZonedDateTime) -> Self {
        Self {
            now: Mutex::new(initial),
        }
    }

    /// Creates a `FakeClock` in UTC at the given civil date and time.
    ///
    /// Returns `None` if the date/time components are invalid.
    #[must_use]
    pub fn from_utc(
        year: i16,
        month: i8,
        day: i8,
        hour: i8,
        minute: i8,
        second: i8,
    ) -> Option<Self> {
        Self::from_fixed_offset(year, month, day, hour, minute, second, 0)
    }

    /// Creates a `FakeClock` in a fixed hour offset from UTC at the given civil date and time.
    ///
    /// Returns `None` if the date/time/offset components are invalid.
    #[must_use]
    pub fn from_fixed_offset(
        year: i16,
        month: i8,
        day: i8,
        hour: i8,
        minute: i8,
        second: i8,
        offset_hours: i8,
    ) -> Option<Self> {
        let offset = Offset::from_seconds(i32::from(offset_hours) * 3600).ok()?;
        let dt = jiff::civil::DateTime::new(year, month, day, hour, minute, second, 0).ok()?;
        let ts = offset.to_timestamp(dt).ok()?;
        Some(Self::new(ts.to_zoned_datetime(TimeZone::fixed(offset))))
    }

    /// Advances the clock by `duration`.
    pub fn advance(&self, duration: Duration) {
        let mut guard = self.now.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(signed) = SignedDuration::try_from(duration)
            && let Ok(next) = guard.checked_add(signed)
        {
            *guard = next;
        }
    }

    /// Sets the current clock time directly.
    pub fn set_now(&self, now: ZonedDateTime) {
        let mut guard = self.now.lock().unwrap_or_else(|e| e.into_inner());
        *guard = now;
    }

    /// Returns the current instant as a UTC [`DateTimeOffset`].
    #[must_use]
    pub fn utc_now(&self) -> DateTimeOffset {
        DateTimeOffset::from_utc(self.now().timestamp())
    }
}

impl Clock for FakeClock {
    fn now(&self) -> ZonedDateTime {
        self.now.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}
