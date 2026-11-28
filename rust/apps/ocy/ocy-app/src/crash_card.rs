//! Crash-report ask card (CR3): on the next start after a crash, while consent is "Ask each
//! time" and reports are pending, a calm corner card (short-break card style, never takes focus)
//! asks once whether to send them.
//!
//! - **Send** calls [`crate::crash_sender::send_now`] with the pending report IDs; **Don't send**
//!   calls [`crate::crash_sender::discard`]. Both close the card.
//! - **Don't ask again** turns the pressed button into the consent (`Always` / `Never`), saved
//!   through the host's settings (`AppHost::save_settings`, via the `save_consent` callback) —
//!   never by writing `settings.json`.
//! - Closing without a choice keeps the reports for the next start; the card shows at most once
//!   per start and waits while a break card or long break is on screen.
//! - Builds without a PostHog key ([`crate::usage_stats::is_available`] false) never show it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::PlatformServices;
use ocy_core::settings::CrashReportsConsent;
use ocy_ui::{CrashPreviewItem, CrashReportCard};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::crash_reports::{CrashFrame, CrashKind, CrashReport};
use crate::strings::{Strings, font_family_for};
use crate::win32_window::{WindowRole, WindowRoleGuard};

/// How often the startup check looks again while a break is on screen.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Delay before the first look, so the card never competes with startup.
const FIRST_LOOK_DELAY: Duration = Duration::from_secs(3);

/// `--demo-crash-card[=card|preview]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoCrashCard {
    /// The card itself.
    Card,
    /// The card with "See what's in it" already open.
    Preview,
}

impl DemoCrashCard {
    /// Parses `card` / `preview`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "card" => Some(Self::Card),
            "preview" => Some(Self::Preview),
            _ => None,
        }
    }
}

/// Whether the startup card should be offered.
#[must_use]
pub fn should_offer(available: bool, consent: CrashReportsConsent, pending: usize) -> bool {
    available && consent == CrashReportsConsent::Ask && pending > 0
}

/// The consent a choice leaves behind: with "Don't ask again" ticked, Send becomes `Always` and
/// Don't send becomes `Never`; without the tick the setting stays as it is (`None`).
#[must_use]
pub const fn consent_after_choice(send: bool, dont_ask_again: bool) -> Option<CrashReportsConsent> {
    match (dont_ask_again, send) {
        (false, _) => None,
        (true, true) => Some(CrashReportsConsent::Always),
        (true, false) => Some(CrashReportsConsent::Never),
    }
}

/// Every localised text on the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashCardTexts {
    pub heading: String,
    pub body: String,
    pub see_what: String,
    pub dont_ask: String,
    pub send: String,
    pub dont_send: String,
    pub close: String,
    pub preview_title: String,
    pub preview_help: String,
    pub copy: String,
    pub back: String,
}

impl CrashCardTexts {
    #[must_use]
    pub fn from_strings(strings: &Strings) -> Self {
        let get = |key: &str| strings.get(key).to_owned();
        Self {
            heading: get("crash.card.title"),
            body: get("crash.card.body"),
            see_what: get("crash.card.seeWhat"),
            dont_ask: get("crash.card.dontAskAgain"),
            send: get("crash.card.send"),
            dont_send: get("crash.card.dontSend"),
            close: get("crash.card.close"),
            preview_title: get("crash.preview.title"),
            preview_help: get("crash.preview.help"),
            copy: get("crash.preview.copy"),
            back: get("crash.preview.back"),
        }
    }
}

/// "Reports waiting: 3 ..." for several reports; empty for one.
#[must_use]
pub fn count_text(strings: &Strings, count: usize) -> String {
    if count <= 1 {
        String::new()
    } else {
        Strings::format_template(
            strings.plural("crash.card.count", u64::try_from(count).unwrap_or(u64::MAX)),
            &[("count", &count)],
        )
    }
}

/// One preview entry per report: its ID, the "Report ID: ..." line and the exact text sent.
#[must_use]
pub fn preview_entries(
    strings: &Strings,
    reports: &[CrashReport],
) -> Vec<(String, String, String)> {
    reports
        .iter()
        .map(|report| {
            let id_line = Strings::format_template(
                strings.get("crash.preview.reportId"),
                &[("id", &report.report_id)],
            );
            (
                report.report_id.clone(),
                id_line,
                crate::crash_reports::preview_text(report),
            )
        })
        .collect()
}

