//! Tray icon presenter, theme set selector, and controller (port of `Tray/*.cs`).

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::{AppThemeProvider, Clock, TaskbarThemeProvider};
use jiff::{SignedDuration, Timestamp};
use ocy_core::engine::{EnginePhase, EngineSnapshot};
use ocy_core::time::Paused;

use crate::strings::Strings;

const LIGHT_ACTIVE_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/light/png/tray-active-32.png");
const LIGHT_PAUSED_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/light/png/tray-paused-32.png");
const LIGHT_ONBREAK_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/light/png/tray-onbreak-32.png");
const LIGHT_ATTENTION_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/light/png/tray-attention-32.png");

const DARK_ACTIVE_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/dark/png/tray-active-32.png");
const DARK_PAUSED_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/dark/png/tray-paused-32.png");
const DARK_ONBREAK_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/dark/png/tray-onbreak-32.png");
const DARK_ATTENTION_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/tray/dark/png/tray-attention-32.png");

/// Visual state of the tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayIconState {
    Active,
    Paused,
    OnBreak,
    Attention,
}

/// Application light/dark theme fallback when the OS taskbar theme is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppTheme {
    Light,
    Dark,
}

/// Converts [`AppThemeProvider::is_light_app_theme`] into `Option<AppTheme>`.
#[must_use]
pub const fn app_theme_from_opt(is_light_app_theme: Option<bool>) -> Option<AppTheme> {
    match is_light_app_theme {
        Some(true) => Some(AppTheme::Light),
        Some(false) => Some(AppTheme::Dark),
        None => None,
    }
}

/// Picks the light or dark tray icon set from taskbar theme and app theme fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct TraySetSelector;

impl TraySetSelector {
    /// Returns `true` when the light-taskbar icon set should be used.
    #[must_use]
    pub const fn use_light_set(
        is_light_taskbar: Option<bool>,
        app_theme: Option<AppTheme>,
    ) -> bool {
        match is_light_taskbar {
            Some(light) => light,
            None => matches!(app_theme, Some(AppTheme::Light)),
        }
    }
}

/// Returns the embedded 32x32 PNG bytes for `(state, use_light_set)`.
#[must_use]
pub const fn tray_icon_png_bytes(state: TrayIconState, use_light_set: bool) -> &'static [u8] {
    match (use_light_set, state) {
        (true, TrayIconState::Active) => LIGHT_ACTIVE_PNG,
        (true, TrayIconState::Paused) => LIGHT_PAUSED_PNG,
        (true, TrayIconState::OnBreak) => LIGHT_ONBREAK_PNG,
        (true, TrayIconState::Attention) => LIGHT_ATTENTION_PNG,
        (false, TrayIconState::Active) => DARK_ACTIVE_PNG,
        (false, TrayIconState::Paused) => DARK_PAUSED_PNG,
        (false, TrayIconState::OnBreak) => DARK_ONBREAK_PNG,
        (false, TrayIconState::Attention) => DARK_ATTENTION_PNG,
    }
}

/// Decodes the embedded 32x32 PNG icon into premultiplied RGBA8 pixels `(width, height, data)`.
pub fn decode_tray_icon_rgba(
    state: TrayIconState,
    use_light_set: bool,
) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = tray_icon_png_bytes(state, use_light_set);
    let pixmap =
        resvg::tiny_skia::Pixmap::decode_png(bytes).map_err(|err| format!("png decode: {err}"))?;
    Ok((pixmap.width(), pixmap.height(), pixmap.take()))
}

/// Which tray pause menu option initiated the current pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PauseSource {
    OneHour,
    ThreeHours,
    UntilResumed,
}

impl PauseSource {
    /// Duration passed to `BreakEngine::pause_for` (`None` for indefinite pause).
    #[must_use]
    pub const fn duration(self) -> Option<Duration> {
        match self {
            Self::OneHour => Some(Duration::from_secs(3_600)),
            Self::ThreeHours => Some(Duration::from_secs(3 * 3_600)),
            Self::UntilResumed => None,
        }
    }
}

