use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::library::{Highlight, PdfHighlight};
use primitive_app::App;
use primitive_ui::Input;
use std::fs;

fn tap_row(app: &mut Reader, title: &str) {
    let (count, items) = app.list_window();
    let index = items
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing {title:?} on {:?}", app.screen));
    let y = super::super::LIST_Y as f32 + (index as i32 * super::super::ROW_HEIGHT + 40) as f32;
    assert!(count >= items.len());
    assert_eq!(app.input(Input::Tap(120.0, y)), Change::Partial);
}

fn settle(app: &mut Reader) {
    for _ in 0..1000 {
        let Some(deadline) = app.export_deadline() else {
            return;
        };
        app.tick(deadline);
    }
    panic!("Export failed to finish");
}

fn save_reflow_mark(app: &mut Reader, path: &std::path::Path) -> u64 {
    app.open_path(path).unwrap();
    let id = app.open_book_id().unwrap();
    app.activate(Action::AddBookmark).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.fragment("lantern").unwrap();
    let mut end = start.clone();
    end.offset += "The copper lantern".len();
    let selection = reading.probe.select_range(&start, &end).unwrap();
    app.library
        .state_mut(id)
        .unwrap()
        .highlights
        .push(Highlight {
            created_at: Some(1_740_777_900),
            style: HighlightStyle::Underline,
            selection,
            note: Some("Keep <this> & compare [notes]".into()),
        });
    id
}

#[test]
fn exports_history_collections_and_selected_books_through_normal_controls() {
    let fixture = Fixture::new();
    let second = fixture.root.join("Earlier edition.epub");
    fs::write(
        &second,
        include_bytes!("../../../tests/fixtures/field-notes-epub2.epub"),
    )
    .unwrap();
    let pdf = fixture.root.join("Survey sheets.pdf");
    fs::write(&pdf, crate::fixed::tests::document()).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let first = save_reflow_mark(&mut app, &fixture.book);
    let second = save_reflow_mark(&mut app, &second);
    assert_ne!(first, second);
    // The export keeps separate editions even when their displayed titles agree.
    app.library.state_mut(second).unwrap().title = app.library.state(first).unwrap().title.clone();
    app.open_path(&pdf).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let pdf_id = fixed.id;
    let layer = fixed.reading.page_text(1).unwrap();
    let word = layer.words.first().unwrap();
    app.library
        .state_mut(pdf_id)
        .unwrap()
        .add_pdf_highlight(PdfHighlight {
            created_at: None,
            style: HighlightStyle::Lighten,
            page: 1,
            bounds: vec![word.bounds],
            continuation: Vec::new(),
            text: layer.text[word.bytes.clone()].into(),
            note: Some("Harbor soundings".into()),
        })
        .unwrap();
    app.library.create_collection("Field reports").unwrap();
    app.library
        .set_collection_membership("Field reports", first, true)
        .unwrap();
    app.library
        .set_collection_membership("Field reports", pdf_id, true)
        .unwrap();
    app.save().unwrap();
    let before = fs::read(&fixture.state).unwrap();
    let origin = app.current_history_location().unwrap();

    app.show(Screen::ReadingData);
    tap_row(&mut app, "Export highlights");
    tap_row(&mut app, "History");
    capture_test_screen(&app, &fonts, "READER_LIBRARY_EXPORT_SCREENSHOT", false);
    tap_row(&mut app, "Choose export format");
    tap_row(&mut app, "KOReader JSON");
    settle(&mut app);
    let file = app.exports.join("library-highlights.json");
    let bytes = fs::read(&file).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let books = json["documents"].as_array().unwrap();
    assert_eq!(books.len(), 3);
    assert_eq!(
        books
            .iter()
            .map(|book| book["entries"].as_array().unwrap().len())
            .sum::<usize>(),
        3
    );
    let mut ids: BTreeSet<_> = BTreeSet::new();
    for book in books {
        assert!(ids.insert(book["primitiveos_book_id"].as_str().unwrap()));
        let entry = &book["entries"][0];
        assert_eq!(entry["sort"], "highlight");
        if book["primitiveos_book_id"] == format!("{pdf_id:016x}") {
            assert_eq!(entry["page"], 2);
            assert!(entry.get("time").is_none());
        } else {
            assert_eq!(entry["time"], 1_740_777_900i64);
            assert_eq!(entry["text"], "The copper lantern");
            assert!(
                entry["page"]
                    .as_str()
                    .unwrap()
                    .starts_with("Chapter 1, page ")
            );
            assert!(entry["primitiveos"]["start"].is_object());
        }
    }
    let parsed = crate::koreader_import::parse(&bytes).unwrap();
    assert_eq!(parsed.documents.len(), 3);
    assert_eq!(app.current_history_location(), Some(origin.clone()));
    assert_eq!(fs::read(&fixture.state).unwrap(), before);

    app.activate_library_export(Command::Open).unwrap();
    tap_row(&mut app, "Collections");
    tap_row(&mut app, "Field reports");
    tap_row(&mut app, "Use selected collections");
    tap_row(&mut app, "Underline");
    tap_row(&mut app, "Choose export format");
    tap_row(&mut app, "KOReader JSON");
    settle(&mut app);
    let single: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert!(single.get("documents").is_none());
    assert_eq!(single["primitiveos_book_id"], format!("{first:016x}"));
    assert_eq!(single["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        Export::load(app.library_export.path.clone()).styles,
        app.library_export.styles
    );

    app.activate_library_export(Command::Style(Some(HighlightStyle::Lighten)))
        .unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    let combined: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(combined["documents"].as_array().unwrap().len(), 2);
    app.activate_library_export(Command::Style(Some(HighlightStyle::Lighten)))
        .unwrap();

    for (format, name) in [
        (Format::Text, "txt"),
        (Format::Markdown, "md"),
        (Format::Html, "html"),
    ] {
        app.activate_library_export(Command::Start(format)).unwrap();
        settle(&mut app);
        let text =
            fs::read_to_string(app.exports.join(format!("library-highlights.{name}"))).unwrap();
        assert!(text.contains("The copper lantern"));
        assert!(text.contains("Chapter 1"));
        if name == "html" {
            assert_eq!(text.matches("<!doctype html>").count(), 1);
            assert!(text.contains("Keep &lt;this&gt; &amp; compare [notes]"));
        }
    }

    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::Style(None)).unwrap();
    tap_row(&mut app, "Choose books");
    app.activate_library_export(Command::ToggleBook(pdf_id))
        .unwrap();
    tap_row(&mut app, "Use selected books");
    tap_row(&mut app, "Choose export format");
    tap_row(&mut app, "KOReader JSON");
    let previous = fs::read(&file).unwrap();
    app.input(Input::Sleep);
    assert!(app.export_deadline().is_none());
    assert_eq!(fs::read(&file).unwrap(), previous);
    assert_eq!(app.current_history_location(), Some(origin));
    assert_eq!(fs::read(&fixture.state).unwrap(), before);
}

