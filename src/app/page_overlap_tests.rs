use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn tap(app: &mut Reader, rect: Rect) {
    assert!(rect.w >= 100 && rect.h >= 100, "{rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn tap_button(app: &mut Reader, label: &str) {
    tap(app, app.reading_button(label));
}

fn tap_row_button(app: &mut Reader, row: &str, label: &str) {
    let rect = app.reading_button_in_row(row, label);
    tap(app, rect);
}

#[test]
fn scroll_page_overlap_controls_are_visible_and_persist_per_book() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();

    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::Text) {
        tap_button(&mut app, "Type");
    }
    tap_button(&mut app, "Scroll");
    assert!(app.active.as_ref().unwrap().reading.config.scroll);

    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::TextLayout) {
        tap_button(&mut app, "Layout");
    }
    tap_row_button(&mut app, "Page overlap", "On");
    tap_button(&mut app, "7");
    tap_row_button(&mut app, "Overlap marker", "Dashed line");

    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert!(settings.page_overlap_enabled);
    assert_eq!(settings.page_overlap_lines, 7);
    assert_eq!(
        settings.page_overlap_style,
        crate::PageOverlapStyle::DashedLine
    );
    capture_test_screen(
        &app,
        &fonts,
        "READER_PAGE_OVERLAP_SETTINGS_SCREENSHOT",
        false,
    );
    app.save().unwrap();

    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    let settings = reopened.active.as_ref().unwrap().reading.config.settings;
    assert!(settings.page_overlap_enabled);
    assert_eq!(settings.page_overlap_lines, 7);
    assert_eq!(
        settings.page_overlap_style,
        crate::PageOverlapStyle::DashedLine
    );
}
