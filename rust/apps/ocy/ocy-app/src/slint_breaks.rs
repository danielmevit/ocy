//! Slint break windows (`ShortBreakCard`, `WarningCard`, `LongBreakCard`) implementing [`BreakWindows`].

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use damt_app::ErrorLog;
use damt_art::FrameSequencer;
#[cfg(not(target_os = "linux"))]
use damt_overlay::enumerate_monitors;
use damt_platform::{Clock, PlatformServices};
use jiff::Timestamp;
use ocy_core::engine::{BreakInfo, EngineSnapshot};
use ocy_core::exercises::{RandomSource, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_ui::{LongBreakCard, ShortBreakCard, WarningCard};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{CloseRequestResponse, ComponentHandle, PhysicalPosition, PhysicalSize, Timer, Weak};

use crate::art::{
    OcyArt, SHORT_BREAK_ART_LOGICAL_PX, SUGGESTION_ART_LOGICAL_PX, SuggestionSchedule, art_theme,
    frame_and_next_delay, scaled_art_px,
};
use crate::break_logic::{
    AppThemeTracker, PrewarmKind, PrewarmPlanner, break_announcement, gentle_card_actions,
    long_break_can_end, long_break_countdown_text_localized, long_break_progress,
    short_break_progress, warning_announcement, warning_buttons, warning_heading,
    warning_postpone_label,
};
use crate::coordinator::BreakWindows;
use crate::layout::{DipSize, OverlayLayout, PixelPoint, ScreenInfo, select_active_screen};
use crate::strings::{Strings, font_family_for};
#[cfg(not(target_os = "linux"))]
use crate::win32_window::install_break_window_subclass_and_style;
use crate::win32_window::{
    WindowRole, WindowRoleGuard, announce_to_screen_reader, should_allow_close,
};

const CARD_WINDOW_WIDTH_DIP: f64 = 392.0;

/// Host actions triggered by interactive buttons on the break and warning cards.
#[derive(Clone)]
pub struct BreakActionCallbacks {
    pub on_skip: Rc<dyn Fn()>,
    pub on_start_long_break_now: Rc<dyn Fn()>,
    pub on_postpone_long_break: Rc<dyn Fn()>,
    /// "Not during this game" on a gentle card.
    pub on_hold_game: Rc<dyn Fn()>,
}

impl fmt::Debug for BreakActionCallbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakActionCallbacks")
            .finish_non_exhaustive()
    }
}

/// Options controlling break latency logging and stdout printing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LatencyOptions {
    pub log_to_file: bool,
    pub print_on_first_paint: bool,
}

/// Measured latency for showing a break or warning window.
#[derive(Debug, Clone, Copy)]
pub struct BreakLatencySample {
    pub kind: &'static str,
    pub prewarmed: bool,
    pub due_to_show_ms: f64,
    pub due_to_first_paint_ms: Option<f64>,
}

struct ShortBreakState {
    card: ShortBreakCard,
    exercise_id: String,
    game_actions: (bool, bool),
    scale: f64,
    fullscreen: bool,
    screen: Option<ScreenInfo>,
    current_frame: Rc<Cell<u32>>,
}

struct WarningState {
    card: WarningCard,
    started_at: Timestamp,
    initial_countdown: Duration,
    screen: Option<ScreenInfo>,
}

impl WarningState {
    fn remaining(&self, clock: &dyn Clock, snapshot_until_long: Option<Duration>) -> Duration {
        let elapsed_signed = clock.now().timestamp().duration_since(self.started_at);
        let elapsed = Duration::try_from(elapsed_signed).unwrap_or(Duration::ZERO);
        let tracked = self.initial_countdown.saturating_sub(elapsed);
        match snapshot_until_long {
            Some(snap) => tracked.min(snap),
            None => tracked,
        }
    }
}

struct LongBreakState {
    card: LongBreakCard,
    variant: usize,
    suggestion_ids: (Option<String>, Option<String>),
    suggestion_frames: (Rc<Cell<u32>>, Rc<Cell<u32>>),
    scale: f64,
    screen: Option<ScreenInfo>,
}

enum ActiveBreakWindow {
    ShortBreak(ShortBreakState),
    Warning(WarningState),
    LongBreak(LongBreakState),
}

enum PrewarmedCard {
    ShortBreak(ShortBreakCard),
    Warning(WarningCard),
    LongBreak(LongBreakCard),
}

impl PrewarmedCard {
    const fn kind(&self) -> PrewarmKind {
        match self {
            Self::ShortBreak(_) => PrewarmKind::ShortBreak,
            Self::Warning(_) => PrewarmKind::Warning,
            Self::LongBreak(_) => PrewarmKind::LongBreak,
        }
    }
}

struct PrewarmEntry {
    card: PrewarmedCard,
    created_at: Instant,
}

enum TrackedBreakWindow {
    Short(Weak<ShortBreakCard>),
    Warning(Weak<WarningCard>),
    Long(Weak<LongBreakCard>),
}

impl TrackedBreakWindow {
    fn is_alive(&self) -> bool {
        match self {
            Self::Short(w) => w.upgrade().is_some(),
            Self::Warning(w) => w.upgrade().is_some(),
            Self::Long(w) => w.upgrade().is_some(),
        }
    }
}

/// Real Slint break windows (`ShortBreakCard`, `WarningCard`, `LongBreakCard`) with Win32
/// non-activating topmost styles, stepped `damt-art` animation, and 3 s pre-warming.
pub struct SlintBreakWindows {
    platform: Arc<dyn PlatformServices>,
    clock: Arc<dyn Clock>,
    strings: Arc<Strings>,
    error_log: ErrorLog,
    callbacks: BreakActionCallbacks,
    art: Rc<OcyArt>,
    theme_tracker: AppThemeTracker,
    dark_theme: Rc<Cell<bool>>,
    break_active: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    settings: OcySettings,
    strict_mode: bool,
    postpones_left: u32,
    rng: SeededRng,
    active: Option<ActiveBreakWindow>,
    prewarm: Option<PrewarmEntry>,
    tracked_windows: RefCell<Vec<TrackedBreakWindow>>,
    last_latency: Rc<RefCell<Option<BreakLatencySample>>>,
    log_latency: bool,
    print_latency_on_first_paint: bool,
}

