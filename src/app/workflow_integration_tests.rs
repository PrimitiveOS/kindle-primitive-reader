//! Regression workflows spanning navigation, source masks, export and typography.
use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::library::{Highlight, PdfHighlight};
use primitive_ui::{Sheet, SheetAction};

fn confirm(app: &mut Reader) {
    let rect = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Confirm);
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn mark(app: &mut Reader, chapter: Option<usize>, page: usize) {
    app.activate(Action::Overview).unwrap();
    app.update_overview(chapter, Some(page)).unwrap();
    let index = page - app.overview.as_ref().unwrap().first;
    let rect = overview_tile(index);
    assert_eq!(
        app.input(Input::LongTap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
    confirm(app);
}

fn read_browser_mark(app: &mut Reader, note: &str) {
    use bookmark_browser::{Command, Source};
    let before = app.current_history_location().unwrap();
    app.activate_bookmark_browser(Command::Open).unwrap();
    app.activate_bookmark_browser(Command::Use(Source::History))
        .unwrap();
    let count = app.list_window().0;
    let action = (0..count.div_ceil(ROWS_PER_PAGE))
        .find_map(|page| {
            app.list_page = page;
            app.list_window()
                .1
                .into_iter()
                .find(|row| row.title == note)
                .map(|row| row.action)
        })
        .expect("The saved note was omitted from Bookmark browser");
    let Action::BookmarkBrowser(Command::Inspect(key)) = action else {
        panic!("Note did not open a saved-mark preview");
    };
    app.activate(action).unwrap();
    assert_eq!(
        app.screen,
        Screen::BookmarkBrowser(bookmark_browser::Page::Detail)
    );
    assert_eq!(app.current_history_location(), Some(before));
    app.activate_bookmark_browser(Command::ViewMark(key))
        .unwrap();
}

fn settle_export(app: &mut Reader) {
    for _ in 0..1000 {
        let Some(due) = app.export_deadline() else {
            return;
        };
        app.tick(due);
    }
    panic!("The background export did not finish");
}

fn masks_cover_only_regular_source(reading: &Reading) {
    let map = reading.custom_flow_map().unwrap();
    assert!(!reading.visible_parts().is_empty());
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
}

fn run_browser_and_export(epub: Option<&Path>, pdf_original: Option<&Path>, chapter: usize) {
    let fixture = Fixture::new();
    if let Some(path) = epub {
        fs::copy(path, &fixture.book).unwrap();
    } else {
        nonlinear_tests::authored_book(&fixture.book);
    }
    let pdf = fixture.root.join("Survey sheets.pdf");
    if let Some(path) = pdf_original {
        fs::copy(path, &pdf).unwrap();
    } else {
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    }
    let before_epub = fs::read(&fixture.book).unwrap();
    let before_pdf = fs::read(&pdf).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(chapter, 0).unwrap();
    app.activate(Action::Flow(hidden_flows::FlowAction::Toggle))
        .unwrap();
    mark(&mut app, Some(chapter), 1);
    mark(&mut app, Some(chapter), 3);
    app.follow_reflow_page(chapter, 0).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.page_anchor(0).unwrap();
    let end = reading.probe.page_anchor(3).unwrap();
    let spanning = reading.probe.select_range(&start, &end).unwrap();
    assert!(!spanning.text.is_empty() && spanning.text.len() <= 8192);
    let hidden_start = reading.probe.page_anchor(1).unwrap();
    let hidden_end = reading.probe.page_anchor(2).unwrap();
    let hidden = reading
        .probe
        .select_range(&hidden_start, &hidden_end)
        .unwrap();
    assert!(spanning.text.contains(&hidden.text));
    let epub_id = app.open_book_id().unwrap();
    app.library.state_mut(epub_id).unwrap().highlights.extend([
        Highlight {
            selection: spanning.clone(),
            note: Some("Spanning reading flow".into()),
            style: crate::highlight_style::HighlightStyle::Underline,
            color: Default::default(),
            created_at: Some(1_740_777_900),
        },
        Highlight {
            selection: hidden.clone(),
            note: Some("Hidden EPUB passage".into()),
            style: crate::highlight_style::HighlightStyle::Lighten,
            color: Default::default(),
            created_at: None,
        },
    ]);
    let origin = app.current_history_location().unwrap();
    read_browser_mark(&mut app, "Hidden EPUB passage");
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.chapter, chapter);
    assert_ne!(reading.custom_flow_map().unwrap().flow(reading.page), 0);
    assert_eq!(reading.anchor(), Some(hidden.start.clone()));
    assert!(!reading.highlight_rects(&hidden).is_empty());
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(origin));
    app.follow_reflow_page(chapter, 1).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    app.library
        .state_mut(epub_id)
        .unwrap()
        .bookmarks
        .last_mut()
        .unwrap()
        .label = "Hidden EPUB page bookmark".into();
    app.follow_reflow_page(chapter, 0).unwrap();
    let bookmark_origin = app.current_history_location().unwrap();
    read_browser_mark(&mut app, "Hidden EPUB page bookmark");
    let reading = &app.active.as_ref().unwrap().reading;
    assert_ne!(reading.custom_flow_map().unwrap().flow(reading.page), 0);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(bookmark_origin));

    app.open_path(&pdf).unwrap();
    let page = (1..app.fixed.as_ref().unwrap().reading.count().min(10))
        .find(|&page| {
            !app.fixed
                .as_ref()
                .unwrap()
                .reading
                .page_text(page)
                .unwrap()
                .words
                .is_empty()
        })
        .unwrap();
    app.activate(Action::Flow(hidden_flows::FlowAction::Toggle))
        .unwrap();
    mark(&mut app, None, page);
    if page + 1 < app.fixed.as_ref().unwrap().reading.count() {
        mark(&mut app, None, page + 1);
    }
    let reading = &app.fixed.as_ref().unwrap().reading;
    let layer = reading.page_text(page).unwrap();
    let word = layer.words.first().unwrap();
    let pdf_id = app.open_book_id().unwrap();
    let pdf_text = layer.text[word.bytes.clone()].to_owned();
    app.library
        .state_mut(pdf_id)
        .unwrap()
        .add_pdf_highlight(PdfHighlight {
            page,
            bounds: vec![word.bounds],
            continuation: Vec::new(),
            text: pdf_text.clone(),
            note: Some("Hidden PDF passage".into()),
            style: crate::highlight_style::HighlightStyle::Strikethrough,
            color: Default::default(),
            created_at: None,
        })
        .unwrap();
    app.follow_fixed(page - 1, None).unwrap();
    let origin = app.current_history_location().unwrap();
    read_browser_mark(&mut app, "Hidden PDF passage");
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), page);
    assert_ne!(reading.hidden_flow_map().unwrap().flow(page), 0);
    assert!(reading.page_text(page).unwrap().text.contains(&pdf_text));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    app.follow_fixed(page, None).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    app.library
        .state_mut(pdf_id)
        .unwrap()
        .bookmarks
        .last_mut()
        .unwrap()
        .label = "Hidden PDF page bookmark".into();
    app.follow_fixed(page - 1, None).unwrap();
    let bookmark_origin = app.current_history_location().unwrap();
    read_browser_mark(&mut app, "Hidden PDF page bookmark");
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), page);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), Some(bookmark_origin));
    app.save().unwrap();
    let before_state = fs::read(&fixture.state).unwrap();
    use library_export::{Command, Format};
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle_export(&mut app);
    let export = fs::read(app.exports.join("library-highlights.json")).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&export).unwrap();
    let documents = json["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 2);
    let epub = documents
        .iter()
        .find(|book| book["primitiveos_book_id"] == format!("{epub_id:016x}"))
        .unwrap();
    let entries = epub["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let cross = entries
        .iter()
        .find(|entry| entry["note"] == "Spanning reading flow")
        .unwrap();
    assert_eq!(cross["text"], spanning.text);
    assert!(cross["text"].as_str().unwrap().contains(&hidden.text));
    assert_eq!(
        cross["primitiveos"]["start"],
        serde_json::to_value(&spanning.start).unwrap()
    );
    assert_eq!(
        cross["primitiveos"]["end"],
        serde_json::to_value(&spanning.end).unwrap()
    );
    let pdf = documents
        .iter()
        .find(|book| book["primitiveos_book_id"] == format!("{pdf_id:016x}"))
        .unwrap();
    assert_eq!(pdf["entries"][0]["text"], pdf_text);
    assert_eq!(pdf["entries"][0]["page"], page + 1);
    assert_eq!(
        crate::koreader_import::parse(&export)
            .unwrap()
            .documents
            .len(),
        2
    );
    assert_eq!(app.current_history_location(), Some(origin));
    assert_eq!(fs::read(&fixture.state).unwrap(), before_state);
    assert_eq!(fs::read(&fixture.book).unwrap(), before_epub);
    assert_eq!(
        fs::read(fixture.root.join("Survey sheets.pdf")).unwrap(),
        before_pdf
    );
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened.library.state(epub_id).unwrap().highlights[0].selection,
        spanning
    );
    assert!(
        reopened
            .library
            .state(epub_id)
            .unwrap()
            .hidden_flows
            .enabled
    );
}

