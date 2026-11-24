use super::*;
use crate::app::{
    Action, FIRST_MATCH_PAGE, FixedJump, ReadingControls,
    selection_popup::SelectionAction,
    tests::{Fixture, capture_test_screen, wait_for_search},
};
use crate::session::PAGE_HEIGHT;
use primitive_app::App;

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

#[test]
fn search_results_navigation_first_equal_and_previous_or_first_between_pages() {
    use Ordering::{Equal, Greater, Less};
    let locate = |orders: &[Ordering]| at_page(orders.iter().copied().enumerate());
    assert_eq!(locate(&[Less, Equal, Equal, Greater]), Some(1));
    assert_eq!(locate(&[Less, Less, Greater]), Some(1));
    assert_eq!(locate(&[Greater, Greater]), Some(0));
    assert_eq!(locate(&[Less, Less]), Some(1));
    assert_eq!(locate(&[]), None);
}

#[test]
fn search_results_navigation_focuses_current_epub_page_and_returns_to_query_origin() {
    let fixture = Fixture::new();
    std::fs::write(
        &fixture.book,
        include_bytes!("../../../tests/fixtures/long-chapter.epub"),
    )
    .unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(0, 1)
        .unwrap();
    let origin = app.current_history_location().unwrap();
    app.start_document_search("Observation").unwrap();
    wait_for_search(&mut app, usize::MAX);
    assert!(app.hits.len() > 150);
    assert_eq!(
        app.results_origin().map(|saved| &saved.location),
        Some(&origin)
    );
    app.activate(Action::SearchHit(137)).unwrap();
    let on_match = app.current_history_location().unwrap();
    assert_ne!(on_match, origin);
    let reading = &app.active.as_ref().unwrap().reading;
    let expected = app
        .hits
        .iter()
        .position(|hit| reading.probe.page_for(&hit.anchor) == Some(reading.page))
        .unwrap();
    assert!(expected > app.match_geometry().rows);
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.input(tap(FIRST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_SEARCH_RESULT_OPTIONS", false);
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(app.input(tap(CURRENT)), Change::Partial);
    assert_eq!(app.list_page, expected / app.match_geometry().rows);
    assert_eq!(app.focused_result, Some(expected));
    assert_eq!(app.current_history_location(), Some(on_match.clone()));
    capture_test_screen(&app, &fonts, "READER_SEARCH_RESULT_CURRENT", false);
    let list_page = app.list_page;
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_page, list_page);
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    let history = app.back.clone();
    let forward = app.forward.clone();
    assert_eq!(app.input(tap(ORIGINAL)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_history_location(), Some(origin));
    assert!(app.search_result.is_none());
    assert!(app.result_selection.is_none());
    assert_eq!(app.back, history);
    assert_eq!(app.forward, forward);
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.list_page, list_page);
    assert!(!app.search_results_navigation.menu_open);

    app.activate(Action::SearchHit(137)).unwrap();
    assert_eq!(app.current_history_location(), Some(on_match));
    app.controls.search_max_lines = Some(4);
    app.clear_flexible_results();
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.input(tap(FIRST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    assert_eq!(app.input(tap(CURRENT)), Change::Partial);
    assert_eq!(app.focused_result, Some(expected));
    assert!(app.flexible_result_range().unwrap().contains(&expected));
}

#[test]
fn search_results_navigation_pdf_selection_restores_its_rotated_view_and_replaces_origin() {
    use std::time::Instant;

    let fixture = Fixture::new();
    let pdf = fixture.root.join("Pages.pdf");
    std::fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let font_paths = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    app.open_path(&pdf).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        selection_search_all: true,
        dictionary_on_single_word: Some(false),
        ..app.controls
    }))
    .unwrap();
    app.activate(Action::SetFixedRotation(1)).unwrap();
    app.activate(Action::SetFixedZoom(3)).unwrap();
    let reading = &mut app.fixed.as_mut().unwrap().reading;
    let word = reading.page_text(0).unwrap().words[0].bounds;
    reading
        .jump_to(0, FixedJump::regions(&[word]).as_ref())
        .unwrap();
    let point = reading.visible_rectangle(0, word).unwrap().center();
    let view = reading.preferences();
    assert_eq!((view.zoom, view.turns), (3, 1));
    assert_ne!((view.offset_x, view.offset_y), (0.0, 0.0));
    let origin = app.current_history_location().unwrap();
    let original_frame = primitive_app::frame(&app, &mut canvas, false);
    assert_eq!(
        app.input(Input::LongTap(point.x as f32, point.y as f32)),
        Change::Partial
    );
    assert!(app.pdf_selection.is_some());
    assert_eq!(
        app.input(tap(app.selection_button(SelectionAction::Find))),
        Change::Partial
    );
    assert!(app.matches_return_to_reading);
    while app.search_at.is_some() {
        app.step_pdf_search(Instant::now()).unwrap();
    }
    assert_eq!(app.pdf_hits.len(), 2);
    assert_eq!(
        app.results_origin().map(|saved| &saved.location),
        Some(&origin)
    );
    app.activate(Action::PdfSearchHit(1)).unwrap();
    app.activate(Action::SetFixedZoom(1)).unwrap();
    app.activate(Action::SetFixedRotation(2)).unwrap();
    let on_match = app.current_history_location();
    assert_eq!(on_match.as_ref().unwrap().position.page, 1);
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    assert_eq!(app.input(tap(CURRENT)), Change::Partial);
    assert_eq!(app.focused_result, Some(1));
    assert_eq!(app.current_history_location(), on_match);
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_SEARCH_RESULT_OPTIONS_PDF", true);
    let history = app.back.clone();
    let forward = app.forward.clone();
    assert_eq!(app.input(tap(ORIGINAL)), Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.current_history_location(), Some(origin));
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), view);
    assert_eq!(app.back, history);
    assert_eq!(app.forward, forward);
    assert!(app.pdf_result_regions.is_empty());
    let page_pixels = (WIDTH * PAGE_HEIGHT) as usize;
    let restored_frame = primitive_app::frame(&app, &mut canvas, false);
    assert_eq!(
        &original_frame[..page_pixels],
        &restored_frame[..page_pixels]
    );

    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&pdf).unwrap();
    assert_eq!(
        reopened.current_history_location(),
        app.current_history_location()
    );
    assert_eq!(reopened.fixed.as_ref().unwrap().reading.preferences(), view);

    app.activate(Action::FixedPage(1)).unwrap();
    let second_origin = app.current_history_location();
    app.start_document_search("PD").unwrap();
    assert_eq!(
        app.results_origin().map(|saved| &saved.location),
        second_origin.as_ref()
    );
    app.open_path(&fixture.book).unwrap();
    assert!(app.results_origin().is_none());
}

