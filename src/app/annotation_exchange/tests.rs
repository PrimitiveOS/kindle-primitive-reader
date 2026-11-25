use super::*;
use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item, tap_reading_control};

fn fonts() -> Vec<PathBuf> {
    ["NotoSans-Regular.ttf", "NotoSans-Bold.ttf"]
        .map(|name| crate::test_fixture(&format!("fonts/{name}")))
        .to_vec()
}

fn setup(fixture: &Fixture) -> Reader {
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app
}

fn settings(app: &mut Reader) {
    // Force the navigation section before selecting Settings so repeated
    // sessions exercise the same controls even when Settings was remembered.
    app.open_reading_edge(true, 130.);
    assert_eq!(tap_reading_control(app, "Settings"), Change::Partial);
    assert_eq!(
        tap_reading_control(app, "Bookmarks and notes"),
        Change::Partial
    );
    tap_contents_item(app, "Annotation exchange");
    assert_eq!(app.screen, Screen::AnnotationExchange);
}

fn add_epub_marks(app: &mut Reader) {
    app.activate(Action::AddBookmark).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.fragment("lantern").unwrap();
    let mut end = start.clone();
    end.offset += "The copper lantern".len();
    app.selection = reading.probe.select_range(&start, &end);
    app.save_highlight(Some(
        "Steel & blue — keep \"this\".\nUse the second shelf.".into(),
    ))
    .unwrap();
}

fn peer(snapshot: &mut exchange::Snapshot) {
    snapshot.device = "authored-koreader-peer".into();
    snapshot.datetime -= 120;
    for mark in &mut snapshot.marks {
        mark.created = Some(snapshot.datetime - 100);
        mark.updated = None;
    }
}

fn stock_book_path(fixture: &Fixture) -> PathBuf {
    fixture
        .book
        .with_extension("sdr")
        .join("Field Notes.epub.annotations.lua")
}

fn save_peer(path: &Path, transfer: &exchange::Snapshot) {
    fs::write(path, exchange::encode(transfer).unwrap()).unwrap();
}

