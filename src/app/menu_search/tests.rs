use super::*;
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item};
use primitive_app::App;
use primitive_ui::Key;

fn open(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap()
}

fn at(rect: Rect) -> (f32, f32) {
    (
        rect.x as f32 + rect.w as f32 / 2.0,
        rect.y as f32 + rect.h as f32 / 2.0,
    )
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    let (x, y) = at(rect);
    app.input(Input::Tap(x, y))
}

fn start(app: &mut Reader) {
    app.open_reading_edge(true, 600.0);
    let search = app.reading_button("Search");
    if app.top_group != Some(TopGroup::Search) {
        assert_eq!(tap(app, search), Change::Partial);
    }
    let search = app.reading_button("Menu search");
    assert_eq!(tap(app, search), Change::Partial);
    assert_eq!(app.screen, Screen::MenuSearch);
}

fn enter(app: &mut Reader, term: &str) {
    let layout = app.navigation_layout().keyboard();
    app.editor.text = term.chars().collect();
    app.editor.caret = app.editor.text.len();
    let go = layout
        .keyboard_keys_with(app.editor.layer, "Search")
        .into_iter()
        .find(|(key, _)| key.key == Key::Go)
        .unwrap()
        .1;
    assert_eq!(tap(app, go), Change::Partial);
    assert_eq!(app.screen, Screen::MenuSearchResults);
}

#[test]
fn menu_search_previews_and_marks_the_real_settings_row_without_toggling_it() {
    let fixture = Fixture::new();
    let mut app = open(&fixture);
    let font = app.fonts.clone();
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let position = app.current_history_location();
    let history = app.back.clone();
    start(&mut app);
    enter(&mut app, "wItH sWiPeS");
    let hit = app
        .menu_search
        .hits
        .iter()
        .find(|hit| hit.title == "With swipes")
        .unwrap();
    assert_eq!(hit.breadcrumb, "Settings › Page turns › With swipes");
    capture_test_screen(&app, &font, "READER_MENU_SEARCH_RESULTS_CAPTURE", false);
    tap_contents_item(&mut app, "With swipes");
    assert!(app.menu_search.preview.is_some());
    capture_test_screen(&app, &font, "READER_MENU_SEARCH_PREVIEW_CAPTURE", false);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::MenuSearchResults);
    assert!(app.menu_search.preview.is_none());
    tap_contents_item(&mut app, "With swipes");
    let confirm = app
        .menu_search_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(tap(&mut app, confirm), Change::Partial);
    assert_eq!(app.screen, Screen::PageTurns);
    assert!(!app.controls.disable_page_swipes);
    assert_eq!(app.current_history_location(), position);
    assert_eq!(app.back, history);
    let ordinal = app.menu_search.focused_list(Screen::PageTurns).unwrap();
    assert_eq!(app.items()[ordinal].title, "With swipes");
    capture_test_screen(&app, &font, "READER_MENU_SEARCH_TARGET_CAPTURE", false);
    tap_contents_item(&mut app, "With swipes");
    assert!(app.controls.disable_page_swipes);

    drop(app);
    let mut reopened = open(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    start(&mut reopened);
    assert_eq!(reopened.editor.string(), "wItH sWiPeS");
}

#[test]
fn menu_search_matches_only_available_fixed_controls_and_supports_direct_jump() {
    let fixture = Fixture::new();
    let file = fixture.root.join("Chart.pdf");
    std::fs::write(&file, crate::fixed::tests::document()).unwrap();
    let mut app = open(&fixture);
    app.open_path(&file).unwrap();
    let position = app.current_history_location();
    start(&mut app);
    enter(&mut app, "line spacing");
    assert!(app.menu_search.hits.is_empty());
    let back = app.navigation_layout().back();
    assert_eq!(tap(&mut app, back), Change::Partial);
    assert_eq!(app.screen, Screen::MenuSearch);
    enter(&mut app, "fit width");
    let index = app
        .menu_search
        .hits
        .iter()
        .position(|hit| hit.title == "Fit width")
        .unwrap();
    let geometry = app.list_geometry();
    let (x, y) = at(geometry.row(index));
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::FixedTools);
    assert_eq!(app.top_group, Some(TopGroup::Appearance));
    assert!(app.menu_search.focused_top(TopGroup::Appearance).is_some());
    assert_eq!(app.current_history_location(), position);
}

#[test]
fn menu_search_keeps_literal_punctuation_and_restores_the_origin_on_cancel() {
    let fixture = Fixture::new();
    let mut app = open(&fixture);
    app.open_path(&fixture.book).unwrap();
    start(&mut app);
    enter(&mut app, ".*");
    assert!(app.menu_search.hits.is_empty());
    let back = app.navigation_layout().back();
    assert_eq!(tap(&mut app, back), Change::Partial);
    assert_eq!(tap(&mut app, back), Change::Partial);
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.top_group, Some(TopGroup::Search));

    start(&mut app);
    enter(&mut app, "line spacing");
    let index = app
        .menu_search
        .hits
        .iter()
        .position(|hit| !matches!(hit.destination.menu, Menu::Top(_)))
        .unwrap();
    let geometry = app.list_geometry();
    let (x, y) = at(geometry.row(index));
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert!(app.reading_bottom);
    assert!(app.reading_panel.is_some());
    assert!(
        app.menu_search
            .focused_bottom(app.reading_panel.unwrap())
            .is_some()
    );
}
