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
            updated_at: None,
            chapter_title: None,
            color: Default::default(),
            created_at: Some(1_740_777_900),
            style: HighlightStyle::Underline,
            selection,
            note: Some("Keep <this> & compare [notes]".into()),
        });
    id
}

#[test]
fn export_color_filters_migrate_old_preferences_and_preserve_empty_selection_semantics() {
    let fixture = Fixture::new();
    let path = fixture.root.join("export-preferences.json");
    for legacy in ["null", "[]", "[\"underscore\"]"] {
        fs::write(&path, legacy).unwrap();
        let mut settings = Export::load(path.clone());
        let expected: Option<Vec<HighlightStyle>> = serde_json::from_str(legacy).unwrap();
        assert_eq!(settings.styles, expected);
        assert!(settings.colors.is_none());
        settings.colors = Some(vec![HighlightColor::Red, HighlightColor::Blue]);
        settings.save_preferences().unwrap();
        let reread = Export::load(path.clone());
        assert_eq!(reread.styles, expected);
        assert_eq!(reread.colors, settings.colors);
    }
    fs::write(&path, r#"{"styles":[],"colors":[]}"#).unwrap();
    let settings = Export::load(path);
    assert_eq!(settings.styles, Some(vec![]));
    assert_eq!(settings.colors, Some(vec![]));
}

#[test]
fn damaged_existing_export_filters_never_fall_back_to_exporting_everything() {
    let fixture = Fixture::new();
    let path = fixture.root.join("invalid-export-filters.json");
    for invalid in [
        "not json",
        r#"{"styles":["future-drawer"],"colors":["red"]}"#,
    ] {
        fs::write(&path, invalid).unwrap();
        let prefs = Export::load(path.clone());
        assert_eq!(prefs.styles, Some(vec![]));
        assert_eq!(prefs.colors, Some(vec![]));
    }
    fs::write(&path,r#"{"styles":["underscore","underscore","underscore","underscore","underscore"],"colors":["red","red","red","red","red","red","red","red","red","red"]}"#).unwrap();
    let prefs = Export::load(path);
    assert_eq!(prefs.styles, Some(vec![HighlightStyle::Underline]));
    assert_eq!(prefs.colors, Some(vec![HighlightColor::Red]));
}

#[test]
fn export_multi_color_and_style_filters_intersect_and_preserve_dates_text_and_original_marks() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let id = save_reflow_mark(&mut app, &fixture.book);
    app.library.state_mut(id).unwrap().highlights[0].color = HighlightColor::Red;
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(1, Some("door")).unwrap();
    let start = reading.probe.fragment("door").unwrap();
    let mut end = start.clone();
    end.offset += "At dusk the door".len();
    let selection = reading.probe.select_range(&start, &end).unwrap();
    app.library
        .state_mut(id)
        .unwrap()
        .highlights
        .push(Highlight {
            updated_at: None,
            chapter_title: None,
            selection,
            note: Some("Keep the blue note".into()),
            created_at: Some(1_730_777_900),
            style: HighlightStyle::Lighten,
            color: HighlightColor::Blue,
        });
    app.mark_book_changed();
    app.save().unwrap();
    let before = app.library.bytes().unwrap();
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    tap_row(&mut app, "Highlight colors");
    for color in [HighlightColor::Red, HighlightColor::Blue] {
        let i = HighlightColor::ALL
            .iter()
            .position(|candidate| *candidate == color)
            .unwrap();
        let rect = super::super::highlight_palette::cell(i);
        assert_eq!(
            app.input(Input::Tap((rect.x + 30) as f32, (rect.y + 40) as f32)),
            Change::Partial
        );
    }
    capture_test_screen(&app, &fonts, "READER_EXPORT_COLOR_FILTER_SCREEN", false);
    capture_test_screen(&app, &fonts, "READER_EXPORT_COLOR_FILTER_DARK_SCREEN", true);
    app.activate_library_export(Command::Style(Some(HighlightStyle::Underline)))
        .unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    let file = app.exports.join("library-highlights.json");
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["color"], "red");
    assert_eq!(entries[0]["time"], 1_740_777_900i64);
    assert!(
        entries[0]["text"]
            .as_str()
            .unwrap()
            .contains("copper lantern")
    );
    app.activate_library_export(Command::Style(None)).unwrap();
    app.activate_library_export(Command::Color(Some(HighlightColor::Red)))
        .unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(value["entries"].as_array().unwrap().len(), 1);
    assert_eq!(value["entries"][0]["color"], "blue");
    app.activate_library_export(Command::Color(Some(HighlightColor::Blue)))
        .unwrap();
    assert_eq!(app.library_export.colors, Some(vec![]));
    let saved = fs::read(&file).unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    settle(&mut app);
    assert_eq!(
        fs::read(&file).unwrap(),
        saved,
        "empty filter must never broaden to all colors or overwrite an earlier export"
    );
    assert!(
        app.notice
            .as_deref()
            .unwrap_or("")
            .contains("No annotations")
    );
    assert_eq!(app.library.bytes().unwrap(), before);
}

