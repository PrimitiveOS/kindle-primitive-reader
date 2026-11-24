use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use primitive_ui::Key;
use std::fs;

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
    capture(
        &app,
        &mut landscape,
        "landscape-corner-hold-pause.png",
        false,
    );
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
