//! CR3: the crash-report card's decision logic, texts and demo flag (no window, no network).
use ocy_app::break_logic::parse_cli_args;
use ocy_app::crash_card::{
    CrashCardTexts, DemoCrashCard, consent_after_choice, count_text, demo_report, preview_entries,
    should_offer,
};
use ocy_app::strings::Strings;
use ocy_core::settings::CrashReportsConsent;

#[test]
fn dont_ask_again_turns_the_pressed_button_into_the_consent() {
    assert_eq!(
        consent_after_choice(true, true),
        Some(CrashReportsConsent::Always)
    );
    assert_eq!(
        consent_after_choice(false, true),
        Some(CrashReportsConsent::Never)
    );
    // Without the tick the setting stays "Ask each time".
    assert_eq!(consent_after_choice(true, false), None);
    assert_eq!(consent_after_choice(false, false), None);
}

#[test]
fn card_is_offered_only_with_a_key_ask_consent_and_pending_reports() {
    assert!(should_offer(true, CrashReportsConsent::Ask, 1));
    assert!(should_offer(true, CrashReportsConsent::Ask, 5));
    assert!(!should_offer(false, CrashReportsConsent::Ask, 1));
    assert!(!should_offer(true, CrashReportsConsent::Ask, 0));
    assert!(!should_offer(true, CrashReportsConsent::Always, 1));
    assert!(!should_offer(true, CrashReportsConsent::Never, 1));
}

#[test]
fn count_line_only_for_several_reports() {
    let strings = Strings::from_language("en");
    assert_eq!(count_text(&strings, 1), "");
    assert_eq!(
        count_text(&strings, 3),
        "Reports waiting: 3. One choice covers all of them."
    );
}

#[test]
fn preview_shows_the_exact_text_that_is_sent_with_the_report_id() {
    let strings = Strings::from_language("en");
    let report = demo_report();
    let entries = preview_entries(&strings, std::slice::from_ref(&report));
    assert_eq!(entries.len(), 1);
    let (id, id_line, text) = &entries[0];
    assert_eq!(id, &report.report_id);
    assert_eq!(id_line, &format!("Report ID: {}", report.report_id));
    assert_eq!(text, &ocy_app::crash_reports::preview_text(&report));
}

#[test]
fn demo_report_is_already_scrubbed() {
    let text = ocy_app::crash_reports::preview_text(&demo_report());
    for forbidden in [":\\", "/home/", "/Users/", "\\\\"] {
        assert!(!text.contains(forbidden), "{forbidden} in demo report");
    }
}

#[test]
fn every_card_text_comes_from_the_table() {
    let t = CrashCardTexts::from_strings(&Strings::from_language("en"));
    assert_eq!(t.heading, "Ocy closed unexpectedly");
    assert_eq!(t.send, "Send");
    assert_eq!(t.dont_send, "Don't send");
    assert_eq!(t.dont_ask, "Don't ask again");
    assert_eq!(t.see_what, "See what's in it");
}

#[test]
fn demo_crash_card_flag_parses_and_makes_the_run_isolated() {
    let args = |items: &[&str]| parse_cli_args(items.iter().map(|s| (*s).to_owned()));
    let cli = args(&["--demo-crash-card"]);
    assert_eq!(cli.demo_crash_card, Some(DemoCrashCard::Card));
    assert!(ocy_app::instance::is_probe_mode(&cli));
    assert_eq!(
        args(&["--demo-crash-card=preview"]).demo_crash_card,
        Some(DemoCrashCard::Preview)
    );
    assert_eq!(args(&["--demo-crash-card=nope"]).demo_crash_card, None);
    assert_eq!(args(&[]).demo_crash_card, None);
}

#[test]
fn card_never_shares_the_screen_with_a_break() {
    use ocy_app::crash_card::{CardAction, CardPhase, next_card_action};
    // Waits while a break is on screen, then shows.
    assert_eq!(
        next_card_action(CardPhase::Waiting, true, false),
        (CardPhase::Waiting, CardAction::Wait)
    );
    assert_eq!(
        next_card_action(CardPhase::Waiting, false, false),
        (CardPhase::Shown, CardAction::Show)
    );
    // A break starting while the card is open hides it (reports kept) and brings it back after.
    assert_eq!(
        next_card_action(CardPhase::Shown, true, true),
        (CardPhase::HiddenForBreak, CardAction::Hide)
    );
    assert_eq!(
        next_card_action(CardPhase::HiddenForBreak, true, true),
        (CardPhase::HiddenForBreak, CardAction::Wait)
    );
    assert_eq!(
        next_card_action(CardPhase::HiddenForBreak, false, true),
        (CardPhase::Shown, CardAction::ReShow)
    );
    assert_eq!(
        next_card_action(CardPhase::Shown, false, true),
        (CardPhase::Shown, CardAction::Wait)
    );
    // Decided or closed: done for this start.
    assert_eq!(
        next_card_action(CardPhase::Shown, false, false),
        (CardPhase::Done, CardAction::Stop)
    );
    assert_eq!(
        next_card_action(CardPhase::Done, false, false),
        (CardPhase::Done, CardAction::Stop)
    );
}

