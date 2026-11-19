#!/usr/bin/env python3
"""Differential testing driver for Ocy's C# (src/Ocy.Core) and Rust (ocy-core) engines.

Usage:
    python3 rust/tools/diff/run_diff.py --scenarios 300 --steps 400 --seed 20261119
    python3 rust/tools/diff/run_diff.py --self-test
"""

from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import json
import os
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from typing import Any, Callable
from zoneinfo import ZoneInfo


ROOT = pathlib.Path(__file__).resolve().parents[3]
DIFF_DIR = pathlib.Path(__file__).resolve().parent
CSHARP_PROJ = DIFF_DIR / "csharp-harness" / "CsharpHarness.csproj"
RUST_DIR = ROOT / "rust"
RUST_MANIFEST = RUST_DIR / "Cargo.toml"
FAILURES_DIR = DIFF_DIR / "failures"

HARNESS_TIMEOUT_SECS = 30
PAUSE_FOREVER_SENTINEL = "9999-12-31T23:59:59.9999999+00:00"

TIME_ZONES = ["UTC", "Europe/Prague", "America/New_York"]
FOCUS_STATES = [
    "Normal",
    "FullscreenApp",
    "Presentation",
    "QuietTime",
    "NotPresent",
    "Unknown",
]
SESSION_EVENTS = ["Locked", "Unlocked", "Suspending", "Resumed", "LoggingOff"]

# Anchor timestamps near local midnight and DST transitions for extra coverage.
INTERESTING_START_TIMES = [
    "2026-03-08T06:55:00Z",  # America/New_York spring-forward DST (01:55 EST -> 03:00 EDT)
    "2026-03-29T00:55:00Z",  # Europe/Prague spring-forward DST (01:55 CET -> 03:00 CEST)
    "2026-10-25T00:55:00Z",  # Europe/Prague fall-back DST (02:55 CEST -> 02:00 CET)
    "2026-11-01T05:55:00Z",  # America/New_York fall-back DST (01:55 EDT -> 01:00 EST)
    "2026-11-19T22:55:00Z",  # Near midnight in Europe/Prague
    "2026-11-20T04:55:00Z",  # Near midnight in America/New_York
    "2026-12-31T23:55:00Z",  # Year rollover in UTC
]

COVERAGE_KEYS = [
    "postpones_succeeded",
    "postpones_refused",
    "skips_succeeded",
    "start_now_actions",
    "short_breaks_completed",
    "long_breaks_completed",
    "short_absences_credited",
    "long_absences_credited",
    "sleep_gaps",
    "fullscreen_deferrals",
    "pauses_expired",
    "pauses_resumed",
    "strict_mode_refusals",
    "midnight_crossings",
    "dst_crossings",
]


@dataclasses.dataclass(frozen=True)
class Mutant:
    name: str
    rel_path: str
    old: str
    new: str


SELF_TEST_MUTANTS: list[Mutant] = [
    Mutant(
        name="M1_sleep_gap_threshold_15s_to_16s",
        rel_path="src/engine.rs",
        old="const SLEEP_GAP_THRESHOLD: Duration = Duration::from_secs(15);",
        new="const SLEEP_GAP_THRESHOLD: Duration = Duration::from_secs(16);",
    ),
    Mutant(
        name="M2_postpone_counter_not_incremented",
        rel_path="src/engine.rs",
        old="self.postpones_used += 1;",
        new="self.postpones_used += 0;",
    ),
    Mutant(
        name="M3_long_absence_strict_gt_boundary",
        rel_path="src/engine.rs",
        old="if away >= self.settings.long_breaks.duration() {",
        new="if away > self.settings.long_breaks.duration() {",
    ),
    Mutant(
        name="M4_pause_expiry_strict_gt_boundary",
        rel_path="src/time.rs",
        old="Self::Until(until) => now >= until.timestamp(),",
        new="Self::Until(until) => now > until.timestamp(),",
    ),
    Mutant(
        name="M5_end_pause_skips_long_counter_reset",
        rel_path="src/engine.rs",
        old=(
            "    fn end_pause(&mut self, events: &mut Vec<EngineEvent>) {\n"
            "        self.paused_until = None;\n"
            "        self.since_short_break = Duration::ZERO;\n"
            "        self.since_long_break = Duration::ZERO;"
        ),
        new=(
            "    fn end_pause(&mut self, events: &mut Vec<EngineEvent>) {\n"
            "        self.paused_until = None;\n"
            "        self.since_short_break = Duration::ZERO;\n"
            "        let _ = self.since_long_break;"
        ),
    ),
    Mutant(
        name="M6_deferral_ignores_presentation_focus",
        rel_path="src/engine.rs",
        old="&& matches!(focus, FocusState::FullscreenApp | FocusState::Presentation)",
        new="&& matches!(focus, FocusState::FullscreenApp)",
    ),
    Mutant(
        name="M7_warning_start_strict_gt_boundary",
        rel_path="src/engine.rs",
        old="if self.since_long_break >= warning_at {",
        new="if self.since_long_break > warning_at {",
    ),
    Mutant(
        name="M8_stats_active_minutes_strict_gt_60",
        rel_path="src/engine.rs",
        old="while self.active_seconds_accumulator >= 60.0 {",
        new="while self.active_seconds_accumulator > 60.0 {",
    ),
    Mutant(
        name="M9_clamp_drops_short_duration_upper_bound",
        rel_path="src/settings.rs",
        old=(
            "        self.duration_seconds = self\n"
            "            .duration_seconds\n"
            "            .clamp(Self::MIN_DURATION_SECONDS, Self::MAX_DURATION_SECONDS);"
        ),
        new=(
            "        self.duration_seconds = self\n"
            "            .duration_seconds\n"
            "            .max(Self::MIN_DURATION_SECONDS);"
        ),
    ),
    Mutant(
        name="M10_warning_skip_ignores_strict_mode",
        rel_path="src/engine.rs",
        old=(
            "            EnginePhase::LongBreakWarning => {\n"
            "                if self.settings.strict_mode {\n"
            "                    return (false, events);\n"
            "                }"
        ),
        new="            EnginePhase::LongBreakWarning => {",
    ),
    Mutant(
        name="M11_short_suppression_strict_lt_2min",
        rel_path="src/schedule.rs",
        old="time_until_long <= SHORT_BREAK_SUPPRESSION_WINDOW",
        new="time_until_long < SHORT_BREAK_SUPPRESSION_WINDOW",
    ),
    Mutant(
        name="M12_short_absence_strict_gt_boundary",
        rel_path="src/engine.rs",
        old="else if away >= self.settings.short_breaks.duration() {",
        new="else if away > self.settings.short_breaks.duration() {",
    ),
]


def find_dotnet() -> str:
    env_dotnet = shutil.which("dotnet")
    if env_dotnet:
        return env_dotnet
    home_dotnet = pathlib.Path.home() / ".dotnet" / "dotnet"
    if home_dotnet.exists():
        return str(home_dotnet)
    raise RuntimeError("Could not find dotnet executable on PATH or at ~/.dotnet/dotnet")


def fmt_dotnet_timespan(seconds: int) -> str:
    days, rem = divmod(max(0, seconds), 86400)
    hours, rem = divmod(rem, 3600)
    minutes, secs = divmod(rem, 60)
    if days > 0:
        return f"{days}.{hours:02d}:{minutes:02d}:{secs:02d}"
    return f"{hours:02d}:{minutes:02d}:{secs:02d}"


def parse_dotnet_timespan_ms(s: str) -> int:
    days = 0
    rest = s
    if "." in s and (":" not in s or s.index(".") < s.index(":")):
        day_part, rest = s.split(".", 1)
        days = int(day_part)
    hms = rest.split(":", 2)
    hours = int(hms[0])
    minutes = int(hms[1])
    sec_part = hms[2]
    if "." in sec_part:
        sec_str, frac_str = sec_part.split(".", 1)
        secs = int(sec_str)
        ms = int((frac_str + "000")[:3])
    else:
        secs = int(sec_part)
        ms = 0
    total_secs = ((days * 24 + hours) * 60 + minutes) * 60 + secs
    return total_secs * 1000 + ms