impl fmt::Debug for SlintBreakWindows {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlintBreakWindows")
            .field("dark_theme", &self.dark_theme.get())
            .field("break_active", &self.break_active.get())
            .field("has_active_window", &self.active.is_some())
            .field("has_prewarm", &self.prewarm.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for SlintBreakWindows {
    fn drop(&mut self) {
        self.prewarm = None;
        self.close_all();
    }
}

impl SlintBreakWindows {
    /// Creates a new [`SlintBreakWindows`] instance.
    #[must_use]
    pub fn new(
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
        strings: Arc<Strings>,
        error_log: ErrorLog,
        callbacks: BreakActionCallbacks,
        initial_settings: OcySettings,
        latency: LatencyOptions,
    ) -> Self {
        let art = Rc::new(OcyArt::from_embedded().unwrap_or_else(|err| {
            error_log.log_error("art_load", &err);
            OcyArt::empty()
        }));
        let theme_tracker = AppThemeTracker::new(platform.app_theme(), Arc::clone(&clock));
        let dark_theme = Rc::new(Cell::new(theme_tracker.is_dark()));

        Self {
            platform,
            clock,
            strings,
            error_log,
            callbacks,
            art,
            theme_tracker,
            dark_theme,
            break_active: Rc::new(Cell::new(false)),
            generation: Rc::new(Cell::new(0)),
            settings: initial_settings,
            strict_mode: false,
            postpones_left: 0,
            rng: SeededRng::from_system_time(),
            active: None,
            prewarm: None,
            tracked_windows: RefCell::new(Vec::new()),
            last_latency: Rc::new(RefCell::new(None)),
            log_latency: latency.log_to_file,
            print_latency_on_first_paint: latency.print_on_first_paint,
        }
    }

    /// Returns the most recently measured break latency sample, if any.
    #[must_use]
    pub fn last_latency(&self) -> Option<BreakLatencySample> {
        *self.last_latency.borrow()
    }

    /// Returns the shared handle to the latest latency measurement (for `--report-memory`).
    #[must_use]
    pub fn latency_handle(&self) -> Rc<RefCell<Option<BreakLatencySample>>> {
        Rc::clone(&self.last_latency)
    }

    /// Pre-creates a hidden `ShortBreakCard` immediately (used by `--report-memory` latency probe).
    pub fn prewarm_short_break_now(&mut self) {
        self.create_prewarm(PrewarmKind::ShortBreak);
    }

    /// Returns the localized skip/close label currently held on the hidden pre-warmed
    /// `ShortBreakCard`, if one is pre-warmed (for live-language-switch tests).
    #[must_use]
    pub fn prewarm_short_close_label(&self) -> Option<String> {
        match self.prewarm.as_ref().map(|e| &e.card) {
            Some(PrewarmedCard::ShortBreak(c)) => Some(c.get_close_label().to_string()),
            _ => None,
        }
    }

    /// Returns the number of live Slint break/warning card instances still referenced anywhere.
    #[must_use]
    pub fn live_break_windows(&self) -> usize {
        let mut tracked = self.tracked_windows.borrow_mut();
        tracked.retain(TrackedBreakWindow::is_alive);
        tracked.len()
    }

    /// Returns the number of cached exercise raster entries currently held by [`OcyArt`].
    #[must_use]
    pub fn art_cache_entries(&self) -> usize {
        self.art.cache_entry_count()
    }

    /// Returns a strong handle to the active [`ShortBreakCard`], if one is open.
    #[must_use]
    pub fn active_short_card(&self) -> Option<ShortBreakCard> {
        match &self.active {
            Some(ActiveBreakWindow::ShortBreak(s)) => Some(s.card.clone_strong()),
            _ => None,
        }
    }

    /// Returns a strong handle to the active [`WarningCard`], if one is open.
    #[must_use]
    pub fn active_warning_card(&self) -> Option<WarningCard> {
        match &self.active {
            Some(ActiveBreakWindow::Warning(w)) => Some(w.card.clone_strong()),
            _ => None,
        }
    }

    /// Returns a strong handle to the active [`LongBreakCard`], if one is open.
    #[must_use]
    pub fn active_long_card(&self) -> Option<LongBreakCard> {
        match &self.active {
            Some(ActiveBreakWindow::LongBreak(l)) => Some(l.card.clone_strong()),
            _ => None,
        }
    }

    fn track_window(&self, entry: TrackedBreakWindow) {
        let mut tracked = self.tracked_windows.borrow_mut();
        tracked.retain(TrackedBreakWindow::is_alive);
        tracked.push(entry);
    }

    fn next_generation(&self) -> u64 {
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        next
    }

    fn active_monitor_info(&self) -> Option<ScreenInfo> {
        #[cfg(not(target_os = "linux"))]
        let monitors = enumerate_monitors();
        #[cfg(target_os = "linux")]
        let monitors = crate::linux_window::enumerate_monitors();
        let active_bounds = self.platform.active_screen().active_screen_bounds();
        select_active_screen(&monitors, active_bounds)
    }

    fn configure_short_card_handlers(&self, card: &ShortBreakCard) {
        let on_skip = Rc::clone(&self.callbacks.on_skip);
        card.on_close_clicked(move || on_skip());

        let on_hold_game = Rc::clone(&self.callbacks.on_hold_game);
        card.on_hold_game_clicked(move || on_hold_game());

        let on_take_long = Rc::clone(&self.callbacks.on_start_long_break_now);
        card.on_take_long_clicked(move || on_take_long());

        let active = Rc::clone(&self.break_active);
        card.window().on_close_requested(move || {
            if should_allow_close(active.get()) {
                CloseRequestResponse::HideWindow
            } else {
                CloseRequestResponse::KeepWindowShown
            }
        });
    }

    fn configure_warning_card_handlers(&self, card: &WarningCard) {
        let on_start = Rc::clone(&self.callbacks.on_start_long_break_now);
        card.on_start_now(move || on_start());

        let on_postpone = Rc::clone(&self.callbacks.on_postpone_long_break);
        card.on_postpone(move || on_postpone());

        let on_skip = Rc::clone(&self.callbacks.on_skip);
        card.on_skip(move || on_skip());

        let active = Rc::clone(&self.break_active);
        card.window().on_close_requested(move || {
            if should_allow_close(active.get()) {
                CloseRequestResponse::HideWindow
            } else {
                CloseRequestResponse::KeepWindowShown
            }
        });
    }

    fn configure_long_card_handlers(&self, card: &LongBreakCard) {
        let on_skip = Rc::clone(&self.callbacks.on_skip);
        card.on_end_break(move || on_skip());

        let active = Rc::clone(&self.break_active);
        card.window().on_close_requested(move || {
            if should_allow_close(active.get()) {
                CloseRequestResponse::HideWindow
            } else {
                CloseRequestResponse::KeepWindowShown
            }
        });
    }

    fn create_short_card(&self, dark: bool) -> Result<ShortBreakCard, slint::PlatformError> {
        let _role = WindowRoleGuard::new(WindowRole::BreakCard);
        let card = ShortBreakCard::new()?;
        ocy_ui::set_dark(&card, dark);
        card.set_font_family(font_family_for(self.strings.language()).into());
        card.set_close_label(self.strings.get("shortbreak.skip").into());
        card.set_dim_backdrop(false);
        card.set_progress(1.0);
        self.configure_short_card_handlers(&card);
        self.track_window(TrackedBreakWindow::Short(card.as_weak()));
        Ok(card)
    }

    fn create_warning_card(&self, dark: bool) -> Result<WarningCard, slint::PlatformError> {
        let _role = WindowRoleGuard::new(WindowRole::BreakCard);
        let card = WarningCard::new()?;
        ocy_ui::set_dark(&card, dark);
        card.set_font_family(font_family_for(self.strings.language()).into());
        let buttons = warning_buttons(self.strict_mode, self.postpones_left);
        card.set_heading(
            warning_heading(&self.strings, self.settings.long_breaks.warning_duration()).into(),
        );
        card.set_start_now_label(self.strings.get("warning.startNow").into());
        card.set_postpone_label(
            warning_postpone_label(&self.strings, self.settings.long_breaks.postpone_minutes)
                .into(),
        );
        card.set_skip_label(self.strings.get("warning.skip").into());
        card.set_strict_label(self.strings.get("warning.strictOn").into());
        card.set_strict(buttons.show_strict_line);
        card.set_can_postpone(buttons.show_postpone);
        self.configure_warning_card_handlers(&card);
        self.track_window(TrackedBreakWindow::Warning(card.as_weak()));
        Ok(card)
    }

    fn create_long_card(&self, dark: bool) -> Result<LongBreakCard, slint::PlatformError> {
        let _role = WindowRoleGuard::new(WindowRole::BreakCard);
        let card = LongBreakCard::new()?;
        ocy_ui::set_dark(&card, dark);
        card.set_font_family(font_family_for(self.strings.language()).into());
        card.set_dim_backdrop(false);
        card.set_countdown_label(self.strings.get("longbreak.countdownLabel").into());
        card.set_countdown(
            long_break_countdown_text_localized(
                &self.strings,
                self.settings.long_breaks.duration(),
            )
            .into(),
        );
        card.set_progress(0.0);
        card.set_end_label(self.strings.get("longbreak.skip").into());
        card.set_can_end(long_break_can_end(self.strict_mode));
        card.set_show_suggestions(true);
        card.set_show_suggestion2(true);
        self.configure_long_card_handlers(&card);
        self.track_window(TrackedBreakWindow::Long(card.as_weak()));
        Ok(card)
    }

    fn take_or_create_short_card(
        &mut self,
    ) -> Result<(ShortBreakCard, bool), slint::PlatformError> {
        if let Some(entry) = self.prewarm.take()
            && let PrewarmedCard::ShortBreak(card) = entry.card
        {
            return Ok((card, true));
        }
        let card = self.create_short_card(self.dark_theme.get())?;
        Ok((card, false))
    }

    fn take_or_create_warning_card(&mut self) -> Result<(WarningCard, bool), slint::PlatformError> {
        if let Some(entry) = self.prewarm.take()
            && let PrewarmedCard::Warning(card) = entry.card
        {
            return Ok((card, true));
        }
        let card = self.create_warning_card(self.dark_theme.get())?;
        Ok((card, false))
    }

    fn take_or_create_long_card(&mut self) -> Result<(LongBreakCard, bool), slint::PlatformError> {
        if let Some(entry) = self.prewarm.take()
            && let PrewarmedCard::LongBreak(card) = entry.card
        {
            return Ok((card, true));
        }
        let card = self.create_long_card(self.dark_theme.get())?;
        Ok((card, false))
    }

    fn prewarm_art_for_kind(&self, kind: PrewarmKind, dark: bool) {
        let scale = self.active_monitor_info().map_or(1.0, |s| s.scaling);
        let theme = art_theme(dark);
        match kind {
            PrewarmKind::ShortBreak => {
                let size_px = scaled_art_px(SHORT_BREAK_ART_LOGICAL_PX, scale);
                let reduced_motion = self.platform.motion().prefers_reduced_motion();
                self.art.prewarm_short_break(
                    theme,
                    size_px,
                    reduced_motion,
                    self.settings.has_window_nearby,
                );
            }
            PrewarmKind::Warning => {}
            PrewarmKind::LongBreak => {
                let reduced_motion = self.platform.motion().prefers_reduced_motion();
                self.art.prewarm_long_break_frames(
                    theme,
                    SUGGESTION_ART_LOGICAL_PX,
                    scale,
                    reduced_motion,
                );
            }
        }
    }

    fn create_prewarm(&mut self, kind: PrewarmKind) {
        let dark = self.dark_theme.get();
        let card = match kind {
            PrewarmKind::ShortBreak => {
                let Ok(card) = self.create_short_card(dark) else {
                    return;
                };
                self.prewarm_art_for_kind(PrewarmKind::ShortBreak, dark);
                PrewarmedCard::ShortBreak(card)
            }
            PrewarmKind::Warning => {
                let Ok(card) = self.create_warning_card(dark) else {
                    return;
                };
                PrewarmedCard::Warning(card)
            }
            PrewarmKind::LongBreak => {
                let Ok(card) = self.create_long_card(dark) else {
                    return;
                };
                self.prewarm_art_for_kind(PrewarmKind::LongBreak, dark);
                PrewarmedCard::LongBreak(card)
            }
        };

        self.prewarm = Some(PrewarmEntry {
            card,
            created_at: Instant::now(),
        });
    }

    fn hide_and_drop_active(&mut self) {
        if let Some(active) = self.active.take() {
            match active {
                ActiveBreakWindow::ShortBreak(s) => {
                    let _ = s.card.hide();
                }
                ActiveBreakWindow::Warning(w) => {
                    let _ = w.card.hide();
                }
                ActiveBreakWindow::LongBreak(l) => {
                    let _ = l.card.hide();
                }
            }
        }
    }

    fn reapply_texts_to_active(&self, snapshot: &EngineSnapshot) {
        let font = font_family_for(self.strings.language());
        match &self.active {
            Some(ActiveBreakWindow::ShortBreak(state)) => {
                state.card.set_font_family(font.into());
                let title_key = format!("exercise.{}.title", state.exercise_id);
                let body_key = format!("exercise.{}.body", state.exercise_id);
                state.card.set_heading(self.strings.get(&title_key).into());
                state.card.set_body(self.strings.get(&body_key).into());
                state
                    .card
                    .set_close_label(self.strings.get("shortbreak.skip").into());
                apply_game_actions(&state.card, &self.strings, state.game_actions);
            }
            Some(ActiveBreakWindow::Warning(state)) => {
                state.card.set_font_family(font.into());
                let remaining = state.remaining(&*self.clock, snapshot.time_until_long_break);
                state
                    .card
                    .set_heading(warning_heading(&self.strings, remaining).into());
                state
                    .card
                    .set_start_now_label(self.strings.get("warning.startNow").into());
                state.card.set_postpone_label(
                    warning_postpone_label(
                        &self.strings,
                        self.settings.long_breaks.postpone_minutes,
                    )
                    .into(),
                );
                state
                    .card
                    .set_skip_label(self.strings.get("warning.skip").into());
                state
                    .card
                    .set_strict_label(self.strings.get("warning.strictOn").into());
            }
            Some(ActiveBreakWindow::LongBreak(state)) => {
                state.card.set_font_family(font.into());
                let title_key = format!("longbreak.title.{}", state.variant);
                let body_key = format!("longbreak.body.{}", state.variant);
                state
                    .card
                    .set_countdown_label(self.strings.get("longbreak.countdownLabel").into());
                if let Some(current) = &snapshot.current_break {
                    state.card.set_countdown(
                        long_break_countdown_text_localized(
                            &self.strings,
                            current.remaining_duration,
                        )
                        .into(),
                    );
                }
                state.card.set_heading(self.strings.get(&title_key).into());
                state.card.set_body(self.strings.get(&body_key).into());
                state
                    .card
                    .set_end_label(self.strings.get("longbreak.skip").into());
                if let Some(ref ex1) = state.suggestion_ids.0 {
                    state.card.set_suggestion1_title(
                        self.strings.get(&format!("exercise.{ex1}.title")).into(),
                    );
                    state.card.set_suggestion1_body(
                        self.strings.get(&format!("exercise.{ex1}.body")).into(),
                    );
                }
                if let Some(ref ex2) = state.suggestion_ids.1 {
                    state.card.set_suggestion2_title(
                        self.strings.get(&format!("exercise.{ex2}.title")).into(),
                    );
                    state.card.set_suggestion2_body(
                        self.strings.get(&format!("exercise.{ex2}.body")).into(),
                    );
                }
            }
            None => {}
        }
    }

    fn apply_theme_to_active(&self) {
        let dark = self.dark_theme.get();
        match &self.active {
            Some(ActiveBreakWindow::ShortBreak(state)) => {
                ocy_ui::set_dark(&state.card, dark);
                let size_px = scaled_art_px(SHORT_BREAK_ART_LOGICAL_PX, state.scale);
                let frame = state.current_frame.get();
                if let Some(Ok(img)) =
                    self.art
                        .render_frame(&state.exercise_id, art_theme(dark), frame, size_px)
                {
                    state.card.set_art(img);
                }
            }
            Some(ActiveBreakWindow::Warning(state)) => {
                ocy_ui::set_dark(&state.card, dark);
            }
            Some(ActiveBreakWindow::LongBreak(state)) => {
                ocy_ui::set_dark(&state.card, dark);
                let theme = art_theme(dark);
                let size_px = scaled_art_px(SUGGESTION_ART_LOGICAL_PX, state.scale);
                if let Some(id1) = &state.suggestion_ids.0
                    && let Some(Ok(img)) =
                        self.art
                            .render_frame(id1, theme, state.suggestion_frames.0.get(), size_px)
                {
                    state.card.set_suggestion1_art(img);
                }
                if let Some(id2) = &state.suggestion_ids.1
                    && let Some(Ok(img)) =
                        self.art
                            .render_frame(id2, theme, state.suggestion_frames.1.get(), size_px)
                {
                    state.card.set_suggestion2_art(img);
                }
            }
            None => {}
        }
    }

    fn show_window_ctx(
        &self,
        kind: &'static str,
        prewarmed: bool,
        due_at: Instant,
        screen: Option<ScreenInfo>,
        centered: bool,
        generation: u64,
    ) -> ShowWindowContext {
        ShowWindowContext {
            kind,
            prewarmed,
            due_at,
            screen,
            centered,
            generation,
            gen_cell: Rc::clone(&self.generation),
            latency_out: Rc::clone(&self.last_latency),
            error_log: self.error_log.clone(),
            log_latency: self.log_latency,
            print_latency_on_first_paint: self.print_latency_on_first_paint,
            announcement: String::new(),
        }
    }

    fn start_short_break_art(
        &self,
        card: &ShortBreakCard,
        exercise_id: &str,
        scale: f64,
        generation: u64,
        current_frame_cell: &Rc<Cell<u32>>,
    ) {
        let reduced_motion = self.platform.motion().prefers_reduced_motion();
        let Some((sequencer, frozen_frame)) = self.art.sequencer_for(exercise_id, reduced_motion)
        else {
            return;
        };

        let size_px = scaled_art_px(SHORT_BREAK_ART_LOGICAL_PX, scale);
        let (initial_frame, next_delay) =
            frame_and_next_delay(&sequencer, Duration::ZERO, reduced_motion, frozen_frame);
        current_frame_cell.set(initial_frame);

        let theme = art_theme(self.dark_theme.get());
        if let Some(res) = self
            .art
            .render_frame(exercise_id, theme, initial_frame, size_px)
        {
            match res {
                Ok(img) => card.set_art(img),
                Err(err) => self.error_log.log_error("art_render", &err),
            }
        }

        if let Some(delay) = next_delay {
            let ctx = ArtAnimationContext {
                art: Rc::clone(&self.art),
                exercise_id: exercise_id.to_owned(),
                sequencer,
                size_px,
                started_at: Instant::now(),
                my_gen: generation,
                current_gen: Rc::clone(&self.generation),
                dark_theme: Rc::clone(&self.dark_theme),
                current_frame: Rc::clone(current_frame_cell),
                error_log: self.error_log.clone(),
            };
            schedule_next_art_frame(card.as_weak(), ctx, initial_frame, delay);
        }
    }

    fn start_long_break_art(
        &self,
        card: &LongBreakCard,
        id1: Option<&str>,
        id2: Option<&str>,
        scale: f64,
        generation: u64,
        frame_cells: &(Rc<Cell<u32>>, Rc<Cell<u32>>),
    ) {
        let reduced_motion = self.platform.motion().prefers_reduced_motion();
        let slot1 = id1.and_then(|id| {
            self.art
                .sequencer_for(id, reduced_motion)
                .map(|(seq, frozen)| (id.to_owned(), seq, frozen))
        });
        let slot2 = id2.and_then(|id| {
            self.art
                .sequencer_for(id, reduced_motion)
                .map(|(seq, frozen)| (id.to_owned(), seq, frozen))
        });

        let mut schedule = SuggestionSchedule::new(slot1, slot2, reduced_motion);
        let (initial_frames, next_delay) = schedule.initial();
        let size_px = scaled_art_px(SUGGESTION_ART_LOGICAL_PX, scale);
        let theme = art_theme(self.dark_theme.get());

        if let (Some(ex1), Some(frame1)) = (schedule.exercise_id(0), initial_frames[0]) {
            frame_cells.0.set(frame1);
            if let Some(res) = self.art.render_frame(ex1, theme, frame1, size_px) {
                match res {
                    Ok(img) => card.set_suggestion1_art(img),
                    Err(err) => self.error_log.log_error("art_render", &err),
                }
            }
        }
        if let (Some(ex2), Some(frame2)) = (schedule.exercise_id(1), initial_frames[1]) {
            frame_cells.1.set(frame2);
            if let Some(res) = self.art.render_frame(ex2, theme, frame2, size_px) {
                match res {
                    Ok(img) => card.set_suggestion2_art(img),
                    Err(err) => self.error_log.log_error("art_render", &err),
                }
            }
        }

        if let Some(delay) = next_delay {
            let ctx = SuggestionAnimationContext {
                art: Rc::clone(&self.art),
                schedule,
                size_px,
                started_at: Instant::now(),
                my_gen: generation,
                current_gen: Rc::clone(&self.generation),
                dark_theme: Rc::clone(&self.dark_theme),
                frame_cells: (Rc::clone(&frame_cells.0), Rc::clone(&frame_cells.1)),
                error_log: self.error_log.clone(),
            };
            schedule_next_suggestion_frame(card.as_weak(), ctx, delay);
        }
    }
}

struct ArtAnimationContext {
    art: Rc<OcyArt>,
    exercise_id: String,
    sequencer: FrameSequencer,
    size_px: u32,
    started_at: Instant,
    my_gen: u64,
    current_gen: Rc<Cell<u64>>,
    dark_theme: Rc<Cell<bool>>,
    current_frame: Rc<Cell<u32>>,
    error_log: ErrorLog,
}

fn schedule_next_art_frame(
    weak_card: Weak<ShortBreakCard>,
    ctx: ArtAnimationContext,
    last_frame: u32,
    delay: Duration,
) {
    Timer::single_shot(delay, move || {
        if ctx.current_gen.get() != ctx.my_gen {
            return;
        }
        let Some(card) = weak_card.upgrade() else {
            return;
        };

        let elapsed = ctx.started_at.elapsed();
        let frame = ctx.sequencer.get_frame(elapsed);
        if frame != last_frame {
            ctx.current_frame.set(frame);
            let theme = art_theme(ctx.dark_theme.get());
            match ctx
                .art
                .render_frame(&ctx.exercise_id, theme, frame, ctx.size_px)
            {
                Some(Ok(img)) => card.set_art(img),
                Some(Err(err)) => ctx.error_log.log_error("art_render", &err),
                None => {}
            }
        }

        let next = ctx.sequencer.next_change_after(elapsed);
        if !next.is_zero() {
            schedule_next_art_frame(weak_card, ctx, frame, next);
        }
    });
}

struct SuggestionAnimationContext {
    art: Rc<OcyArt>,
    schedule: SuggestionSchedule,
    size_px: u32,
    started_at: Instant,
    my_gen: u64,
    current_gen: Rc<Cell<u64>>,
    dark_theme: Rc<Cell<bool>>,
    frame_cells: (Rc<Cell<u32>>, Rc<Cell<u32>>),
    error_log: ErrorLog,
}

fn schedule_next_suggestion_frame(
    weak_card: Weak<LongBreakCard>,
    mut ctx: SuggestionAnimationContext,
    delay: Duration,
) {
    Timer::single_shot(delay, move || {
        if ctx.current_gen.get() != ctx.my_gen {
            return;
        }
        let Some(card) = weak_card.upgrade() else {
            return;
        };

        let elapsed = ctx.started_at.elapsed();
        let (updates, next) = ctx.schedule.advance_at(elapsed);
        let theme = art_theme(ctx.dark_theme.get());

        if let (Some(ex1), Some(frame1)) = (ctx.schedule.exercise_id(0), updates[0]) {
            ctx.frame_cells.0.set(frame1);
            match ctx.art.render_frame(ex1, theme, frame1, ctx.size_px) {
                Some(Ok(img)) => card.set_suggestion1_art(img),
                Some(Err(err)) => ctx.error_log.log_error("art_render", &err),
                None => {}
            }
        }
        if let (Some(ex2), Some(frame2)) = (ctx.schedule.exercise_id(1), updates[1]) {
            ctx.frame_cells.1.set(frame2);
            match ctx.art.render_frame(ex2, theme, frame2, ctx.size_px) {
                Some(Ok(img)) => card.set_suggestion2_art(img),
                Some(Err(err)) => ctx.error_log.log_error("art_render", &err),
                None => {}
            }
        }

        if let Some(next_delay) = next {
            schedule_next_suggestion_frame(weak_card, ctx, next_delay);
        }
    });
}

fn compute_card_rect(
    screen: Option<ScreenInfo>,
    dip_size: DipSize,
    centered: bool,
) -> (PixelPoint, u32, u32) {
    match screen {
        Some(s) => {
            let (w, h) = OverlayLayout::physical_size(dip_size, s.scaling);
            let pos = if centered {
                OverlayLayout::compute_centered_position(&s, dip_size)
            } else {
                OverlayLayout::compute_corner_position(&s, dip_size, 0.0)
            };
            (pos, w, h)
        }
        None => {
            let (w, h) = OverlayLayout::physical_size(dip_size, 1.0);
            (PixelPoint::new(100, 100), w, h)
        }
    }
}

struct ShowWindowContext {
    kind: &'static str,
    prewarmed: bool,
    due_at: Instant,
    screen: Option<ScreenInfo>,
    centered: bool,
    generation: u64,
    gen_cell: Rc<Cell<u64>>,
    latency_out: Rc<RefCell<Option<BreakLatencySample>>>,
    error_log: ErrorLog,
    log_latency: bool,
    print_latency_on_first_paint: bool,
    /// What Narrator announces when the card appears (it never takes focus).
    announcement: String,
}

fn install_break_window_role_platform(window: &slint::Window) -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::linux_window::install_break_window_role(window)
    }
    #[cfg(not(target_os = "linux"))]
    {
        install_break_window_subclass_and_style(window)
    }
}

