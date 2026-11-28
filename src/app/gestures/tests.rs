use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use primitive_app::WIDTH;
use primitive_ui::Key;
#[cfg(feature = "multi-touch")]
use primitive_ui::MultiTouch;
use std::{fs, path::PathBuf};

fn canvas_reader(fixture: &Fixture) -> Reader {
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut app = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app
}

fn release(x: f32, y: f32) -> Input {
    Input::HoldRelease {
        start_x: x,
        start_y: y,
        x,
        y,
        held_ms: 900,
    }
}

fn long_plain_text(fixture: &Fixture) -> PathBuf {
    let path = fixture.root.join("Long reading session.txt");
    fs::write(
        &path,
        "A measured sentence for reading gesture coverage.\n".repeat(5_000),
    )
    .unwrap();
    path
}

#[test]
fn the_skim_binding_is_labelled_for_the_skim_screen_it_opens() {
    let label = Binding::ALL
        .iter()
        .find(|entry| entry.0 == Binding::Skim)
        .map(|entry| entry.1);
    assert_eq!(label, Some("Skim"));
}

#[test]
fn double_tap_page_jump_delays_single_taps_and_replays_them_at_timeout() {
    let fixture = Fixture::new();
    let mut app = canvas_reader(&fixture);
    app.open_path(&long_plain_text(&fixture)).unwrap();
    app.controls.double_tap_page_jump = true;
    let before = app.active.as_ref().unwrap().reading.page;
    let (width, height) = ReadingViewport::PORTRAIT.dimensions();
    let point = (width as f32 - 24.0, height as f32 / 2.0);

    assert_eq!(app.input(Input::Tap(point.0, point.1)), Change::None);
    let pending = app.gestures.pending_double_tap.unwrap();
    assert_eq!(app.active.as_ref().unwrap().reading.page, before);
    // The replayed tap queues a page turn behind the refresh gap; apply it at its deadline.
    assert!(app.finish_double_tap(pending.deadline).is_some());
    if let Some(due) = app.deadline() {
        app.tick(due + std::time::Duration::from_secs(1));
    }
    assert!(app.active.as_ref().unwrap().reading.page > before);
}

#[test]
fn double_tap_right_jumps_forward_ten_reflow_pages() {
    let fixture = Fixture::new();
    let mut app = canvas_reader(&fixture);
    app.open_path(&long_plain_text(&fixture)).unwrap();
    app.controls.double_tap_page_jump = true;
    let reading = &app.active.as_ref().unwrap().reading;
    assert!(reading.probe.page_count() > 10);
    let (width, height) = ReadingViewport::PORTRAIT.dimensions();
    let point = (width as f32 * 0.75, height as f32 / 2.0);

    assert_eq!(app.input(Input::Tap(point.0, point.1)), Change::None);
    let change = app.input(Input::Tap(point.0, point.1));
    assert!(matches!(change, Change::Partial | Change::Full));
    assert_eq!(app.active.as_ref().unwrap().reading.page, 10);
}

#[test]
fn corner_gestures_follow_screen_eighths_and_leave_exit_available() {
    for viewport in [ReadingViewport::PORTRAIT, ReadingViewport::LANDSCAPE] {
        let (width, height) = viewport.dimensions();
        let (width, height) = (width as f32, height as f32);
        assert_eq!(
            Gesture::touch(
                Input::LongTap(width / 8.0 - 1.0, height / 8.0 - 1.0),
                viewport
            ),
            Some(Gesture::HoldTopLeft)
        );
        assert_eq!(
            Gesture::touch(Input::LongTap(width / 8.0, height / 8.0 - 1.0), viewport),
            None
        );
        assert_eq!(
            Gesture::touch(Input::Tap(width * 0.875, height * 0.875), viewport),
            Some(Gesture::TapBottomRight)
        );
        assert_eq!(
            Gesture::touch(
                Input::LongTap(width * 0.875, height * 0.875 - 1.0),
                viewport
            ),
            None
        );
        assert_eq!(
            Gesture::touch(
                Input::LongTap(width - 1.0, viewport.page_height() as f32),
                viewport
            ),
            None
        );
    }
}