#[test]
fn annotation_exchange_settings_switch_closing_and_preserve_book_bytes() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    let original = fs::read(&fixture.book).unwrap();
    let id = app.open_book_id().unwrap();
    add_epub_marks(&mut app);
    assert_eq!(app.exit(), Change::Exit);
    assert!(!stock_book_path(&fixture).exists());
    app.open_path(&fixture.book).unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Export annotations on book closing");
    tap_contents_item(&mut app, "Keep all annotations on import");
    let folder = fixture.root.join("Shared notes");
    fs::create_dir(&folder).unwrap();
    tap_contents_item(&mut app, "Export / import folder");
    tap_contents_item(&mut app, "Shared notes");
    tap_contents_item(&mut app, "Use this folder");
    let custom = folder.join("Field Notes.epub.annotations.lua");
    let expected = app.current_position().unwrap();
    capture_test_screen(&app, &fonts(), "READER_EXCHANGE_CAPTURE_SETTINGS", false);
    assert_eq!(app.input(Input::Sleep), Change::None);
    assert!(!custom.exists());
    app.input(Input::Wake);
    let damaged = fixture.root.join("Damaged.epub");
    fs::write(&damaged, b"not an EPUB").unwrap();
    assert!(app.open_path(&damaged).is_err());
    assert!(!custom.exists());
    assert_eq!(app.current_position(), Some(expected.clone()));
    app.open_path(&fixture.book).unwrap();
    assert!(!custom.exists());
    app.activate(Action::Show(Screen::Library)).unwrap();
    assert_eq!(app.screen, Screen::Library, "{:?}", app.notice);
    let transfer = exchange::decode(&fs::read(&custom).unwrap()).unwrap();
    assert_eq!((transfer.marks.len(), transfer.fixed), (2, false));
    assert!(
        transfer
            .marks
            .iter()
            .find_map(|mark| mark.highlight.as_ref())
            .unwrap()
            .note
            .as_deref()
            .unwrap()
            .contains("Steel & blue")
    );
    assert!(!stock_book_path(&fixture).exists());
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    if let Some(out) = std::env::var_os("READER_EXCHANGE_EPUB_ORACLE") {
        fs::copy(&custom, PathBuf::from(out)).unwrap();
    }

    let mut app = setup(&fixture);
    assert_eq!(app.open_book_id(), Some(id));
    assert_eq!(app.current_position(), Some(expected));
    settings(&mut app);
    assert_eq!(
        app.annotation_exchange.toggle(Command::ToggleOnClose),
        Some(true)
    );
    assert_eq!(
        app.annotation_exchange.toggle(Command::ToggleKeepAll),
        Some(true)
    );
    tap_contents_item(&mut app, "Use book metadata folder");
    assert_eq!(app.exit(), Change::Exit);
    assert!(stock_book_path(&fixture).exists());
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn annotation_exchange_keep_all_and_default_replace_older_unmatched_marks_only() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    let original = fs::read(&fixture.book).unwrap();
    add_epub_marks(&mut app);
    settings(&mut app);
    tap_contents_item(&mut app, "Export annotation transfer");
    let path = stock_book_path(&fixture);
    let mut transfer = exchange::decode(&fs::read(&path).unwrap()).unwrap();
    peer(&mut transfer);
    transfer.marks.retain(|mark| mark.highlight.is_some());
    transfer.marks[0].highlight.as_mut().unwrap().note = Some("Peer’s note".into());
    transfer.marks[0].updated = Some(transfer.datetime - 20);
    let id = app.open_book_id().unwrap();
    {
        let state = app.library.state_mut(id).unwrap();
        state.bookmarks[0].created_at = Some(transfer.datetime - 200);
        state.highlights[0].created_at = Some(transfer.datetime - 200);
    }
    let old = app.current_position();
    let back = app.back.clone();
    let note = app.library.state(id).unwrap().highlights[0].note.clone();
    save_peer(&path, &transfer);
    let transfer_bytes = fs::read(&path).unwrap();

    tap_contents_item(&mut app, "Keep all annotations on import");
    tap_contents_item(&mut app, "Import annotation transfer");
    assert_eq!(
        app.annotation_exchange.preview.as_ref().unwrap().summary,
        Summary {
            updated: 1,
            ..Default::default()
        }
    );
    assert_eq!(app.library.state(id).unwrap().highlights[0].note, note);
    app.input(Input::Tap((WIDTH - 90) as f32, 85.));
    assert_eq!(app.screen, Screen::AnnotationExchange);

    tap_contents_item(&mut app, "Keep all annotations on import");
    tap_contents_item(&mut app, "Import annotation transfer");
    assert_eq!(
        app.annotation_exchange.preview.as_ref().unwrap().summary,
        Summary {
            updated: 1,
            removed: 1,
            ..Default::default()
        }
    );
    capture_test_screen(&app, &fonts(), "READER_EXCHANGE_CAPTURE_PREVIEW", false);
    let blocked = fixture.state.with_extension("json.tmp");
    fs::create_dir(&blocked).unwrap();
    assert!(app.apply_annotation_exchange().is_err());
    assert_eq!(app.library.state(id).unwrap().bookmarks.len(), 1);
    assert_eq!(app.library.state(id).unwrap().highlights[0].note, note);
    fs::remove_dir(&blocked).unwrap();
    tap_contents_item(&mut app, "Import and remove older marks");
    let button = app
        .annotation_exchange_sheet()
        .unwrap()
        .button(SheetAction::Confirm);
    assert_eq!(
        app.input(Input::Tap(
            (button.x + button.w / 2) as f32,
            (button.y + button.h / 2) as f32
        )),
        Change::Partial
    );

    let state = app.library.state(id).unwrap();
    assert!(state.bookmarks.is_empty());
    assert_eq!(state.highlights[0].note.as_deref(), Some("Peer’s note"));
    assert_eq!(state.highlights[0].updated_at, transfer.marks[0].updated);
    assert_eq!(app.current_position(), old);
    assert_eq!(app.back, back);
    assert_eq!(fs::read(&path).unwrap(), transfer_bytes);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
    tap_contents_item(&mut app, "Import annotation transfer");
    assert!(app.annotation_exchange.preview.as_ref().unwrap().imported);
    let reopened = setup(&fixture);
    assert_eq!(reopened.library.state(id).unwrap().highlights.len(), 1);
}

#[test]
fn annotation_exchange_unmatched_files_preserve_other_transfer_and_native_marks() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    add_epub_marks(&mut app);
    settings(&mut app);
    tap_contents_item(&mut app, "Export annotation transfer");
    let path = stock_book_path(&fixture);
    let mut transfer = exchange::decode(&fs::read(&path).unwrap()).unwrap();
    peer(&mut transfer);
    transfer.marks.retain(|mark| mark.highlight.is_some());
    transfer.marks[0].text = "Mismatch in another book".into();
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().bookmarks[0].created_at = Some(transfer.datetime - 20);
    save_peer(&path, &transfer);
    let previous = fs::read(&path).unwrap();
    tap_contents_item(&mut app, "Import annotation transfer");
    assert_eq!(
        app.annotation_exchange.preview.as_ref().unwrap().summary,
        Summary {
            unavailable: 1,
            ..Default::default()
        }
    );
    assert_eq!(app.library.state(id).unwrap().bookmarks.len(), 1);
    assert!(app.annotation_exchange_command(Command::Export).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(app.library.state(id).unwrap().bookmarks.len(), 1);
}