fn schedule_subclass_retry<C: ComponentHandle + 'static>(
    weak: Weak<C>,
    subclassed: Rc<Cell<bool>>,
    gen_cell: Rc<Cell<u64>>,
    generation: u64,
    attempts_left: u32,
) {
    if attempts_left == 0 || subclassed.get() || gen_cell.get() != generation {
        return;
    }
    Timer::single_shot(Duration::from_millis(1), move || {
        if subclassed.get() || gen_cell.get() != generation {
            return;
        }
        let Some(card) = weak.upgrade() else {
            return;
        };
        if install_break_window_role_platform(card.window()) {
            subclassed.set(true);
        } else {
            schedule_subclass_retry(weak, subclassed, gen_cell, generation, attempts_left - 1);
        }
    });
}

fn show_card_window<C: ComponentHandle + 'static>(
    card: &C,
    ctx: ShowWindowContext,
    dip_size_fn: impl Fn(&C, Option<ScreenInfo>) -> DipSize,
) {
    let dip_size = dip_size_fn(card, ctx.screen);
    let (pos, phys_w, phys_h) = compute_card_rect(ctx.screen, dip_size, ctx.centered);
    card.window()
        .set_position(PhysicalPosition::new(pos.x, pos.y));
    card.window().set_size(PhysicalSize::new(phys_w, phys_h));

    let subclassed = Rc::new(Cell::new(false));
    let first_paint_recorded = Rc::new(Cell::new(false));
    let announced = Rc::new(Cell::new(false));
    let announcement = ctx.announcement.clone();

    *ctx.latency_out.borrow_mut() = Some(BreakLatencySample {
        kind: ctx.kind,
        prewarmed: ctx.prewarmed,
        due_to_show_ms: 0.0,
        due_to_first_paint_ms: None,
    });

    let subclassed_ev = Rc::clone(&subclassed);
    let first_paint_ev = Rc::clone(&first_paint_recorded);
    let gen_ref = Rc::clone(&ctx.gen_cell);
    let lat_ref = Rc::clone(&ctx.latency_out);
    let log_ref = ctx.error_log.clone();
    let generation = ctx.generation;
    let kind = ctx.kind;
    let prewarmed = ctx.prewarmed;
    let due_at = ctx.due_at;
    let log_latency = ctx.log_latency;
    let print_latency = ctx.print_latency_on_first_paint;

    card.window().on_winit_window_event(move |win, event| {
        if !subclassed_ev.get() && install_break_window_role_platform(win) {
            subclassed_ev.set(true);
        }
        // Announce once the window is on screen (its first paint) and only for this generation.
        if !announced.get()
            && matches!(event, WindowEvent::RedrawRequested)
            && gen_ref.get() == generation
            && announce_to_screen_reader(win, &announcement)
        {
            announced.set(true);
        }

        if matches!(event, WindowEvent::RedrawRequested)
            && !first_paint_ev.get()
            && gen_ref.get() == generation
        {
            first_paint_ev.set(true);
            let fp_ms = due_at.elapsed().as_secs_f64() * 1000.0;
            let show_ms_val = {
                let mut lat = lat_ref.borrow_mut();
                if let Some(sample) = lat.as_mut() {
                    sample.due_to_first_paint_ms = Some(fp_ms);
                    sample.due_to_show_ms
                } else {
                    0.0
                }
            };
            if log_latency {
                log_ref.log(
                    Some("latency"),
                    &format!(
                        "kind={kind} prewarmed={prewarmed} due_to_show_ms={show_ms_val:.2} due_to_first_paint_ms={fp_ms:.2}"
                    ),
                );
            }
            if print_latency {
                println!(
                    "Ocy break latency: kind={kind} prewarmed={prewarmed} due_to_show_ms={show_ms_val:.2} due_to_first_paint_ms={fp_ms:.2}"
                );
            }
        }

        EventResult::Propagate
    });

    if let Err(err) = card.show() {
        ctx.error_log.log_error("break_window_show", &err);
    }

    let show_ms = ctx.due_at.elapsed().as_secs_f64() * 1000.0;
    if let Some(sample) = ctx.latency_out.borrow_mut().as_mut() {
        sample.due_to_show_ms = show_ms;
    }

    let updated_dip = dip_size_fn(card, ctx.screen);
    if (updated_dip.height - dip_size.height).abs() > 0.5
        || (updated_dip.width - dip_size.width).abs() > 0.5
    {
        let (new_pos, new_w, new_h) = compute_card_rect(ctx.screen, updated_dip, ctx.centered);
        card.window()
            .set_position(PhysicalPosition::new(new_pos.x, new_pos.y));
        card.window().set_size(PhysicalSize::new(new_w, new_h));
    }

    if install_break_window_role_platform(card.window()) {
        subclassed.set(true);
    } else {
        schedule_subclass_retry(
            card.as_weak(),
            subclassed,
            ctx.gen_cell,
            ctx.generation,
            200,
        );
    }
}

