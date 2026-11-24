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
    assert!(app.fixed_search_fit_active());
    app.close_fixed_search_fit().unwrap();
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

#[test]
fn document_search_viewport_epub_keyboard_pagers_context_and_return_follow_the_view() {
    use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap as press_tap};
    use crate::app::{FIND_CASE, FIND_PATTERN, LAST_MATCH_PAGE, MATCH_PREV, NEXT_MATCH_PAGE};
    use crate::session::ReadingViewport;
    use primitive_ui::Key;

    let fixture = Fixture::new();
    std::fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    let portrait_rows = app.match_geometry().rows;
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let origin = app.current_history_location();
    app.show(Screen::Find);
    let layout = app.search_layout();
    let (_, height) = layout.viewport.dimensions();
    let pattern = layout.find_option(FIND_PATTERN);
    assert!(pattern.x + pattern.w / 2 > WIDTH as i32);
    press(&app, &mut canvas, layout.find_option(FIND_CASE), false);
    press_tap(&mut app, layout.find_option(FIND_CASE));
    assert!(app.find_match_case);
    press_tap(&mut app, layout.find_option(FIND_CASE));
    press_tap(&mut app, pattern);
    assert!(app.find_regex);
    press_tap(&mut app, pattern);
    app.clipboard_text = Some("changing ".into());
    let paste = app.editor_paste_rect();
    assert!(paste.x > WIDTH as i32);
    press(&app, &mut canvas, paste, false);
    press_tap(&mut app, paste);
    for ch in "wind".chars() {
        let rect = key(&app, Key::Char(ch), "Find all");
        press_tap(&mut app, rect);
    }
    assert_eq!(app.editor.string(), "changing wind");
    let find = key(&app, Key::Go, "Find all");
    assert!(find.x > WIDTH as i32 && find.bottom() <= height as i32);
    capture(&app, &mut canvas, "landscape-find-document.png", false);
    press_tap(&mut app, find);
    assert_eq!(app.screen, Screen::Matches);
    wait_for_search(&mut app, usize::MAX);
    let geometry = app.match_geometry();
    assert!(geometry.rows < portrait_rows && app.hits.len() > geometry.rows * 2);
    assert!(geometry.row(0).right() > WIDTH as i32);
    let next = layout.list_button(NEXT_MATCH_PAGE);
    press(&app, &mut canvas, next, false);
    capture(&app, &mut canvas, "landscape-document-matches.png", false);
    press_tap(&mut app, next);
    assert_eq!(app.list_page, 1);
    let last = layout.list_button(LAST_MATCH_PAGE);
    press_tap(&mut app, last);
    let ordinal = app.list_page * geometry.rows;
    let row = geometry.row(0);
    let (x, y) = ((row.right() - 30) as f32, (row.y + row.h / 2) as f32);
    assert!(x > WIDTH as f32);
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert!(app.result_context.is_some());
    assert_eq!(app.focused_result, Some(ordinal));
    assert_eq!(app.current_history_location(), origin);
    capture(&app, &mut canvas, "landscape-held-result-context.png", true);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert!(app.result_context.is_none());
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(
        app.result_selection.as_ref().unwrap().start,
        app.hits[ordinal].anchor
    );
    let previous = layout.reading_footer(MATCH_PREV);
    press_tap(&mut app, previous);
    assert_eq!(app.search_result, Some(ordinal - 1));
    app.show(Screen::Matches);
    let options = layout.options_button(OPTIONS);
    assert!(options.x > WIDTH as i32);
    press(&app, &mut canvas, options, false);
    press_tap(&mut app, options);
    press_tap(&mut app, layout.centered(ORIGINAL));
    assert_eq!(app.current_history_location(), origin);

    app.show(Screen::Matches);
    press_tap(&mut app, last);
    let first = app.list_page * geometry.rows;
    app.show(Screen::Book);
    let _portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 3);
    app.show(Screen::Matches);
    let page = app.list_page;
    assert!(page > 0);
    assert!((page * portrait_rows..(page + 1) * portrait_rows).contains(&first));

    let candidate = primitive_app::viewport::Viewport {
        generation: 4,
        turn: primitive_app::viewport::LogicalTurn::Clockwise,
        logical_size: (1680, 1264),
    };
    app.stage_viewport(candidate).unwrap();
    canvas.begin();
    app.draw_staged_viewport(&mut canvas).unwrap();
    assert_eq!(app.list_page, page);
    app.finish_viewport(candidate, primitive_app::viewport::ViewportDecision::Abort);
    assert_eq!(app.list_page, page);
    assert_eq!(app.reading_viewport(), ReadingViewport::PORTRAIT);
    let _landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 5);
    assert_eq!(app.screen, Screen::Matches);
    let active = app.list_page * geometry.rows;
    assert!((active..active + geometry.rows).contains(&first));
    assert_eq!(app.current_history_location(), origin);
}

#[test]
fn search_result_anchor_survives_round_trip_and_resets_on_explicit_page_turns() {
    use crate::app::selection_viewport_tests::adopt;
    use crate::session::ReadingViewport;

    let fixture = Fixture::new();
    std::fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.start_document_search("Observation").unwrap();
    wait_for_search(&mut app, usize::MAX);
    let portrait_rows = app.match_geometry().rows;
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_page, 1);
    let origin = app.current_history_location();

    let _ = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let landscape_rows = app.match_geometry().rows;
    assert!(landscape_rows < portrait_rows);
    let landscape_page = portrait_rows / landscape_rows;
    assert!(landscape_page > 0);
    assert_eq!(app.list_page, landscape_page);
    let _ = adopt(&mut app, ReadingViewport::PORTRAIT, 2);
    assert_eq!(
        app.list_page, 1,
        "a round trip must carry the original result ordinal"
    );

    let _ = adopt(&mut app, ReadingViewport::LANDSCAPE, 3);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_page, landscape_page);
    let reset = landscape_page * landscape_rows;
    assert!(reset < portrait_rows);
    let _ = adopt(&mut app, ReadingViewport::PORTRAIT, 4);
    assert_eq!(
        app.list_page, 0,
        "explicit paging chooses the new leading result"
    );

    app.show(Screen::Book);
    let _ = adopt(&mut app, ReadingViewport::LANDSCAPE, 5);
    app.show(Screen::Matches);
    assert_eq!(
        app.list_page, landscape_page,
        "page zero can still contain a retained nonzero ordinal"
    );
    assert_eq!(app.current_history_location(), origin);

    app.start_document_search("changing wind").unwrap();
    wait_for_search(&mut app, usize::MAX);
    assert_eq!(app.list_page, 0);
}