def make_settings(
    *,
    short_enabled: bool = True,
    short_interval: int = 10,
    short_duration: int = 15,
    short_fullscreen: bool = False,
    long_enabled: bool = True,
    long_interval: int = 60,
    long_duration: int = 5,
    warning_seconds: int = 60,
    postpone_minutes: int = 3,
    max_postpones: int = 3,
    strict_mode: bool = False,
    sounds: bool = False,
    inactivity_tracking: bool = True,
    away_threshold_seconds: int = 120,
    pause_during_fullscreen: bool = True,
    has_window_nearby: bool = True,
    status_notifications: bool = True,
    start_with_system: bool = False,
    language: str = "en",
) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "shortBreaks": {
            "enabled": short_enabled,
            "intervalMinutes": short_interval,
            "durationSeconds": short_duration,
            "fullscreen": short_fullscreen,
        },
        "longBreaks": {
            "enabled": long_enabled,
            "intervalMinutes": long_interval,
            "durationMinutes": long_duration,
            "warningSeconds": warning_seconds,
            "postponeMinutes": postpone_minutes,
            "maxPostpones": max_postpones,
        },
        "strictMode": strict_mode,
        "sounds": sounds,
        "inactivityTracking": inactivity_tracking,
        "awayThresholdSeconds": away_threshold_seconds,
        "pauseDuringFullscreen": pause_during_fullscreen,
        "hasWindowNearby": has_window_nearby,
        "statusNotifications": status_notifications,
        "startWithSystem": start_with_system,
        "language": language,
    }


def gen_settings(rng: random.Random) -> dict[str, Any]:
    short_enabled = rng.random() > 0.15
    long_enabled = rng.random() > 0.15

    # Bias intervals/durations toward smaller values so many cycles occur in 400 steps,
    # while still exercising the full documented ranges.
    if rng.random() < 0.75:
        short_interval = rng.randint(2, 10)
        short_duration = rng.randint(5, 30)
        long_interval = rng.randint(15, 30)
        long_duration = rng.randint(1, 5)
        warning_secs = rng.choice([0, 5, 10, 15, 30, 60])
        postpone_mins = rng.randint(1, 5)
    else:
        short_interval = rng.randint(2, 60)
        short_duration = rng.randint(5, 120)
        long_interval = rng.randint(15, 240)
        long_duration = rng.randint(1, 30)
        warning_secs = rng.randint(0, 300)
        postpone_mins = rng.randint(1, 30)

    max_postpones = rng.randint(0, 10) if rng.random() < 0.4 else rng.randint(0, 4)
    away_threshold = (
        rng.randint(30, 120) if rng.random() < 0.7 else rng.randint(30, 1800)
    )

    return make_settings(
        short_enabled=short_enabled,
        short_interval=short_interval,
        short_duration=short_duration,
        short_fullscreen=rng.choice([False, True]),
        long_enabled=long_enabled,
        long_interval=long_interval,
        long_duration=long_duration,
        warning_seconds=warning_secs,
        postpone_minutes=postpone_mins,
        max_postpones=max_postpones,
        strict_mode=rng.random() < 0.30,
        sounds=rng.choice([False, True]),
        inactivity_tracking=rng.random() > 0.15,
        away_threshold_seconds=away_threshold,
        pause_during_fullscreen=rng.random() > 0.20,
        has_window_nearby=rng.random() > 0.25,
        status_notifications=rng.choice([False, True]),
        start_with_system=rng.choice([False, True]),
        language=rng.choice(["en", "mk"]),
    )


def gen_initial_state(
    rng: random.Random, initial_dt: datetime, settings: dict[str, Any]
) -> dict[str, Any] | None:
    if rng.random() < 0.65:
        return None

    short_secs = rng.randint(0, max(1, settings["shortBreaks"]["intervalMinutes"]) * 60 + 30)
    if rng.random() < 0.15:
        long_secs = rng.randint(86400, 120000)  # >= 24h to exercise d.hh:mm:ss TimeSpan format
    else:
        long_secs = rng.randint(0, max(1, settings["longBreaks"]["intervalMinutes"]) * 60 + 30)
    postpones_used = rng.randint(0, max(0, settings["longBreaks"]["maxPostpones"]) + 2)

    pause_roll = rng.random()
    if pause_roll < 0.55:
        paused_until = None
    elif pause_roll < 0.70:
        paused_until = PAUSE_FOREVER_SENTINEL
    elif pause_roll < 0.85:
        dt = initial_dt + timedelta(seconds=rng.randint(1, 600))
        paused_until = dt.astimezone(timezone(timedelta(hours=1))).isoformat()
    elif pause_roll < 0.92:
        paused_until = initial_dt.strftime("%Y-%m-%dT%H:%M:%SZ")  # exact boundary now == pausedUntilUtc
    else:
        dt = initial_dt - timedelta(seconds=rng.randint(1, 600))
        paused_until = dt.strftime("%Y-%m-%dT%H:%M:%SZ")

    tick_roll = rng.random()
    if tick_roll < 0.15:
        last_tick = None
    elif tick_roll < 0.45:
        dt = initial_dt - timedelta(seconds=rng.randint(1, 15))
        last_tick = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    elif tick_roll < 0.75:
        dt = initial_dt - timedelta(seconds=rng.randint(16, 120))
        last_tick = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    elif tick_roll < 0.92:
        dt = initial_dt - timedelta(minutes=rng.randint(5, 40))
        last_tick = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    else:
        # Future lastTickUtc (negative startup gap)
        dt = initial_dt + timedelta(seconds=rng.randint(5, 60))
        last_tick = dt.strftime("%Y-%m-%dT%H:%M:%SZ")

    return {
        "schemaVersion": 1,
        "sinceShortBreak": fmt_dotnet_timespan(short_secs),
        "sinceLongBreak": fmt_dotnet_timespan(long_secs),
        "postponesUsed": postpones_used,
        "pausedUntilUtc": paused_until,
        "lastTickUtc": last_tick,
    }


def make_step(
    advance_ms: int,
    action: dict[str, Any] | None = None,
    idle_ms: int = 0,
    focus: str = "Normal",
) -> dict[str, Any]:
    return {
        "advanceMs": advance_ms,
        "idleMs": idle_ms,
        "focus": focus,
        "action": action if action is not None else {"type": "tick"},
    }