#[test]
fn multitouch_gestures_classify_full_screen_corners_and_koreader_directions() {
    let viewport = ReadingViewport::PORTRAIT;
    assert_eq!(
        Gesture::multi_touch(
            MultiTouch::TwoFingerTap {
                x: 20.0,
                y: 20.0,
                held_ms: 180,
                fallback_x: 20.0,
                fallback_y: 20.0,
            },
            viewport,
        ),
        Some(Gesture::TwoFingerTapTopLeft)
    );
    assert_eq!(
        Gesture::multi_touch(
            MultiTouch::TwoFingerSwipe {
                dx: 180.0,
                dy: 4.0,
                start_x: 300.0,
                start_y: 400.0,
            },
            viewport,
        ),
        Some(Gesture::TwoFingerSwipeEast)
    );
    assert_eq!(
        Gesture::multi_touch(
            MultiTouch::TwoFingerSwipe {
                dx: 90.0,
                dy: -100.0,
                start_x: 300.0,
                start_y: 400.0,
            },
            viewport,
        ),
        Some(Gesture::TwoFingerSwipeNorthEast)
    );
    assert_eq!(
        Gesture::multi_touch(
            MultiTouch::TwoFingerSwipe {
                dx: 180.0,
                dy: 0.0,
                start_x: 300.0,
                start_y: 20.0,
            },
            viewport,
        ),
        Some(Gesture::TwoFingerSwipeEast),
        "KOReader registers two-finger swipes across the full screen"
    );
}

#[test]
fn multitouch_reader_defaults_match_stock_actions_and_custom_bindings_persist() {
    let fixture = Fixture::new();
    let mut app = canvas_reader(&fixture);
    app.open_path(&long_plain_text(&fixture)).unwrap();
    assert_eq!(
        app.multi_touch_input(MultiTouch::TwoFingerSwipe {
            dx: 180.0,
            dy: 0.0,
            start_x: 300.0,
            start_y: 400.0,
        }),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Contents);

    app.show(Screen::Book);
    app.gestures
        .set(Gesture::TwoFingerSwipeEast, Binding::Bookmark)
        .unwrap();
    app.multi_touch_input(MultiTouch::TwoFingerSwipe {
        dx: 180.0,
        dy: 0.0,
        start_x: 300.0,
        start_y: 400.0,
    });
    assert!(app.page_is_bookmarked());

    app.touch_locked = true;
    app.show(Screen::Book);
    assert_eq!(
        app.multi_touch_input(MultiTouch::TwoFingerSwipe {
            dx: 0.0,
            dy: -180.0,
            start_x: 300.0,
            start_y: 400.0,
        }),
        Change::None
    );
    assert_eq!(app.screen, Screen::Book);
}

#[test]
fn multitouch_tap_uses_saved_stock_duration_boundaries() {
    let fixture = Fixture::new();
    let mut app = canvas_reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.gestures
        .set(Gesture::TwoFingerTapTopRight, Binding::Bookmark)
        .unwrap();
    app.multi_touch_input(MultiTouch::TwoFingerTap {
        x: WIDTH as f32 - 20.0,
        y: 20.0,
        held_ms: 300,
        fallback_x: WIDTH as f32 / 2.0,
        fallback_y: crate::app::HEIGHT as f32 / 2.0,
    });
    assert!(!app.page_is_bookmarked());

    app.multi_touch_input(MultiTouch::TwoFingerTap {
        x: WIDTH as f32 - 20.0,
        y: 20.0,
        held_ms: 301,
        fallback_x: WIDTH as f32 / 2.0,
        fallback_y: crate::app::HEIGHT as f32 / 2.0,
    });
    assert!(!app.page_is_bookmarked());

    app.controls.two_finger_tap_duration_ms = Some(400);
    app.multi_touch_input(MultiTouch::TwoFingerTap {
        x: WIDTH as f32 - 20.0,
        y: 20.0,
        held_ms: 399,
        fallback_x: WIDTH as f32 / 2.0,
        fallback_y: crate::app::HEIGHT as f32 / 2.0,
    });
    assert!(app.page_is_bookmarked());
}