fn chapter_result_indices(app: &Reader) -> Vec<usize> {
    let order = app.search_result_order();
    (0..order.len())
        .filter_map(|ordinal| order.provider_index(ordinal))
        .collect()
}

fn choose(app: &mut Reader, control: Rect) {
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    assert_eq!(app.input(tap(control)), Change::Partial);
}

#[test]
fn search_results_chapter_epub_filters_flexible_rows_and_exactly_steps_retained_hits() {
    let fixture = Fixture::new();
    crate::app::search_chapter_scope::tests::authored_epub(&fixture.book, true);
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.controls.search_max_lines = Some(4);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(0, 1)
        .unwrap();
    let origin = app.current_history_location();
    app.start_document_search("marker").unwrap();
    choose(&mut app, CHAPTER);
    assert_eq!(app.search_result_order().len(), 0);
    wait_for_search(&mut app, usize::MAX);
    assert_eq!(app.hits.len(), 6);
    assert_eq!(chapter_result_indices(&app), [2, 3]);
    assert_eq!(app.current_history_location(), origin);
    assert!(app.activate(Action::SearchHit(4)).is_err());
    capture_test_screen(&app, &fonts, "READER_SEARCH_CHAPTER_EPUB", false);
    assert_eq!(app.input(tap(OPTIONS)), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_SEARCH_CHAPTER_OPTIONS", false);
    assert_eq!(app.input(tap(CLOSE)), Change::Partial);

    let row_x = (crate::app::LIST_X + 50) as f32;
    let row_y = (crate::app::LIST_Y + 24) as f32;
    assert_eq!(app.input(Input::LongTap(row_x, row_y)), Change::Partial);
    assert_eq!(app.focused_result, Some(2));
    assert!(app.result_context.is_some());
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(Input::Tap(row_x, row_y)), Change::Partial);
    assert_eq!(
        app.result_selection.as_ref().unwrap().start,
        app.hits[2].anchor
    );
    assert_eq!(app.search_caption().as_deref(), Some("Match 1 of 2"));
    assert_eq!(app.input(tap(crate::app::MATCH_NEXT)), Change::Partial);
    assert_eq!(
        app.result_selection.as_ref().unwrap().start,
        app.hits[3].anchor
    );
    assert_eq!(app.search_caption().as_deref(), Some("Match 2 of 2"));
    assert!(app.search_step(1).is_none());

    app.activate(Action::Show(Screen::Matches)).unwrap();
    choose(&mut app, CURRENT);
    assert_eq!(app.focused_result, Some(3));
    assert!(app.flexible_result_range().unwrap().contains(&1));
    choose(&mut app, ORIGINAL);
    assert_eq!(app.current_history_location(), origin);
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(chapter_result_indices(&app), [2, 3]);

    app.start_document_search("Delta").unwrap();
    assert!(!app.search_chapter_active());
    choose(&mut app, CHAPTER);
    wait_for_search(&mut app, usize::MAX);
    assert!(app.hits.len() == 1 && chapter_result_indices(&app).is_empty());
    choose(&mut app, ALL);
    assert_eq!(chapter_result_indices(&app), [0]);
    assert_eq!(app.current_history_location(), origin);
}