/// Sets the gentle game actions (and their labels) on a short-break card. A reused prewarmed card
/// may still carry the last break's flags, so every show sets them.
fn apply_game_actions(card: &ShortBreakCard, strings: &Strings, (hold, take_long): (bool, bool)) {
    card.set_show_hold_game(hold);
    card.set_hold_game_label(strings.get("games.holdThisGame").into());
    card.set_show_take_long(take_long);
    card.set_take_long_label(strings.get("games.takeLongBreak").into());
}

impl BreakWindows for SlintBreakWindows {
    fn sync_state(&mut self, snapshot: &EngineSnapshot, settings: &OcySettings) {
        let language_changed = settings.language != self.settings.language;
        self.strict_mode = snapshot.strict_mode;
        self.postpones_left = snapshot.postpones_left;
        self.settings = settings.clone();
        if language_changed {
            self.strings = Arc::new(Strings::from_language(&settings.language));
            if let Some(rtl) = self.strings.rtl_fallback_from() {
                self.error_log.log(
                    Some("i18n"),
                    &format!("RTL language {rtl:?} is not supported yet; falling back to English"),
                );
            }
            self.reapply_texts_to_active(snapshot);
            if let Some(entry) = self.prewarm.take() {
                self.create_prewarm(entry.card.kind());
            }
        }
    }