#[test]
fn export_filter_preferences_roll_back_when_storage_rejects_the_change() {
    let fixture = Fixture::new();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.activate_library_export(Command::Color(Some(HighlightColor::Blue)))
        .unwrap();
    let path = app.library_export.path.clone();
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        app.activate_library_export(Command::Color(Some(HighlightColor::Red)))
            .is_err()
    );
    assert_eq!(app.library_export.colors, Some(vec![HighlightColor::Blue]));
    assert!(
        app.activate_library_export(Command::Style(Some(HighlightStyle::Underline)))
            .is_err()
    );
    assert!(app.library_export.styles.is_none());
    fs::remove_dir(&path).unwrap();
    fs::write(path, saved).unwrap();
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
            updated_at: None,
            chapter_title: None,
            color: Default::default(),
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
        // Two editions with a bookmark and a highlight each, plus the PDF highlight.
        5
    );
    let mut ids: BTreeSet<_> = BTreeSet::new();
    for book in books {
        assert!(ids.insert(book["primitiveos_book_id"].as_str().unwrap()));
        // Entries are in reading order, so a bookmark can precede the highlight.
        let entry = book["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["sort"] == "highlight")
            .unwrap();
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
            updated_at: None,
            chapter_title: None,
            color: Default::default(),
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
    // The page bookmark leads in reading order; the highlights follow it.
    assert_eq!(value["entries"][0]["sort"], "bookmark");
    assert_eq!(value["entries"][1]["text"], "The copper lantern");
    assert_eq!(value["entries"][2]["text"], "At dusk the door");
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
    assert!(app.notice.as_deref().unwrap().contains("No annotations"));
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
                    updated_at: None,
                    chapter_title: None,
                    color: Default::default(),
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
                    updated_at: None,
                    chapter_title: None,
                    color: Default::default(),
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

#[test]
fn export_history_combines_kindle_and_reader_clippings_without_editing_sources() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let id = save_reflow_mark(&mut app, &fixture.book);
    let book = app.library.state(id).unwrap();
    let title = book.title.clone();
    let author = book.author.clone();
    let same_book = author
        .as_ref()
        .map_or_else(|| title.clone(), |author| format!("{title} ({author})"));
    let clipping = format!(
        "{same_book}\n- Your Highlight on page 42 | Added on 2024-08-09 13:11:12\n\nA stock highlight.\n==========\n",
    );
    let stock = format!(
        "\u{feff}{clipping}{clipping}{same_book}\n- Your Note on Location 94 | Added on 2024-08-09 13:12:13\n\nA stock note.\n==========\n{title} (Other Author)\n- Your Bookmark on Location 132 | Added on 2024-08-09 13:13:14\n\n\n==========\n"
    );
    let stock_path = fixture.root.join(kindle_clippings::FILE);
    fs::write(&stock_path, &stock).unwrap();
    app.mark_book_changed();
    app.save().unwrap();
    let state_before = app.library.bytes().unwrap();
    let position_before = app.current_history_location();

    app.activate_library_export(Command::Open).unwrap();
    tap_row(&mut app, "History");
    assert_eq!(app.library_export.kindle_clippings, Some(true));
    capture_test_screen(&app, &fonts, "READER_EXPORT_KINDLE_SOURCES_FRAME", false);
    tap_row(&mut app, "Choose export format");
    capture_test_screen(&app, &fonts, "READER_EXPORT_KINDLE_FORMATS_FRAME", false);
    tap_row(&mut app, "KOReader JSON");
    settle(&mut app);

    let exports = &app.exports;
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(exports.join("library-highlights.json")).unwrap())
            .unwrap();
    let books = json["documents"].as_array().unwrap();
    assert_eq!(books.len(), 2);
    let entries = books[0]["entries"].as_array().unwrap();
    // Reading order: the page bookmark, then the highlights and the note.
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0]["sort"], "bookmark");
    assert_eq!(entries[1]["text"], "The copper lantern");
    assert_eq!(entries[2]["page"], 42);
    assert!(entries[2].get("primitiveos").is_none());
    assert_eq!(entries[3]["page"], "Location 94");
    assert_eq!(entries[3]["sort"], "note");
    assert_eq!(books[1]["author"], "Other Author");
    assert_eq!(books[1]["entries"][0]["sort"], "bookmark");
    assert!(books[1].get("file").is_none());

    tap_row(&mut app, "Kindle clippings text");
    settle(&mut app);
    let exported = fs::read(app.exports.join("library-highlights.clippings.txt")).unwrap();
    let records = kindle_clippings::parse(&exported).unwrap();
    // Reading order: the chapter bookmark, the copper highlight and its note, the stock
    // highlight and note, then the position-132 bookmark.
    assert_eq!(records.len(), 6);
    assert_eq!(records[0].kind, Kind::Bookmark);
    assert_eq!(records[2].kind, Kind::Note);
    assert_eq!(records[2].text, "Keep <this> & compare [notes]");
    assert_eq!(records[4].text, "A stock note.");
    assert_eq!(records[5].location, Some(Location::Position("132")));
    assert_eq!(fs::read_to_string(stock_path).unwrap(), stock);
    assert_eq!(app.library.bytes().unwrap(), state_before);
    assert_eq!(app.current_history_location(), position_before);
}