#[test]
fn export_uses_reading_order_and_rejects_a_source_replaced_during_export() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    let id = save_reflow_mark(&mut app, &fixture.book);
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(1, Some("door")).unwrap();
    let start = reading.probe.fragment("door").unwrap();
    let mut end = start.clone();
    end.offset += "At dusk the door".len();
    let selection = reading.probe.select_range(&start, &end).unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.highlights.insert(
        0,
        Highlight {
            created_at: None,
            style: HighlightStyle::Lighten,
            selection,
            note: None,
        },
    );
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    let file = app.exports.join("library-highlights.json");
    let prior = fs::read(&file).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&prior).unwrap();
    assert_eq!(value["entries"][0]["text"], "The copper lantern");
    assert_eq!(value["entries"][1]["text"], "At dusk the door");
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    app.tick(app.export_deadline().unwrap());
    assert!(app.library_export.job.as_ref().unwrap().pending.is_some());
    fs::write(
        &fixture.book,
        include_bytes!("../../../tests/fixtures/long-chapter.epub"),
    )
    .unwrap();
    settle(&mut app);
    assert_eq!(fs::read(file).unwrap(), prior);
    assert_eq!(app.screen, Screen::LibraryExport(Page::Options));
    assert!(app.notice.as_deref().unwrap().contains("No highlights"));
}

#[test]
fn export_controls_reverse_only_the_pressed_row_in_both_themes() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    save_reflow_mark(&mut app, &fixture.book);
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    let font_paths: Vec<_> = fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas =
        primitive_ui::Canvas::new(primitive_app::WIDTH, primitive_app::HEIGHT, &font_paths)
            .unwrap();
    for page in [
        Page::Sources,
        Page::Books,
        Page::Collections,
        Page::Options,
        Page::Formats,
        Page::Running,
    ] {
        if page == Page::Running {
            app.activate_library_export(Command::Start(Format::Json))
                .unwrap();
        } else {
            app.show(Screen::LibraryExport(page));
        }
        for dark in [false, true] {
            canvas.clear_press();
            let before = primitive_app::frame(&app, &mut canvas, dark);
            let (_, items) = app.list_window();
            for (index, item) in items.iter().enumerate() {
                if matches!(item.action, Action::Heading) {
                    continue;
                }
                let y0 = super::super::LIST_Y + index as i32 * super::super::ROW_HEIGHT;
                let rect = primitive_ui::Rect::new(
                    super::super::LIST_X,
                    y0,
                    primitive_app::WIDTH as i32 - 2 * super::super::LIST_X,
                    super::super::ROW_HEIGHT,
                );
                assert!(canvas.press_at(120.0, (y0 + 40) as f32));
                let after = primitive_app::frame(&app, &mut canvas, dark);
                let mut changed = 0;
                for (offset, (left, right)) in before.iter().zip(&after).enumerate() {
                    if left != right {
                        let x = (offset % primitive_app::WIDTH as usize) as f32;
                        let y = (offset / primitive_app::WIDTH as usize) as f32;
                        assert!(rect.contains(x, y), "feedback escaped row on {page:?}");
                        changed += 1;
                    }
                }
                assert!(changed > 100);
                let background = (y0 as usize + 40) * primitive_app::WIDTH as usize + 60;
                assert_eq!(before[background], if dark { 0 } else { 255 });
                assert_eq!(after[background], if dark { 255 } else { 0 });
                canvas.clear_press();
                primitive_app::frame(&app, &mut canvas, dark);
            }
        }
    }
}

