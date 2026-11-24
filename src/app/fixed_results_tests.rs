use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use std::fmt::Write as _;
use std::thread;

fn tide_register() -> Vec<u8> {
    let pages = 120;
    let kids = (0..pages)
        .map(|i| format!("{} 0 R", 4 + 2 * i))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        format!(
            "<< /Type /Pages /Count {pages} /Kids [{kids}] /Resources << /Font << /F1 3 0 R >> >> >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    for page in 0..pages {
        let stream = objects.len() + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 390 550] /Contents {stream} 0 R >>"
        ));
        let mut content = format!(
            "BT /F1 18 Tf 22 510 Td (Harbour register) Tj 0 -25 Td /F1 11 Tf (Station observations - log {}) Tj 0 -30 Td",
            page + 1
        );
        for row in 0..22 {
            write!(
                content,
                " ({:02}:00   Tide at West Pier: {} cm.   Tide at East Pier: {} cm.) Tj 0 -18 Td",
                row + 1,
                72 + (row + page) % 54,
                83 + (2 * row + page) % 67
            )
            .unwrap();
        }
        content.push_str(" ET\n");
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    crate::fixed::tests::pdf_objects(&objects)
}

fn tap(app: &mut Reader, rect: Rect) -> Change {
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}

fn find(app: &mut Reader, query: &str) {
    app.show(Screen::Find);
    app.editor.text = query.chars().collect();
    app.editor.caret = app.editor.text.len();
    let go = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Find all")
        .into_iter()
        .find(|(key, _)| key.key == primitive_ui::Key::Go)
        .unwrap()
        .1;
    assert_eq!(tap(app, go), Change::Partial);
    assert_eq!(app.screen, Screen::Matches);
}

#[test]
fn complete_fixed_results_page_through_occurrences_and_restore_the_passage() {
    let fixture = Fixture::new();
    let external = std::env::var_os("READER_TEST_FIXED_ALL_DOCUMENT").map(PathBuf::from);
    let authored = external.is_none();
    let path = external.unwrap_or_else(|| {
        let path = fixture.root.join("Harbour register.pdf");
        fs::write(&path, tide_register()).unwrap();
        path
    });
    let query = std::env::var("READER_TEST_FIXED_ALL_QUERY").unwrap_or_else(|_| "tide".into());
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
    app.open_path(&path).unwrap();
    let pages = app.fixed.as_ref().unwrap().reading.count();
    app.fixed.as_mut().unwrap().reading.jump(pages / 2).unwrap();
    let origin = app.current_history_location().unwrap();

    let clock = Instant::now();
    find(&mut app, &query);
    let mut first_visible = None;
    while let Some(at) = app.search_at {
        // Use the real scheduler delays on hardware and when recording latency.
        if std::env::var_os("READER_TEST_FIXED_ALL_TIMING").is_some() {
            thread::sleep(at.saturating_duration_since(Instant::now()));
        }
        let change = app.step_pdf_search(Instant::now()).unwrap();
        if first_visible.is_none() && change != Change::None && !app.pdf_hits.is_empty() {
            first_visible = Some(clock.elapsed());
        }
    }
    let completed = clock.elapsed();
    let count = app.pdf_hits.len();
    assert!((101..=MAX_FIXED_SEARCH_HITS).contains(&count));
    let mut per_page = vec![0usize; pages];
    for hit in &app.pdf_hits {
        per_page[hit.page] += 1;
        assert!(!hit.bounds.is_empty());
    }
    if authored {
        assert_eq!(count, 5000);
        assert_eq!(&per_page[..113], &[44; 113]);
        assert_eq!(per_page[113], 28);
        assert_eq!(per_page[114..], [0; 6]);
        assert_eq!(app.notice.as_deref(), Some("Showing first 5,000 matches"));
    }
    let search = app.pdf_search.as_ref().unwrap();
    let (bytes, scanned) = (search.retained_bytes, search.next_page);
    assert!(!search.memory_limited);
    let geometry = app.match_geometry();
    let last_page = (count - 1) / geometry.rows;
    let final_window = Instant::now();
    assert_eq!(tap(&mut app, LAST_MATCH_PAGE), Change::Partial);
    assert_eq!(app.list_page, last_page);
    let (total, rows) = app.list_window();
    let final_window_us = final_window.elapsed().as_micros();
    assert_eq!(total, count);
    assert_eq!(rows.len(), count - last_page * geometry.rows);
    assert!(matches!(rows.last().unwrap().action, Action::PdfSearchHit(i) if i == count - 1));
    if authored {
        capture_test_screen(
            &app,
            std::slice::from_ref(&font),
            "READER_FIXED_ALL_LAST_SCREENSHOT",
            false,
        );
    }
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.list_page, last_page - 1);
    app.activate(Action::Show(Screen::Find)).unwrap();
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.list_page, last_page - 1);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_page, last_page);
    assert_eq!(tap(&mut app, FIRST_MATCH_PAGE), Change::Partial);
    assert_eq!(app.list_page, 0);
    assert_eq!(tap(&mut app, LAST_MATCH_PAGE), Change::Partial);
    let final_row = geometry.row((count - 1) % geometry.rows);
    app.input(Input::Tap(
        (LIST_X + 30) as f32,
        (final_row.y + final_row.h / 2) as f32,
    ));
    assert_eq!(app.screen, Screen::Fixed);
    let hit = &app.pdf_hits[count - 1];
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), hit.page);
    assert_eq!(
        app.pdf_result_regions,
        hit.bounds
            .iter()
            .map(|&b| (hit.page, b))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        app.search_caption(),
        Some(format!("Match {count} of {count}"))
    );
    app.activate(Action::Show(Screen::Matches)).unwrap();
    assert_eq!(app.list_page, last_page);
    app.activate(Action::ReturnToLocation).unwrap();
    let restored = app.current_history_location().unwrap();
    assert_eq!(restored.position, origin.position);
    if let (Some(a), Some(b)) = (restored.scan_point, origin.scan_point) {
        assert!((a - b).hypot() < 0.01);
    } else {
        assert_eq!(restored.scan_point, origin.scan_point);
    }
    if std::env::var_os("READER_TEST_FIXED_ALL_TIMING").is_some() {
        println!(
            "fixed-all {}",
            serde_json::json!({"counts": per_page, "first_ms": first_visible.unwrap().as_millis(), "complete_ms": completed.as_millis(), "last_window_us": final_window_us, "bytes": bytes, "scanned_pages": scanned, "restored": true})
        );
    }
}

