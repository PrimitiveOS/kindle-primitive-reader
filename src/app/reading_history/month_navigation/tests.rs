use super::*;
use crate::app::reading_history::tests::{seed, setup};
use crate::app::selection_viewport_tests::{canvas, capture, tap};
use crate::app::tests::Fixture;
use crate::session::ReadingViewport;
use primitive_app::App;
use primitive_ui::{Key, Layer};

fn month(year: i32, number: u8) -> Date {
    Date::new(year, number, 1).unwrap()
}

fn press_key(app: &mut Reader, desired: Key) {
    let layer = app.history_view.month_entry.as_ref().unwrap().editor.layer;
    let rect = app
        .month_editor_layout()
        .keyboard
        .keyboard_keys_with(layer, "Go to month")
        .into_iter()
        .find(|(key, _)| key.key == desired)
        .unwrap()
        .1;
    tap(app, rect);
}

fn enter_month(app: &mut Reader, text: &str) {
    tap(app, MONTH);
    let clear = app.month_editor_layout().clear;
    tap(app, clear);
    for ch in text.chars() {
        if ch == '-' {
            press_key(app, Key::Layer(Layer::Symbols));
            press_key(app, Key::Char('-'));
            press_key(app, Key::Layer(Layer::Lower));
        } else {
            press_key(app, Key::Char(ch));
        }
    }
    press_key(app, Key::Go);
}

#[test]
fn month_navigation_exact_input_edges_and_cross_month_return_preserve_book() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let id = app.open_book_id().unwrap();
    let source = app.current_history_location();
    let december = month(2024, 12);
    let january = month(2025, 1);
    let latest = Date::today().first();
    seed(&mut app, id, "Field Notes", december, 90);
    seed(
        &mut app,
        id,
        "Field Notes",
        january.add_days(6).unwrap(),
        90,
    );
    app.activate_reading_history(Command::Open(Some(id)))
        .unwrap();
    app.activate_reading_history(Command::Show(Page::Calendar))
        .unwrap();
    let before = app.reading_history.summary(Some(id), None);
    assert_eq!(app.history_view.month, latest);

    tap(&mut app, FIRST);
    assert_eq!(app.history_view.month, december);
    tap(&mut app, LATEST);
    assert_eq!(app.history_view.month, latest);
    enter_month(&mut app, "2025-01");
    assert_eq!(app.history_view.month, january);
    tap(&mut app, MONTH_PREVIOUS);
    assert_eq!(app.history_view.month, december);
    tap(&mut app, MONTH_NEXT);
    assert_eq!(app.history_view.month, january);
    let mut portrait = canvas(&app, ReadingViewport::PORTRAIT);
    capture(
        &app,
        &mut portrait,
        "reading-calendar-month-navigation.png",
        false,
    );

    let _ = app.input(Input::LongTap(600.0, 190.0));
    let entry = app.history_view.month_entry.as_ref().unwrap();
    assert_eq!(entry.editor.string(), "2025-01");
    capture(
        &app,
        &mut portrait,
        "reading-calendar-month-editor.png",
        false,
    );
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert!(app.history_view.month_entry.is_none());
    assert_eq!(app.history_view.month, january);

    enter_month(&mut app, "2025-00");
    assert!(app.history_view.month_entry.as_ref().unwrap().invalid);
    assert_eq!(app.history_view.month, january);
    let cancel = app.month_editor_layout().cancel;
    tap(&mut app, cancel);
    assert!(app.history_view.month_entry.is_none());
    enter_month(&mut app, "9999-12");
    assert_eq!(app.history_view.month, month(9999, 12));
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.history_view.month, month(9999, 12));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.history_view.month, month(9999, 11));
    enter_month(&mut app, "1000-01");
    assert_eq!(app.history_view.month, month(1000, 1));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.history_view.month, month(1000, 1));
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.history_view.month, month(1000, 2));

    enter_month(&mut app, "2025-01");
    let first_day_column =
        usize::from((january.weekday() + 7 - app.reading_history.options().week_start) % 7);
    tap(&mut app, cell(first_day_column));
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Day(january)));
    tap(&mut app, PREVIOUS);
    assert_eq!(
        app.screen,
        Screen::ReadingHistory(Page::Day(january.add_days(-1).unwrap()))
    );
    tap(&mut app, back_button(WIDTH as i32));
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Calendar));
    assert_eq!(app.history_view.month, december);

    app.activate_reading_history(Command::Show(Page::Daily))
        .unwrap();
    app.activate_reading_history(Command::Show(Page::Day(january)))
        .unwrap();
    tap(&mut app, back_button(WIDTH as i32));
    assert_eq!(app.screen, Screen::ReadingHistory(Page::Daily));
    assert_eq!(app.history_view.month, december);
    assert_eq!(app.reading_history.summary(Some(id), None), before);
    app.history_back();
    app.history_back();
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_history_location(), source);
}

#[test]
fn month_navigation_requires_ascii_four_digit_year_and_two_digit_month() {
    for input in [
        "999-01",
        "0999-01",
        "2025-1",
        "2025-13",
        "２０２５-０１",
        "2025-+1",
        "2025-01 ",
        "2025/01",
    ] {
        assert!(target_month(input).is_none(), "accepted {input:?}");
    }
    assert_eq!(target_month("1000-01"), Some(month(1000, 1)));
    assert_eq!(target_month("9999-12"), Some(month(9999, 12)));
}