    fn on_tick(&mut self, snapshot: &EngineSnapshot) {
        if self.theme_tracker.refresh(self.platform.app_theme(), false) {
            self.dark_theme.set(self.theme_tracker.is_dark());
            self.apply_theme_to_active();
            let prewarm_kind = self.prewarm.as_ref().map(|entry| entry.card.kind());
            if let Some(entry) = &self.prewarm {
                let dark = self.dark_theme.get();
                match &entry.card {
                    PrewarmedCard::ShortBreak(c) => ocy_ui::set_dark(c, dark),
                    PrewarmedCard::Warning(c) => ocy_ui::set_dark(c, dark),
                    PrewarmedCard::LongBreak(c) => ocy_ui::set_dark(c, dark),
                }
            }
            if let Some(kind) = prewarm_kind
                && matches!(kind, PrewarmKind::ShortBreak | PrewarmKind::LongBreak)
            {
                if self.active.is_none() {
                    self.art.clear_cache();
                }
                self.prewarm_art_for_kind(kind, self.dark_theme.get());
            }
        }

        if let Some(entry) = &self.prewarm
            && PrewarmPlanner::should_drop(snapshot.phase, entry.created_at.elapsed())
        {
            self.prewarm = None;
            if self.active.is_none() {
                self.art.clear_cache();
            }
        }

        let warning_remaining = match &self.active {
            Some(ActiveBreakWindow::Warning(state)) => {
                Some(state.remaining(&*self.clock, snapshot.time_until_long_break))
            }
            _ => None,
        };

        if let Some(desired_kind) =
            PrewarmPlanner::decide(snapshot, &self.settings, warning_remaining)
        {
            let already_matches = self
                .prewarm
                .as_ref()
                .is_some_and(|e| e.card.kind() == desired_kind);
            if !already_matches {
                self.create_prewarm(desired_kind);
            }
        }
    }

