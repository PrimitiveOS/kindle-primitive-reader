use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{BookPreferences, ReadingOrientation};
use crate::reading_profiles::opening::Conditions;
use crate::session::ReadingViewport;
use primitive_app::{App, viewport::LogicalTurn};
use primitive_ui::Key;
use serde_json::json;

fn app(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn schedule(app: &mut Reader, start: u16, end: u16) {
    app.save_opening_time(0, Some(TimeWindow { start, end }))
        .unwrap();
}

#[test]
fn opening_time_editor_uses_stock_steps_and_keeps_drafts_and_triggers_independent() {
    let fixture = Fixture::new();
    let mut app = app(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.save_reading_profile("Evening reading").unwrap();
    let overview = Screen::ReadingProfileOpening(0, Page::Overview);
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    app.show(overview);
    tap_contents_item(&mut app, "Always apply");
    tap_contents_item(&mut app, "Only within time window");
    tap_contents_item(&mut app, "Start time");
    app.profile_opening_time_draft = 21 * 60 + 58;
    let place = |rect| app.navigation_layout().centered(rect);
    let minute_up = place(STEPS[3].2);
    let hour_down = place(STEPS[0].2);
    let save = place(SAVE);
    tap(&mut app, minute_up);
    assert_eq!(app.profile_opening_time_draft, 21 * 60 + 59);
    let hold =
        |rect: Rect| Input::LongTap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
    assert_eq!(app.input(hold(minute_up)), Change::Partial);
    assert_eq!(app.profile_opening_time_draft, 21 * 60);
    assert_eq!(app.input(hold(hour_down)), Change::Partial);
    assert_eq!(app.profile_opening_time_draft, 17 * 60);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert!(app.profiles[0].opening_time().is_none());

    tap_contents_item(&mut app, "Start time");
    app.profile_opening_time_draft = 22 * 60;
    capture(
        &app,
        &mut canvas,
        "landscape-profile-opening-start-time.png",
        false,
    );
    tap(&mut app, save);
    assert_eq!(
        app.profiles[0].opening_time(),
        Some(TimeWindow {
            start: 1320,
            end: 1320
        })
    );
    tap_contents_item(&mut app, "End time");
    assert_eq!(app.profile_opening_time_draft, 1320);
    app.profile_opening_time_draft = 6 * 60;
    let minute = app
        .navigation_layout()
        .centered(Rect::new(804, 786, 188, 100));
    tap(&mut app, minute);
    let entry = Name::OpeningTime(0, Endpoint::End, Part::Minute);
    assert_eq!(app.screen, Screen::ProfileName(entry));
    let press = |app: &mut Reader, value| {
        let button = key(app, value, "Set");
        tap(app, button);
    };
    press(&mut app, Key::Char('9'));
    press(&mut app, Key::Char('9'));
    press(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::ProfileName(entry));
    assert!(app.notice.as_deref().unwrap().contains("0 to 59"));
    press(&mut app, Key::Delete);
    press(&mut app, Key::Delete);
    press(&mut app, Key::Char('3'));
    press(&mut app, Key::Char('0'));
    capture(
        &app,
        &mut canvas,
        "landscape-profile-opening-minute-entry.png",
        false,
    );
    press(&mut app, Key::Go);
    assert_eq!(app.screen, entry.back());
    assert_eq!(app.profile_opening_time_draft, 390);
    assert_eq!(app.profiles[0].opening_time().unwrap().end, 1320);
    tap(&mut app, save);
    let time = TimeWindow {
        start: 1320,
        end: 390,
    };
    assert_eq!(app.profiles[0].opening_time(), Some(time));
    assert!(app.profiles[0].applies_on_open());
    capture(
        &app,
        &mut canvas,
        "landscape-profile-opening-time-window.png",
        false,
    );

    let saved_profile = serde_json::to_value(&app.profiles).unwrap();
    assert_eq!(
        crate::reading_profiles::decode(saved_profile.clone()).unwrap(),
        app.profiles
    );
    for invalid in [
        json!({"start":1440,"end":0}),
        json!({"start":0,"end":-1}),
        json!({"start":0}),
    ] {
        let mut saved_profile = saved_profile.clone();
        saved_profile[0]["opening_time"] = invalid;
        assert!(crate::reading_profiles::decode(saved_profile).is_err());
    }
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, overview);
    tap_contents_item(&mut app, "Always apply");
    assert_eq!(app.profiles[0].opening_time(), Some(time));
    tap_contents_item(&mut app, "Only within time window");
    tap_contents_item(&mut app, "Clear time window");
    assert!(app.profiles[0].opening_time().is_none());
}

#[test]
fn opening_time_filters_automatic_opening_and_snapshots_the_negotiated_match() {
    let fixture = Fixture::new();
    let mut app = app(&fixture);
    app.open_path(&fixture.book).unwrap();
    let baseline = app.current_text_preferences().unwrap();
    let adjusted = BookPreferences {
        logical_font_size: Some(28.0),
        ..baseline.clone()
    };
    app.apply_text_profile(adjusted.clone()).unwrap();
    app.save_reading_profile("At night").unwrap();
    app.apply_text_profile(baseline.clone()).unwrap();
    let mut profiles = app.profiles.clone();
    profiles[0].set_apply_on_open(true);
    app.commit_profiles(profiles).unwrap();
    let now = current_minute().unwrap();
    let distant = (now + 720) % 1440;
    schedule(&mut app, distant, (distant + 30) % 1440);
    let position = app.current_position().unwrap();
    app.open_path(&fixture.book).unwrap();
    assert_eq!(app.current_text_preferences().unwrap(), baseline);
    app.apply_reading_profile(0).unwrap();
    assert_eq!(app.current_text_preferences().unwrap(), adjusted);
    assert_eq!(app.current_position().unwrap().anchor, position.anchor);

    app.apply_text_profile(baseline).unwrap();
    schedule(&mut app, 1320, 1320);
    app.open_path(&fixture.book).unwrap();
    assert_eq!(app.current_text_preferences().unwrap(), adjusted);

    let mut profiles = app.profiles.clone();
    profiles[0].set_opening_conditions(Conditions {
        orientations: vec![ReadingOrientation::Upright],
        ..Default::default()
    });
    app.commit_profiles(profiles).unwrap();
    schedule(&mut app, 1320, 390);
    app.negotiated_opening_profiles = true;
    let id = app.open_book_id().unwrap();
    for time in [Some(1319), Some(391), None] {
        assert!(
            app.opening_profile_plan(id, &fixture.book, false, None, None, time)
                .is_none()
        );
    }
    for minute in [0, 390, 1320, 1439] {
        let plan = app
            .opening_profile_plan(id, &fixture.book, false, None, None, Some(minute))
            .unwrap();
        assert_eq!(plan.profiles(LogicalTurn::Upright).count(), 1);
        assert_eq!(plan.profiles(LogicalTurn::UpsideDown).count(), 0);
    }
}

#[test]
fn opening_time_includes_both_bounds_and_treats_equal_endpoints_as_all_day() {
    let window = |start, end| TimeWindow { start, end };
    for minute in 0..1440 {
        assert_eq!(
            window(360, 750).includes(minute),
            (360..=750).contains(&minute)
        );
        assert_eq!(
            window(1320, 390).includes(minute),
            !(391..1320).contains(&minute)
        );
        assert!(window(540, 540).includes(minute));
    }
}