/// Presentation state of a single tray menu item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItemState {
    pub text: String,
    pub is_visible: bool,
    pub is_enabled: bool,
    pub is_checked: bool,
}

impl TrayMenuItemState {
    fn new(text: &str, is_visible: bool, is_enabled: bool) -> Self {
        Self {
            text: text.to_owned(),
            is_visible,
            is_enabled,
            is_checked: false,
        }
    }

    fn checkable(text: &str, is_visible: bool, is_enabled: bool, is_checked: bool) -> Self {
        Self {
            text: text.to_owned(),
            is_visible,
            is_enabled,
            is_checked,
        }
    }
}

/// Logical slot in the tray context menu, in top-to-bottom display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayMenuSlot {
    PauseStatus,
    TakeShortBreakNow,
    TakeLongBreakNow,
    Separator1,
    Pause1Hour,
    Pause3Hours,
    PauseUntilResumed,
    Resume,
    Separator2,
    Settings,
    Quit,
}

/// Canonical top-to-bottom order of entries in the tray context menu.
pub const TRAY_MENU_ORDER: [TrayMenuSlot; 11] = [
    TrayMenuSlot::PauseStatus,
    TrayMenuSlot::TakeShortBreakNow,
    TrayMenuSlot::TakeLongBreakNow,
    TrayMenuSlot::Separator1,
    TrayMenuSlot::Pause1Hour,
    TrayMenuSlot::Pause3Hours,
    TrayMenuSlot::PauseUntilResumed,
    TrayMenuSlot::Resume,
    TrayMenuSlot::Separator2,
    TrayMenuSlot::Settings,
    TrayMenuSlot::Quit,
];

/// One ordered entry from [`TrayPresentation::ordered_menu_entries`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuEntry<'a> {
    Item(TrayMenuSlot, &'a TrayMenuItemState),
    Separator,
}

/// Returns `true` when the engine can start a short break now from the tray menu:
/// only in `Working` or `Deferred` phases (disabled during any break or warning, while paused or
/// away), regardless of whether scheduled short breaks are enabled.
#[must_use]
pub const fn can_take_short_break_now(snapshot: &EngineSnapshot) -> bool {
    matches!(snapshot.phase, EnginePhase::Working | EnginePhase::Deferred)
}

/// Returns `true` when the engine can start a long break now from the tray menu.
#[must_use]
pub const fn can_take_long_break_now(snapshot: &EngineSnapshot) -> bool {
    matches!(
        snapshot.phase,
        EnginePhase::Working | EnginePhase::LongBreakWarning | EnginePhase::Deferred
    )
}

/// Complete tray icon + tooltip + menu presentation derived from an [`EngineSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayPresentation {
    pub tooltip: String,
    pub icon_state: TrayIconState,
    pub pause_status: TrayMenuItemState,
    pub take_short_break_now: TrayMenuItemState,
    pub take_long_break_now: TrayMenuItemState,
    pub pause_1_hour: TrayMenuItemState,
    pub pause_3_hours: TrayMenuItemState,
    pub pause_until_resumed: TrayMenuItemState,
    pub resume: TrayMenuItemState,
    pub settings: TrayMenuItemState,
    pub quit: TrayMenuItemState,
}