/// A fake, already scrubbed report for `--demo-crash-card` and the review renders.
#[must_use]
pub fn demo_report() -> CrashReport {
    let mut report = CrashReport::new(CrashKind::Panic, 1_764_230_400);
    report.report_id = "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0".to_owned();
    report.message = "called `Option::unwrap()` on a `None` value".to_owned();
    report.location = "apps/ocy/ocy-app/src/scheduler.rs:118:22".to_owned();
    let mut top = CrashFrame::new("ocy_app::scheduler::next_tick".to_owned(), true);
    top.filename = Some("apps/ocy/ocy-app/src/scheduler.rs".to_owned());
    top.lineno = Some(118);
    top.colno = Some(22);
    report.frames = vec![
        top,
        CrashFrame::new(
            "slint::timer::Timer::start at slint-1.19.0/src/timer.rs".to_owned(),
            false,
        ),
        CrashFrame::new("std::rt::lang_start_internal".to_owned(), false),
    ];
    report
}

/// The monospace family for the preview text on this platform.
const fn mono_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "monospace"
    }
}

/// Fills `card` with texts and the preview entries (shared by the app and the review renders).
pub fn fill_card(card: &CrashReportCard, strings: &Strings, reports: &[CrashReport]) {
    let t = CrashCardTexts::from_strings(strings);
    card.set_font_family(font_family_for(strings.language()).into());
    card.set_mono_font_family(mono_font_family().into());
    card.set_heading(t.heading.into());
    card.set_body(t.body.into());
    card.set_count_text(count_text(strings, reports.len()).into());
    card.set_see_what_label(t.see_what.into());
    card.set_dont_ask_label(t.dont_ask.into());
    card.set_send_label(t.send.into());
    card.set_dont_send_label(t.dont_send.into());
    card.set_close_label(t.close.into());
    card.set_preview_title(t.preview_title.into());
    card.set_preview_help(t.preview_help.into());
    card.set_copy_label(t.copy.into());
    card.set_back_label(t.back.into());
    let items: Vec<CrashPreviewItem> = preview_entries(strings, reports)
        .into_iter()
        .map(|(id, id_line, text)| CrashPreviewItem {
            report_id: SharedString::from(id),
            id_line: SharedString::from(id_line),
            text: SharedString::from(text),
        })
        .collect();
    card.set_reports(ModelRc::new(VecModel::from(items)));
}

/// What the card needs from the host.
#[derive(Clone)]
pub struct CrashCardDeps {
    pub platform: Arc<dyn PlatformServices>,
    pub strings: Arc<Strings>,
    pub error_log: ErrorLog,
    /// `true` while a break card, warning or long break is on screen.
    pub break_active: Rc<Cell<bool>>,
    /// The host's current in-memory crash-report consent.
    pub consent: Rc<dyn Fn() -> CrashReportsConsent>,
    /// Saves a new consent through `AppHost::save_settings`.
    pub save_consent: Rc<dyn Fn(CrashReportsConsent)>,
}

impl std::fmt::Debug for CrashCardDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrashCardDeps").finish_non_exhaustive()
    }
}

/// Keeps the startup check timer and the open card alive; drop it to stop both.
pub struct CrashCardScheduler {
    _timer: Rc<Timer>,
    _card: Rc<RefCell<Option<CrashReportCard>>>,
}

impl std::fmt::Debug for CrashCardScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrashCardScheduler").finish_non_exhaustive()
    }
}