def gen_random_steps(rng: random.Random, count: int, profile: str) -> list[dict[str, Any]]:
    steps: list[dict[str, Any]] = []
    for _ in range(count):
        if profile == "workday":
            adv_roll = rng.random()
            if adv_roll < 0.35:
                advance_ms = 1000
            elif adv_roll < 0.96:
                advance_ms = rng.choice([5000, 10000, 15000])
            elif adv_roll < 0.98:
                advance_ms = rng.randint(15001, 45000)
            else:
                advance_ms = rng.choice([500, 1500, 333])

            if rng.random() < 0.985:
                idle_ms = rng.randint(0, 5000)
            else:
                idle_ms = rng.randint(30000, 350000)

            focus = rng.choices(
                FOCUS_STATES, weights=[90, 4, 2, 2, 1, 1], k=1
            )[0]

            act_roll = rng.random()
            if act_roll < 0.78:
                action: dict[str, Any] = {"type": "tick"}
            elif act_roll < 0.92:
                action = {"type": "postpone"}
            elif act_roll < 0.95:
                action = {"type": "skip"}
            elif act_roll < 0.965:
                action = {"type": "start_short_now"}
            elif act_roll < 0.975:
                action = {"type": "start_long_now"}
            elif act_roll < 0.985:
                mins = None if rng.random() < 0.3 else rng.choice([1, 2])
                action = {"type": "pause", "minutes": mins}
            elif act_roll < 0.992:
                action = {"type": "resume"}
            elif act_roll < 0.997:
                action = {"type": "session", "kind": rng.choice(SESSION_EVENTS)}
            else:
                action = {"type": "update_settings", "settings": gen_settings(rng)}
        elif profile == "interactive":
            adv_roll = rng.random()
            if adv_roll < 0.65:
                advance_ms = 1000
            elif adv_roll < 0.85:
                advance_ms = rng.choice([5000, 10000, 15000, rng.randint(2000, 15000)])
            elif adv_roll < 0.93:
                advance_ms = rng.randint(15001, 60000)
            else:
                advance_ms = rng.choice([0, 250, 500, 1500, 777])

            idle_ms = (
                rng.randint(0, 10000)
                if rng.random() < 0.88
                else rng.randint(30000, 400000)
            )
            focus = rng.choices(
                FOCUS_STATES, weights=[70, 10, 8, 5, 4, 3], k=1
            )[0]

            act_roll = rng.random()
            if act_roll < 0.60:
                action = {"type": "tick"}
            elif act_roll < 0.67:
                action = {"type": "start_long_now"}
            elif act_roll < 0.74:
                action = {"type": "start_short_now"}
            elif act_roll < 0.81:
                action = {"type": "postpone"}
            elif act_roll < 0.87:
                action = {"type": "skip"}
            elif act_roll < 0.91:
                mins = None if rng.random() < 0.3 else rng.choice([0, 1, 2, 3, 5, 10])
                action = {"type": "pause", "minutes": mins}
            elif act_roll < 0.94:
                action = {"type": "resume"}
            elif act_roll < 0.98:
                action = {"type": "session", "kind": rng.choice(SESSION_EVENTS)}
            else:
                action = {"type": "update_settings", "settings": gen_settings(rng)}
        else:
            adv_roll = rng.random()
            if adv_roll < 0.70:
                advance_ms = 1000
            elif adv_roll < 0.82:
                advance_ms = rng.choice([5000, 10000, 15000, rng.randint(2000, 15000)])
            elif adv_roll < 0.90:
                advance_ms = rng.randint(15001, 60000)
            elif adv_roll < 0.95:
                advance_ms = rng.randint(60001, 600000)
            else:
                advance_ms = rng.choice([0, 250, 500, 1500, 333, 777])

            idle_roll = rng.random()
            if idle_roll < 0.70:
                idle_ms = rng.choice([0, 0, 0, rng.randint(100, 15000)])
            elif idle_roll < 0.90:
                idle_ms = rng.randint(30000, 300000)
            else:
                idle_ms = rng.randint(300001, 2000000)

            focus = rng.choices(
                FOCUS_STATES, weights=[65, 10, 8, 6, 6, 5], k=1
            )[0]

            act_roll = rng.random()
            if act_roll < 0.74:
                action = {"type": "tick"}
            elif act_roll < 0.78:
                action = {"type": "start_long_now"}
            elif act_roll < 0.82:
                action = {"type": "start_short_now"}
            elif act_roll < 0.86:
                action = {"type": "postpone"}
            elif act_roll < 0.90:
                action = {"type": "skip"}
            elif act_roll < 0.93:
                mins = None if rng.random() < 0.3 else rng.choice([0, 1, 2, 3, 5, 10, 15])
                action = {"type": "pause", "minutes": mins}
            elif act_roll < 0.95:
                action = {"type": "resume"}
            elif act_roll < 0.985:
                action = {"type": "session", "kind": rng.choice(SESSION_EVENTS)}
            else:
                action = {"type": "update_settings", "settings": gen_settings(rng)}

        steps.append(
            make_step(advance_ms, action=action, idle_ms=idle_ms, focus=focus)
        )
    return steps


def finish_targeted_scenario(
    rng: random.Random,
    num_steps: int,
    *,
    initial_utc: str,
    time_zone: str,
    settings: dict[str, Any],
    initial_state: dict[str, Any] | None,
    steps: list[dict[str, Any]],
    idle_supported: bool = True,
    focus_supported: bool = True,
    tail_profile: str = "workday",
) -> dict[str, Any]:
    if len(steps) < num_steps:
        steps = steps + gen_random_steps(rng, num_steps - len(steps), tail_profile)
    else:
        steps = steps[:num_steps]
    out: dict[str, Any] = {
        "initialUtc": initial_utc,
        "timeZone": time_zone,
        "settings": settings,
        "initialState": initial_state,
        "steps": steps,
    }
    if not idle_supported:
        out["idleSupported"] = False
    if not focus_supported:
        out["focusSupported"] = False
    return out


# Targeted scenario 1: Warning phase (postpone to limit + 1, skip in warning, strict refusals, start-now, midnight)
def gen_targeted_warning_postpone_skip(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=10,
        short_duration=10,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        postpone_minutes=2,
        max_postpones=2,
        strict_mode=False,
    )
    # Start close to warning (870s = 14m30s) and near Prague midnight (22:55 UTC = 23:55 CET)
    initial_state = {
        "schemaVersion": 1,
        "sinceShortBreak": fmt_dotnet_timespan(0),
        "sinceLongBreak": fmt_dotnet_timespan(660),
        "postponesUsed": 0,
        "pausedUntilUtc": None,
        "lastTickUtc": "2026-11-19T22:54:55Z",
    }
    steps: list[dict[str, Any]] = []
    # 14 * 15s = 210s -> sinceLong = 870s exact (warning_at), hits M7 (>= vs >) and short break along the way
    for _ in range(14):
        steps.append(make_step(15000))
    # Inside LongBreakWarning: tick 1s, postpone #1 (succeeds -> sinceLong = 780s)
    steps.append(make_step(1000))
    steps.append(make_step(0, action={"type": "postpone"}))
    # Advance 6 * 15s = 90s -> back to 870s (LongBreakWarning)
    for _ in range(6):
        steps.append(make_step(15000))
    # Postpone #2 (hits maxPostpones = 2)
    steps.append(make_step(1000))
    steps.append(make_step(0, action={"type": "postpone"}))
    # Advance 6 * 15s = 90s -> back to 870s (LongBreakWarning)
    for _ in range(6):
        steps.append(make_step(15000))
    # Try once more past limit -> REFUSED
    steps.append(make_step(0, action={"type": "postpone"}))
    # Skip during warning -> SUCCEEDS (resets counters and postpones_used = 0)
    steps.append(make_step(0, action={"type": "skip"}))

    # Start short break now & skip it; start short break now & complete it
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    steps.append(make_step(1000, action={"type": "skip"}))
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    steps.append(make_step(10000))  # completes 10s short break

    # Start long break now & complete it (60s)
    steps.append(make_step(1000, action={"type": "start_long_now"}))
    for _ in range(4):
        steps.append(make_step(15000))

    # Enable strictMode and walk from 0 to warning (58 * 15s = 870s exact)
    strict_settings = make_settings(
        short_enabled=False,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        postpone_minutes=2,
        max_postpones=2,
        strict_mode=True,
    )
    steps.append(make_step(0, action={"type": "update_settings", "settings": strict_settings}))
    for _ in range(58):
        steps.append(make_step(15000))
    # Strict-mode refusals in warning (postpone & skip)
    steps.append(make_step(0, action={"type": "postpone"}))
    steps.append(make_step(0, action={"type": "skip"}))
    # Let warning count down (30s) -> enters LongBreak
    steps.append(make_step(15000))
    steps.append(make_step(15000))
    # Strict-mode refusal of skip in LongBreak, then complete LongBreak (60s)
    steps.append(make_step(0, action={"type": "skip"}))
    for _ in range(4):
        steps.append(make_step(15000))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-11-19T22:55:00Z",
        time_zone="Europe/Prague",
        settings=settings,
        initial_state=initial_state,
        steps=steps,
        tail_profile="workday",
    )


# Targeted scenario 2: Exact idle away spans at short & long break duration ± 1 ms
def gen_targeted_exact_idle_boundaries(rng: random.Random, num_steps: int) -> dict[str, Any]:
    # awayThreshold=30s, shortDuration=45s, longDuration=60s (1 min)
    settings = make_settings(
        short_enabled=True,
        short_interval=10,
        short_duration=45,
        long_enabled=True,
        long_interval=20,
        long_duration=1,
        warning_seconds=30,
        inactivity_tracking=True,
        away_threshold_seconds=30,
    )
    steps: list[dict[str, Any]] = []
    # Test exact boundaries: short-1ms, short, short+1ms, long-1ms, long, long+1ms
    for away_ms in [44999, 45000, 45001, 59999, 60000, 60001]:
        # Build active time (4 * 15s = 60s -> tests active_seconds_accumulator >= 60.0 exact!)
        for _ in range(4):
            steps.append(make_step(15000, idle_ms=0))
        # Enter Away: started = now - (away_ms - 10_000)
        steps.append(make_step(1000, idle_ms=away_ms - 10000))
        # Return after 10_000ms (<= 15_000ms so not a sleep gap) -> away == away_ms exact!
        steps.append(make_step(10000, idle_ms=0))

    # Also test Locked/Unlocked at exact short (45_000ms) and exact long (60_000ms)
    for _ in range(4):
        steps.append(make_step(15000, idle_ms=0))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    for _ in range(3):
        steps.append(make_step(15000))
    steps.append(make_step(0, action={"type": "session", "kind": "Unlocked"}))  # 45_000ms exact

    for _ in range(4):
        steps.append(make_step(15000, idle_ms=0))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    for _ in range(4):
        steps.append(make_step(15000))
    steps.append(make_step(0, action={"type": "session", "kind": "Unlocked"}))  # 60_000ms exact

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-06-15T10:00:00Z",
        time_zone="UTC",
        settings=settings,
        initial_state=None,
        steps=steps,
        tail_profile="workday",
    )