#[test]
fn annotation_exchange_round_trips_fixed_page_quotes_and_original_rectangles() {
    let sender = Fixture::new();
    let source = sender.root.join("Chart.pdf");
    let content = "BT /F1 14 Tf 55 60 Td (PDF) Tj 0 -18 Td (Notes) Tj ET\n";
    let original = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents 4 0 R >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ]);
    fs::write(&source, &original).unwrap();
    let mut app = sender.app();
    app.open_path(&source).unwrap();
    app.activate(Action::AddBookmark).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let word = fixed.reading.page_text(0).unwrap().words[0].bounds;
    let id = fixed.id;
    app.library
        .state_mut(id)
        .unwrap()
        .add_pdf_highlight(PdfHighlight {
            updated_at: None,
            chapter_title: None,
            created_at: Some(1_740_777_900),
            style: crate::highlight_style::HighlightStyle::Underline,
            color: crate::highlight_style::HighlightColor::Yellow,
            page: 0,
            bounds: vec![word],
            text: "PDF".into(),
            note: Some("Check the chart".into()),
            continuation: Vec::new(),
        })
        .unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Export annotation transfer");
    let file = source
        .with_extension("sdr")
        .join("Chart.pdf.annotations.lua");
    let wire = fs::read(&file).unwrap();
    assert!(exchange::decode(&wire).unwrap().fixed);
    assert_eq!(fs::read(&source).unwrap(), original);
    if let Some(directory) = std::env::var_os("READER_EXCHANGE_FIXED_ORACLE") {
        let directory = PathBuf::from(directory);
        fs::copy(&source, directory.join("Chart.pdf")).unwrap();
        fs::copy(&file, directory.join("Chart.pdf.annotations.lua")).unwrap();
    }

    let receiver = Fixture::new();
    let pdf = receiver.root.join("Chart.pdf");
    fs::write(&pdf, &original).unwrap();
    let sidecar = pdf.with_extension("sdr");
    fs::create_dir(&sidecar).unwrap();
    fs::write(sidecar.join("Chart.pdf.annotations.lua"), &wire).unwrap();
    let mut app = receiver.app();
    app.open_path(&pdf).unwrap();
    let position = app.current_position();
    settings(&mut app);
    tap_contents_item(&mut app, "Import annotation transfer");
    assert_eq!(
        app.annotation_exchange.preview.as_ref().unwrap().summary,
        Summary {
            added: 2,
            ..Default::default()
        }
    );
    tap_contents_item(&mut app, "Apply import");
    let state = app.library.state(id).unwrap();
    assert_eq!(state.bookmarks.len(), 1);
    let mark = &state.pdf_highlights[0];
    assert_eq!(mark.note.as_deref(), Some("Check the chart"));
    let found = mark.bounds[0];
    assert!(
        [
            found.x0 - word.x0,
            found.y0 - word.y0,
            found.x1 - word.x1,
            found.y1 - word.y1
        ]
        .into_iter()
        .all(|delta| delta.abs() <= 0.000001)
    );
    assert_eq!(app.current_position(), position);
    assert_eq!(fs::read(&pdf).unwrap(), original);
}

