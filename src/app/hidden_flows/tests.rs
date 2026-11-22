use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn center(rect: Rect) -> (f32, f32) {
    ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn confirm(app: &mut Reader) {
    let sheet = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "");
    let (x, y) = center(sheet.button(SheetAction::Confirm));
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    assert!(app.flow_popup.is_none());
}

fn mark(app: &mut Reader, chapter: usize, page: usize) {
    app.activate(Action::Overview).unwrap();
    app.update_overview(Some(chapter), Some(page)).unwrap();
    let first = app.overview.as_ref().unwrap().first;
    let (x, y) = center(super::super::overview_tile(page - first));
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert!(app.flow_popup.is_some());
    confirm(app);
}

fn turn(app: &mut Reader, forward: bool) {
    app.show(if app.fixed.is_some() {
        Screen::Fixed
    } else {
        Screen::Book
    });
    app.input(if forward {
        Input::PageForward
    } else {
        Input::PageBack
    });
    let deadline = app
        .active
        .as_ref()
        .and_then(|book| book.reading.deadline())
        .or_else(|| app.fixed.as_ref().and_then(|book| book.reading.deadline()));
    if let Some(due) = deadline {
        app.tick(due + Duration::from_millis(1));
    }
}

#[test]
fn custom_fixed_page_flows_skip_pages_in_paged_and_continuous_views_and_resume_notes() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Survey sheets.pdf");
    let source = crate::fixed_session::tests::striped_pdf_bytes(6);
    fs::write(&path, &source).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();
    app.controls.end_action = crate::reading_controls::EndAction::Stay;
    app.controls.finish_at_end = false;
    app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
    for page in [1, 3] {
        app.activate(Action::Overview).unwrap();
        app.update_overview(None, Some(page)).unwrap();
        let index = page - app.overview.as_ref().unwrap().first;
        let (x, y) = center(super::super::overview_tile(index));
        app.input(Input::LongTap(x, y));
        assert!(app.flow_popup.is_some());
        confirm(&mut app);
    }
    let id = app.open_book_id().unwrap();
    app.follow_fixed(0, None).unwrap();
    turn(&mut app, true);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 3);
    turn(&mut app, false);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
    app.follow_fixed(1, None).unwrap();
    turn(&mut app, true);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 2);
    assert_eq!(
        app.status_caption().as_deref(),
        Some("Hidden 1 · Page 2 of 2")
    );
    app.record_position();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 2);
    assert!(reopened.library.state(id).unwrap().hidden_flows.enabled);
    assert_ne!(
        reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .hidden_flow_map()
            .unwrap()
            .flow(2),
        0
    );
    reopened.follow_fixed(0, None).unwrap();
    reopened.activate(Action::FixedScroll(true)).unwrap();
    let visible = reopened
        .fixed
        .as_ref()
        .unwrap()
        .reading
        .visible_pages()
        .collect::<Vec<_>>();
    assert_eq!(visible, [0, 3]);
    capture_test_screen(
        &reopened,
        &reopened.fonts,
        "READER_CUSTOM_FLOW_PDF_SCREENSHOT",
        false,
    );
    turn(&mut reopened, true);
    for page in reopened.fixed.as_ref().unwrap().reading.visible_pages() {
        assert!(![1, 2].contains(&page));
    }
    reopened.show(Screen::BookMap);
    capture_test_screen(
        &reopened,
        &reopened.fonts,
        "READER_CUSTOM_FLOW_MAP_SCREENSHOT",
        false,
    );
    // The final hidden source page is shorter than a continuous viewport.
    // Reopening must not save the preceding regular page instead.
    reopened.activate(Action::Overview).unwrap();
    reopened.update_overview(None, Some(5)).unwrap();
    let index = 5 - reopened.overview.as_ref().unwrap().first;
    let (x, y) = center(super::super::overview_tile(index));
    reopened.input(Input::LongTap(x, y));
    confirm(&mut reopened);
    reopened.follow_fixed(5, None).unwrap();
    assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), 5);
    assert!(reopened.fixed.as_ref().unwrap().reading.is_continuous());
    reopened.record_position();
    reopened.save().unwrap();
    let mut resumed = Reader::open(&fixture.root, &fixture.state, reopened.fonts.clone()).unwrap();
    resumed.open_path(&path).unwrap();
    assert_eq!(resumed.fixed.as_ref().unwrap().reading.page(), 5);
    assert_eq!(
        resumed
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .visible_pages()
            .collect::<Vec<_>>(),
        [5]
    );
    assert!(resumed.fixed.as_ref().unwrap().reading.is_continuous());
    assert!(
        source == fs::read(&path).unwrap(),
        "Reading changed the source PDF"
    );
}