# Targeted scenario 3: Sleep gaps at exact 15_000ms and 15_001ms + short suppression 120s boundary
def gen_targeted_sleep_gaps_and_suppression(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=20,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=0,
    )
    steps: list[dict[str, Any]] = []
    # Work 3 * 15s = 45s
    for _ in range(3):
        steps.append(make_step(15000))
    # Gap == 15_000ms exact -> NOT a sleep gap, counters grow to 60s
    steps.append(make_step(15000))
    # Gap == 15_001ms -> IS a sleep gap (< 20s short duration so counters stay 60s!)
    steps.append(make_step(15001))
    # Gap == 20_000ms -> sleep gap == short duration (20s) -> resets sinceShortBreak!
    steps.append(make_step(20000))
    # Work 2 * 15s = 30s
    for _ in range(2):
        steps.append(make_step(15000))
    # Gap == 60_000ms -> sleep gap == long duration (60s) -> AbsenceCredited(Long)
    steps.append(make_step(60000))

    # Test short-break suppression exact 120s boundary (M11: <= 120s vs < 120s):
    # long_interval = 15m (900s), short_interval = 2m (120s).
    # Advance 52 * 15s = 780s!
    # At step 16 (240s), short fires -> tick 15s completes -> wait, let's make short complete cleanly:
    # Actually 8 * 15s = 120s -> short fires! Tick 10s + 10s (20s) completes short break.
    # Better: update settings so short_interval = 13m (780s -> wait: 13*60=780s), long_interval = 15m (900s)!
    # At t = 780s, since_short == 780s (due!), time_until_long = 900 - 780 = 120s exact!
    # That triggers suppression at the EXACT 120s boundary!
    supp_settings = make_settings(
        short_enabled=True,
        short_interval=13,
        short_duration=10,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=0,
    )
    steps.append(make_step(0, action={"type": "update_settings", "settings": supp_settings}))
    # 52 * 15s = 780s -> since_short == 780s, since_long == 780s -> time_until_long == 120s exact!
    for _ in range(52):
        steps.append(make_step(15000))
    # Continue 8 * 15s = 120s -> long break triggers at 900s, complete it (4 * 15s = 60s)
    for _ in range(12):
        steps.append(make_step(15000))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-07-01T08:00:00Z",
        time_zone="UTC",
        settings=settings,
        initial_state=None,
        steps=steps,
        tail_profile="interactive",
    )


# Targeted scenario 4: Pause expiry at exact boundary, manual resume, and pause interrupting breaks
def gen_targeted_pause_expiry_and_resume(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=5,
        short_duration=15,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
    )
    steps: list[dict[str, Any]] = []
    # Build up active counters (4 * 15s = 60s)
    for _ in range(4):
        steps.append(make_step(15000))
    # Pause for 1 minute (60_000ms)
    steps.append(make_step(1000, action={"type": "pause", "minutes": 1}))
    # Advance 59_999ms (still < pausedUntilUtc) -> MUST stay Paused
    for _ in range(5):
        steps.append(make_step(10000))
    steps.append(make_step(9999))
    # Advance 1ms -> now == pausedUntilUtc EXACT -> pause expires, counters reset to 0!
    steps.append(make_step(1))

    # Build counters again (2 * 15s = 30s), pause indefinitely, survive sleep gap & lock/unlock, then resume
    for _ in range(2):
        steps.append(make_step(15000))
    steps.append(make_step(1000, action={"type": "pause", "minutes": None}))
    steps.append(make_step(30000))  # sleep gap while paused
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(10000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Unlocked"}))
    steps.append(make_step(1000, action={"type": "resume"}))

    # Pause for 1m, advance 60_000ms without ticking, then resume directly
    steps.append(make_step(1000, action={"type": "pause", "minutes": 1}))
    steps.append(make_step(60000, action={"type": "resume"}))

    # Pause interrupting ShortBreak and LongBreak
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    steps.append(make_step(5000))
    steps.append(make_step(1000, action={"type": "pause", "minutes": 1}))
    steps.append(make_step(1000, action={"type": "resume"}))

    steps.append(make_step(1000, action={"type": "start_long_now"}))
    steps.append(make_step(5000))
    steps.append(make_step(1000, action={"type": "pause", "minutes": 1}))
    steps.append(make_step(1000, action={"type": "resume"}))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-05-10T12:00:00Z",
        time_zone="UTC",
        settings=settings,
        initial_state=None,
        steps=steps,
        tail_profile="interactive",
    )


# Targeted scenario 5: Out-of-range settings clamping + mid-break & mid-warning updates (shorter than elapsed)
def gen_targeted_settings_clamp_and_mid_updates(rng: random.Random, num_steps: int) -> dict[str, Any]:
    # Deliberately out-of-range settings (tests both min and max clamping)
    unclamped = make_settings(
        short_enabled=True,
        short_interval=0,       # -> clamped to 2
        short_duration=999,     # -> clamped to 120 (kills M9!)
        long_enabled=True,
        long_interval=5,        # -> clamped to 15
        long_duration=99,       # -> clamped to 30
        warning_seconds=999,    # -> clamped to 300
        postpone_minutes=0,     # -> clamped to 1
        max_postpones=99,       # -> clamped to 10
        away_threshold_seconds=5,  # -> clamped to 30
        language="   ",         # -> clamped to "en"
    )
    steps: list[dict[str, Any]] = []
    # Trigger short break immediately and verify clamped duration = 120s (8 * 15s)
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    for _ in range(8):
        steps.append(make_step(15000))

    # Update to 20m long interval / 60s warning, accumulate > 15m, then shrink interval < elapsed mid-warning!
    s_20m = make_settings(
        short_enabled=True,
        short_interval=5,
        short_duration=30,
        long_enabled=True,
        long_interval=20,
        long_duration=2,
        warning_seconds=60,
        postpone_minutes=2,
        max_postpones=2,
    )
    steps.append(make_step(1000, action={"type": "update_settings", "settings": s_20m}))
    # Start a short break (30s), update settings mid-break (keeps 30s totalDuration!)
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    steps.append(make_step(10000))
    s_mid_short = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=5,
        long_enabled=True,
        long_interval=20,
        long_duration=2,
        warning_seconds=60,
        has_window_nearby=False,
    )
    steps.append(make_step(1000, action={"type": "update_settings", "settings": s_mid_short}))
    steps.append(make_step(10000))
    steps.append(make_step(10000))  # completes 30s short break

    # Advance to 19m (1140s = 76 * 15s) to trigger LongBreakWarning (with short disabled temporarily)
    s_long_only = make_settings(
        short_enabled=False,
        long_enabled=True,
        long_interval=20,
        long_duration=2,
        warning_seconds=60,
        postpone_minutes=2,
        max_postpones=2,
    )
    steps.append(make_step(1000, action={"type": "update_settings", "settings": s_long_only}))
    for _ in range(76):
        steps.append(make_step(15000))
    # Now in LongBreakWarning with sinceLong == 1140s; shrink long_interval to 15m (900s < 1140s)!
    s_shorter = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=5,
        long_enabled=True,
        long_interval=15,
        long_duration=2,
        warning_seconds=30,
        postpone_minutes=2,
        max_postpones=2,
    )
    steps.append(make_step(1000, action={"type": "update_settings", "settings": s_shorter}))
    steps.append(make_step(5000))
    # Also update mid-LongBreak
    steps.append(make_step(1000, action={"type": "start_long_now"}))
    steps.append(make_step(10000))
    steps.append(make_step(1000, action={"type": "update_settings", "settings": unclamped}))
    # Remaining 110s of 2m LongBreak (8 * 15s = 120s)
    for _ in range(8):
        steps.append(make_step(15000))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-08-01T09:00:00Z",
        time_zone="UTC",
        settings=unclamped,
        initial_state=None,
        steps=steps,
        tail_profile="chaotic",
    )