impl TrayPresentation {
    /// Returns the tray menu entries in top-to-bottom order matching `OcyTray`.
    #[must_use]
    pub fn ordered_menu_entries(&self) -> Vec<TrayMenuEntry<'_>> {
        TRAY_MENU_ORDER
            .iter()
            .map(|&slot| match slot {
                TrayMenuSlot::PauseStatus => TrayMenuEntry::Item(slot, &self.pause_status),
                TrayMenuSlot::TakeShortBreakNow => {
                    TrayMenuEntry::Item(slot, &self.take_short_break_now)
                }
                TrayMenuSlot::TakeLongBreakNow => {
                    TrayMenuEntry::Item(slot, &self.take_long_break_now)
                }
                TrayMenuSlot::Separator1 | TrayMenuSlot::Separator2 => TrayMenuEntry::Separator,
                TrayMenuSlot::Pause1Hour => TrayMenuEntry::Item(slot, &self.pause_1_hour),
                TrayMenuSlot::Pause3Hours => TrayMenuEntry::Item(slot, &self.pause_3_hours),
                TrayMenuSlot::PauseUntilResumed => {
                    TrayMenuEntry::Item(slot, &self.pause_until_resumed)
                }
                TrayMenuSlot::Resume => TrayMenuEntry::Item(slot, &self.resume),
                TrayMenuSlot::Settings => TrayMenuEntry::Item(slot, &self.settings),
                TrayMenuSlot::Quit => TrayMenuEntry::Item(slot, &self.quit),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayMenuSnapshot {
    pause_status: TrayMenuItemState,
    take_short_break_now: TrayMenuItemState,
    take_long_break_now: TrayMenuItemState,
    pause_1_hour: TrayMenuItemState,
    pause_3_hours: TrayMenuItemState,
    pause_until_resumed: TrayMenuItemState,
    resume: TrayMenuItemState,
    settings: TrayMenuItemState,
    quit: TrayMenuItemState,
}

impl From<&TrayPresentation> for TrayMenuSnapshot {
    fn from(p: &TrayPresentation) -> Self {
        Self {
            pause_status: p.pause_status.clone(),
            take_short_break_now: p.take_short_break_now.clone(),
            take_long_break_now: p.take_long_break_now.clone(),
            pause_1_hour: p.pause_1_hour.clone(),
            pause_3_hours: p.pause_3_hours.clone(),
            pause_until_resumed: p.pause_until_resumed.clone(),
            resume: p.resume.clone(),
            settings: p.settings.clone(),
            quit: p.quit.clone(),
        }
    }
}

/// Pure presenter mapping [`EngineSnapshot`] -> [`TrayPresentation`].
pub struct TrayPresenter {
    strings: Arc<Strings>,
    clock: Arc<dyn Clock>,
    active_pause_source: Cell<Option<PauseSource>>,
}

impl fmt::Debug for TrayPresenter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayPresenter")
            .field("active_pause_source", &self.active_pause_source.get())
            .finish_non_exhaustive()
    }
}

impl TrayPresenter {
    /// Creates a `TrayPresenter` backed by `strings` and `clock`.
    #[must_use]
    pub fn new(strings: Arc<Strings>, clock: Arc<dyn Clock>) -> Self {
        Self {
            strings,
            clock,
            active_pause_source: Cell::new(None),
        }
    }

    /// Replaces the active [`Strings`] table (used for live language switching).
    pub fn set_strings(&mut self, strings: Arc<Strings>) {
        self.strings = strings;
    }

    /// Borrows the active [`Strings`] table.
    #[must_use]
    pub const fn strings(&self) -> &Arc<Strings> {
        &self.strings
    }

    /// Records which tray pause option initiated the current pause.
    pub fn record_pause(&self, source: PauseSource) {
        self.active_pause_source.set(Some(source));
    }

    /// Clears the remembered pause option (used when paused outside the tray presets or resumed).
    pub fn clear_pause_source(&self) {
        self.active_pause_source.set(None);
    }

    /// Returns the currently remembered pause source, if any.
    #[must_use]
    pub fn active_pause_source(&self) -> Option<PauseSource> {
        self.active_pause_source.get()
    }