fn setup(fixture: &Fixture) -> Reader {
    crate::app::nonlinear_tests::authored_book(&fixture.book);
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
    app
}

#[test]
fn custom_flow_markers_skip_source_ranges_and_preserve_explicit_jumps_and_repagination() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.follow_reflow_page(2, 0).unwrap();
    let count = app.active.as_ref().unwrap().reading.probe.page_count();
    assert!(count >= 4);
    mark(&mut app, 2, 1);
    mark(&mut app, 2, 3);
    let id = app.open_book_id().unwrap();
    let data = app.library.state(id).unwrap().hidden_flows.clone();
    assert_eq!(data.points().len(), 2);
    assert!(data.points()[0].hidden && !data.points()[1].hidden);
    let original_points: Vec<_> = data
        .points()
        .iter()
        .map(|point| point.position.clone())
        .collect();
    app.follow_reflow_page(2, 0).unwrap();
    let origin = app.current_history_location().unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.page, 3);
    turn(&mut app, false);
    assert_eq!(app.active.as_ref().unwrap().reading.page, 0);
    app.follow_reflow_page(2, 1).unwrap();
    assert_ne!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .custom_flow_map()
            .unwrap()
            .flow(1),
        0
    );
    assert!(app.status_caption().unwrap().ends_with("Page 1 of 2"));
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.page, 2);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(origin));

    app.activate(Action::Overview).unwrap();
    app.update_overview(Some(2), Some(0)).unwrap();
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CUSTOM_FLOW_PAGES_SCREENSHOT",
        false,
    );
    let (x, y) = center(super::super::overview_tile(1));
    app.input(Input::LongTap(x, y));
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CUSTOM_FLOW_MARK_SCREENSHOT",
        false,
    );
    let cancel = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Cancel);
    let (x, y) = center(cancel);
    app.input(Input::Tap(x, y));
    app.show(Screen::HiddenFlows);
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CUSTOM_FLOW_SETTINGS_SCREENSHOT",
        false,
    );
    capture_test_screen(&app, &app.fonts, "READER_CUSTOM_FLOW_DARK_SCREENSHOT", true);

    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: settings.font_size + 8.0,
        logical_font_size: None,
        ..settings
    }))
    .unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    for (index, point) in original_points.iter().enumerate() {
        let page = reading
            .probe
            .page_for(point.anchor.as_ref().unwrap())
            .unwrap();
        assert_eq!(reading.flow_marker_at(2, page), Some(index));
    }
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .hidden_flows
            .points()
            .iter()
            .map(|point| point.position.clone())
            .collect::<Vec<_>>(),
        original_points
    );
    let target = original_points[0].clone();
    app.restore_position(&target).unwrap();
    app.record_position();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, app.fonts.clone()).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.current_position().unwrap().anchor, target.anchor);
    assert!(reopened.library.state(id).unwrap().hidden_flows.enabled);
    assert_eq!(
        reopened
            .library
            .state(id)
            .unwrap()
            .hidden_flows
            .points()
            .len(),
        2
    );
    reopened
        .activate(Action::Flow(FlowAction::ClearAll))
        .unwrap();
    confirm(&mut reopened);
    assert!(
        reopened
            .library
            .state(id)
            .unwrap()
            .hidden_flows
            .points()
            .is_empty()
    );
    assert!(reopened.library.state(id).unwrap().hidden_flows.enabled);
}