    fn show_short_break(&mut self, info: &BreakInfo) {
        let due_at = Instant::now();
        self.hide_and_drop_active();
        let (card, prewarmed) = match self.take_or_create_short_card() {
            Ok(pair) => pair,
            Err(err) => {
                self.error_log.log_error("short_break_create", &err);
                return;
            }
        };

        let generation = self.next_generation();
        self.break_active.set(true);

        if self.theme_tracker.refresh(self.platform.app_theme(), false) {
            self.dark_theme.set(self.theme_tracker.is_dark());
        }
        ocy_ui::set_dark(&card, self.dark_theme.get());
        card.set_font_family(font_family_for(self.strings.language()).into());

        let title_key = format!("exercise.{}.title", info.exercise_id);
        let body_key = format!("exercise.{}.body", info.exercise_id);
        let title = self.strings.get(&title_key);
        let body = self.strings.get(&body_key);
        let close_label = self.strings.get("shortbreak.skip");

        card.set_heading(title.into());
        card.set_body(body.into());
        card.set_close_label(close_label.into());
        let game_actions = gentle_card_actions(info.kind, info.surface);
        apply_game_actions(&card, &self.strings, game_actions);
        let hold_label = self.strings.get("games.holdThisGame");
        let take_long_label = self.strings.get("games.takeLongBreak");
        card.set_dim_backdrop(false);
        card.set_progress(short_break_progress(
            info.total_duration,
            info.total_duration,
        ));

        let screen = self.active_monitor_info();
        let scale = screen.map_or(1.0, |s| s.scaling);
        let fullscreen = self.settings.short_breaks.fullscreen;
        let current_frame = Rc::new(Cell::new(1));

        self.start_short_break_art(&card, &info.exercise_id, scale, generation, &current_frame);

        show_card_window(
            &card,
            ShowWindowContext {
                announcement: break_announcement(&[
                    title,
                    body,
                    if game_actions.1 { take_long_label } else { "" },
                    if game_actions.0 { hold_label } else { "" },
                ]),
                ..self.show_window_ctx("short", prewarmed, due_at, screen, fullscreen, generation)
            },
            |c, _| {
                DipSize::new(
                    CARD_WINDOW_WIDTH_DIP,
                    f64::from(c.get_content_height()).max(100.0),
                )
            },
        );

        self.active = Some(ActiveBreakWindow::ShortBreak(ShortBreakState {
            card,
            exercise_id: info.exercise_id.clone(),
            game_actions,
            scale,
            fullscreen,
            screen,
            current_frame,
        }));
    }