    /// Computes the tray presentation for `snapshot` at `now` (defaults to `clock.now()`).
    #[must_use]
    pub fn present(&self, snapshot: &EngineSnapshot, now: Option<Timestamp>) -> TrayPresentation {
        let current_zoned = self.clock.now();
        let current_utc = now.unwrap_or_else(|| current_zoned.timestamp());
        let icon_state = compute_icon_state(snapshot.phase);
        let tooltip = self.compute_tooltip(snapshot, current_utc);

        let is_strict_long_break = snapshot.phase == EnginePhase::LongBreak && snapshot.strict_mode;
        let is_paused = snapshot.phase == EnginePhase::Paused;

        let effective_pause_source = if !is_paused {
            self.active_pause_source.set(None);
            None
        } else {
            match snapshot.paused_until_utc {
                None | Some(Paused::UntilResumed) => Some(PauseSource::UntilResumed),
                Some(Paused::Until(_)) => match self.active_pause_source.get() {
                    Some(PauseSource::OneHour) => Some(PauseSource::OneHour),
                    Some(PauseSource::ThreeHours) => Some(PauseSource::ThreeHours),
                    Some(PauseSource::UntilResumed) | None => None,
                },
            }
        };

        let pause_status = if is_paused {
            let text = self.compute_pause_status_text(snapshot, &current_zoned, current_utc);
            TrayMenuItemState::new(&text, true, false)
        } else {
            TrayMenuItemState::new("", false, false)
        };

        let take_short_break_text = self.strings.get("tray.menu.takeShortBreak");
        let take_long_break_text = self.strings.get("tray.menu.takeLongBreak");
        let pause_1h_text = self.strings.get("tray.menu.pause1h");
        let pause_3h_text = self.strings.get("tray.menu.pause3h");
        let pause_until_resumed_text = self.strings.get("tray.menu.pauseUntilResumed");
        let resume_text = self.strings.get("tray.menu.resume");
        let settings_text = self.strings.get("tray.menu.settings");
        let quit_text = self.strings.get("tray.menu.quit");

        if is_strict_long_break {
            return TrayPresentation {
                tooltip,
                icon_state,
                pause_status,
                take_short_break_now: TrayMenuItemState::new(take_short_break_text, true, false),
                take_long_break_now: TrayMenuItemState::new(take_long_break_text, true, false),
                pause_1_hour: TrayMenuItemState::new(pause_1h_text, true, false),
                pause_3_hours: TrayMenuItemState::new(pause_3h_text, true, false),
                pause_until_resumed: TrayMenuItemState::new(pause_until_resumed_text, true, false),
                resume: TrayMenuItemState::new(resume_text, false, false),
                settings: TrayMenuItemState::new(settings_text, true, false),
                quit: TrayMenuItemState::new(quit_text, true, true),
            };
        }

        let can_start_short_break = can_take_short_break_now(snapshot);
        let can_start_long_break = can_take_long_break_now(snapshot);

        TrayPresentation {
            tooltip,
            icon_state,
            pause_status,
            take_short_break_now: TrayMenuItemState::new(
                take_short_break_text,
                true,
                can_start_short_break,
            ),
            take_long_break_now: TrayMenuItemState::new(
                take_long_break_text,
                true,
                can_start_long_break,
            ),
            pause_1_hour: TrayMenuItemState::checkable(
                pause_1h_text,
                true,
                true,
                effective_pause_source == Some(PauseSource::OneHour),
            ),
            pause_3_hours: TrayMenuItemState::checkable(
                pause_3h_text,
                true,
                true,
                effective_pause_source == Some(PauseSource::ThreeHours),
            ),
            pause_until_resumed: TrayMenuItemState::checkable(
                pause_until_resumed_text,
                true,
                true,
                effective_pause_source == Some(PauseSource::UntilResumed),
            ),
            resume: TrayMenuItemState::new(resume_text, is_paused, is_paused),
            settings: TrayMenuItemState::new(settings_text, true, true),
            quit: TrayMenuItemState::new(quit_text, true, true),
        }
    }