#[test]
fn export_can_use_only_kindle_clippings_and_respects_filter_and_source_choices() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    let stock_path = fixture.root.join(kindle_clippings::FILE);
    fs::write(&stock_path,
        "Kindle book\n- Your Highlight on page 4 | Added on 2024-03-01 00:00:00\n\nPassage.\n==========\n")
        .unwrap();
    app.activate_library_export(Command::Open).unwrap();
    tap_row(&mut app, "History");
    assert!(app.library_export.books.is_empty());
    tap_row(&mut app, "Kindle clippings");
    assert_eq!(app.library_export.kindle_clippings, Some(false));
    assert!(
        app.activate_library_export(Command::Start(Format::Json))
            .is_err()
    );
    tap_row(&mut app, "Kindle clippings");
    app.activate_library_export(Command::Color(Some(HighlightColor::Red)))
        .unwrap();
    assert!(
        app.activate_library_export(Command::Start(Format::Json))
            .is_err()
    );
    app.activate_library_export(Command::Color(None)).unwrap();
    app.activate_library_export(Command::Start(Format::Html))
        .unwrap();
    settle(&mut app);
    let html = fs::read_to_string(app.exports.join("library-highlights.html")).unwrap();
    assert!(html.contains("Passage."));
    assert!(html.contains("Page 4"));
}

#[test]
fn exports_the_current_book_as_clippings_with_a_local_location_and_note() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    let id = save_reflow_mark(&mut app, &fixture.book);
    let before = app.current_history_location();
    app.activate(Action::Show(Screen::ExportAnnotations))
        .unwrap();
    tap_row(&mut app, "Kindle clippings text");
    let exported = fs::read(app.exports.join(format!("{id:016x}.clippings.txt"))).unwrap();
    let records = kindle_clippings::parse(&exported).unwrap();
    // The highlight, its note, and the saved page bookmark.
    assert_eq!(records.len(), 3);
    assert_eq!(records[2].kind, Kind::Bookmark);
    assert_eq!(records[0].kind, Kind::Highlight);
    assert_eq!(
        records[0].location,
        Some(Location::Chapter("Chapter 1, page 1"))
    );
    assert_eq!(records[0].text, "The copper lantern");
    assert_eq!(records[0].time, Some(1_740_777_900));
    assert_eq!(records[1].kind, Kind::Note);
    assert_eq!(records[1].text, "Keep <this> & compare [notes]");
    assert_eq!(app.current_history_location(), before);
}