#[test]
fn corner_gestures_claim_the_hold_once_and_yield_to_text_when_paused() {
    let fixture = Fixture::new();
    let book = fixture.root.join("Corner text.html");
    fs::write(
        &book,
        "<html><head><title>Harbor chart</title><style>p { text-indent: 0; margin: 0; }</style></head><body><p>Corner words on the harbor chart.</p></body></html>",
    )
    .unwrap();
    let mut app = canvas_reader(&fixture);
    app.open_path(&book).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.matches("Corner").remove(0);
    let mut end = start.clone();
    end.offset += "Corner".len();
    let selection = reading.probe.select_range(&start, &end).unwrap();
    let rect = reading.highlight_rects(&selection)[0];
    let x = (rect.x + rect.width / 2) as f32;
    let y = (rect.y + rect.height / 2) as f32;
    assert_eq!(
        Gesture::touch(Input::LongTap(x, y), ReadingViewport::PORTRAIT),
        Some(Gesture::HoldTopLeft),
        "held word center was ({x}, {y})"
    );
    let position = app.current_history_location();
    app.gestures
        .set(Gesture::HoldTopLeft, Binding::Contents)
        .unwrap();
    app.gestures
        .set(Gesture::TapTopLeft, Binding::GoTo)
        .unwrap();
    assert_eq!(app.input(Input::HoldStart(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::Contents);
    assert!(app.selection.is_none());
    assert_eq!(app.input(Input::HoldPan(x + 20.0, y)), Change::None);
    assert_eq!(app.input(release(x, y)), Change::None);
    assert_eq!(app.screen, Screen::Contents);
    assert_eq!(app.current_history_location(), position);

    app.show(Screen::Gestures);
    tap_contents_item(&mut app, "Ignore long-press on corners");
    assert!(app.controls.ignore_corner_holds);
    let mut portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    capture(
        &app,
        &mut portrait,
        "corner-hold-gesture-settings.png",
        false,
    );
    drop(app);

    let mut app = canvas_reader(&fixture);
    app.open_path(&book).unwrap();
    assert!(app.controls.ignore_corner_holds);
    app.controls.dictionary_on_single_word = Some(false);
    assert_eq!(app.input(Input::HoldStart(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.selection.as_ref().unwrap().text, "Corner");
    assert_eq!(app.input(release(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.selection.as_ref().unwrap().text, "Corner");
    let mut portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 2);
    capture(
        &app,
        &mut portrait,
        "corner-hold-gesture-text-selection.png",
        false,
    );
    app.selection = None;
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::GoToPage);
}

#[test]
fn corner_gestures_pause_on_pdf_without_rearming_or_claiming_an_old_release() {
    let fixture = Fixture::new();
    let book = fixture.root.join("Corner chart.pdf");
    fs::write(&book, crate::fixed::tests::document()).unwrap();
    let mut app = canvas_reader(&fixture);
    app.open_path(&book).unwrap();
    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);

    app.show(Screen::Gestures);
    tap_contents_item(&mut app, "Corner hold pause");
    assert_eq!(app.editor.string(), "10");
    capture(
        &app,
        &mut landscape,
        "landscape-corner-hold-pause.png",
        false,
    );
    let apply = key(&app, Key::Go, "Set");
    app.editor.text = "31".chars().collect();
    app.editor.caret = 2;
    tap(&mut app, apply);
    assert_eq!(
        app.screen,
        Screen::NumberingValue(super::super::NumberField::CornerHoldPause)
    );
    assert_eq!(corner_pause_seconds(&app.controls), 10);
    app.editor.text = "5".chars().collect();
    app.editor.caret = 1;
    tap(&mut app, apply);
    assert_eq!(app.screen, Screen::Gestures);
    assert_eq!(corner_pause_seconds(&app.controls), 5);
    app.show(Screen::Fixed);
    app.gestures
        .set(Gesture::TapTopRight, Binding::PauseCornerHolds)
        .unwrap();
    app.gestures
        .set(Gesture::HoldTopLeft, Binding::Contents)
        .unwrap();
    let before = app.current_history_location();
    let before_activation = Instant::now();
    let pause = Input::Tap(
        ReadingViewport::LANDSCAPE.dimensions().0 as f32 - 20.0,
        20.0,
    );
    assert_eq!(app.input(pause), Change::Partial);
    let until = app.gestures.corner_pause_until.unwrap();
    assert!(until >= before_activation + Duration::from_secs(5));
    assert!(until <= Instant::now() + Duration::from_secs(5));
    assert_eq!(app.input(pause), Change::None);
    assert_eq!(app.gestures.corner_pause_until, Some(until));
    assert!(
        app.gestures
            .corner_holds_paused(&app.controls, until - Duration::from_millis(1))
    );
    assert!(!app.gestures.corner_holds_paused(&app.controls, until));
    assert_eq!(app.current_history_location(), before);
    app.input(Input::HoldStart(20.0, 20.0));
    app.gestures.corner_pause_until = Some(Instant::now() - Duration::from_millis(1));
    app.input(release(20.0, 20.0));
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.current_history_location(), before);
    assert_eq!(app.input(Input::HoldStart(20.0, 20.0)), Change::Partial);
    assert_eq!(app.screen, Screen::FixedContents);
    assert_eq!(app.input(release(20.0, 20.0)), Change::None);

    let reopened = canvas_reader(&fixture);
    assert_eq!(corner_pause_seconds(&reopened.controls), 5);
    assert!(reopened.gestures.corner_pause_until.is_none());
}

#[test]
fn assigned_reading_size_and_mode_gestures_preserve_location_and_reopen() {
    let fixture = Fixture::new();
    let mut app = canvas_reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(crate::Settings {
        font_size: 42.0,
        logical_font_size: None,
        ..settings
    }))
    .unwrap();
    let initial = app.active.as_ref().unwrap().reading.anchor().unwrap();
    let scale = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .font_scale;
    app.gestures
        .set(Gesture::TapTopLeft, Binding::IncreaseFont)
        .unwrap();
    app.gestures
        .set(Gesture::TapTopRight, Binding::DecreaseFont)
        .unwrap();
    app.gestures
        .set(Gesture::TapBottomLeft, Binding::TogglePageScroll)
        .unwrap();
    app.input(Input::Tap(5.0, 5.0));
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        (42.0 + scale.physical_delta(0.5)).clamp(20.0, 100.0)
    );
    assert_eq!(
        app.active.as_ref().unwrap().reading.anchor(),
        Some(initial.clone())
    );
    app.input(Input::Tap(
        app.reading_viewport().dimensions().0 as f32 - 5.0,
        5.0,
    ));
    assert!(
        (app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size
            - 42.0)
            .abs()
            < 0.001
    );
    let y = app.reading_viewport().page_height() as f32 - 10.0;
    app.input(Input::Tap(5.0, y));
    assert!(app.active.as_ref().unwrap().reading.config.scroll);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), Some(initial));
    app.save().unwrap();
    drop(app);
    let mut reopened = canvas_reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert!(reopened.active.as_ref().unwrap().reading.config.scroll);
    assert_eq!(
        reopened.gestures.get(Gesture::TapTopLeft),
        Binding::IncreaseFont
    );
    assert_eq!(
        reopened.gestures.get(Gesture::TapTopRight),
        Binding::DecreaseFont
    );
    assert_eq!(
        reopened.gestures.get(Gesture::TapBottomLeft),
        Binding::TogglePageScroll
    );
    reopened.show(Screen::GestureBinding(Gesture::TapTopLeft));
    let index = reopened
        .gestures
        .choices(Gesture::TapTopLeft, &reopened.book_shortcuts)
        .iter()
        .position(|item| item.title == "Increase font size")
        .unwrap();
    reopened.list_page = index / reopened.list_geometry().rows;
    crate::app::tests::capture_test_screen(
        &reopened,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        "READER_GESTURE_READING_ACTIONS_FRAME",
        false,
    );
}