# Targeted scenario 6: Fullscreen deferral (both FullscreenApp and Presentation) + warningSeconds=0
def gen_targeted_fullscreen_deferral(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=10,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        pause_during_fullscreen=True,
    )
    initial_state = {
        "schemaVersion": 1,
        "sinceShortBreak": fmt_dotnet_timespan(105),
        "sinceLongBreak": fmt_dotnet_timespan(855),
        "postponesUsed": 0,
        "pausedUntilUtc": None,
        "lastTickUtc": "2026-09-01T10:00:00Z",
    }
    steps: list[dict[str, Any]] = []
    # FullscreenApp: 15s tick drops short break & enters Deferred
    steps.append(make_step(15000, focus="FullscreenApp"))
    steps.append(make_step(10000, focus="FullscreenApp"))
    # Return to Normal -> enters LongBreakWarning, skip it
    steps.append(make_step(1000, focus="Normal"))
    steps.append(make_step(0, action={"type": "skip"}))

    # Now reach warning threshold again (58 * 15s = 870s), last step in Presentation (kills M6!)
    pres_settings = make_settings(
        short_enabled=False,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        pause_during_fullscreen=True,
    )
    steps.append(make_step(0, action={"type": "update_settings", "settings": pres_settings}))
    for _ in range(57):
        steps.append(make_step(15000, focus="Normal"))
    steps.append(make_step(15000, focus="Presentation"))
    steps.append(make_step(5000, focus="Presentation"))
    # Update warningSeconds=0 while Deferred, then return to Normal -> immediate LongBreak!
    no_warn = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=10,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=0,
        pause_during_fullscreen=True,
    )
    steps.append(make_step(1000, focus="Presentation", action={"type": "update_settings", "settings": no_warn}))
    steps.append(make_step(1000, focus="Normal"))
    # Complete LongBreak (60s)
    for _ in range(4):
        steps.append(make_step(15000, focus="Normal"))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-09-01T10:00:00Z",
        time_zone="UTC",
        settings=settings,
        initial_state=initial_state,
        steps=steps,
        tail_profile="interactive",
    )


# Targeted scenario 7: Lock/Unlock in every phase + Suspend/Resume during ShortBreak and LongBreak
def gen_targeted_lock_unlock_all_phases_and_suspend(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=5,
        short_duration=30,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        inactivity_tracking=True,
        away_threshold_seconds=30,
        pause_during_fullscreen=True,
        strict_mode=True,
    )
    initial_state = {
        "schemaVersion": 1,
        "sinceShortBreak": fmt_dotnet_timespan(100),
        "sinceLongBreak": fmt_dotnet_timespan(855),
        "postponesUsed": 0,
        "pausedUntilUtc": None,
        "lastTickUtc": "2026-10-01T12:00:00Z",
    }
    steps: list[dict[str, Any]] = []
    # 1. Working: Lock & Unlock
    steps.append(make_step(5000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Unlocked"}))

    # 2. LongBreakWarning: reach warning (15s), Lock & Unlock -> Working
    steps.append(make_step(15000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Unlocked"}))

    # 3. ShortBreak: Lock & Unlock mid-break; then Suspending & Resumed mid-break; then complete while Locked
    steps.append(make_step(1000, action={"type": "start_short_now"}))
    steps.append(make_step(2000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000, action={"type": "session", "kind": "Unlocked"}))  # still ShortBreak
    steps.append(make_step(1000, action={"type": "session", "kind": "Suspending"}))
    steps.append(make_step(5000, action={"type": "session", "kind": "Resumed"}))  # still ShortBreak
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(20000))  # completes while locked, enters Away
    steps.append(make_step(1000, action={"type": "session", "kind": "Unlocked"}))  # -> Working

    # 4. LongBreak (in strictMode!): Lock & Unlock mid-break; Suspending & Resumed completing break
    steps.append(make_step(1000, action={"type": "start_long_now"}))
    steps.append(make_step(5000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000, action={"type": "session", "kind": "Unlocked"}))  # still LongBreak
    steps.append(make_step(1000, action={"type": "session", "kind": "Suspending"}))
    for _ in range(4):
        steps.append(make_step(15000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Resumed"}))

    # 5. Deferred: disable short breaks temporarily, advance to 870s in FullscreenApp, Lock, change focus Normal, Unlock
    def_settings = make_settings(
        short_enabled=False,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
        inactivity_tracking=True,
        away_threshold_seconds=30,
        pause_during_fullscreen=True,
        strict_mode=True,
    )
    steps.append(make_step(0, action={"type": "update_settings", "settings": def_settings}))
    for _ in range(58):
        steps.append(make_step(15000, focus="FullscreenApp" if _ >= 56 else "Normal"))
    steps.append(make_step(1000, focus="FullscreenApp", action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000, focus="Normal"))
    steps.append(make_step(1000, focus="Normal", action={"type": "session", "kind": "Unlocked"}))

    # 6. Paused: pause 1m, Lock, Unlock (stays Paused), Lock, wait 60s, tick, Unlock
    steps.append(make_step(1000, action={"type": "pause", "minutes": 1}))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000, action={"type": "session", "kind": "Unlocked"}))
    steps.append(make_step(1000, action={"type": "session", "kind": "Locked"}))
    for _ in range(4):
        steps.append(make_step(15000))
    steps.append(make_step(1000, action={"type": "session", "kind": "Unlocked"}))

    # 7. Away (idle): enter Away, Lock, Unlock while still idle, then idle=0 -> Working
    steps.append(make_step(1000, idle_ms=35000))
    steps.append(make_step(1000, idle_ms=36000, action={"type": "session", "kind": "Locked"}))
    steps.append(make_step(5000, idle_ms=41000, action={"type": "session", "kind": "Unlocked"}))
    steps.append(make_step(1000, idle_ms=0))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-10-01T12:00:00Z",
        time_zone="UTC",
        settings=settings,
        initial_state=initial_state,
        steps=steps,
        tail_profile="chaotic",
    )


# Targeted scenario 8: DST Spring-Forward (America/New_York 2026-03-08 01:55 EST -> 03:00 EDT)
def gen_targeted_dst_spring(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=3,
        short_duration=15,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
    )
    steps: list[dict[str, Any]] = []
    # 40 * 15s = 600s (10 mins across 01:55 EST -> 03:05 EDT)
    for _ in range(45):
        steps.append(make_step(15000))
    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-03-08T06:55:00Z",
        time_zone="America/New_York",
        settings=settings,
        initial_state=None,
        steps=steps,
        tail_profile="workday",
    )


# Targeted scenario 9: DST Fall-Back (Europe/Prague 2026-10-25 02:55 CEST -> 02:00 CET)
def gen_targeted_dst_fall(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=3,
        short_duration=15,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=30,
    )
    steps: list[dict[str, Any]] = []
    for _ in range(45):
        steps.append(make_step(15000))
    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-10-25T00:55:00Z",
        time_zone="Europe/Prague",
        settings=settings,
        initial_state=None,
        steps=steps,
        tail_profile="workday",
    )


# Targeted scenario 10: Startup boundaries (15_001ms startup gap, FocusState.NotPresent, unsupported platform)
def gen_targeted_startup_and_focus_not_present(rng: random.Random, num_steps: int) -> dict[str, Any]:
    settings = make_settings(
        short_enabled=True,
        short_interval=2,
        short_duration=15,
        long_enabled=True,
        long_interval=15,
        long_duration=1,
        warning_seconds=15,
        inactivity_tracking=True,
        away_threshold_seconds=30,
    )
    # Startup gap == 15_001ms (>= 15s short duration) -> credits short absence on first tick!
    initial_state = {
        "schemaVersion": 1,
        "sinceShortBreak": fmt_dotnet_timespan(90),
        "sinceLongBreak": fmt_dotnet_timespan(90),
        "postponesUsed": 0,
        "pausedUntilUtc": None,
        "lastTickUtc": "2026-11-19T22:59:44.999Z",
    }
    steps: list[dict[str, Any]] = []
    # First tick consumes pending_startup_gap (15_001ms >= 15s -> resets sinceShortBreak)
    steps.append(make_step(0))
    # FocusState.NotPresent during Working -> Away -> Normal -> Working
    for _ in range(4):
        steps.append(make_step(15000))
    steps.append(make_step(1000, focus="NotPresent"))
    for _ in range(4):
        steps.append(make_step(15000, focus="NotPresent"))
    steps.append(make_step(1000, focus="Normal"))  # 61s NotPresent -> AbsenceCredited(Long)
    # Midnight crossing in Prague (22:59:45Z -> past 23:00Z = 00:00 CET)
    for _ in range(10):
        steps.append(make_step(15000))

    return finish_targeted_scenario(
        rng,
        num_steps,
        initial_utc="2026-11-19T23:00:00Z",
        time_zone="Europe/Prague",
        settings=settings,
        initial_state=initial_state,
        steps=steps,
        tail_profile="interactive",
    )