#[test]
#[ignore = "requires an authorized original My Clippings.txt in READER_ORIGINAL_KINDLE_DOCUMENTS"]
fn exports_original_kindle_clippings_to_an_isolated_profile() {
    let root = std::path::PathBuf::from(
        std::env::var_os("READER_ORIGINAL_KINDLE_DOCUMENTS").expect("Kindle Documents root"),
    );
    let original = root.join(kindle_clippings::FILE);
    let before = fs::read(&original).unwrap();
    let count = kindle_clippings::parse(&before).unwrap().len();
    assert!(
        count > 0,
        "The authorized clippings file has no recognized entries"
    );

    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&root, &fixture.state, fonts).unwrap();
    app.activate_library_export(Command::Open).unwrap();
    tap_row(&mut app, "History");
    tap_row(&mut app, "Choose export format");
    tap_row(&mut app, "KOReader JSON");
    settle(&mut app);
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(app.exports.join("library-highlights.json")).unwrap())
            .unwrap();
    let documents = json["documents"].as_array();
    let entries = |book: &serde_json::Value| book["entries"].as_array().unwrap().len();
    let exported =
        documents.map_or_else(|| entries(&json), |books| books.iter().map(entries).sum());
    assert!(exported > 0 && exported <= count);
    assert_eq!(fs::read(original).unwrap(), before);
    println!(
        "Original Kindle clippings: {count} recognized records, {exported} exported after deduplication"
    );
}

#[test]
fn local_annotation_export_settings_persist_and_preserve_existing_outputs() {
    use crate::app::export_settings::{Origin, Page as SettingsPage};
    let fixture = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let sources = fixture.root.join("Books");
    let destination = fixture.root.join("Study").join("Reading notes");
    fs::create_dir(&sources).unwrap();
    fs::create_dir_all(&destination).unwrap();
    let source = sources.join("Field Notes.epub");
    fs::rename(&fixture.book, &source).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let id = save_reflow_mark(&mut app, &source);
    app.mark_book_changed();
    app.save().unwrap();
    let reading_data = fs::read(&fixture.state).unwrap();

    app.show(Screen::ExportAnnotations);
    let default = app.exports.join(format!("{id:016x}.txt"));
    tap_row(&mut app, "Plain text");
    let text = fs::read(&default).unwrap();
    assert!(String::from_utf8_lossy(&text).contains("Keep <this> & compare [notes]"));
    fs::write(&default, b"previous default export").unwrap();
    tap_row(&mut app, "Plain text");
    assert_eq!(fs::read(&default).unwrap(), text);

    let back_button = primitive_ui::back_button(super::super::WIDTH as i32);
    let back = Input::Tap((back_button.x + 20) as f32, (back_button.y + 20) as f32);
    tap_row(&mut app, "Export settings");
    assert_eq!(
        app.screen,
        Screen::ExportSettings(Origin::Book, SettingsPage::Settings)
    );
    tap_row(&mut app, "Choose export folder");
    tap_row(&mut app, "Study");
    tap_row(&mut app, "Reading notes");
    app.input(back);
    let (_, rows) = app.list_window();
    assert_eq!(rows[0].detail.as_deref(), Some("Reader data / exports"));

    tap_row(&mut app, "Choose export folder");
    tap_row(&mut app, "Study");
    capture_test_screen(&app, &fonts, "READER_EXPORT_FOLDER_SCREEN", false);
    tap_row(&mut app, "Reading notes");
    tap_row(&mut app, "Use this folder");
    tap_row(&mut app, "Use book titles for filenames");
    capture_test_screen(&app, &fonts, "READER_EXPORT_SETTINGS_SCREEN", false);
    app.input(back);
    assert_eq!(app.screen, Screen::ExportAnnotations);

    let occupied = destination.join("Field Notes at Dusk (EPUB 3).txt");
    fs::write(&occupied, b"keep my file").unwrap();
    let alias = destination.join("Field Notes at Dusk (EPUB 3)-2.txt");
    std::os::unix::fs::symlink(&occupied, &alias).unwrap();
    tap_row(&mut app, "Plain text");
    assert_eq!(fs::read(&occupied).unwrap(), b"keep my file");
    assert_eq!(fs::read_link(&alias).unwrap(), occupied);
    assert_eq!(
        fs::read(destination.join("Field Notes at Dusk (EPUB 3)-3.txt")).unwrap(),
        text
    );
    assert!(
        app.notice
            .as_ref()
            .unwrap()
            .contains("/Field Notes at Dusk (EPUB 3)-3.txt")
    );

    tap_row(&mut app, "Export settings");
    tap_row(&mut app, "Export beside the book");
    assert_eq!(fs::read(&fixture.state).unwrap(), reading_data);
    drop(app);

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.activate(Action::Continue).unwrap();
    app.show(Screen::ExportAnnotations);
    tap_row(&mut app, "Kindle clippings text");
    let alongside = sources.join("Field Notes at Dusk (EPUB 3).clippings.txt");
    assert!(
        fs::read_to_string(alongside)
            .unwrap()
            .contains("copper lantern")
    );
    assert!(
        !destination
            .join("Field Notes at Dusk (EPUB 3).clippings.txt")
            .exists()
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        include_bytes!("../../../tests/fixtures/field-notes-epub3.epub")
    );

    // A single surviving book uses the same alongside rule but keeps the bundle name.
    app.show(Screen::ReadingData);
    tap_row(&mut app, "Export highlights");
    tap_row(&mut app, "History");
    tap_row(&mut app, "Export settings");
    assert_eq!(
        app.screen,
        Screen::ExportSettings(Origin::Library, SettingsPage::Settings)
    );
    app.input(back);
    tap_row(&mut app, "Choose export format");
    let owned = sources.join("library-highlights.md");
    fs::write(&owned, "existing library notes").unwrap();
    tap_row(&mut app, "Markdown");
    settle(&mut app);
    assert_eq!(
        fs::read_to_string(&owned).unwrap(),
        "existing library notes"
    );
    assert!(
        fs::read_to_string(sources.join("library-highlights-2.md"))
            .unwrap()
            .contains("copper lantern")
    );
    assert!(!destination.join("library-highlights.md").exists());
    capture_test_screen(&app, &fonts, "READER_EXPORT_SAVED_SCREEN", false);

    app.show(Screen::LibraryExport(Page::Options));
    tap_row(&mut app, "Export settings");
    tap_row(&mut app, "Use Reader exports");
    tap_row(&mut app, "Export beside the book");
    app.input(back);
    tap_row(&mut app, "Choose export format");
    tap_row(&mut app, "HTML");
    settle(&mut app);
    assert!(app.exports.join("library-highlights.html").is_file());
}