fn report_with_id(id: &str) -> ocy_app::crash_reports::CrashReport {
    let mut r = demo_report();
    id.clone_into(&mut r.report_id);
    r
}

#[test]
fn consent_never_while_waiting_for_a_break_means_never_shown() {
    use ocy_app::crash_card::{CardAction, CardPhase, card_step, fresh_offer};
    // First look: Ask, one pending, break on screen -> wait.
    let offered = fresh_offer(true, CrashReportsConsent::Ask, vec![demo_report()]).is_some();
    let (phase, action) = card_step(CardPhase::Waiting, true, false, offered);
    assert_eq!(action, CardAction::Wait);
    // Consent changes to Never (CR1 deletes the reports); break ends -> never shown.
    let offered = fresh_offer(true, CrashReportsConsent::Never, vec![demo_report()]).is_some();
    assert!(!offered);
    assert_eq!(
        card_step(phase, false, false, offered),
        (CardPhase::Done, CardAction::Stop)
    );
}

#[test]
fn consent_always_while_open_closes_the_card() {
    use ocy_app::crash_card::{CardAction, CardPhase, card_step, fresh_offer};
    let offered = fresh_offer(true, CrashReportsConsent::Always, vec![demo_report()]).is_some();
    for phase in [CardPhase::Shown, CardPhase::HiddenForBreak] {
        for break_active in [false, true] {
            assert_eq!(
                card_step(phase, break_active, true, offered),
                (CardPhase::Done, CardAction::Close)
            );
        }
    }
}

#[test]
fn emptied_pending_list_is_not_shown() {
    use ocy_app::crash_card::{CardAction, CardPhase, card_step, fresh_offer};
    assert!(fresh_offer(true, CrashReportsConsent::Ask, Vec::new()).is_none());
    assert_eq!(
        card_step(CardPhase::Waiting, false, false, false),
        (CardPhase::Done, CardAction::Stop)
    );
    assert_eq!(
        card_step(CardPhase::HiddenForBreak, false, true, false),
        (CardPhase::Done, CardAction::Close)
    );
}

#[test]
fn normal_ask_flow_is_unchanged() {
    use ocy_app::crash_card::{CardPhase, card_step, fresh_offer, next_card_action};
    let offered = fresh_offer(true, CrashReportsConsent::Ask, vec![demo_report()]);
    assert_eq!(offered.map(|v| v.len()), Some(1));
    for phase in [
        CardPhase::Waiting,
        CardPhase::Shown,
        CardPhase::HiddenForBreak,
        CardPhase::Done,
    ] {
        for b in [false, true] {
            for open in [false, true] {
                assert_eq!(
                    card_step(phase, b, open, true),
                    next_card_action(phase, b, open)
                );
            }
        }
    }
}

#[test]
fn buttons_act_only_on_ids_still_pending() {
    use ocy_app::crash_card::still_pending_ids;
    let ids = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let pending = vec![report_with_id("c"), report_with_id("a")];
    assert_eq!(still_pending_ids(&ids, &pending), vec!["a", "c"]);
    assert!(still_pending_ids(&ids, &[]).is_empty());
}

#[test]
fn stale_card_click_never_changes_consent_and_dont_send_still_discards() {
    use ocy_app::crash_card::{ClickOutcome, click_outcome};
    let ids = || vec!["a".to_owned(), "c".to_owned()];
    let discard_only = ClickOutcome {
        send: vec![],
        discard: ids(),
        save: None,
    };
    let nothing = ClickOutcome {
        send: vec![],
        discard: vec![],
        save: None,
    };
    // Live Always + Don't send (with and without the tick): discard, consent unchanged.
    for tick in [false, true] {
        assert_eq!(
            click_outcome(false, tick, CrashReportsConsent::Always, ids()),
            discard_only
        );
        assert_eq!(
            click_outcome(false, tick, CrashReportsConsent::Never, ids()),
            discard_only
        );
    }
    // Live Never/Always + Send (with and without the tick): nothing, consent unchanged.
    for tick in [false, true] {
        assert_eq!(
            click_outcome(true, tick, CrashReportsConsent::Never, ids()),
            nothing
        );
        assert_eq!(
            click_outcome(true, tick, CrashReportsConsent::Always, ids()),
            nothing
        );
    }
}

#[test]
fn ask_card_click_flows_are_unchanged() {
    use ocy_app::crash_card::{ClickOutcome, click_outcome};
    let ids = || vec!["a".to_owned()];
    let ask = CrashReportsConsent::Ask;
    assert_eq!(
        click_outcome(true, false, ask, ids()),
        ClickOutcome {
            send: ids(),
            discard: vec![],
            save: None
        }
    );
    assert_eq!(
        click_outcome(true, true, ask, ids()),
        ClickOutcome {
            send: ids(),
            discard: vec![],
            save: Some(CrashReportsConsent::Always)
        }
    );
    assert_eq!(
        click_outcome(false, false, ask, ids()),
        ClickOutcome {
            send: vec![],
            discard: ids(),
            save: None
        }
    );
    assert_eq!(
        click_outcome(false, true, ask, ids()),
        ClickOutcome {
            send: vec![],
            discard: ids(),
            save: Some(CrashReportsConsent::Never)
        }
    );
}