    fn compute_pause_status_text(
        &self,
        snapshot: &EngineSnapshot,
        current_zoned: &jiff::ZonedDateTime,
        now: Timestamp,
    ) -> String {
        match snapshot.paused_until_utc {
            None | Some(Paused::UntilResumed) => {
                self.strings.get("tray.menu.pausedIndefinitely").to_owned()
            }
            Some(Paused::Until(until)) => {
                let remaining = until
                    .timestamp()
                    .duration_since(now)
                    .max(SignedDuration::ZERO);
                let time_text = self.format_pause_remainder(remaining);
                let local_until = until
                    .timestamp()
                    .to_zoned_datetime(current_zoned.time_zone().clone());
                let clock_text = format!("{:02}:{:02}", local_until.hour(), local_until.minute());
                self.strings.format(
                    "tray.menu.pausedUntil",
                    &[("clock", &clock_text), ("time", &time_text)],
                )
            }
        }
    }

    fn format_pause_remainder(&self, remaining: SignedDuration) -> String {
        if remaining < SignedDuration::from_mins(1) {
            self.strings.get("time.underOneMinute").to_owned()
        } else {
            self.strings.format_time_signed(remaining)
        }
    }

    fn compute_tooltip(&self, snapshot: &EngineSnapshot, now: Timestamp) -> String {
        if matches!(
            snapshot.phase,
            EnginePhase::ShortBreak | EnginePhase::LongBreak
        ) {
            return self.strings.get("tray.tooltip.onBreak").to_owned();
        }

        if snapshot.phase == EnginePhase::Paused {
            return match snapshot.paused_until_utc {
                None | Some(Paused::UntilResumed) => self
                    .strings
                    .get("tray.tooltip.pausedIndefinitely")
                    .to_owned(),
                Some(Paused::Until(until)) => {
                    let remaining = until
                        .timestamp()
                        .duration_since(now)
                        .max(SignedDuration::ZERO);
                    let formatted = self.strings.format_time_signed(remaining);
                    self.strings
                        .format("tray.tooltip.paused", &[("time", &formatted)])
                }
            };
        }

        let next_break = match (
            snapshot.time_until_short_break,
            snapshot.time_until_long_break,
        ) {
            (Some(s), Some(l)) => Some(s.min(l)),
            (Some(s), None) => Some(s),
            (None, Some(l)) => Some(l),
            (None, None) => None,
        };

        if let Some(next) = next_break {
            let formatted = self.strings.format_time(next);
            return self
                .strings
                .format("tray.tooltip.working", &[("time", &formatted)]);
        }

        self.strings.get("app.name").to_owned()
    }
}

const fn compute_icon_state(phase: EnginePhase) -> TrayIconState {
    match phase {
        EnginePhase::LongBreakWarning => TrayIconState::Attention,
        EnginePhase::Paused => TrayIconState::Paused,
        EnginePhase::ShortBreak | EnginePhase::LongBreak => TrayIconState::OnBreak,
        _ => TrayIconState::Active,
    }
}

/// Platform/UI tray backend sink driven by [`TrayController`].
pub trait TraySink {
    /// Updates the hover tooltip text.
    fn set_tooltip(&mut self, tooltip: &str);

    /// Applies a new tray icon for `(state, use_light_set)`.
    fn set_icon(&mut self, state: TrayIconState, use_light_set: bool);