#[test]
fn custom_flow_spreads_and_continuous_parts_keep_selection_on_visible_source_text() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.follow_reflow_page(2, 0).unwrap();
    mark(&mut app, 2, 1);
    mark(&mut app, 2, 3);
    app.follow_reflow_page(2, 0).unwrap();
    app.activate(Action::FacingPages(true)).unwrap();
    let first = app.current_position().unwrap();
    {
        let reading = &app.active.as_ref().unwrap().reading;
        let map = reading.custom_flow_map().unwrap();
        let ranges = reading.visible_columns().collect::<Vec<_>>();
        assert_eq!(ranges.len(), 2);
        for (range, _) in ranges {
            assert_eq!(map.flow(reading.probe.page_at(range.start)), 0);
        }
    }
    turn(&mut app, true);
    turn(&mut app, false);
    assert_eq!(app.current_position().unwrap().anchor, first.anchor);
    app.activate(Action::FacingPages(false)).unwrap();
    app.activate(Action::ReadingMode(true)).unwrap();
    app.follow_reflow_page(2, 0).unwrap();
    let input = Input::Swipe {
        dx: 0.0,
        dy: -450.0,
        start_y: 700.0,
    };
    app.input(input);
    if let Some(due) = app.active.as_ref().unwrap().reading.deadline() {
        app.tick(due + Duration::from_millis(1));
    }
    let reading = &app.active.as_ref().unwrap().reading;
    let parts = reading.visible_parts();
    assert!(parts.len() >= 2);
    assert!(parts[0].range.end < parts[1].range.start);
    let map = reading.custom_flow_map().unwrap();
    let mut checked = 0;
    for part in parts {
        for y in ((part.y + reading.probe.settings.top_margin() + 8)
            ..(part.y
                + reading.probe.settings.top_margin()
                + (part.range.end - part.range.start) as u32))
            .step_by(13)
        {
            for x in (80..WIDTH - 80).step_by(47) {
                if let Some(word) = reading.word_at(x as f32, y as f32) {
                    let rects = reading.highlight_rects(&word);
                    // The dictionary also accepts nearby whitespace. Sample actual
                    // word rectangles when checking the composed-view geometry.
                    if !rects.iter().any(|rect| {
                        x as i32 >= rect.x
                            && (x as i32) < rect.x + rect.width
                            && y as i32 >= rect.y
                            && (y as i32) < rect.y + rect.height
                    }) {
                        continue;
                    }
                    let source_page = reading.probe.page_for(&word.start).unwrap();
                    assert_eq!(map.flow(source_page), 0);
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 20);
    capture_test_screen(
        &app,
        &app.fonts,
        "READER_CUSTOM_FLOW_SCROLL_SCREENSHOT",
        false,
    );
}

#[test]
fn publisher_global_default_and_explicit_book_override_survive_reopen() {
    let fixture = Fixture::new();
    crate::app::nonlinear_tests::authored_book(&fixture.book);
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.activate(Action::SetReadingControls(ReadingControls {
        hide_nonlinear_default: true,
        ..app.controls
    }))
    .unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(app.active.as_ref().unwrap().reading.config.hide_nonlinear);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    let id = app.open_book_id().unwrap();
    assert_eq!(app.library.state(id).unwrap().hide_nonlinear, None);
    app.activate(Action::ToggleNonlinear).unwrap();
    assert_eq!(app.library.state(id).unwrap().hide_nonlinear, Some(false));
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert!(reopened.controls.hide_nonlinear_default);
    assert!(
        !reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .hide_nonlinear
    );
    let old: BookState = serde_json::from_str(
        r#"{"id":4,"path":"book.epub","title":"Legacy","hide_nonlinear":true}"#,
    )
    .unwrap();
    assert_eq!(old.hide_nonlinear, Some(true));
}

#[test]
fn cross_chapter_hidden_ranges_keep_their_flow_and_previous_spread() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    mark(&mut app, 2, 1);
    mark(&mut app, 4, 0);
    app.follow_reflow_page(2, 0).unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 4);
    turn(&mut app, false);
    assert_eq!(
        (
            app.active.as_ref().unwrap().reading.chapter,
            app.active.as_ref().unwrap().reading.page
        ),
        (2, 0)
    );
    let count = app.active.as_ref().unwrap().reading.probe.page_count();
    app.follow_reflow_page(2, count - 1).unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 3);
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 4);
    app.activate(Action::FacingPages(true)).unwrap();
    app.follow_reflow_page(3, 0).unwrap();
    turn(&mut app, false);
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.chapter, 2);
    assert_eq!(reading.last_visible_page(), reading.probe.page_count() - 1);
    assert_eq!(reading.visible_columns().count(), 2);
}