#[test]
fn workflow_hidden_ranges_bookmark_browser_and_cross_book_export_preserve_full_source() {
    run_browser_and_export(None, None, 2);
}

#[test]
#[ignore = "requires READER_WORKFLOW_EPUB and READER_WORKFLOW_PDF private originals"]
fn workflow_original_hidden_ranges_browser_and_export_leave_both_documents_unchanged() {
    let epub = PathBuf::from(std::env::var_os("READER_WORKFLOW_EPUB").unwrap());
    let pdf = PathBuf::from(std::env::var_os("READER_WORKFLOW_PDF").unwrap());
    run_browser_and_export(Some(&epub), Some(&pdf), 3);
}

#[test]
fn workflow_numeric_font_controls_keep_hidden_spread_scroll_masks_and_resume_anchors() {
    let fixture = Fixture::new();
    nonlinear_tests::authored_book(&fixture.book);
    let source = fs::read(&fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(2, 0).unwrap();
    app.activate(Action::Flow(hidden_flows::FlowAction::Toggle))
        .unwrap();
    mark(&mut app, Some(2), 1);
    mark(&mut app, Some(2), 3);
    app.follow_reflow_page(2, 3).unwrap();
    let source_anchor = app.current_position().unwrap().anchor.unwrap();
    let id = app.open_book_id().unwrap();
    let boundaries = serde_json::to_value(&app.library.state(id).unwrap().hidden_flows).unwrap();
    app.activate(Action::FacingPages(true)).unwrap();
    let layout = app.active.as_ref().unwrap().reading.probe.lines.as_ptr();
    let position = app.current_position().unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let before = reading.probe.capture(reading.page).unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_gamma_index: 25,
        ..settings
    }))
    .unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(reading.probe.lines.as_ptr(), layout);
    assert_eq!(reading.anchor(), position.anchor);
    let darkened = reading.probe.capture(reading.page).unwrap();
    assert!(before.iter().zip(&darkened).any(|(old, new)| old > new));
    assert!(before.iter().zip(&darkened).all(|(old, new)| old >= new));
    masks_cover_only_regular_source(reading);
    let settings = reading.config.settings;
    app.activate(Action::Text(Settings {
        font_weight_delta: 0.75,
        ..settings
    }))
    .unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.anchor(), Some(source_anchor.clone()));
    masks_cover_only_regular_source(reading);
    app.show(Screen::Book);
    capture_test_screen(&app, &fonts, "READER_WORKFLOW_SPREAD_SCREENSHOT", false);
    app.activate(Action::FacingPages(false)).unwrap();
    app.activate(Action::ReadingMode(true)).unwrap();
    assert_eq!(
        app.current_position().unwrap().anchor,
        Some(source_anchor.clone())
    );
    masks_cover_only_regular_source(&app.active.as_ref().unwrap().reading);
    assert_eq!(
        serde_json::to_value(&app.library.state(id).unwrap().hidden_flows).unwrap(),
        boundaries
    );
    app.record_position();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    let reading = &reopened.active.as_ref().unwrap().reading;
    assert_eq!(reading.anchor(), Some(source_anchor));
    assert_eq!(reading.config.settings.font_gamma_index, 25);
    assert_eq!(reading.config.settings.font_weight_delta, 0.75);
    assert!(reading.config.scroll);
    masks_cover_only_regular_source(reading);
    capture_test_screen(
        &reopened,
        &fonts,
        "READER_WORKFLOW_SCROLL_SCREENSHOT",
        false,
    );
    let before_theme = reopened.current_history_location();
    reopened.system_theme = None;
    reopened
        .activate(Action::SetReadingControls(ReadingControls {
            night_pages: true,
            ..reopened.controls
        }))
        .unwrap();
    capture_test_screen(&reopened, &fonts, "READER_WORKFLOW_DARK_SCREENSHOT", true);
    assert_eq!(reopened.current_history_location(), before_theme);
    masks_cover_only_regular_source(&reopened.active.as_ref().unwrap().reading);
    assert_eq!(fs::read(&fixture.book).unwrap(), source);
}