    /// Updates menu item labels, visibility, and enabled flags.
    fn set_menu(&mut self, presentation: &TrayPresentation);
}

/// Minimum interval between taskbar theme registry re-checks.
const TASKBAR_THEME_RECHECK_INTERVAL: SignedDuration = SignedDuration::from_secs(60);

/// Drives a [`TraySink`] from engine snapshots, rate-limiting taskbar theme checks to 60 s
/// and swapping icons only when state or theme set changes.
pub struct TrayController<S: TraySink> {
    presenter: TrayPresenter,
    sink: S,
    taskbar_theme: Rc<dyn TaskbarThemeProvider>,
    clock: Arc<dyn Clock>,
    app_theme: Option<AppTheme>,
    last_taskbar_theme_check_utc: Option<Timestamp>,
    cached_is_light_taskbar: Option<bool>,
    use_light_set: bool,
    current_icon_state: Option<TrayIconState>,
    current_use_light_set: Option<bool>,
    last_tooltip: Option<String>,
    last_menu: Option<TrayMenuSnapshot>,
}

impl<S: TraySink + fmt::Debug> fmt::Debug for TrayController<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayController")
            .field("sink", &self.sink)
            .field("use_light_set", &self.use_light_set)
            .field("current_icon_state", &self.current_icon_state)
            .finish_non_exhaustive()
    }
}

impl<S: TraySink> TrayController<S> {
    /// Creates a `TrayController`, immediately evaluates the taskbar theme, and renders `initial_snapshot`.
    pub fn new(
        presenter: TrayPresenter,
        sink: S,
        taskbar_theme: Rc<dyn TaskbarThemeProvider>,
        clock: Arc<dyn Clock>,
        app_theme: Option<AppTheme>,
        initial_snapshot: &EngineSnapshot,
    ) -> Self {
        let mut controller = Self {
            presenter,
            sink,
            taskbar_theme,
            clock,
            app_theme,
            last_taskbar_theme_check_utc: None,
            cached_is_light_taskbar: None,
            use_light_set: false,
            current_icon_state: None,
            current_use_light_set: None,
            last_tooltip: None,
            last_menu: None,
        };
        controller.reevaluate_set(true);
        controller.update(initial_snapshot);
        controller
    }

    /// Updates the tray presentation from `snapshot`.
    pub fn update(&mut self, snapshot: &EngineSnapshot) {
        self.reevaluate_set(false);

        let now = self.clock.now().timestamp();
        let presentation = self.presenter.present(snapshot, Some(now));

        if self.last_tooltip.as_deref() != Some(presentation.tooltip.as_str()) {
            self.sink.set_tooltip(&presentation.tooltip);
            self.last_tooltip = Some(presentation.tooltip.clone());
        }

        if self.current_icon_state != Some(presentation.icon_state)
            || self.current_use_light_set != Some(self.use_light_set)
        {
            self.current_icon_state = Some(presentation.icon_state);
            self.current_use_light_set = Some(self.use_light_set);
            self.sink
                .set_icon(presentation.icon_state, self.use_light_set);
        }

        let menu = TrayMenuSnapshot::from(&presentation);
        if self.last_menu.as_ref() != Some(&menu) {
            self.sink.set_menu(&presentation);
            self.last_menu = Some(menu);
        }
    }

    /// Forces an immediate taskbar theme re-evaluation and tray update.
    pub fn force_theme_refresh(&mut self, snapshot: &EngineSnapshot) {
        self.reevaluate_set(true);
        self.update(snapshot);
    }

    /// Whether the light-taskbar icon set is currently active.
    #[must_use]
    pub const fn current_use_light_set(&self) -> bool {
        self.use_light_set
    }

    /// Current icon state if rendered at least once.
    #[must_use]
    pub const fn current_icon_state(&self) -> Option<TrayIconState> {
        self.current_icon_state
    }

    /// Records which tray pause option initiated the current pause and invalidates the cached
    /// menu snapshot so the next `update` forces a sink sync (restoring Slint check state even if
    /// the user re-clicked the already-checked option).
    pub fn record_pause(&mut self, source: PauseSource) {
        self.presenter.record_pause(source);
        self.last_menu = None;
    }

    /// Clears the remembered pause option.
    pub fn clear_pause_source(&mut self) {
        self.presenter.clear_pause_source();
    }