#[test]
fn assigned_reflow_actions_do_not_turn_or_mutate_fixed_page_documents() {
    let fixture = Fixture::new();
    let source = fixture.root.join("Gesture.pdf");
    let original = crate::fixed::tests::document();
    fs::write(&source, &original).unwrap();
    let mut app = canvas_reader(&fixture);
    app.open_path(&source).unwrap();
    let before = app.current_position();
    app.gestures
        .set(Gesture::TapTopLeft, Binding::IncreaseFont)
        .unwrap();
    app.gestures
        .set(Gesture::TapTopRight, Binding::TogglePageScroll)
        .unwrap();
    app.input(Input::Tap(5.0, 5.0));
    app.input(Input::Tap(
        app.reading_viewport().dimensions().0 as f32 - 5.0,
        5.0,
    ));
    assert_eq!(app.current_position(), before);
    assert!(app.fixed.is_some());
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn a_style_tweak_binding_round_trips_and_refuses_an_unknown_preset() {
    let preset = crate::user_styles::preset("paragraph_whitespace").unwrap();
    let binding = Binding::StyleTweak(TweakId::new(&preset));
    let saved = serde_json::to_string(&binding).unwrap();
    assert_eq!(serde_json::from_str::<Binding>(&saved).unwrap(), binding);
    assert!(matches!(binding, Binding::StyleTweak(id) if id.id() == "paragraph_whitespace"));
    assert!(serde_json::from_str::<Binding>("{\"style_tweak\":\"no_such_tweak\"}").is_err());
}