#[test]
fn search_results_chapter_pdf_reuses_filtered_ordinals_for_pagers_fit_and_origin() {
    use crate::app::{LAST_MATCH_PAGE, ReadingPosition};

    let fixture = Fixture::new();
    let path = fixture.root.join("Survey sheets.pdf");
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 430] /Contents 11 0 R >>";
    let body = "BT /F1 16 Tf 25 370 Td (Survey marker) Tj ET";
    let stream = format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
    std::fs::write(
        &path,
        crate::fixed::tests::pdf_objects(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R] /Count 8 /Resources << /Font << /F1 12 0 R >> >> >>",
            page, page, page, page, page, page, page, page,
            &stream,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ]),
    )
    .unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();
    app.activate(Action::FixedPage(3)).unwrap();
    app.activate(Action::SetFixedRotation(1)).unwrap();
    app.activate(Action::SetFixedZoom(3)).unwrap();
    app.controls.search_rows = Some(2);
    let id = app.open_book_id().unwrap();
    let custom = &mut app.library.state_mut(id).unwrap().custom_contents;
    custom.enabled = true;
    for (page, title) in [(2, "Middle chapter"), (6, "Closing chapter")] {
        custom
            .save(
                ReadingPosition {
                    chapter: 0,
                    page,
                    anchor: None,
                },
                title,
            )
            .unwrap();
    }
    let origin = app.current_history_location();
    let view = app.fixed.as_ref().unwrap().reading.preferences();
    app.start_document_search("marker").unwrap();
    choose(&mut app, CHAPTER);
    assert!(chapter_result_indices(&app).is_empty());
    while app.search_at.is_some() {
        app.step_pdf_search(std::time::Instant::now()).unwrap();
    }
    assert_eq!(app.pdf_hits.len(), 8);
    assert_eq!(chapter_result_indices(&app), [2, 3, 4, 5]);
    assert!(app.activate(Action::PdfSearchHit(6)).is_err());
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_page, 1);
    assert_eq!(app.input(tap(FIRST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.input(tap(LAST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_window().0, 4);
    capture_test_screen(&app, &fonts, "READER_SEARCH_CHAPTER_PDF", true);

    let row = app.match_geometry().row(1);
    let x = (row.x + 65) as f32;
    let y = (row.y + row.h / 2) as f32;
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert_eq!(app.focused_result, Some(5));
    assert!(app.result_context.is_some());
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(tap(row)), Change::Partial);
    assert!(app.fixed_search_fit_active());
    assert_eq!(app.search_caption().as_deref(), Some("Match 4 of 4"));
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 5);
    // Exercise the actual fitted footer controls from a transformed page.
    let footer_y = (crate::app::MATCH_PREV.y + crate::app::MATCH_PREV.h / 2) as f32;
    assert_eq!(app.input(Input::Tap(770.0, footer_y)), Change::None);
    assert_eq!(app.input(Input::Tap(470.0, footer_y)), Change::Partial);
    assert_eq!(app.search_result, Some(4));
    assert_eq!(app.input(Input::LongTap(470.0, footer_y)), Change::Partial);
    assert_eq!(app.search_result, Some(2));
    assert_eq!(app.input(Input::Tap(470.0, footer_y)), Change::None);
    assert_eq!(app.input(Input::LongTap(770.0, footer_y)), Change::Partial);
    assert_eq!(app.search_result, Some(5));
    assert_eq!(app.input(Input::Tap(620.0, footer_y)), Change::Partial);
    assert!(!app.fixed_search_fit_active());
    assert_eq!(app.screen, Screen::Matches);
    assert_eq!(app.list_page, 1);
    assert_eq!(chapter_result_indices(&app), [2, 3, 4, 5]);
    choose(&mut app, CURRENT);
    assert_eq!(app.focused_result, Some(5));
    choose(&mut app, ORIGINAL);
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), view);

    app.activate(Action::Show(Screen::Matches)).unwrap();
    choose(&mut app, ALL);
    assert_eq!(app.list_page, 0);
    assert_eq!(chapter_result_indices(&app), (0..8).collect::<Vec<_>>());
    choose(&mut app, CHAPTER);
    app.open_path(&fixture.book).unwrap();
    assert!(!app.search_chapter_active());
}
