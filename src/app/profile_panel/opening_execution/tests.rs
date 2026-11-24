use super::*;
use crate::app::profile_panel::opening_rules::Page;
use crate::app::reading_viewport_stage::draw_test_candidate;
use crate::app::selection_viewport_tests::{adopt, capture, press, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, ReadingOrientation};
use crate::library::BookPreferences;
use crate::reading_profiles::{
    Field,
    opening::{Conditions, Execution},
};
use crate::session::ReadingViewport;
use primitive_app::App;
use primitive_app::viewport::{LogicalTurn, ViewportDecision};
use std::fs;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = ["fonts/NotoSans-Regular.ttf", "fonts/NotoSans-Bold.ttf"]
        .into_iter()
        .map(crate::test_fixture)
        .collect();
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

fn configure(
    app: &mut Reader,
    index: usize,
    selection: Vec<Field>,
    timing: Timing,
    ask: bool,
    condition: Option<Conditions>,
) {
    let mut profiles = app.profiles.clone();
    let profile = &mut profiles[index];
    profile.set_fields(selection).unwrap();
    profile.set_opening_execution(Execution { ask, timing });
    if let Some(rules) = condition {
        profile.set_opening_conditions(rules);
    } else {
        profile.set_apply_on_open(true);
    }
    app.commit_profiles(profiles).unwrap();
}

fn point(rect: Rect, long: bool) -> Input {
    let x = (rect.x + rect.w / 2) as f32;
    let y = (rect.y + rect.h / 2) as f32;
    if long {
        Input::LongTap(x, y)
    } else {
        Input::Tap(x, y)
    }
}

#[test]
fn opening_execution_editors_persist_only_on_save_in_both_viewports() {
    for (viewport, suffix) in [
        (ReadingViewport::PORTRAIT, "portrait"),
        (ReadingViewport::LANDSCAPE, "landscape"),
    ] {
        let f = Fixture::new();
        let mut app = reader(&f);
        app.open_path(&f.book).unwrap();
        app.save_reading_profile("Evening reading").unwrap();
        app.show(Screen::ReadingProfile(0));
        let mut canvas = adopt(&mut app, viewport, 1);
        tap_contents_item(&mut app, "Apply on book opening");
        tap_contents_item(&mut app, "Always apply");
        tap_contents_item(&mut app, "Ask to execute");
        assert_eq!(
            app.profiles[0].opening_execution(),
            Some(Execution {
                ask: true,
                timing: Timing::Default
            })
        );
        tap_contents_item(&mut app, "Execution delay");
        tap_contents_item(&mut app, "Prompt");
        assert_eq!(
            app.profiles[0].opening_execution().unwrap().timing,
            Timing::Prompt
        );
        tap_contents_item(&mut app, "Custom");
        assert_eq!(app.opening_delay_draft, 30);
        let layout = app.navigation_layout();
        press(&app, &mut canvas, layout.centered(MORE), false);
        tap(&mut app, layout.centered(MORE));
        assert_eq!(app.opening_delay_draft, 40);
        assert_eq!(
            app.input(point(layout.centered(LESS), true)),
            Change::Partial
        );
        assert_eq!(app.opening_delay_draft, 39);
        tap(&mut app, layout.centered(CANCEL));
        assert_eq!(
            app.profiles[0].opening_execution().unwrap().timing,
            Timing::Prompt
        );
        assert_eq!(app.screen, Screen::ReadingProfileOpening(0, Page::Delay));
        tap_contents_item(&mut app, "Custom");
        assert_eq!(app.opening_delay_draft, 30);
        for _ in 0..10 {
            tap(&mut app, layout.centered(MORE));
        }
        assert_eq!(app.opening_delay_draft, 100);
        for _ in 0..12 {
            tap(&mut app, layout.centered(LESS));
        }
        assert_eq!(app.opening_delay_draft, 10);
        tap(&mut app, layout.centered(MORE));
        tap(&mut app, layout.centered(MORE));
        assert_eq!(
            app.input(point(layout.centered(MORE), true)),
            Change::Partial
        );
        capture(
            &app,
            &mut canvas,
            &format!("profile-opening-delay-{suffix}.png"),
            false,
        );
        press(&app, &mut canvas, layout.centered(SAVE), true);
        tap(&mut app, layout.centered(SAVE));
        let expected = Some(Execution {
            ask: true,
            timing: Timing::Custom { tenths: 31 },
        });
        assert_eq!(app.profiles[0].opening_execution(), expected);
        let reopened = reader(&f);
        assert_eq!(reopened.profiles[0].opening_execution(), expected);
        let mut value = serde_json::to_value(&app.profiles).unwrap();
        value[0]["opening_execution"]["timing"]["tenths"] = serde_json::json!(101);
        assert!(crate::reading_profiles::decode(value).is_err());
    }
}

