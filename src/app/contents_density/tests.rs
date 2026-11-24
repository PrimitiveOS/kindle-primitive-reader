use super::super::tests::{Fixture, capture_test_screen, tap_contents_item, tap_reading_control};
use super::super::{Screen, TopGroup};
use super::*;
use crate::reading_controls::ReadingControls;
use primitive_app::App;
use std::fs;

fn center(rect: Rect) -> (f32, f32) {
    ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = center(rect);
    app.input(Input::Tap(x, y))
}

fn hold(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = center(rect);
    app.input(Input::LongTap(x, y))
}

fn open(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    app
}

fn settings(app: &mut Reader) {
    if app.screen.is_contents() {
        assert_eq!(
            tap(app, primitive_ui::back_button(WIDTH as i32)),
            Change::Partial
        );
    } else {
        assert_eq!(app.input(Input::Tap(630.0, 25.0)), Change::Partial);
    }
    if app.top_group != Some(TopGroup::Navigation) {
        assert_eq!(tap_reading_control(app, "Navigation"), Change::Partial);
    }
    assert_eq!(
        tap_reading_control(app, "Contents settings"),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::ContentsSettings);
}

fn contents_from_settings(app: &mut Reader) {
    assert_eq!(
        tap(app, primitive_ui::back_button(WIDTH as i32)),
        Change::Partial
    );
    assert_eq!(app.top_group, Some(TopGroup::Navigation));
    assert_eq!(tap_reading_control(app, "Contents"), Change::Partial);
    assert_eq!(app.screen, Screen::Contents);
}

#[test]
fn contents_density_uses_stock_defaults_size_scale_and_font_cap() {
    let defaults = ContentsDensity::default();
    assert_eq!(defaults.rows(), 14);
    assert_eq!(defaults.font(), 19);
    let expected = [
        24, 23, 22, 22, 21, 21, 20, 20, 19, 19, 18, 17, 17, 16, 16, 15, 15, 14, 14,
    ];
    for (rows, expected) in (6..=24).zip(expected) {
        let density = ContentsDensity {
            rows_per_page: Some(rows),
            ..defaults
        };
        assert_eq!(density.font(), expected);
    }
    assert_eq!(reference_scale().scaled_pixels(10.), 22);
    assert_eq!(reference_scale().scaled_pixels(19.), 41);
    assert_eq!(list_text_pixels(19, 91), 41);
    assert_eq!(list_text_pixels(72, 212), 152);
    assert_eq!(list_text_pixels(72, 53), 36);
    assert_eq!(list_text_pixels(14, 53), 30);
    assert_eq!(
        ContentsDensity {
            rows_per_page: Some(250),
            font_size: Some(2)
        }
        .rows(),
        14
    );
    assert_eq!(
        ContentsDensity {
            rows_per_page: Some(24),
            font_size: Some(73)
        }
        .font(),
        14
    );

    let legacy: ReadingControls = serde_json::from_str(r#"{"bookmark_reverse":true}"#).unwrap();
    assert!(legacy.bookmark_reverse);
    assert_eq!(legacy.contents_density, defaults);
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("contents_density")
            .is_none()
    );
}

#[test]
fn contents_density_settings_resize_the_real_list_and_survive_restart() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
    let mut app = open(&fixture);
    let source_anchor = app.current_position().unwrap();
    let history_count = app.back.len();

    settings(&mut app);
    tap_contents_item(&mut app, "Rows per page");
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::None);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.contents_density.rows(), 6);
    assert_eq!(app.controls.contents_density.font(), 24);
    contents_from_settings(&mut app);
    assert_eq!(app.list_window().1.len(), 6);
    let first_page = app.list_window().1[0].title.clone();
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_ne!(app.list_window().1[0].title, first_page);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_window().1[0].title, first_page);
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_DENSITY_SPARSE", false);

    settings(&mut app);
    tap_contents_item(&mut app, "Rows per page");
    for _ in 0..5 {
        assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    }
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.contents_density.rows(), 24);
    assert_eq!(app.controls.contents_density.font(), 14);

    tap_contents_item(&mut app, "Entry font size");
    assert_eq!(hold(&mut app, DECREASE), Change::Partial);
    assert_eq!(tap(&mut app, DECREASE), Change::None);
    assert_eq!(tap(&mut app, CANCEL), Change::Partial);
    assert_eq!(app.controls.contents_density.font(), 14);
    tap_contents_item(&mut app, "Entry font size");
    for _ in 0..15 {
        assert_eq!(hold(&mut app, INCREASE), Change::Partial);
    }
    assert_eq!(tap(&mut app, INCREASE), Change::None);
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_DENSITY_PICKER", true);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.contents_density.font(), 72);
    contents_from_settings(&mut app);
    assert_eq!(app.list_window().1.len(), 24);
    capture_test_screen(&app, &app.fonts, "READER_CONTENTS_DENSITY_DENSE", false);
    assert_eq!(app.current_position().unwrap(), source_anchor);
    assert_eq!(app.back.len(), history_count);

    let mut reopened = open(&fixture);
    assert_eq!(reopened.controls.contents_density.rows(), 24);
    assert_eq!(reopened.controls.contents_density.font(), 72);
    reopened.show(Screen::Contents);
    let target = reopened.list_window().1[23].title.clone();
    let before_jumps = reopened.back.len();
    tap_contents_item(&mut reopened, &target);
    assert_eq!(reopened.screen, Screen::Book);
    assert_eq!(reopened.back.len(), before_jumps + 1);

    settings(&mut reopened);
    tap_contents_item(&mut reopened, "Entry font size");
    assert_eq!(tap(&mut reopened, DEFAULT), Change::Partial);
    assert_eq!(tap(&mut reopened, APPLY), Change::Partial);
    tap_contents_item(&mut reopened, "Rows per page");
    assert_eq!(tap(&mut reopened, DEFAULT), Change::Partial);
    assert_eq!(tap(&mut reopened, APPLY), Change::Partial);
    assert_eq!(reopened.controls.contents_density, Default::default());
    assert_eq!(reopened.controls.contents_density.font(), 19);
}

#[test]
fn contents_density_failed_save_keeps_the_active_preferences_and_draft() {
    let fixture = Fixture::new();
    let mut app = open(&fixture);
    fs::create_dir(&app.controls_path).unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Rows per page");
    assert_eq!(tap(&mut app, INCREASE), Change::Partial);
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert_eq!(app.controls.contents_density.rows(), 14);
    assert_eq!(
        app.contents_density_picker.as_ref().unwrap().value,
        Some(15)
    );
    assert!(app.notice.is_some());
    fs::remove_dir(&app.controls_path).unwrap();
    assert_eq!(tap(&mut app, APPLY), Change::Partial);
    assert!(app.contents_density_picker.is_none());
    assert_eq!(app.controls.contents_density.rows(), 15);
}