#[test]
fn local_annotation_library_job_freezes_settings_and_revalidates_destination() {
    use crate::app::export_settings::{Command as Setting, Origin};
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let chosen = fixture.root.join("Notes");
    let other = fixture.root.join("Unselected");
    fs::create_dir(&chosen).unwrap();
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, fixture.root.join("Shortcut")).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    save_reflow_mark(&mut app, &fixture.book);
    let open = |app: &mut Reader, directory: &str| {
        app.export_settings_command(Origin::Library, Setting::Open)
            .unwrap();
        app.export_settings_command(Origin::Library, Setting::Browse(Path::new("").into()))
            .unwrap();
        assert!(!app.items().iter().any(|row| row.title == "Shortcut"));
        app.export_settings_command(
            Origin::Library,
            Setting::Browse(Path::new(directory).into()),
        )
        .unwrap();
        app.export_settings_command(Origin::Library, Setting::UseFolder)
            .unwrap();
    };
    let start = |app: &mut Reader| {
        app.activate_library_export(Command::Open).unwrap();
        app.activate_library_export(Command::History).unwrap();
        app.activate_library_export(Command::Start(Format::Text))
            .unwrap();
    };

    open(&mut app, "Notes");
    start(&mut app);
    open(&mut app, "Unselected");
    settle(&mut app);
    assert!(chosen.join("library-highlights.txt").is_file());
    assert!(fs::read_dir(&other).unwrap().next().is_none());

    start(&mut app);
    app.activate_library_export(Command::Cancel).unwrap();
    settle(&mut app);
    assert!(fs::read_dir(&other).unwrap().next().is_none());

    start(&mut app);
    fs::remove_dir(&other).unwrap();
    std::os::unix::fs::symlink(&chosen, &other).unwrap();
    settle(&mut app);
    assert_eq!(app.screen, Screen::LibraryExport(Page::Options));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("Choose a folder from the library")
    );
    assert!(app.library_export.saved.is_none());
    assert!(!chosen.join("library-highlights-2.txt").exists());
    assert!(!app.exports.exists());

    // A missing configured directory falls back to the Documents chooser, only for browsing.
    app.export_settings_command(Origin::Library, Setting::Open)
        .unwrap();
    tap_row(&mut app, "Choose export folder");
    assert_eq!(app.items()[0].detail.as_deref(), Some("Documents"));
}