#[test]
#[ignore = "requires privately selected original EPUB and PDF documents"]
fn exports_original_document_marks_without_changing_sources_or_reading_position() {
    let root = std::path::PathBuf::from(std::env::var_os("READER_TEST_EXPORT_DOCUMENTS").unwrap());
    let list = std::path::PathBuf::from(std::env::var_os("READER_TEST_EXPORT_LIST").unwrap());
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&root, &fixture.state, fonts).unwrap();
    let mut expected = Vec::new();
    let mut formats = [0; 2];
    for path in fs::read_to_string(&list).unwrap().lines() {
        let path = std::path::Path::new(path);
        app.open_path(path).unwrap();
        let id = app.open_book_id().unwrap();
        if let Some(fixed) = &app.fixed {
            let found = (0..fixed.reading.count().min(24))
                .find_map(|page| {
                    let layer = fixed.reading.page_text(page).ok()?;
                    let word = layer
                        .words
                        .iter()
                        .find(|word| !layer.text[word.bytes.clone()].trim().is_empty())?;
                    Some((page, word.bounds, layer.text[word.bytes.clone()].to_owned()))
                })
                .expect("A selected original PDF must include selectable text");
            expected.push((id, found.2.clone(), None));
            app.library
                .state_mut(id)
                .unwrap()
                .add_pdf_highlight(PdfHighlight {
                    created_at: None,
                    style: HighlightStyle::Lighten,
                    page: found.0,
                    bounds: vec![found.1],
                    continuation: Vec::new(),
                    text: found.2,
                    note: Some("Export workflow check".into()),
                })
                .unwrap();
            formats[1] += 1;
        } else {
            let chapters = Book::open(&root.join(path)).unwrap().chapters.len();
            let mut selection = None;
            for chapter in 0..chapters.min(24) {
                let reading = &mut app.active.as_mut().unwrap().reading;
                reading.jump(chapter, None).unwrap();
                for page in 0..reading.probe.page_count().min(3) {
                    for y in (72..1500).step_by(36) {
                        for x in (72..1180).step_by(36) {
                            selection = reading.probe.word_at(page, x as f32, y as f32);
                            if selection.is_some() {
                                break;
                            }
                        }
                        if selection.is_some() {
                            break;
                        }
                    }
                    if selection.is_some() {
                        break;
                    }
                }
                if selection.is_some() {
                    break;
                }
            }
            let selection =
                selection.expect("A selected original EPUB must include selectable text");
            let anchor = serde_json::to_value(&selection.start).unwrap();
            expected.push((id, selection.text.clone(), Some(anchor)));
            app.library
                .state_mut(id)
                .unwrap()
                .highlights
                .push(Highlight {
                    created_at: Some(1_740_777_900),
                    style: HighlightStyle::Underline,
                    selection,
                    note: Some("Export workflow check".into()),
                });
            formats[0] += 1;
        }
    }
    assert!(formats.iter().all(|count| *count > 0));
    app.save().unwrap();
    let position = app.current_history_location().unwrap();
    let state = fs::read(&fixture.state).unwrap();
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    let bytes = fs::read(app.exports.join("library-highlights.json")).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let documents = json["documents"].as_array().unwrap();
    assert_eq!(documents.len(), expected.len());
    for (id, quote, anchor) in expected {
        let document = documents
            .iter()
            .find(|book| book["primitiveos_book_id"] == format!("{id:016x}"))
            .unwrap();
        let entry = &document["entries"][0];
        assert_eq!(entry["text"], quote);
        assert_eq!(entry["note"], "Export workflow check");
        if let Some(anchor) = anchor {
            assert_eq!(entry["primitiveos"]["start"], anchor);
            assert!(entry["page"].as_str().unwrap().starts_with("Chapter "));
            assert_eq!(entry["time"], 1_740_777_900i64);
        } else {
            assert!(entry["page"].as_u64().unwrap() > 0);
            assert!(entry.get("time").is_none());
        }
    }
    assert_eq!(
        crate::koreader_import::parse(&bytes)
            .unwrap()
            .documents
            .len(),
        documents.len()
    );
    assert_eq!(app.current_history_location(), Some(position));
    assert_eq!(fs::read(&fixture.state).unwrap(), state);
    eprintln!(
        "Original export covered {} EPUBs and {} PDFs with exact quote, note, date and source-location checks",
        formats[0], formats[1]
    );
}