#[test]
fn partial_fixed_results_cancel_and_replace_without_stale_rows() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbour register.pdf");
    fs::write(&path, tide_register()).unwrap();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&path).unwrap();
    find(&mut app, "Tide");
    assert_eq!(
        app.step_pdf_search(Instant::now()).unwrap(),
        Change::Partial
    );
    assert!(!app.pdf_hits.is_empty());
    assert!(app.search_at.is_some());
    assert_eq!(
        tap(&mut app, primitive_ui::back_button(WIDTH as i32)),
        Change::Partial
    );
    assert!(app.search_at.is_none());
    assert_eq!(app.screen, Screen::Find);
    find(&mut app, "observations");
    while app.search_at.is_some() {
        app.step_pdf_search(Instant::now()).unwrap();
    }
    assert_eq!(app.pdf_hits.len(), 120);
    assert!(
        app.pdf_hits
            .iter()
            .all(|h| h.snippet.contains("observations"))
    );
    assert_eq!(app.list_page, 0);
}

#[test]
fn document_search_viewport_pdf_flexible_rows_fit_hold_and_return_to_the_result_list() {
    use crate::app::selection_viewport_tests::{adopt, capture, key, press};
    use crate::session::ReadingViewport;
    use primitive_ui::Key;

    let fixture = Fixture::new();
    let file = fixture.root.join("Harbour register.pdf");
    fs::write(&file, tide_register()).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&file).unwrap();
    app.controls.search_max_lines = Some(3);
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let origin = app.current_history_location();
    app.show(Screen::Find);
    for ch in "station".chars() {
        let rect = key(&app, Key::Char(ch), "Find all");
        assert_eq!(tap(&mut app, rect), Change::Partial);
    }
    let go = key(&app, Key::Go, "Find all");
    assert_eq!(tap(&mut app, go), Change::Partial);
    while let Some(at) = app.search_at {
        app.step_pdf_search(at).unwrap();
    }
    assert_eq!(app.pdf_hits.len(), 120);
    let layout = app.search_layout();
    let next = layout.list_button(NEXT_MATCH_PAGE);
    press(&app, &mut canvas, next, true);
    capture(
        &app,
        &mut canvas,
        "landscape-flexible-pdf-matches.png",
        true,
    );
    assert_eq!(tap(&mut app, next), Change::Partial);
    let start = app.flexible_result_range().unwrap().start;
    assert!(start > 0 && start < app.pdf_hits.len());
    let point = ((layout.width() - 64) as f32, (LIST_Y + 45) as f32);
    assert!(point.0 > WIDTH as f32);
    assert_eq!(app.input(Input::LongTap(point.0, point.1)), Change::Partial);
    assert!(app.result_context.is_some());
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.input(Input::Tap(point.0, point.1)), Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(
        app.current_position().unwrap().page,
        app.pdf_hits[start].page
    );
    assert!(!app.pdf_result_regions.is_empty());
    assert!(app.fixed_search_fit_active());
    let results = layout.reading_footer(Rect::new(534, MATCH_PREV.y, 176, MATCH_PREV.h));
    press(&app, &mut canvas, results, false);
    assert_eq!(tap(&mut app, results), Change::Partial);
    assert_eq!(app.screen, Screen::Matches);
    assert!(app.flexible_result_range().unwrap().contains(&start));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), origin);
}