    /// Replaces the presenter's [`Strings`] table and invalidates cached tooltip/menu snapshots
    /// so the next [`Self::update`] pushes all localized texts to the [`TraySink`].
    pub fn set_strings(&mut self, strings: Arc<Strings>) {
        self.presenter.set_strings(strings);
        self.last_tooltip = None;
        self.last_menu = None;
    }

    /// Switches the tray presenter to the embedded table for `language_setting` (`"auto"`,
    /// `"en"`, `"de"`, ...) and invalidates cached tooltip/menu snapshots.
    pub fn set_language(&mut self, language_setting: &str) {
        self.set_strings(Arc::new(Strings::from_language(language_setting)));
    }

    /// Returns the resolved BCP-47 language tag currently active in the presenter.
    #[must_use]
    pub fn current_language(&self) -> &str {
        self.presenter.strings().language()
    }

    /// Returns a shared reference to the underlying [`TrayPresenter`].
    #[must_use]
    pub const fn presenter(&self) -> &TrayPresenter {
        &self.presenter
    }

    /// Returns a shared reference to the underlying [`TraySink`].
    #[must_use]
    pub const fn sink(&self) -> &S {
        &self.sink
    }

    /// Returns a mutable reference to the underlying [`TraySink`].
    pub const fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    fn reevaluate_set(&mut self, force_provider_read: bool) {
        let now = self.clock.now().timestamp();
        let interval_elapsed = self
            .last_taskbar_theme_check_utc
            .is_none_or(|last| now.duration_since(last) >= TASKBAR_THEME_RECHECK_INTERVAL);

        if force_provider_read || interval_elapsed {
            self.last_taskbar_theme_check_utc = Some(now);
            self.cached_is_light_taskbar = self.taskbar_theme.is_light_taskbar();
        }

        self.use_light_set =
            TraySetSelector::use_light_set(self.cached_is_light_taskbar, self.app_theme);
    }
}

/// Rate-limited tracker for the taskbar-theme icon variant (`use_light_set`) used by normal
/// windows, sharing [`TraySetSelector::use_light_set`] and the 60 s re-check cadence.
pub struct TaskbarIconTracker {
    clock: Arc<dyn Clock>,
    last_check_utc: Option<Timestamp>,
    use_light_set: bool,
}

impl fmt::Debug for TaskbarIconTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskbarIconTracker")
            .field("last_check_utc", &self.last_check_utc)
            .field("use_light_set", &self.use_light_set)
            .finish_non_exhaustive()
    }
}

impl TaskbarIconTracker {
    /// Creates a tracker and performs an initial read from `taskbar` and `app_theme`.
    #[must_use]
    pub fn new(
        taskbar: &dyn TaskbarThemeProvider,
        app_theme: &dyn AppThemeProvider,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let mut tracker = Self {
            clock,
            last_check_utc: None,
            use_light_set: false,
        };
        tracker.refresh(taskbar, app_theme, true);
        tracker
    }

    /// Whether the light-taskbar icon variant should be used.
    #[must_use]
    pub const fn use_light_set(&self) -> bool {
        self.use_light_set
    }

    /// Re-queries `taskbar` and `app_theme` when `force` is true or at least 60 s have elapsed
    /// since the last check. Returns `true` if `use_light_set()` changed.
    pub fn refresh(
        &mut self,
        taskbar: &dyn TaskbarThemeProvider,
        app_theme: &dyn AppThemeProvider,
        force: bool,
    ) -> bool {
        let now = self.clock.now().timestamp();
        let interval_elapsed = self
            .last_check_utc
            .is_none_or(|last| now.duration_since(last) >= TASKBAR_THEME_RECHECK_INTERVAL);

        if !force && !interval_elapsed {
            return false;
        }

        self.last_check_utc = Some(now);
        let next = TraySetSelector::use_light_set(
            taskbar.is_light_taskbar(),
            app_theme_from_opt(app_theme.is_light_app_theme()),
        );
        let changed = next != self.use_light_set;
        self.use_light_set = next;
        changed
    }
}