/// Starts the once-per-start check. `demo` shows the card with [`demo_report`] whatever the
/// consent and key (isolated demo runs only).
#[must_use]
pub fn start(deps: CrashCardDeps, demo: Option<DemoCrashCard>) -> CrashCardScheduler {
    let timer = Rc::new(Timer::default());
    let card_slot: Rc<RefCell<Option<CrashReportCard>>> = Rc::new(RefCell::new(None));
    let first = Rc::new(Cell::new(true));
    let phase = Rc::new(Cell::new(CardPhase::Waiting));
    let screen = Rc::new(Cell::new(None));
    let weak_timer = Rc::downgrade(&timer);
    let slot = Rc::clone(&card_slot);
    let delay = if demo.is_some() {
        Duration::from_millis(200)
    } else {
        FIRST_LOOK_DELAY
    };
    let stop = move |weak: &std::rc::Weak<Timer>| {
        if let Some(t) = weak.upgrade() {
            t.stop();
        }
    };
    timer.start(TimerMode::Repeated, delay, move || {
        if first.replace(false)
            && let Some(t) = weak_timer.upgrade()
        {
            t.set_interval(POLL_INTERVAL);
        }
        // Live re-check on every look: current consent and a fresh pending list (CR3b).
        let live = if demo.is_some() {
            Some(vec![demo_report()])
        } else {
            fresh_offer(
                crate::usage_stats::is_available(),
                (deps.consent)(),
                crate::crash_reports::pending_reports(),
            )
        };
        let card_open = slot.borrow().is_some();
        let (next, action) = card_step(
            phase.get(),
            deps.break_active.get(),
            card_open,
            live.is_some(),
        );
        phase.set(next);
        match action {
            CardAction::Wait => {}
            CardAction::Stop => stop(&weak_timer),
            CardAction::Close => {
                close_card(&slot);
                stop(&weak_timer);
            }
            CardAction::Show => {
                let Some(pending) = live else {
                    stop(&weak_timer);
                    return;
                };
                screen.set(crate::slint_breaks::active_screen_info(
                    deps.platform.as_ref(),
                ));
                let open_preview = demo == Some(DemoCrashCard::Preview);
                match show_card(&deps, pending, &slot, &screen, open_preview, demo.is_some()) {
                    Ok(card) => *slot.borrow_mut() = Some(card),
                    Err(err) => {
                        deps.error_log.log_error("crash_card_show", &err);
                        phase.set(CardPhase::Done);
                        stop(&weak_timer);
                    }
                }
            }
            CardAction::Hide => {
                if let Some(card) = slot.borrow().as_ref() {
                    let _ = card.hide();
                }
            }
            CardAction::ReShow => {
                screen.set(crate::slint_breaks::active_screen_info(
                    deps.platform.as_ref(),
                ));
                if let (Some(card), Some(pending)) = (slot.borrow().as_ref(), live) {
                    // Only the reports still pending are shown.
                    fill_card(card, &deps.strings, &pending);
                    present(&deps, card, screen.get());
                }
            }
        }
    });
    CrashCardScheduler {
        _timer: timer,
        _card: card_slot,
    }
}

/// The reports to offer right now, or `None` when the card must not be (or stay) on screen:
/// re-evaluated with the live consent and a fresh pending list before every show or re-show.
#[must_use]
pub fn fresh_offer(
    available: bool,
    consent: CrashReportsConsent,
    pending: Vec<CrashReport>,
) -> Option<Vec<CrashReport>> {
    should_offer(available, consent, pending.len()).then_some(pending)
}

/// The IDs from `ids` that are still in `pending` (no stale IDs reach send or discard).
#[must_use]
pub fn still_pending_ids(ids: &[String], pending: &[CrashReport]) -> Vec<String> {
    ids.iter()
        .filter(|id| pending.iter().any(|r| &r.report_id == *id))
        .cloned()
        .collect()
}

/// What a click on the card's buttons does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickOutcome {
    /// Report IDs to send now.
    pub send: Vec<String>,
    /// Report IDs to discard.
    pub discard: Vec<String>,
    /// The consent to save, if any.
    pub save: Option<CrashReportsConsent>,
}

/// Decides a button click from the live consent and the still-pending IDs (read once).
///
/// With live consent "Ask" the choice applies as usual. On a stale card (consent changed
/// meanwhile) "Don't send" still discards the pending reports (privacy first), "Send" does
/// nothing, and consent is never changed.
#[must_use]
pub fn click_outcome(
    send: bool,
    dont_ask: bool,
    live: CrashReportsConsent,
    still_pending: Vec<String>,
) -> ClickOutcome {
    let none = ClickOutcome {
        send: Vec::new(),
        discard: Vec::new(),
        save: None,
    };
    if live == CrashReportsConsent::Ask {
        let save = consent_after_choice(send, dont_ask);
        if send {
            ClickOutcome {
                send: still_pending,
                save,
                ..none
            }
        } else {
            ClickOutcome {
                discard: still_pending,
                save,
                ..none
            }
        }
    } else if send {
        none
    } else {
        ClickOutcome {
            discard: still_pending,
            ..none
        }
    }
}

/// [`next_card_action`] with the live offer: once the offer lapses (consent left "Ask", or no
/// report is pending any more) a waiting card is never shown and an open one closes at once.
#[must_use]
pub const fn card_step(
    phase: CardPhase,
    break_active: bool,
    card_open: bool,
    offered: bool,
) -> (CardPhase, CardAction) {
    if !offered && !matches!(phase, CardPhase::Done) {
        let action = if card_open {
            CardAction::Close
        } else {
            CardAction::Stop
        };
        return (CardPhase::Done, action);
    }
    next_card_action(phase, break_active, card_open)
}

/// Where the startup card is in its once-per-start life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardPhase {
    /// Offered, waiting for the screen to be clear of breaks.
    Waiting,
    /// On screen.
    Shown,
    /// Hidden while a break card or long break is on screen; the reports stay.
    HiddenForBreak,
    /// Decided or closed; never again this start.
    Done,
}

/// What the startup timer does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Wait,
    Show,
    Hide,
    ReShow,
    Stop,
    /// The offer lapsed while the card was open: close it now.
    Close,
}