fn tap_export_control(app: &mut Reader, rect: primitive_ui::Rect) {
    let x = rect.x + rect.w / 2;
    let y = rect.y + rect.h / 2;
    assert_eq!(app.input(Input::Tap(x as f32, y as f32)), Change::Partial);
}

fn tap_export_key(app: &mut Reader, target: primitive_ui::Key) {
    use super::super::{HEIGHT, WIDTH};
    let (_, key) = primitive_ui::Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Save")
        .into_iter()
        .find(|(key, _)| key.key == target)
        .unwrap();
    tap_export_control(app, key);
}

fn save_export_template(app: &mut Reader, text: &str) {
    if !text.is_empty() {
        app.clipboard_text = Some(text.into());
        tap_export_control(app, super::super::external_link::PASTE);
        app.clipboard_text = None;
    }
    tap_export_key(app, primitive_ui::Key::Go);
}

#[test]
fn export_filename_templates_include_real_metadata_and_keep_template_scope_after_reopen() {
    use crate::app::export_settings::{Command as Setting, Origin, Page as SettingsPage, Scope};
    let fixture = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let folder = fixture.root.join("Exported notes");
    let books = fixture.root.join("Books");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(&books).unwrap();
    let first = books.join("First Book.epub");
    fs::rename(&fixture.book, &first).unwrap();
    let second = fixture.root.join("Last Book.epub");
    fs::copy(crate::test_fixture("field-notes-epub2.epub"), &second).unwrap();

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let first_id = save_reflow_mark(&mut app, &first);
    let state = app.library.state_mut(first_id).unwrap();
    state.title = "Dusk: Relay/%A".into();
    state.author = Some("Linh|Nguyen".into());
    app.mark_book_changed();
    app.save().unwrap();

    app.show(Screen::ExportAnnotations);
    tap_row(&mut app, "Export settings");
    tap_row(&mut app, "Choose export folder");
    tap_row(&mut app, "Exported notes");
    tap_row(&mut app, "Use this folder");
    tap_row(&mut app, "Single-book filename");
    assert_eq!(
        app.screen,
        Screen::ExportSettings(Origin::Book, SettingsPage::Template(Scope::Single))
    );
    assert_eq!(app.editor.string(), "");
    save_export_template(&mut app, "%%T - %A - %Z");
    tap_row(&mut app, "Single-book filename");
    assert_eq!(app.editor.string(), "%%T - %A - %Z");
    capture_test_screen(&app, &fonts, "READER_EXPORT_TEMPLATE_EDITOR", false);
    let clear = primitive_ui::Rect::new(super::super::LIST_X, super::super::LIST_Y + 240, 320, 96);
    tap_export_control(&mut app, clear);
    assert!(app.editor.string().is_empty());
    use primitive_ui::{Key, Layer};
    tap_export_key(&mut app, Key::Layer(Layer::Symbols));
    tap_export_key(&mut app, Key::Layer(Layer::More));
    tap_export_key(&mut app, Key::Char('%'));
    assert_eq!(app.editor.string(), "%");
    assert_eq!(app.input(Input::PageBack), Change::Partial);

    tap_row(&mut app, "Multiple-book filename");
    save_export_template(&mut app, "Bundle %f - %T %%");
    tap_row(&mut app, "Filename tokens");
    capture_test_screen(&app, &fonts, "READER_EXPORT_TEMPLATE_TOKENS", false);
    let back = primitive_ui::back_button(super::super::WIDTH as i32);
    tap_export_control(&mut app, back);
    capture_test_screen(&app, &fonts, "READER_EXPORT_TEMPLATE_SETTINGS", false);
    tap_export_control(&mut app, back);

    let existing = folder.join("%Dusk- Relay-%A - Linh-Nguyen - N-A.txt");
    fs::write(&existing, "User file").unwrap();
    tap_row(&mut app, "Plain text");
    assert_eq!(fs::read_to_string(&existing).unwrap(), "User file");
    assert!(
        fs::read_to_string(folder.join("%Dusk- Relay-%A - Linh-Nguyen - N-A-2.txt"))
            .unwrap()
            .contains("copper lantern")
    );

    let second_id = save_reflow_mark(&mut app, &second);
    app.mark_book_changed();
    app.save().unwrap();
    drop(app);

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.activate(Action::Continue).unwrap();
    assert_eq!(app.open_book_id(), Some(second_id));
    let last_title = "Desk log";
    app.library.state_mut(second_id).unwrap().title = last_title.into();
    app.export_settings_command(Origin::Library, Setting::Open)
        .unwrap();
    tap_row(&mut app, "Single-book filename");
    assert_eq!(app.editor.string(), "%%T - %A - %Z");
    tap_export_control(&mut app, back);
    tap_row(&mut app, "Multiple-book filename");
    assert_eq!(app.editor.string(), "Bundle %f - %T %%");
    tap_export_control(&mut app, back);
    tap_row(&mut app, "Export beside the book");

    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    app.activate_library_export(Command::Start(Format::Json))
        .unwrap();
    // The multi-book scope and original last-opened metadata are frozen at launch.
    app.library.state_mut(second_id).unwrap().title = "Title edited while the job runs".into();
    app.export_settings_command(Origin::Library, Setting::Open)
        .unwrap();
    tap_row(&mut app, "Multiple-book filename");
    tap_export_control(&mut app, clear);
    save_export_template(&mut app, "New queued template");
    settle(&mut app);
    let bundle_name = format!("Bundle Last Book.epub - {last_title} %%.json");
    let bundle = folder.join(&bundle_name);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&bundle).unwrap()).unwrap();
    assert_eq!(value["documents"].as_array().unwrap().len(), 2);
    assert!(!folder.join("New queued template.json").exists());
    assert!(!books.join(&bundle_name).exists());
    capture_test_screen(&app, &fonts, "READER_EXPORT_TEMPLATE_SAVED", false);

    // The final number after selection and filtering determines the template and the folder.
    app.activate_library_export(Command::ToggleBook(second_id))
        .unwrap();
    app.activate_library_export(Command::Start(Format::Markdown))
        .unwrap();
    settle(&mut app);
    assert!(
        fs::read_to_string(books.join("%Dusk- Relay-%A - Linh-Nguyen - N-A.md"))
            .unwrap()
            .contains("copper lantern")
    );

    app.export_settings_command(Origin::Library, Setting::Open)
        .unwrap();
    tap_row(&mut app, "Single-book filename");
    tap_export_control(&mut app, clear);
    save_export_template(&mut app, "");
    app.activate_library_export(Command::Start(Format::Html))
        .unwrap();
    settle(&mut app);
    assert!(books.join("library-highlights.html").is_file());
    assert!(!books.join("New queued template.html").exists());

    // A sole Kindle clipping can supply its title, while its missing source cannot override the folder.
    let kindle = fixture.root.join(kindle_clippings::FILE);
    fs::write(
        &kindle,
        "Kindle fixture\n- Your Highlight on page 9 | Added on 2024-03-01 00:00:00\n\nPaper shore.\n==========\n",
    )
    .unwrap();
    app.export_settings_command(Origin::Library, Setting::Open)
        .unwrap();
    tap_row(&mut app, "Single-book filename");
    save_export_template(&mut app, "Notes %T from %f");
    app.activate_library_export(Command::Open).unwrap();
    app.activate_library_export(Command::History).unwrap();
    app.activate_library_export(Command::ToggleBook(first_id))
        .unwrap();
    app.activate_library_export(Command::ToggleBook(second_id))
        .unwrap();
    app.activate_library_export(Command::Start(Format::KindleClippings))
        .unwrap();
    settle(&mut app);
    assert!(
        fs::read_to_string(folder.join("Notes Kindle fixture from N-A.clippings.txt"))
            .unwrap()
            .contains("Paper shore.")
    );
}