#[test]
fn a_fully_hidden_book_stays_open_and_can_clear_or_disable_markers() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    mark(&mut app, 0, 0);
    app.follow_reflow_page(1, 0).unwrap();
    let id = app.open_book_id().unwrap();
    let mut fresh = Reading::open(app.active.as_ref().unwrap().reading.config.clone()).unwrap();
    assert!(fresh.custom_flow_map().unwrap().flow(fresh.page) != 0);
    assert!(fresh.jump_progress(0.5).is_err());
    assert!(app.library.state(id).unwrap().hidden_flows.points().len() == 1);
    app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .hidden_flows
            .enabled
    );
    assert_eq!(
        app.library.state(id).unwrap().hidden_flows.points().len(),
        1
    );
    app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
    app.activate(Action::Flow(FlowAction::ClearAll)).unwrap();
    confirm(&mut app);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .custom_flow_map()
            .unwrap()
            .flow(0),
        0
    );
    assert!(
        app.library
            .state(id)
            .unwrap()
            .hidden_flows
            .points()
            .is_empty()
    );
}

#[test]
#[ignore = "reads authorized private original EPUBs using READER_CUSTOM_FLOW_ORIGINALS"]
fn custom_flow_original_epubs_keep_source_selections_and_resume_after_repagination() {
    let root = PathBuf::from(std::env::var_os("READER_CUSTOM_FLOW_ORIGINALS").unwrap());
    let mut receipt = Vec::new();
    for (number, chapter) in [(36, 3), (45, 9), (68, 1)] {
        let fixture = Fixture::new();
        let path = root.join(format!("{number:03}.epub"));
        let before = fs::read(&path).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
        app.follow_reflow_page(chapter, 0).unwrap();
        let count = app.active.as_ref().unwrap().reading.probe.page_count();
        assert!(
            count >= 4,
            "Original {number} chapter {chapter} has fewer than four pages"
        );
        mark(&mut app, chapter, 1);
        mark(&mut app, chapter, 3);
        app.follow_reflow_page(chapter, 0).unwrap();
        turn(&mut app, true);
        assert_eq!(app.active.as_ref().unwrap().reading.page, 3);
        turn(&mut app, false);
        assert_eq!(app.active.as_ref().unwrap().reading.page, 0);
        app.follow_reflow_page(chapter, 1).unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        let anchor = reading.probe.page_anchor(1).unwrap();
        assert_eq!(reading.probe.page_for(&anchor), Some(1));
        let mut selected = None;
        for y in (100..PAGE_HEIGHT - 100).step_by(23) {
            for x in (70..WIDTH - 70).step_by(37) {
                if let Some(word) = reading.word_at(x as f32, y as f32) {
                    if reading.probe.page_for(&word.start) == Some(1)
                        && reading.highlight_rects(&word).iter().any(|rect| {
                            x as i32 >= rect.x
                                && (x as i32) < rect.x + rect.width
                                && y as i32 >= rect.y
                                && (y as i32) < rect.y + rect.height
                        })
                    {
                        selected = Some(word);
                        break;
                    }
                }
            }
            if selected.is_some() {
                break;
            }
        }
        let selected = selected.expect("The original hidden source page has no selectable word");
        assert_eq!(reading.probe.page_for(&selected.start), Some(1));
        let settings = reading.config.settings;
        app.activate(Action::Text(Settings {
            font_size: settings.font_size + 6.0,
            logical_font_size: None,
            ..settings
        }))
        .unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(
            reading
                .probe
                .select_range(&selected.start, &selected.end)
                .unwrap(),
            selected
        );
        let page = reading.probe.page_for(&selected.start).unwrap();
        app.follow_reflow_page(chapter, page).unwrap();
        app.record_position();
        let position = app.current_position().unwrap();
        let id = app.open_book_id().unwrap();
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.current_position().unwrap().anchor, position.anchor);
        assert_eq!(
            reopened
                .library
                .state(id)
                .unwrap()
                .hidden_flows
                .points()
                .len(),
            2
        );
        reopened.activate(Action::ReadingMode(true)).unwrap();
        reopened.follow_reflow_page(chapter, 0).unwrap();
        for _ in 0..3 {
            turn(&mut reopened, true);
        }
        let reading = &reopened.active.as_ref().unwrap().reading;
        let map = reading.custom_flow_map().unwrap();
        for part in reading.visible_parts() {
            assert_eq!(map.flow(reading.probe.page_at(part.range.start)), 0);
            assert_eq!(
                map.flow(
                    reading
                        .probe
                        .page_at((part.range.end - 0.01).max(part.range.start))
                ),
                0
            );
        }
        assert!(
            before == fs::read(&path).unwrap(),
            "Reading changed original EPUB {number}"
        );
        receipt.push(serde_json::json!({"original":number,"chapter":chapter,"source_pages":count,
            "markers":2,"source_unchanged":true,"repagination_selection":true,"resume":true,"continuous":true}));
    }
    if let Some(path) = std::env::var_os("READER_CUSTOM_FLOW_ORIGINAL_RECEIPT") {
        fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "reads authorized private PDF originals using READER_CUSTOM_FLOW_PDFS"]