    fn show_warning(&mut self, countdown: Duration) {
        let due_at = Instant::now();
        self.hide_and_drop_active();
        let (card, prewarmed) = match self.take_or_create_warning_card() {
            Ok(pair) => pair,
            Err(err) => {
                self.error_log.log_error("warning_create", &err);
                return;
            }
        };

        let generation = self.next_generation();
        self.break_active.set(true);

        if self.theme_tracker.refresh(self.platform.app_theme(), false) {
            self.dark_theme.set(self.theme_tracker.is_dark());
        }
        ocy_ui::set_dark(&card, self.dark_theme.get());
        card.set_font_family(font_family_for(self.strings.language()).into());

        let buttons = warning_buttons(self.strict_mode, self.postpones_left);
        let heading = warning_heading(&self.strings, countdown);
        let start_now = self.strings.get("warning.startNow");
        let postpone =
            warning_postpone_label(&self.strings, self.settings.long_breaks.postpone_minutes);
        let skip = self.strings.get("warning.skip");
        let strict_line = self.strings.get("warning.strictOn");
        let announcement =
            warning_announcement(&heading, buttons, start_now, &postpone, skip, strict_line);
        card.set_heading(heading.as_str().into());
        card.set_start_now_label(start_now.into());
        card.set_postpone_label(postpone.as_str().into());
        card.set_skip_label(skip.into());
        card.set_strict_label(strict_line.into());
        card.set_strict(buttons.show_strict_line);
        card.set_can_postpone(buttons.show_postpone);

        let screen = self.active_monitor_info();
        show_card_window(
            &card,
            ShowWindowContext {
                announcement,
                ..self.show_window_ctx("warning", prewarmed, due_at, screen, false, generation)
            },
            |c, _| {
                DipSize::new(
                    CARD_WINDOW_WIDTH_DIP,
                    f64::from(c.get_content_height()).max(100.0),
                )
            },
        );

        self.active = Some(ActiveBreakWindow::Warning(WarningState {
            card,
            started_at: self.clock.now().timestamp(),
            initial_countdown: countdown,
            screen,
        }));
    }

