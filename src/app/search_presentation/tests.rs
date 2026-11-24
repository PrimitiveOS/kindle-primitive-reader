use super::*;
use crate::app::{
    FIRST_MATCH_PAGE, LAST_MATCH_PAGE, LIST_X, LIST_Y, ROW_HEIGHT, Screen,
    list_density::{APPLY, DEFAULT, INCREASE, LIST_HEIGHT},
    tests::{Fixture, capture_test_screen, wait_for_search},
};
use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{Input, Rect};

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn tap_setting(app: &mut Reader, title: &str) {
    let row = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap();
    assert_eq!(
        app.input(Input::Tap(
            (LIST_X + 45) as f32,
            (LIST_Y + row as i32 * ROW_HEIGHT + 35) as f32
        )),
        Change::Partial
    );
}

#[test]
fn search_presentation_defaults_paging_settings_and_result_actions_share_geometry() {
    let fixture = Fixture::new();
    let document = fixture.root.join("Coastal observations.epub");
    std::fs::write(
        &document,
        include_bytes!("../../../tests/fixtures/long-chapter.epub"),
    )
    .unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&document).unwrap();
    let origin = app.current_history_location();
    assert_eq!(app.search_context_words(), 5);
    assert_eq!(app.match_geometry().rows, 10);
    app.start_document_search("Observation").unwrap();
    wait_for_search(&mut app, usize::MAX);
    let (count, items) = app.list_window();
    assert!(count > 100);
    assert_eq!(items.len(), 10);
    let original = app.hits[0].clone();
    capture_test_screen(&app, &fonts, "READER_SEARCH_DEFAULT_PRESENTATION", false);

    app.open_reading_edge(true, WIDTH as f32 / 2.0);
    let group = app.reading_button("Search");
    assert_eq!(app.input(tap(group)), Change::Partial);
    let control = app.reading_button("Search settings");
    assert_eq!(app.input(tap(control)), Change::Partial);
    assert_eq!(app.screen, Screen::SearchSettings);
    tap_setting(&mut app, "Results per page");
    let hold = Input::LongTap((INCREASE.x + 45) as f32, (INCREASE.y + 45) as f32);
    for _ in 0..4 {
        assert_eq!(app.input(hold), Change::Partial);
    }
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(app.input(tap(APPLY)), Change::Partial);
    assert_eq!(app.match_geometry().rows, 24);
    assert_eq!(app.hits.len(), count);
    tap_setting(&mut app, "Words in context");
    assert_eq!(app.input(hold), Change::Partial);
    assert_eq!(app.input(tap(APPLY)), Change::Partial);
    assert_eq!(app.search_context_words(), 10);
    assert!(app.hits.is_empty());
    assert_eq!(app.current_history_location(), origin);

    app.start_document_search("Observation").unwrap();
    wait_for_search(&mut app, usize::MAX);
    assert_eq!(app.hits.len(), count);
    assert_eq!(app.hits[0].anchor, original.anchor);
    assert_eq!(app.hits[0].end, original.end);
    assert!(app.hits[0].snippet.len() > original.snippet.len());
    let geometry = app.match_geometry();
    assert_eq!(app.list_window().1.len(), 24);
    assert!(geometry.row(23).bottom() <= LIST_Y + LIST_HEIGHT);
    assert!(geometry.row(23).bottom() < HEIGHT as i32 - 104);
    capture_test_screen(&app, &fonts, "READER_SEARCH_DENSE_PRESENTATION", false);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert!(matches!(
        app.list_window().1[0].action,
        Action::SearchHit(24)
    ));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(tap(LAST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_page, (count - 1) / geometry.rows);
    assert_eq!(app.input(tap(FIRST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_page, 0);
    let last = geometry.row(23);
    assert_eq!(
        app.input(Input::LongTap(
            (last.x + 75) as f32,
            (last.y + last.h / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.focused_result, Some(23));
    assert_eq!(app.input(Input::Tap(12.0, 12.0)), Change::Partial);
    assert_eq!(app.input(tap(last)), Change::Partial);
    assert_eq!(app.search_result, Some(23));
    let landed = app.result_selection.as_ref().unwrap();
    assert_eq!(
        (&landed.start, &landed.end),
        (&app.hits[23].anchor, &app.hits[23].end)
    );
    app.save().unwrap();

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&document).unwrap();
    assert_eq!(
        (app.match_geometry().rows, app.search_context_words()),
        (24, 10)
    );
    app.show(Screen::SearchSettings);
    tap_setting(&mut app, "Results per page");
    assert_eq!(app.input(tap(DEFAULT)), Change::Partial);
    assert_eq!(app.input(tap(APPLY)), Change::Partial);
    assert_eq!(app.match_geometry().rows, 10);
}