fn custom_flow_original_pdfs_keep_source_text_and_skip_hidden_pages_in_text_and_scroll() {
    let root = PathBuf::from(std::env::var_os("READER_CUSTOM_FLOW_PDFS").unwrap());
    let mut receipt = Vec::new();
    for number in [30, 54] {
        let fixture = Fixture::new();
        let path = root.join(format!("{number:03}.pdf"));
        let before = fs::read(&path).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&path).unwrap();
        app.activate(Action::Flow(FlowAction::Toggle)).unwrap();
        let count = app.fixed.as_ref().unwrap().reading.count();
        assert!(count > 10);
        // Choose actual text-bearing pages after any cover/front matter.
        let mut start = None;
        for page in 3..20 {
            if !app
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .page_text(page)
                .unwrap()
                .words
                .is_empty()
                && !app
                    .fixed
                    .as_ref()
                    .unwrap()
                    .reading
                    .page_text(page + 3)
                    .unwrap()
                    .words
                    .is_empty()
            {
                start = Some(page);
                break;
            }
        }
        let start = start.expect("The original has no selectable text in its opening pages");
        for page in [start + 1, start + 3] {
            app.follow_fixed(page, None).unwrap();
            app.activate(Action::Flow(FlowAction::Here)).unwrap();
            confirm(&mut app);
        }
        app.follow_fixed(start, None).unwrap();
        turn(&mut app, true);
        assert_eq!(app.fixed.as_ref().unwrap().reading.page(), start + 3);
        app.follow_fixed(start + 1, None).unwrap();
        let original_text = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .page_text(start + 1)
            .unwrap()
            .text;
        assert!(!original_text.trim().is_empty());
        app.record_position();
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, fonts).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.fixed.as_ref().unwrap().reading.page(), start + 1);
        assert_eq!(
            reopened
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .page_text(start + 1)
                .unwrap()
                .text,
            original_text
        );
        reopened.follow_fixed(start, None).unwrap();
        reopened.activate(Action::ReadFixedText).unwrap();
        let mut changed = false;
        for _ in 0..128 {
            reopened.input(Input::PageForward);
            if let Some(due) = reopened.text_flow.as_ref().and_then(TextFlow::deadline) {
                reopened.tick(due + Duration::from_millis(1));
            }
            let page = reopened.text_flow.as_ref().unwrap().document_page();
            assert!(![start + 1, start + 2].contains(&page));
            if page != start {
                assert_eq!(page, start + 3);
                changed = true;
                break;
            }
        }
        assert!(changed);
        reopened.show(Screen::Fixed);
        reopened.follow_fixed(start, None).unwrap();
        reopened.activate(Action::FixedScroll(true)).unwrap();
        let mut saw_next = false;
        for _ in 0..8 {
            let visible = reopened
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .visible_pages()
                .collect::<Vec<_>>();
            assert!(
                visible
                    .iter()
                    .all(|page| ![start + 1, start + 2].contains(page))
            );
            saw_next |= visible.iter().any(|&page| page >= start + 3);
            turn(&mut reopened, true);
        }
        assert!(saw_next);
        assert!(
            before == fs::read(&path).unwrap(),
            "Reading changed original PDF {number}"
        );
        receipt.push(
            serde_json::json!({"original":number,"source_pages":count,"first_tested":start,
            "markers":2,"source_unchanged":true,"resume":true,"text_view":true,"continuous":true}),
        );
    }
    if let Some(path) = std::env::var_os("READER_CUSTOM_FLOW_PDF_RECEIPT") {
        fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}