#[test]
#[ignore = "Set READER_EXCHANGE_ORIGINAL_EPUB to an authorized, unmodified original EPUB"]
fn annotation_exchange_original_book_keeps_same_bytes_and_passage() {
    let source = PathBuf::from(std::env::var_os("READER_EXCHANGE_ORIGINAL_EPUB").unwrap());
    let bytes = fs::read(&source).unwrap();
    let fixture = Fixture::new();
    fs::write(&fixture.book, &bytes).unwrap();
    let mut app = setup(&fixture);
    let mut original = Book::open(&fixture.book).unwrap();
    let chosen = (0..original.chapters.len().min(12))
        .find_map(|chapter| {
            let reading = &mut app.active.as_mut()?.reading;
            reading.go_to_page(chapter, 0).ok()?;
            reading
                .probe
                .source
                .blocks
                .iter()
                .take(48)
                .find_map(|block| {
                    let word = block.text.split_whitespace().find(|word| {
                        word.chars().count() >= 3 && word.chars().all(char::is_alphabetic)
                    })?;
                    let at = block.text.find(word)?;
                    let start = reading.probe.anchor(block.key, at);
                    let end = reading.probe.anchor(block.key, at + word.len());
                    crate::cre_pointer::encode(&mut original, &start, false).ok()?;
                    crate::cre_pointer::encode(&mut original, &end, true).ok()?;
                    reading.probe.select_range(&start, &end)
                })
        })
        .expect("Original publication needs a verifiable source range");
    // Direct fixture positioning skips the event-loop checkpoint used by
    // page turns and chapter jumps.
    app.record_position();
    let location = app.current_position().unwrap();
    app.selection = Some(chosen.clone());
    app.save_highlight(Some("Authorized local transfer test".into()))
        .unwrap();
    settings(&mut app);
    tap_contents_item(&mut app, "Export annotations on book closing");
    assert_eq!(app.exit(), Change::Exit);
    let encoded = exchange::decode(&fs::read(stock_book_path(&fixture)).unwrap()).unwrap();
    assert_eq!(encoded.marks.len(), 1);
    assert_eq!(encoded.marks[0].text, chosen.text);
    assert_eq!(
        encoded.marks[0].highlight.as_ref().unwrap().note.as_deref(),
        Some("Authorized local transfer test")
    );
    assert!(fs::read(&fixture.book).unwrap() == bytes);
    assert!(fs::read(&source).unwrap() == bytes);
    let reopened = setup(&fixture);
    assert_eq!(reopened.current_position(), Some(location));
}

#[test]
fn annotation_exchange_preserves_multi_page_pdf_marks_and_unmatched_local_data() {
    let fixture = Fixture::new();
    let source = fixture.root.join("Two pages.pdf");
    let content = "BT /F1 14 Tf 20 45 Td (PDF) Tj ET\n";
    let original = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] /Resources << /Font << /F1 6 0 R >> >> >>"
            .to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents 5 0 R >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents 5 0 R >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ]);
    fs::write(&source, &original).unwrap();
    let mut app = fixture.app();
    app.open_path(&source).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let id = fixed.id;
    let first = fixed.reading.page_text(0).unwrap().words[0].bounds;
    let second = fixed.reading.page_text(1).unwrap().words[0].bounds;
    app.library
        .state_mut(id)
        .unwrap()
        .add_pdf_highlight(PdfHighlight {
            updated_at: None,
            chapter_title: None,
            created_at: Some(1_740_777_800),
            style: crate::highlight_style::HighlightStyle::Underline,
            color: crate::highlight_style::HighlightColor::Yellow,
            page: 0,
            bounds: vec![first],
            text: "PDF\nPDF".into(),
            note: Some("Both pages".into()),
            continuation: vec![crate::library::HighlightPage {
                page: 1,
                bounds: vec![second],
                text_offset: 4,
            }],
        })
        .unwrap();
    let state = app.library.state(id).unwrap();
    let (marks, unavailable) = app.export_transfer_marks(&source, state).unwrap();
    assert_eq!(unavailable, 0);
    let snapshot = exchange::Snapshot {
        device: "authored-peer".into(),
        fixed: true,
        datetime: 1_740_777_900,
        fingerprint: None,
        incomplete: false,
        unavailable: 0,
        marks,
    };
    let wire = exchange::encode(&snapshot).unwrap();
    let parsed = exchange::decode(wire.as_bytes()).unwrap();
    let mut restored = state.clone();
    restored.pdf_highlights.clear();
    let summary = app
        .apply_transfer(&source, parsed.clone(), &mut restored, true)
        .unwrap();
    assert_eq!(summary.added, 1);
    let mark = &restored.pdf_highlights[0];
    assert_eq!(mark.text, "PDF\nPDF");
    assert_eq!(mark.note.as_deref(), Some("Both pages"));
    assert_eq!(mark.continuation.len(), 1);
    assert_eq!(mark.continuation[0].page, 1);
    assert_eq!(mark.continuation[0].text_offset, 4);
    let mut bad = parsed;
    bad.marks[0].text = "different words".into();
    let before = restored.pdf_highlights.clone();
    let result = app
        .apply_transfer(&source, bad, &mut restored, false)
        .unwrap();
    assert_eq!(result.unavailable, 1);
    assert_eq!(restored.pdf_highlights, before);
    assert_eq!(fs::read(&source).unwrap(), original);
}