TARGETED_GENERATORS: list[Callable[[random.Random, int], dict[str, Any]]] = [
    gen_targeted_warning_postpone_skip,
    gen_targeted_exact_idle_boundaries,
    gen_targeted_sleep_gaps_and_suppression,
    gen_targeted_pause_expiry_and_resume,
    gen_targeted_settings_clamp_and_mid_updates,
    gen_targeted_fullscreen_deferral,
    gen_targeted_lock_unlock_all_phases_and_suspend,
    gen_targeted_dst_spring,
    gen_targeted_dst_fall,
    gen_targeted_startup_and_focus_not_present,
]


def gen_random_scenario(rng: random.Random, num_steps: int) -> dict[str, Any]:
    tz = rng.choice(TIME_ZONES)
    if rng.random() < 0.35:
        initial_str = rng.choice(INTERESTING_START_TIMES)
        initial_dt = datetime.fromisoformat(initial_str.replace("Z", "+00:00"))
    else:
        base = datetime(2026, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
        offset_secs = rng.randint(0, 364 * 86400)
        initial_dt = base + timedelta(seconds=offset_secs)
        initial_str = initial_dt.strftime("%Y-%m-%dT%H:%M:%SZ")

    settings = gen_settings(rng)
    initial_state = gen_initial_state(rng, initial_dt, settings)
    profile = rng.choices(["workday", "interactive", "chaotic"], weights=[40, 30, 30], k=1)[0]
    steps = gen_random_steps(rng, num_steps, profile)

    out: dict[str, Any] = {
        "initialUtc": initial_str,
        "timeZone": tz,
        "settings": settings,
        "initialState": initial_state,
        "steps": steps,
    }
    if rng.random() < 0.05:
        out["idleSupported"] = False
    if rng.random() < 0.05:
        out["focusSupported"] = False
    return out


def generate_scenarios(
    rng: random.Random, num_scenarios: int, num_steps: int
) -> list[dict[str, Any]]:
    scenarios: list[dict[str, Any]] = []
    for idx in range(num_scenarios):
        if idx < len(TARGETED_GENERATORS):
            scenarios.append(TARGETED_GENERATORS[idx](rng, num_steps))
        elif rng.random() < 0.15:
            gen_fn = rng.choice(TARGETED_GENERATORS)
            scenarios.append(gen_fn(rng, num_steps))
        else:
            scenarios.append(gen_random_scenario(rng, num_steps))
    return scenarios


def first_diff(
    cs_val: Any, rust_val: Any, path: str = ""
) -> tuple[str, Any, Any] | None:
    if type(cs_val) is not type(rust_val):
        return (path or "<root>", cs_val, rust_val)
    if isinstance(cs_val, dict):
        all_keys = list(dict.fromkeys(list(cs_val.keys()) + list(rust_val.keys())))
        for k in all_keys:
            sub_path = f"{path}.{k}" if path else k
            if k not in cs_val:
                return (sub_path, "<missing>", rust_val[k])
            if k not in rust_val:
                return (sub_path, cs_val[k], "<missing>")
            d = first_diff(cs_val[k], rust_val[k], sub_path)
            if d is not None:
                return d
        return None
    if isinstance(cs_val, list):
        if len(cs_val) != len(rust_val):
            return (f"{path}.length", len(cs_val), len(rust_val))
        for idx, (a, b) in enumerate(zip(cs_val, rust_val)):
            d = first_diff(a, b, f"{path}[{idx}]")
            if d is not None:
                return d
        return None
    if cs_val != rust_val:
        return (path or "<root>", cs_val, rust_val)
    return None


def compute_scenario_coverage(
    scenario: dict[str, Any], step_objs: list[dict[str, Any]]
) -> dict[str, int]:
    counts = {k: 0 for k in COVERAGE_KEYS}

    tz = ZoneInfo(scenario["timeZone"])
    initial_utc = datetime.fromisoformat(scenario["initialUtc"].replace("Z", "+00:00"))
    cur_utc = initial_utc
    cur_local = cur_utc.astimezone(tz)
    prev_local_date = cur_local.date()
    prev_utcoffset = cur_local.utcoffset()

    init_state = scenario.get("initialState")
    prev_phase = "Working"
    prev_paused_until: str | None = None
    prev_since_short_ms = 0
    prev_since_long_ms = 0
    has_pending_startup_gap = False

    if init_state:
        prev_since_short_ms = parse_dotnet_timespan_ms(init_state["sinceShortBreak"])
        prev_since_long_ms = parse_dotnet_timespan_ms(init_state["sinceLongBreak"])
        paused_str = init_state.get("pausedUntilUtc")
        if paused_str is not None:
            if paused_str == PAUSE_FOREVER_SENTINEL:
                prev_phase = "Paused"
                prev_paused_until = "until_resumed"
            else:
                paused_dt = datetime.fromisoformat(paused_str.replace("Z", "+00:00"))
                if initial_utc >= paused_dt:
                    prev_since_short_ms = 0
                    prev_since_long_ms = 0
                else:
                    prev_phase = "Paused"
                    prev_paused_until = paused_str
        elif init_state.get("lastTickUtc"):
            last_tick_dt = datetime.fromisoformat(
                init_state["lastTickUtc"].replace("Z", "+00:00")
            )
            gap_ms = int((initial_utc - last_tick_dt).total_seconds() * 1000)
            if gap_ms > 15000:
                has_pending_startup_gap = True

    ms_since_last_tick = 0

    for step, obj in zip(scenario["steps"], step_objs):
        adv_ms = int(step["advanceMs"])
        ms_since_last_tick += adv_ms
        if adv_ms > 0:
            cur_utc = cur_utc + timedelta(milliseconds=adv_ms)
            cur_local = cur_utc.astimezone(tz)
            if cur_local.date() != prev_local_date:
                counts["midnight_crossings"] += 1
                prev_local_date = cur_local.date()
            if cur_local.utcoffset() != prev_utcoffset:
                counts["dst_crossings"] += 1
                prev_utcoffset = cur_local.utcoffset()

        action = step.get("action") or {"type": "none"}
        atype = action["type"]
        res = obj.get("actionResult")
        cur_phase = obj["phase"]
        events = obj["events"]
        strict_mode = bool(obj["snapshot"]["strictMode"])

        if atype == "postpone":
            if res is True:
                counts["postpones_succeeded"] += 1
            elif res is False:
                counts["postpones_refused"] += 1
                if strict_mode and prev_phase == "LongBreakWarning":
                    counts["strict_mode_refusals"] += 1
        elif atype == "skip":
            if res is True:
                counts["skips_succeeded"] += 1
            elif res is False and strict_mode and prev_phase in ("LongBreak", "LongBreakWarning"):
                counts["strict_mode_refusals"] += 1
        elif atype in ("start_long_now", "start_short_now") and res is True:
            counts["start_now_actions"] += 1

        has_long_absence_ev = False
        has_break_ended_ev = False
        for ev in events:
            ev_type = ev.get("type")
            if ev_type == "BreakEnded":
                has_break_ended_ev = True
                if ev.get("outcome") == "Completed":
                    if ev.get("kind") == "Long":
                        counts["long_breaks_completed"] += 1
                    elif ev.get("kind") == "Short":
                        counts["short_breaks_completed"] += 1
            elif ev_type == "AbsenceCredited" and ev.get("kind") == "Long":
                has_long_absence_ev = True
                counts["long_absences_credited"] += 1
            elif ev_type == "PhaseChanged" and ev.get("phase") == "Deferred":
                counts["fullscreen_deferrals"] += 1

        is_sleep_gap = (
            atype == "tick"
            and prev_phase != "Paused"
            and not has_pending_startup_gap
            and ms_since_last_tick > 15000
        )
        if is_sleep_gap:
            counts["sleep_gaps"] += 1

        if (
            atype == "tick"
            and prev_phase == "Paused"
            and cur_phase == "Working"
            and prev_paused_until != "until_resumed"
        ):
            counts["pauses_expired"] += 1
        if atype == "resume" and prev_phase == "Paused" and cur_phase == "Working":
            counts["pauses_resumed"] += 1

        # Short absence credit resets sinceShortMs to 0 without an AbsenceCredited/BreakEnded/end_pause event.
        returned_from_gap_or_away = (
            (atype == "tick" and (has_pending_startup_gap or (is_sleep_gap and prev_phase not in ("ShortBreak", "LongBreak", "Away"))))
            or (prev_phase == "Away" and cur_phase == "Working")
        )
        if (
            returned_from_gap_or_away
            and prev_phase != "Paused"
            and not has_long_absence_ev
            and not has_break_ended_ev
            and prev_since_short_ms > 0
            and obj["sinceShortMs"] == 0
        ):
            counts["short_absences_credited"] += 1

        if atype == "tick":
            has_pending_startup_gap = False
            ms_since_last_tick = 0
        elif prev_phase == "Away" and cur_phase != "Away":
            ms_since_last_tick = 0

        prev_phase = cur_phase
        prev_paused_until = obj["pausedUntil"]
        prev_since_short_ms = obj["sinceShortMs"]
        prev_since_long_ms = obj["sinceLongMs"]

    return counts


def build_csharp_harness(dotnet: str) -> list[str]:
    print("Building C# harness...", flush=True)
    cs_cmd = [
        dotnet,
        "build",
        str(CSHARP_PROJ),
        "-c",
        "Release",
        "-nodeReuse:false",
        "-p:UseSharedCompilation=false",
    ]
    subprocess.run(cs_cmd, cwd=ROOT, check=True)
    cs_dll = (
        DIFF_DIR
        / "csharp-harness"
        / "bin"
        / "Release"
        / "net10.0"
        / "CsharpHarness.dll"
    )
    if not cs_dll.exists():
        raise RuntimeError(f"Expected C# harness DLL at {cs_dll}")
    return [dotnet, "exec", str(cs_dll)]


def build_rust_harness(
    manifest_path: pathlib.Path,
    cwd: pathlib.Path,
    *,
    release: bool = True,
    target_dir: pathlib.Path | None = None,
    quiet: bool = False,
) -> list[str]:
    if not quiet:
        print("Building Rust diff_harness...", flush=True)
    rs_cmd = [
        "cargo",
        "build",
        *(["--release"] if release else []),
        "--example",
        "diff_harness",
        "--locked",
        "--manifest-path",
        str(manifest_path),
        *(["--target-dir", str(target_dir)] if target_dir is not None else []),
        "--message-format=json-render-diagnostics",
    ]
    rs_build = subprocess.run(
        rs_cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8"
    )
    if rs_build.returncode != 0:
        raise RuntimeError(
            f"Rust cargo build failed (exit {rs_build.returncode}):\n{rs_build.stderr.strip()}"
        )
    rs_bin: pathlib.Path | None = None
    for line in rs_build.stdout.splitlines():
        if not line.startswith("{"):
            continue
        msg = json.loads(line)
        if (
            msg.get("reason") == "compiler-artifact"
            and msg.get("target", {}).get("name") == "diff_harness"
        ):
            exe = msg.get("executable")
            if exe:
                rs_bin = pathlib.Path(exe)
    if rs_bin is None or not rs_bin.exists():
        raise RuntimeError(f"Expected Rust harness binary from cargo build (got {rs_bin})")
    return [str(rs_bin)]


def run_harness_process(
    cmd: list[str], scenario_path: pathlib.Path, label: str
) -> tuple[list[str] | None, str | None]:
    try:
        proc = subprocess.run(
            [*cmd, str(scenario_path)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=HARNESS_TIMEOUT_SECS,
        )
    except subprocess.TimeoutExpired:
        return None, f"{label} harness timed out after {HARNESS_TIMEOUT_SECS}s"
    except OSError as exc:
        return None, f"{label} harness failed to launch: {exc}"

    if proc.returncode != 0:
        stderr = proc.stderr.strip() or "<no stderr>"
        return None, f"{label} harness exited {proc.returncode}: {stderr}"

    return proc.stdout.splitlines(), None


def compare_harness_outputs(
    scenario: dict[str, Any],
    cs_lines: list[str],
    rs_lines: list[str],
) -> dict[str, Any] | None:
    if len(cs_lines) != len(rs_lines):
        return {
            "step": "line_count",
            "field": "lines",
            "cs": len(cs_lines),
            "rust": len(rs_lines),
            "scenario": scenario,
        }

    for line_idx, (cs_line, rs_line) in enumerate(zip(cs_lines, rs_lines)):
        try:
            cs_obj = json.loads(cs_line)
        except json.JSONDecodeError as exc:
            return {
                "error": f"C# harness emitted invalid JSON at line {line_idx}: {exc}",
                "step": line_idx,
                "field": "json",
                "scenario": scenario,
            }
        try:
            rs_obj = json.loads(rs_line)
        except json.JSONDecodeError as exc:
            return {
                "error": f"Rust harness emitted invalid JSON at line {line_idx}: {exc}",
                "step": line_idx,
                "field": "json",
                "scenario": scenario,
            }
        diff = first_diff(cs_obj, rs_obj)
        if diff is not None:
            field, cs_val, rs_val = diff
            step_label = (
                "exportedState"
                if "exportedState" in cs_obj
                else cs_obj.get("step", line_idx)
            )
            return {
                "step": step_label,
                "field": field,
                "cs": cs_val,
                "rust": rs_val,
                "scenario": scenario,
            }

    return None


def parse_step_objects(lines: list[str], expected_steps: int) -> list[dict[str, Any]] | None:
    if len(lines) < expected_steps:
        return None
    objs: list[dict[str, Any]] = []
    for line in lines[:expected_steps]:
        try:
            objs.append(json.loads(line))
        except json.JSONDecodeError:
            return None
    return objs


def run_one_scenario(
    idx: int,
    scenario: dict[str, Any],
    cs_base_cmd: list[str],
    rs_base_cmd: list[str],
    tmp_dir: pathlib.Path,
) -> tuple[int, dict[str, Any] | None, dict[str, int] | None]:
    try:
        scenario_path = tmp_dir / f"scenario_{idx:05d}.json"
        scenario_path.write_text(json.dumps(scenario), encoding="utf-8")

        cs_lines, cs_err = run_harness_process(cs_base_cmd, scenario_path, "C#")
        if cs_err is not None or cs_lines is None:
            return idx, {"error": cs_err or "C# harness failed", "scenario": scenario}, None

        cov: dict[str, int] | None = None
        cs_step_objs = parse_step_objects(cs_lines, len(scenario["steps"]))
        if cs_step_objs is not None:
            cov = compute_scenario_coverage(scenario, cs_step_objs)

        rs_lines, rs_err = run_harness_process(rs_base_cmd, scenario_path, "Rust")
        if rs_err is not None or rs_lines is None:
            return idx, {"error": rs_err or "Rust harness failed", "scenario": scenario}, cov

        diff_fail = compare_harness_outputs(scenario, cs_lines, rs_lines)
        return idx, diff_fail, cov
    except Exception as exc:
        return idx, {"error": f"unexpected scenario runner error: {exc}", "scenario": scenario}, None


def print_coverage_table(totals: dict[str, int]) -> None:
    print("Coverage counters:", flush=True)
    for key in COVERAGE_KEYS:
        print(f"  {key:<28} {totals[key]}", flush=True)


def run_normal_mode(args: argparse.Namespace) -> int:
    failures: list[tuple[int, dict[str, Any]]] = []
    totals = {k: 0 for k in COVERAGE_KEYS}
    fatal_error: str | None = None
    cov_failures: list[str] = []

    try:
        dotnet = find_dotnet()
        cs_base_cmd = build_csharp_harness(dotnet)
        rs_base_cmd = build_rust_harness(RUST_MANIFEST, RUST_DIR, release=True)

        rng = random.Random(args.seed)
        scenarios = generate_scenarios(rng, args.scenarios, args.steps)

        with tempfile.TemporaryDirectory(prefix="ocy-diff-") as tmp:
            tmp_dir = pathlib.Path(tmp)
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
                futures = [
                    pool.submit(
                        run_one_scenario, i, sc, cs_base_cmd, rs_base_cmd, tmp_dir
                    )
                    for i, sc in enumerate(scenarios)
                ]
                for fut in concurrent.futures.as_completed(futures):
                    idx, fail, cov = fut.result()
                    if cov is not None:
                        for k, v in cov.items():
                            totals[k] += v
                    if fail is not None:
                        failures.append((idx, fail))

        failures.sort(key=lambda item: item[0])

        if failures:
            FAILURES_DIR.mkdir(parents=True, exist_ok=True)
            print(
                f"Diverging scenarios ({len(failures)}/{args.scenarios}):",
                flush=True,
            )
            for idx, fail in failures:
                fail_file = FAILURES_DIR / f"seed{args.seed}_scenario{idx:05d}.json"
                fail_file.write_text(
                    json.dumps(fail["scenario"], indent=2), encoding="utf-8"
                )
                if "error" in fail:
                    print(
                        f"  scenario {idx}: ERROR: {fail['error']} (saved {fail_file.name})",
                        flush=True,
                    )
                else:
                    print(
                        f"  scenario {idx}: step={fail['step']} field={fail['field']} "
                        f"cs={json.dumps(fail['cs'])} rust={json.dumps(fail['rust'])} "
                        f"(saved {fail_file.name})",
                        flush=True,
                    )

        print_coverage_table(totals)
        if args.scenarios >= len(TARGETED_GENERATORS):
            cov_failures = [k for k in COVERAGE_KEYS if totals[k] == 0]
            if cov_failures:
                print(
                    f"COVERAGE FAILURE: zero-count counters: {', '.join(cov_failures)}",
                    flush=True,
                )
        else:
            print(
                f"Note: coverage gate skipped because --scenarios {args.scenarios} < {len(TARGETED_GENERATORS)}",
                flush=True,
            )
    except Exception as exc:
        fatal_error = str(exc)
        print(f"diff: fatal error: {fatal_error}", file=sys.stderr, flush=True)
    finally:
        if fatal_error is not None:
            print(
                f"diff: {args.scenarios} scenarios x {args.steps} steps (seed={args.seed}) -> ERROR ({fatal_error})",
                flush=True,
            )
        elif failures:
            print(
                f"diff: {args.scenarios} scenarios x {args.steps} steps (seed={args.seed}) -> {len(failures)} divergences",
                flush=True,
            )
        elif cov_failures:
            print(
                f"diff: {args.scenarios} scenarios x {args.steps} steps (seed={args.seed}) -> 0 divergences (COVERAGE FAIL: {len(cov_failures)} zero counters)",
                flush=True,
            )
        else:
            print(
                f"diff: {args.scenarios} scenarios x {args.steps} steps (seed={args.seed}) -> 0 divergences",
                flush=True,
            )

    if fatal_error is not None or failures or cov_failures:
        return 1
    return 0


def run_self_test(args: argparse.Namespace) -> int:
    selftest_scenarios = 16
    selftest_steps = 180
    selftest_seed = 20261119
    results: list[tuple[str, str, str, str, str]] = []
    undetected: list[str] = []
    fatal_error: str | None = None

    try:
        dotnet = find_dotnet()
        cs_base_cmd = build_csharp_harness(dotnet)

        rng = random.Random(selftest_seed)
        scenarios = generate_scenarios(rng, selftest_scenarios, selftest_steps)

        with tempfile.TemporaryDirectory(prefix="ocy-selftest-") as tmp:
            tmp_root = pathlib.Path(tmp)
            scenarios_dir = tmp_root / "scenarios"
            scenarios_dir.mkdir()

            print(
                f"Running C# oracle once on {selftest_scenarios} scenarios x {selftest_steps} steps (seed={selftest_seed})...",
                flush=True,
            )
            cs_outputs: list[tuple[pathlib.Path, list[str]]] = []
            for idx, sc in enumerate(scenarios):
                sc_path = scenarios_dir / f"scenario_{idx:05d}.json"
                sc_path.write_text(json.dumps(sc), encoding="utf-8")
                cs_lines, cs_err = run_harness_process(cs_base_cmd, sc_path, "C#")
                if cs_err is not None or cs_lines is None:
                    raise RuntimeError(f"C# oracle failed on scenario {idx}: {cs_err}")
                cs_outputs.append((sc_path, cs_lines))

            # Build an isolated temporary copy of the whole Rust workspace (every member
            # must be present for the root manifest to resolve; only build output is
            # excluded).
            ws_dir = tmp_root / "rust"
            shutil.copytree(RUST_DIR, ws_dir, ignore=shutil.ignore_patterns("target"))
            ocy_core_tmp = ws_dir / "apps" / "ocy" / "ocy-core"
            target_dir = tmp_root / "target"

            # Cache pristine source bytes for all mutant target files.
            pristine_files: dict[str, str] = {}
            for m in SELF_TEST_MUTANTS:
                if m.rel_path not in pristine_files:
                    pristine_files[m.rel_path] = (ocy_core_tmp / m.rel_path).read_text(
                        encoding="utf-8"
                    )

            print(
                f"Evaluating {len(SELF_TEST_MUTANTS)} mutants in isolated temp copy...",
                flush=True,
            )
            for m in SELF_TEST_MUTANTS:
                target_file = ocy_core_tmp / m.rel_path
                src_text = pristine_files[m.rel_path]
                occurrences = src_text.count(m.old)
                if occurrences != 1:
                    raise RuntimeError(
                        f"Mutant {m.name} expected 1 occurrence in {m.rel_path}, found {occurrences}"
                    )

                target_file.write_text(src_text.replace(m.old, m.new, 1), encoding="utf-8")
                try:
                    rs_cmd = build_rust_harness(
                        ws_dir / "Cargo.toml",
                        ws_dir,
                        release=False,
                        target_dir=target_dir,
                        quiet=True,
                    )

                    def eval_one(item: tuple[int, dict[str, Any], pathlib.Path, list[str]]) -> tuple[int, dict[str, Any] | None]:
                        idx, sc, sc_path, cs_lines = item
                        rs_lines, rs_err = run_harness_process(rs_cmd, sc_path, "Rust")
                        if rs_err is not None or rs_lines is None:
                            return idx, {"error": rs_err or "Rust harness failed", "step": 0, "field": "crash/timeout", "scenario": sc}
                        return idx, compare_harness_outputs(sc, cs_lines, rs_lines)

                    first_hit: tuple[int, dict[str, Any]] | None = None
                    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
                        items = [
                            (idx, scenarios[idx], cs_outputs[idx][0], cs_outputs[idx][1])
                            for idx in range(selftest_scenarios)
                        ]
                        hits = [
                            (idx, fail)
                            for idx, fail in pool.map(eval_one, items)
                            if fail is not None
                        ]
                    if hits:
                        hits.sort(key=lambda h: h[0])
                        first_hit = hits[0]

                    if first_hit is None:
                        undetected.append(m.name)
                        results.append((m.name, "MISSED", "-", "-", "0 divergences"))
                    else:
                        s_idx, fail = first_hit
                        step_str = str(fail.get("step", 0))
                        detail = fail.get("field", fail.get("error", "divergence"))
                        results.append((m.name, "DETECTED", str(s_idx), step_str, str(detail)))
                except Exception as exc:
                    undetected.append(m.name)
                    results.append((m.name, "ERROR", "-", "-", str(exc)))
                finally:
                    target_file.write_text(src_text, encoding="utf-8")

        print("", flush=True)
        print(
            f"{'Mutant':<44} {'Status':<10} {'Scenario':<9} {'Step':<14} {'Field / Detail'}",
            flush=True,
        )
        print("-" * 100, flush=True)
        for name, status, sc_s, step_s, detail in results:
            print(
                f"{name:<44} {status:<10} {sc_s:<9} {step_s:<14} {detail}",
                flush=True,
            )
    except Exception as exc:
        fatal_error = str(exc)
        print(f"self-test: fatal error: {fatal_error}", file=sys.stderr, flush=True)
    finally:
        detected_count = len(SELF_TEST_MUTANTS) - len(undetected)
        if fatal_error is not None:
            print(
                f"self-test: ERROR ({fatal_error}) -> {detected_count}/{len(SELF_TEST_MUTANTS)} mutants detected",
                flush=True,
            )
        else:
            print(
                f"self-test: {detected_count}/{len(SELF_TEST_MUTANTS)} mutants detected "
                f"({selftest_scenarios} scenarios x {selftest_steps} steps, seed={selftest_seed})",
                flush=True,
            )

    if fatal_error is not None or undetected:
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Ocy C# vs Rust differential test runner")
    parser.add_argument("--scenarios", type=int, default=300, help="Number of scenarios")
    parser.add_argument("--steps", type=int, default=400, help="Steps per scenario")
    parser.add_argument("--seed", type=int, default=20261119, help="PRNG seed")
    parser.add_argument(
        "--workers",
        type=int,
        default=min(8, os.cpu_count() or 4),
        help="Parallel worker processes",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run mutation self-test on an isolated temporary copy of ocy-core",
    )
    args = parser.parse_args()

    if args.self_test:
        return run_self_test(args)
    return run_normal_mode(args)


if __name__ == "__main__":
    sys.exit(main())