#[test]
fn opening_execution_asks_before_starting_a_delay_and_keeps_the_current_epub_anchor() {
    let f = Fixture::new();
    let mut app = reader(&f);
    app.open_path(&f.book).unwrap();
    let base = app.current_text_preferences().unwrap();
    let sized = |app: &mut Reader, name, size| {
        app.apply_text_profile(BookPreferences {
            logical_font_size: Some(size),
            ..base.clone()
        })
        .unwrap();
        app.save_reading_profile(name).unwrap();
    };
    sized(&mut app, "Alpha large print", 34.0);
    sized(&mut app, "Beta large print", 38.0);
    sized(&mut app, "Zulu default", 26.0);
    configure(
        &mut app,
        0,
        vec![Field::FontSize],
        Timing::Custom { tenths: 23 },
        true,
        None,
    );
    configure(
        &mut app,
        1,
        vec![Field::FontSize],
        Timing::Prompt,
        true,
        None,
    );
    configure(
        &mut app,
        2,
        vec![Field::FontSize],
        Timing::Default,
        false,
        None,
    );
    app.apply_text_profile(base.clone()).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.open_path(&f.book).unwrap();
    let id = app.open_book_id().unwrap();
    assert_eq!(app.library.state(id).unwrap().preferences, base);
    let opened = app.opening_execution.as_ref().unwrap().opened;
    assert!(!app.opening_ask_visible());
    assert_eq!(app.tick(opened), Change::Partial);
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(26.0)
    );
    assert_eq!(
        app.opening_execution
            .as_ref()
            .unwrap()
            .shown()
            .unwrap()
            .profile
            .name(),
        "Alpha large print"
    );
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    let yes = app
        .opening_ask_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    press(&app, &mut canvas, yes, false);
    capture(&app, &mut canvas, "profile-opening-ask-portrait.png", false);
    let location = app.current_position();
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(app.current_position(), location);
    let confirmed = opened + Duration::from_secs(8);
    assert_eq!(
        app.opening_ask_input(point(yes, false), confirmed),
        Some(Change::Partial)
    );
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(26.0)
    );
    assert_eq!(app.tick(confirmed), Change::Partial);
    assert_eq!(
        app.opening_execution
            .as_ref()
            .unwrap()
            .shown()
            .unwrap()
            .profile
            .name(),
        "Beta large print"
    );
    assert_eq!(app.input(Input::Tap(20.0, 20.0)), Change::Partial);
    assert!(!app.opening_ask_visible());
    app.follow_reflow_page(0, 0).unwrap();
    let moved = app.current_position().unwrap().anchor;
    app.show(Screen::BookTools);
    assert_eq!(app.opening_execution_deadline(), None);
    app.tick(confirmed + Duration::from_millis(2300));
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(26.0)
    );
    app.show(Screen::Book);
    assert_eq!(
        app.opening_execution_deadline(),
        Some(confirmed + Duration::from_millis(2300))
    );
    app.tick(confirmed + Duration::from_millis(2299));
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(26.0)
    );
    app.input(Input::Sleep);
    assert_eq!(app.opening_execution_deadline(), None);
    app.tick(confirmed + Duration::from_millis(2300));
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(26.0)
    );
    assert!(app.opening_execution.is_some());
    app.input(Input::Wake);
    assert_eq!(
        app.opening_execution_deadline(),
        Some(confirmed + Duration::from_millis(2300))
    );
    app.tick(confirmed + Duration::from_millis(2301));
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(34.0)
    );
    assert_eq!(app.current_position().unwrap().anchor, moved);
    assert!(app.opening_execution.is_none());

    // A different opening gets new decisions. Prompt executes on consent
    // itself, and the manual profile action skips automatic scheduling.
    app.open_path(&f.book).unwrap();
    let reopened = app.opening_execution.as_ref().unwrap().opened;
    app.tick(reopened);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    app.tick(reopened);
    let confirm = app
        .opening_ask_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    tap(&mut app, confirm);
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(38.0)
    );
    assert!(app.opening_execution.is_none());
    app.apply_reading_profile(0).unwrap();
    assert_eq!(
        app.library.state(id).unwrap().preferences.logical_font_size,
        Some(34.0)
    );
    assert!(app.opening_execution.is_none());
}