    fn show_long_break(&mut self, info: &BreakInfo) {
        let due_at = Instant::now();
        self.hide_and_drop_active();
        let (card, prewarmed) = match self.take_or_create_long_card() {
            Ok(pair) => pair,
            Err(err) => {
                self.error_log.log_error("long_break_create", &err);
                return;
            }
        };

        let generation = self.next_generation();
        self.break_active.set(true);

        if self.theme_tracker.refresh(self.platform.app_theme(), false) {
            self.dark_theme.set(self.theme_tracker.is_dark());
        }
        let dark = self.dark_theme.get();
        ocy_ui::set_dark(&card, dark);
        card.set_font_family(font_family_for(self.strings.language()).into());

        let variant = self.rng.next_usize(5) + 1;
        let title_key = format!("longbreak.title.{variant}");
        let body_key = format!("longbreak.body.{variant}");

        let countdown_label = self.strings.get("longbreak.countdownLabel");
        let countdown = long_break_countdown_text_localized(&self.strings, info.total_duration);
        let heading = self.strings.get(&title_key);
        let body = self.strings.get(&body_key);
        let end_label = self.strings.get("longbreak.skip");
        let timer = format!("{countdown_label} {countdown}");
        let announcement = break_announcement(&[
            heading,
            body,
            &timer,
            if long_break_can_end(self.strict_mode) {
                end_label
            } else {
                ""
            },
        ]);
        card.set_dim_backdrop(false);
        card.set_countdown_label(countdown_label.into());
        card.set_countdown(countdown.as_str().into());
        card.set_heading(heading.into());
        card.set_body(body.into());
        card.set_progress(long_break_progress(
            info.total_duration,
            info.total_duration,
        ));
        card.set_end_label(self.strings.get("longbreak.skip").into());
        card.set_can_end(long_break_can_end(self.strict_mode));

        let screen = self.active_monitor_info();
        let scale = screen.map_or(1.0, |s| s.scaling);

        let ids = info.suggestion_exercise_ids.as_deref().unwrap_or(&[]);
        let id1 = ids.first().filter(|id| !id.is_empty()).cloned();
        let id2 = ids.get(1).filter(|id| !id.is_empty()).cloned();

        card.set_show_suggestions(id1.is_some() || id2.is_some());
        if let Some(ref ex1) = id1 {
            card.set_suggestion1_title(self.strings.get(&format!("exercise.{ex1}.title")).into());
            card.set_suggestion1_body(self.strings.get(&format!("exercise.{ex1}.body")).into());
        }
        card.set_show_suggestion2(id2.is_some());
        if let Some(ref ex2) = id2 {
            card.set_suggestion2_title(self.strings.get(&format!("exercise.{ex2}.title")).into());
            card.set_suggestion2_body(self.strings.get(&format!("exercise.{ex2}.body")).into());
        }

        let suggestion_frames = (Rc::new(Cell::new(1)), Rc::new(Cell::new(1)));
        self.start_long_break_art(
            &card,
            id1.as_deref(),
            id2.as_deref(),
            scale,
            generation,
            &suggestion_frames,
        );

        show_card_window(
            &card,
            ShowWindowContext {
                announcement,
                ..self.show_window_ctx("long", prewarmed, due_at, screen, true, generation)
            },
            |c, scr| {
                let h = f64::from(c.get_content_height()).max(200.0);
                match scr {
                    Some(s) => OverlayLayout::compute_long_break_window_size(h, &s),
                    None => DipSize::new(544.0, h + 80.0),
                }
            },
        );

        self.active = Some(ActiveBreakWindow::LongBreak(LongBreakState {
            card,
            variant,
            suggestion_ids: (id1, id2),
            suggestion_frames,
            scale,
            screen,
        }));
    }

    fn update_countdown(&mut self, snapshot: &EngineSnapshot) {
        match &self.active {
            Some(ActiveBreakWindow::ShortBreak(state)) => {
                if let Some(current) = &snapshot.current_break {
                    state.card.set_progress(short_break_progress(
                        current.remaining_duration,
                        current.total_duration,
                    ));
                }
                let _ = (&state.screen, state.fullscreen);
            }
            Some(ActiveBreakWindow::Warning(state)) => {
                let remaining = state.remaining(&*self.clock, snapshot.time_until_long_break);
                let buttons = warning_buttons(snapshot.strict_mode, snapshot.postpones_left);
                state
                    .card
                    .set_heading(warning_heading(&self.strings, remaining).into());
                state.card.set_postpone_label(
                    warning_postpone_label(
                        &self.strings,
                        self.settings.long_breaks.postpone_minutes,
                    )
                    .into(),
                );
                state.card.set_strict(buttons.show_strict_line);
                state.card.set_can_postpone(buttons.show_postpone);
                let _ = state.screen;
            }
            Some(ActiveBreakWindow::LongBreak(state)) => {
                if let Some(current) = &snapshot.current_break {
                    state.card.set_countdown(
                        long_break_countdown_text_localized(
                            &self.strings,
                            current.remaining_duration,
                        )
                        .into(),
                    );
                    state.card.set_progress(long_break_progress(
                        current.remaining_duration,
                        current.total_duration,
                    ));
                }
                state
                    .card
                    .set_can_end(long_break_can_end(snapshot.strict_mode));
                let _ = state.screen;
            }
            None => {}
        }
    }

    fn close_all(&mut self) {
        self.break_active.set(false);
        let _ = self.next_generation();
        self.hide_and_drop_active();
        if self.prewarm.is_none() {
            self.art.clear_cache();
        }
    }
}