#[test]
fn inactive_boundaries_are_retained_until_their_confirmed_removal() {
    use crate::library::hidden_flows::FlowPoint;
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.follow_reflow_page(2, 0).unwrap();
    let mut data = app.hidden_flow_data().unwrap().clone();
    for (page, hidden) in [(1, true), (2, true), (3, false)] {
        data.insert(FlowPoint {
            position: ReadingPosition {
                chapter: 2,
                page,
                anchor: app.active.as_ref().unwrap().reading.probe.page_anchor(page),
            },
            hidden,
        })
        .unwrap();
    }
    app.apply_hidden_flow_data(data).unwrap();
    assert_eq!(app.inactive_flow_markers(), [1]);
    app.activate(Action::Flow(FlowAction::ClearInactive))
        .unwrap();
    assert!(matches!(app.flow_popup, Some(FlowPopup::ClearInactive)));
    let cancel = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Cancel);
    let (x, y) = center(cancel);
    app.input(Input::Tap(x, y));
    assert_eq!(app.hidden_flow_data().unwrap().points().len(), 3);
    app.activate(Action::Flow(FlowAction::ClearInactive))
        .unwrap();
    confirm(&mut app);
    assert_eq!(app.hidden_flow_data().unwrap().points().len(), 2);
    assert!(app.inactive_flow_markers().is_empty());
    assert!(app.active.as_ref().unwrap().reading.hidden_chapter(2) == Some(false));
    assert!(
        app.book_map_items()[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("Includes hidden pages")
    );
}

#[test]
fn an_unavailable_flow_boundary_remains_reported_without_moving_the_book() {
    use crate::library::hidden_flows::FlowPoint;
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.follow_reflow_page(2, 0).unwrap();
    let before = app.current_position().unwrap();
    let mut data = app.hidden_flow_data().unwrap().clone();
    data.insert(FlowPoint {
        position: ReadingPosition {
            chapter: usize::MAX,
            page: 0,
            anchor: None,
        },
        hidden: true,
    })
    .unwrap();
    app.apply_hidden_flow_data(data).unwrap();
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .unresolved_flow_markers(),
        [0]
    );
    assert!(
        app.hidden_flow_items()
            .last()
            .unwrap()
            .detail
            .as_deref()
            .unwrap()
            .contains("Location unavailable")
    );
    assert!(app.activate(Action::Flow(FlowAction::Open(0))).is_err());
    assert_eq!(app.current_position().unwrap().anchor, before.anchor);
    assert_eq!(app.hidden_flow_data().unwrap().points().len(), 1);
}

#[test]
fn editing_source_boundaries_reuses_the_current_chapter_layout_and_location() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.follow_reflow_page(2, 0).unwrap();
    let source = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .source
        .blocks
        .as_ptr();
    let anchor = app.current_position().unwrap().anchor;
    for action in [
        FlowAction::Edit,
        FlowAction::Edit,
        FlowAction::Toggle,
        FlowAction::Toggle,
    ] {
        app.activate(Action::Flow(action)).unwrap();
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .probe
                .source
                .blocks
                .as_ptr(),
            source
        );
        assert_eq!(app.current_position().unwrap().anchor, anchor);
    }
    mark(&mut app, 2, 1);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .source
            .blocks
            .as_ptr(),
        source
    );
    assert_eq!(app.current_position().unwrap().anchor, anchor);
    mark(&mut app, 2, 3);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .probe
            .source
            .blocks
            .as_ptr(),
        source
    );
    app.follow_reflow_page(2, 0).unwrap();
    turn(&mut app, true);
    assert_eq!(app.active.as_ref().unwrap().reading.page, 3);
}

#[test]
fn hidden_flow_numbers_distinguish_separate_notes_and_renumber_after_removal() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    for page in [0, 1, 3, 4] {
        mark(&mut app, 2, page);
    }
    app.follow_reflow_page(2, 3).unwrap();
    assert!(app.status_caption().unwrap().starts_with("Hidden 2 ·"));
    app.follow_reflow_page(2, 0).unwrap();
    assert!(app.status_caption().unwrap().starts_with("Hidden 1 ·"));
    let mut data = app.hidden_flow_data().unwrap().clone();
    data.remove(0).unwrap();
    app.apply_hidden_flow_data(data).unwrap();
    app.follow_reflow_page(2, 3).unwrap();
    assert!(app.status_caption().unwrap().starts_with("Hidden 1 ·"));
}