/// One step of the card's life: it never shares the screen with a break card or the long break.
#[must_use]
pub const fn next_card_action(
    phase: CardPhase,
    break_active: bool,
    card_open: bool,
) -> (CardPhase, CardAction) {
    match phase {
        CardPhase::Waiting if break_active => (CardPhase::Waiting, CardAction::Wait),
        CardPhase::Waiting => (CardPhase::Shown, CardAction::Show),
        CardPhase::Shown | CardPhase::HiddenForBreak if !card_open => {
            (CardPhase::Done, CardAction::Stop)
        }
        CardPhase::Shown if break_active => (CardPhase::HiddenForBreak, CardAction::Hide),
        CardPhase::HiddenForBreak if !break_active => (CardPhase::Shown, CardAction::ReShow),
        CardPhase::Shown | CardPhase::HiddenForBreak => (phase, CardAction::Wait),
        CardPhase::Done => (CardPhase::Done, CardAction::Stop),
    }
}

/// Hides the card and drops it on the next turn of the event loop (never inside its callback).
fn close_card(slot: &Rc<RefCell<Option<CrashReportCard>>>) {
    if let Some(card) = slot.borrow().as_ref() {
        let _ = card.hide();
    }
    let slot = Rc::clone(slot);
    Timer::single_shot(Duration::ZERO, move || {
        slot.borrow_mut().take();
    });
}

fn show_card(
    deps: &CrashCardDeps,
    reports: Vec<CrashReport>,
    slot: &Rc<RefCell<Option<CrashReportCard>>>,
    screen: &Rc<Cell<Option<crate::layout::ScreenInfo>>>,
    open_preview: bool,
    demo: bool,
) -> Result<CrashReportCard, slint::PlatformError> {
    let card = {
        let _role = WindowRoleGuard::new(WindowRole::BreakCard);
        CrashReportCard::new()?
    };
    #[cfg(target_os = "macos")]
    card.set_card_inset(0.0);
    let dark = deps.platform.app_theme().is_light_app_theme() == Some(false);
    ocy_ui::set_dark(&card, dark);
    fill_card(&card, &deps.strings, &reports);
    card.set_show_preview(open_preview);

    let ids: Vec<String> = reports.iter().map(|r| r.report_id.clone()).collect();
    {
        let slot = Rc::clone(slot);
        let save_consent = Rc::clone(&deps.save_consent);
        let consent_now = Rc::clone(&deps.consent);
        let weak = card.as_weak();
        card.on_decided(move |send| {
            let dont_ask = weak.upgrade().is_some_and(|c| c.get_dont_ask_again());
            // Read the live consent and the still-pending IDs once.
            let outcome = if demo {
                ClickOutcome {
                    send: if send { ids.clone() } else { Vec::new() },
                    discard: if send { Vec::new() } else { ids.clone() },
                    save: consent_after_choice(send, dont_ask),
                }
            } else {
                let pending = still_pending_ids(&ids, &crate::crash_reports::pending_reports());
                click_outcome(send, dont_ask, (consent_now)(), pending)
            };
            if !outcome.send.is_empty() {
                let refs: Vec<&str> = outcome.send.iter().map(String::as_str).collect();
                let _ = crate::crash_sender::send_now(&refs);
            }
            if !outcome.discard.is_empty() {
                let refs: Vec<&str> = outcome.discard.iter().map(String::as_str).collect();
                let _ = crate::crash_sender::discard(&refs);
            }
            if let Some(consent) = outcome.save {
                save_consent(consent);
            }
            close_card(&slot);
        });
    }
    {
        let slot = Rc::clone(slot);
        card.on_closed(move || close_card(&slot));
    }

    {
        let weak = card.as_weak();
        let screen = Rc::clone(screen);
        card.on_layout_changed(move || {
            let weak = weak.clone();
            let screen = Rc::clone(&screen);
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(c) = weak.upgrade() {
                    crate::slint_breaks::resize_corner_card(
                        c.window(),
                        screen.get(),
                        f64::from(c.get_content_height()),
                    );
                }
            });
        });
    }

    present(deps, &card, screen.get());
    Ok(card)
}

/// Puts the card on screen (again) in the corner of `screen`, without taking focus.
fn present(
    deps: &CrashCardDeps,
    card: &CrashReportCard,
    screen: Option<crate::layout::ScreenInfo>,
) {
    let t = CrashCardTexts::from_strings(&deps.strings);
    crate::slint_breaks::show_corner_card(
        card,
        screen,
        deps.error_log.clone(),
        format!("{}. {}", t.heading, t.body),
        |c: &CrashReportCard| f64::from(c.get_content_height()),
    );
}
