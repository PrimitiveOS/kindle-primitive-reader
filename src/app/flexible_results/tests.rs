use super::super::list_density::{APPLY, DECREASE, DISABLE_LINES, INCREASE, LINE_DEFAULT};
use super::super::tests::{Fixture, capture_test_screen, tap_contents_item, wait_for_search};
use super::*;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn hold(rect: Rect) -> Input {
    Input::LongTap((rect.x + 35) as f32, (rect.bottom() - 7) as f32)
}

fn row(app: &Reader, offset: usize) -> (usize, Rect, usize) {
    app.with_flexible_results(|view| {
        let page = &view.pages[app.list_page];
        let row = &page.rows[offset];
        (page.first + offset, row.bounds, row.lines)
    })
    .unwrap()
}

fn narrow_or_long(prefix: &str, index: usize) -> String {
    if index.is_multiple_of(3) {
        format!("Survey observation {index}: name the station.")
    } else {
        format!(
            "From checkpoint {index}, {} observation confirms {}",
            prefix.repeat(20),
            "the measured difference in the shoreline recorded in the coastal survey. ".repeat(6)
        )
    }
}

#[test]
fn flexible_result_settings_and_epub_rows_keep_their_source_ranges() {
    let fixture = Fixture::new();
    let document = fixture.root.join("Observations.epub");
    fs::copy(crate::test_fixture("long-chapter.epub"), &document).unwrap();
    let mut app = open(&fixture, &document);
    app.controls.search_rows = Some(8);
    assert_eq!(app.search_max_lines(), None);

    app.open_reading_edge(true, WIDTH as f32 / 2.0);
    let search = app.reading_button("Search");
    assert_eq!(app.input(tap(search)), Change::Partial);
    let settings = app.reading_button("Search settings");
    assert_eq!(app.input(tap(settings)), Change::Partial);
    tap_contents_item(&mut app, "Max lines per result");
    assert_eq!(app.list_density_picker.as_ref().unwrap().value, Some(4));
    assert_eq!(app.input(hold(DECREASE)), Change::Partial);
    assert_eq!(app.input(Input::PageBack), Change::None);
    for _ in 0..3 {
        assert_eq!(app.input(hold(INCREASE)), Change::Partial);
    }
    assert_eq!(app.input(Input::PageForward), Change::None);
    assert_eq!(app.list_density_picker.as_ref().unwrap().value, Some(10));
    assert_eq!(app.input(tap(LINE_DEFAULT)), Change::Partial);
    assert_eq!(app.input(tap(APPLY)), Change::Partial);
    assert_eq!(app.search_max_lines(), Some(4));
    let rows = app
        .items()
        .into_iter()
        .find(|item| item.title == "Results per page")
        .unwrap();
    assert!(matches!(rows.action, Action::Heading));
    assert_eq!(rows.detail.as_deref(), Some("Flexible"));

    let origin = app.current_history_location();
    app.start_document_search("Observation").unwrap();
    wait_for_search(&mut app, usize::MAX);
    // Keep real provider anchors and vary only the presented context lengths.
    app.hits.truncate(23);
    for (index, result) in app.hits.iter_mut().enumerate() {
        let text = narrow_or_long("longer preceding context ", index);
        let start = text.find("observation").unwrap();
        result.snippet = text;
        result.matched = start..start + "observation".len();
    }
    app.clear_flexible_results();

    let first = row(&app, 0);
    let second = row(&app, 1);
    assert_eq!((first.0, first.2, second.0, second.2), (0, 1, 1, 4));
    assert_eq!(first.1.bottom(), second.1.y);
    assert!(first.1.h < second.1.h);
    let source = app.hits[1].clone();
    capture_test_screen(&app, &app.fonts, "READER_FLEXIBLE_EPUB_RESULTS", false);
    assert_eq!(app.input(hold(second.1)), Change::Partial);
    assert_eq!(app.focused_result, Some(1));
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(tap(second.1)), Change::Partial);
    assert_eq!(app.search_result, Some(1));
    let landed = app.result_selection.as_ref().unwrap();
    assert_eq!((&landed.start, &landed.end), (&source.anchor, &source.end));

    app.show(Screen::Matches);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let start = row(&app, 0).0;
    assert_eq!(app.match_page_for_index(start), 1);
    assert_eq!(app.input(tap(LAST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.flexible_result_range().unwrap().end, app.hits.len());
    assert_eq!(app.input(tap(FIRST_MATCH_PAGE)), Change::Partial);
    assert_eq!(app.list_page, 0);

    let mut reopened = open(&fixture, &document);
    assert_eq!(reopened.search_max_lines(), Some(4));
    assert_eq!(reopened.match_geometry().rows, 8);
    reopened.show(Screen::SearchSettings);
    tap_contents_item(&mut reopened, "Max lines per result");
    assert_eq!(reopened.input(tap(DISABLE_LINES)), Change::Partial);
    assert_eq!(reopened.input(tap(APPLY)), Change::Partial);
    assert_eq!(reopened.search_max_lines(), None);
    assert_eq!(reopened.match_geometry().rows, 8);
}

fn register_pdf(page_count: usize) -> Vec<u8> {
    let kids = (0..page_count)
        .map(|page| format!("{} 0 R", 4 + page * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Count {page_count} /Kids [{kids}] /Resources << /Font << /F1 3 0 R >> >> >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    for page in 0..page_count {
        let stream = objects.len() + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 460 480] /Contents {stream} 0 R >>"
        ));
        let mut content = format!("BT /F1 13 Tf 30 420 Td (Survey observation {page}) Tj");
        if !page.is_multiple_of(3) {
            for _ in 0..8 {
                content.push_str(
                    " 0 -24 Td (Coastal measurements confirm that the shoreline moved.) Tj",
                );
            }
        }
        content.push_str(" ET\n");
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    crate::fixed::tests::pdf_objects(&objects)
}

#[test]
fn flexible_pdf_rows_keep_exact_ranges_when_the_final_page_receives_more_results() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Register.pdf");
    fs::write(&path, register_pdf(16)).unwrap();
    let mut app = open(&fixture, &path);
    app.controls.search_max_lines = Some(3);
    app.controls.search_context_words = Some(35);
    app.start_document_search("observation").unwrap();
    assert_eq!(app.search_results_empty_label(), "Searching pages");
    while app.search_at.is_some() {
        app.step_pdf_search(Instant::now()).unwrap();
    }
    assert_eq!(app.pdf_hits.len(), 16);
    let remainder = app.pdf_hits.split_off(4);
    app.clear_flexible_results();
    assert_eq!(app.flexible_result_range(), Some(0..4));
    let first = row(&app, 0);
    let second = row(&app, 1);
    assert_eq!(first.2, 1);
    assert!(second.2 > first.2);
    app.pdf_hits.extend(remainder);
    assert_eq!(row(&app, 0), first);
    assert!(app.flexible_result_range().unwrap().end > 4);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let (index, rect, _) = row(&app, 1);
    let expected = &app.pdf_hits[index];
    let page = expected.page;
    let regions: Vec<_> = expected.bounds.iter().map(|rect| (page, *rect)).collect();
    assert_ne!(app.pdf_hits[index - 1].page, page);
    capture_test_screen(&app, &app.fonts, "READER_FLEXIBLE_PDF_RESULTS", true);
    assert_eq!(app.input(hold(rect)), Change::Partial);
    assert_eq!(app.focused_result, Some(index));
    assert_eq!(app.input(Input::Tap(10.0, 10.0)), Change::Partial);
    assert_eq!(app.input(tap(rect)), Change::Partial);
    assert_eq!(app.search_result, Some(index));
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), page);
    assert_eq!(app.pdf_result_regions, regions);
    app.show(Screen::Matches);
    assert_eq!(app.list_page, 1);
    assert_eq!(row(&app, 1).0, index);
}