#[test]
fn opening_execution_queues_only_verified_pdf_turns_and_discards_stale_jobs() {
    let f = Fixture::new();
    let pdf = f.root.join("Harbor report.pdf");
    let bytes = crate::fixed_session::tests::striped_pdf_sized_bytes(4, 612, 820);
    fs::write(&pdf, &bytes).unwrap();
    let mut app = reader(&f);
    app.open_path(&pdf).unwrap();
    app.activate(Action::FixedPage(2)).unwrap();
    for (name, gamma) in [("Prompt contrast", 23), ("Wide view", 19)] {
        app.activate(Action::ScanGamma(gamma)).unwrap();
        app.save_reading_profile(name).unwrap();
    }
    configure(
        &mut app,
        0,
        vec![Field::ScanContrast],
        Timing::Prompt,
        false,
        None,
    );
    configure(
        &mut app,
        1,
        vec![Field::ScanContrast],
        Timing::Custom { tenths: 10 },
        true,
        Some(Conditions {
            orientations: vec![ReadingOrientation::LandscapeRight],
            ..Conditions::default()
        }),
    );
    app.activate(Action::ScanGamma(31)).unwrap();
    let _ = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let gamma = |r: &Reader| {
        r.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .scan_gamma_tenths
    };
    app.negotiated_opening_profiles = true;
    app.open_path(&pdf).unwrap();
    let denied = draw_test_candidate(&mut app, 2, LogicalTurn::Clockwise, "", false);
    assert!(app.opening_execution.is_none());
    assert_eq!(gamma(&app), Some(31));
    app.finish_viewport(denied, ViewportDecision::Abort);
    assert!(app.opening_execution.is_none());
    let upright = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    assert_eq!(gamma(&app), Some(23));
    assert!(app.opening_execution.is_none());

    app.open_path(&pdf).unwrap();
    let accepted = draw_test_candidate(&mut app, 4, LogicalTurn::Clockwise, "", false);
    assert!(app.opening_execution.is_none());
    app.finish_viewport(accepted, ViewportDecision::Commit);
    let started = app.opening_execution.as_ref().unwrap().opened;
    assert_eq!(app.tick(started), Change::Partial);
    assert_eq!(gamma(&app), Some(23));
    let mut canvas = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::LANDSCAPE);
    capture(&app, &mut canvas, "profile-opening-ask-landscape.png", true);
    let yes = app
        .opening_ask_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    press(&app, &mut canvas, yes, true);
    app.opening_ask_input(point(yes, false), started);
    app.storage_returned().unwrap();
    app.tick(started + Duration::from_secs(3));
    assert_eq!(gamma(&app), Some(23));
    assert!(app.opening_execution.is_none());

    app.open_path(&pdf).unwrap();
    let renewed = draw_test_candidate(&mut app, 5, LogicalTurn::Clockwise, "", false);
    app.finish_viewport(renewed, ViewportDecision::Commit);
    let reopened = app.opening_execution.as_ref().unwrap().opened;
    app.tick(reopened);
    assert!(app.opening_ask_visible());
    app.opening_ask_input(point(yes, false), reopened);
    let previous_epoch = app.opening_profile_epoch;
    app.open_path(&pdf).unwrap();
    assert_ne!(app.opening_profile_epoch, previous_epoch);
    app.tick(reopened + Duration::from_secs(1));
    assert_eq!(gamma(&app), Some(23));
    assert!(app.opening_execution.is_none());

    let fresh = draw_test_candidate(&mut app, 6, LogicalTurn::Clockwise, "", false);
    app.finish_viewport(fresh, ViewportDecision::Commit);
    let opened = app.opening_execution.as_ref().unwrap().opened;
    app.tick(opened);
    assert!(app.opening_ask_visible());
    app.opening_ask_input(point(yes, false), opened);
    app.jump_fixed(1, None).unwrap();
    app.tick(opened + Duration::from_secs(1));
    assert_eq!(gamma(&app), Some(19));
    assert_eq!(app.current_position().unwrap().page, 1);
    assert_eq!(app.reading_viewport(), ReadingViewport::LANDSCAPE);
    assert_eq!(fs::read(&pdf).unwrap(), bytes);
    app.open_path(&pdf).unwrap();
    let last = draw_test_candidate(&mut app, 7, LogicalTurn::Clockwise, "", false);
    app.finish_viewport(last, ViewportDecision::Commit);
    assert!(app.opening_execution.is_some());
    app.close_book_to_library();
    assert!(app.opening_execution.is_none());
}

#[test]
fn opening_execution_orders_late_jobs_by_due_time_then_profile_name_order() {
    let f = Fixture::new();
    let mut app = reader(&f);
    app.open_path(&f.book).unwrap();
    app.save_reading_profile("Original").unwrap();
    let mut profiles = Vec::new();
    for (name, timing) in [
        ("Alpha", Timing::Custom { tenths: 15 }),
        ("Bravo", Timing::Default),
        ("Charlie", Timing::Custom { tenths: 15 }),
        ("Delta", Timing::Custom { tenths: 10 }),
    ] {
        let mut profile = app.profiles[0].clone();
        profile.set_name(name);
        profile.set_opening_execution(Execution { ask: false, timing });
        profiles.push(profile);
    }
    let now = Instant::now();
    let mut run = OpeningExecution::new(17, 42, profiles, now).unwrap();
    let ready = run.take_due(now + Duration::from_secs(2));
    assert_eq!(
        ready.iter().map(Profile::name).collect::<Vec<_>>(),
        ["Bravo", "Delta", "Alpha", "Charlie"]
    );
    assert!(run.tasks.is_empty());
}
