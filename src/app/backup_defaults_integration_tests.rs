//! Exercise a real v2 restore across global defaults, known editions and new editions.
use super::*;
use crate::app::tests::Fixture;
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn reader(fixture: &Fixture) -> Reader {
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app
}
fn tap(app: &mut Reader, rect: Rect) {
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}
fn row(app: &mut Reader, title: &str) {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("Missing visible row {title}"));
    let page = index / ROWS_PER_PAGE;
    while app.list_page < page {
        let before = app.list_page;
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.list_page, before + 1);
    }
    while app.list_page > page {
        let before = app.list_page;
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.list_page + 1, before);
    }
    tap(
        app,
        Rect::new(
            LIST_X,
            LIST_Y + (index % ROWS_PER_PAGE) as i32 * ROW_HEIGHT,
            WIDTH as i32 - 2 * LIST_X,
            ROW_HEIGHT,
        ),
    );
}
fn keyboard(app: &mut Reader, key: primitive_ui::Key) {
    let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Save")
        .into_iter()
        .find(|(candidate, _)| candidate.key == key)
        .unwrap()
        .1;
    tap(app, rect);
}
fn save_defaults(app: &mut Reader, fixed: bool) {
    app.activate(Action::Show(Screen::DocumentDefaults))
        .unwrap();
    // The first visible button in the relevant format group is Save current.
    tap(
        app,
        Rect::new(
            48,
            if fixed { 790 } else { 350 },
            (WIDTH as i32 - 128) / 3,
            108,
        ),
    );
    assert_eq!(app.screen, Screen::DocumentDefaults);
}
fn text_settings(app: &mut Reader, font_size: f32, render_dpi: u16) {
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(crate::Settings {
        font_size,
        logical_font_size: None,
        render_dpi,
        ..settings
    }))
    .unwrap();
}
fn add_note(app: &mut Reader) {
    let point = if let Some(active) = &app.active {
        let reading = &active.reading;
        reading
            .probe
            .source
            .blocks
            .iter()
            .flat_map(|block| {
                crate::selection::word_ranges(&block.text).filter_map(|range| {
                    if range.len() < 3 {
                        return None;
                    }
                    let mark = reading.probe.select_range(
                        &reading.probe.anchor(block.key, range.start),
                        &reading.probe.anchor(block.key, range.end),
                    )?;
                    reading
                        .probe
                        .selection_rects(reading.page, &mark)
                        .into_iter()
                        .find(|rect| rect.width > 4 && rect.height > 4)
                        .map(|rect| (rect.x + rect.width / 2, rect.y + rect.height / 2))
                })
            })
            .next()
            .unwrap()
    } else {
        let reading = &app.fixed.as_ref().unwrap().reading;
        let text = reading.page_text(reading.page()).unwrap();
        let (size, viewport) = reading.page_viewport().unwrap();
        let bounds = text.words[0].bounds.in_viewport(size, viewport).unwrap();
        let center = bounds.center();
        (center.x as i32, center.y as i32)
    };
    assert_eq!(
        app.input(Input::LongTap(point.0 as f32, point.1 as f32)),
        Change::Partial
    );
    assert!(app.selection.is_some() || app.pdf_selection.is_some());
    let note = app.selection_button(selection_popup::SelectionAction::Note);
    tap(app, note);
    assert_eq!(app.screen, Screen::Note);
    for character in "local".chars() {
        keyboard(app, primitive_ui::Key::Char(character));
    }
    assert_eq!(app.editor.string(), "local");
    keyboard(app, primitive_ui::Key::Go);
    assert!(matches!(app.screen, Screen::Book | Screen::Fixed));
}
fn changed_epub(original: &[u8]) -> Vec<u8> {
    let mut source = ZipArchive::new(Cursor::new(original)).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let mut changed = false;
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "EPUB/chapters/shore.xhtml" {
            let text = String::from_utf8(bytes).unwrap();
            let edition=text.replace("</body>","<p id=\"new-edition\">This distinct edition adds a walk beyond the northern harbor.</p></body>");
            assert_ne!(edition, text);
            bytes = edition.into_bytes();
            changed = true;
        }
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    assert!(changed);
    output.finish().unwrap().into_inner()
}
fn changed_pdf(original: &[u8]) -> Vec<u8> {
    let mut bytes = original.to_vec();
    let offset = bytes.windows(5).position(|part| part == b"(PDF)").unwrap();
    // Equal-length authored text keeps the original stream length and xref valid.
    bytes[offset..offset + 5].copy_from_slice(b"(NEW)");
    bytes
}
fn sha(bytes: &[u8]) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .to_vec()
}
fn identify(app: &Reader, path: &Path) -> u64 {
    app.library
        .identify(&app.library.resolve(path).unwrap())
        .unwrap()
}

#[test]
fn backup_restore_reloads_defaults_for_unseen_editions_and_keeps_known_reading_data() {
    let source = Fixture::new();
    let known_epub = fs::read(&source.book).unwrap();
    let known_pdf = crate::fixed::tests::document();
    let source_pdf = source.root.join("Known.pdf");
    fs::write(&source_pdf, &known_pdf).unwrap();
    let mut donor = reader(&source);
    donor.open_path(&source.book).unwrap();
    text_settings(&mut donor, 58., 212);
    save_defaults(&mut donor, false);
    donor.open_path(&source_pdf).unwrap();
    donor.activate(Action::SetFixedZoom(1)).unwrap();
    save_defaults(&mut donor, true);
    assert_eq!(
        donor
            .document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .font_size,
        Some(58.)
    );
    donor.activate(Action::ExportBackup).unwrap();
    let backup = fs::read_dir(&donor.exports)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("reader-backup-")
        })
        .unwrap();
    let bytes = fs::read(backup).unwrap();
    let envelope = crate::reading_backup::Envelope::parse(&bytes).unwrap();
    assert!(
        envelope
            .components
            .iter()
            .any(|component| component.key == crate::reading_backup::Key::DocumentDefaults)
    );
    let saved_defaults = envelope
        .components
        .iter()
        .find(|component| component.key == crate::reading_backup::Key::DocumentDefaults)
        .unwrap();
    assert_eq!(
        saved_defaults.data["reflow"]["render_dpi"].as_u64(),
        Some(212)
    );
    let target = Fixture::new();
    let target_pdf = target.root.join("Known.pdf");
    fs::write(&target_pdf, &known_pdf).unwrap();
    let unseen_epub = target.root.join("Unseen edition.epub");
    let unseen_pdf = target.root.join("Unseen edition.pdf");
    fs::write(&unseen_epub, changed_epub(&known_epub)).unwrap();
    fs::write(&unseen_pdf, changed_pdf(&known_pdf)).unwrap();
    let protected = vec![
        (source.book.clone(), sha(&known_epub)),
        (source_pdf.clone(), sha(&known_pdf)),
        (target.book.clone(), sha(&known_epub)),
        (target_pdf.clone(), sha(&known_pdf)),
        (unseen_epub.clone(), sha(&fs::read(&unseen_epub).unwrap())),
        (unseen_pdf.clone(), sha(&fs::read(&unseen_pdf).unwrap())),
    ];
    let mut app = reader(&target);
    app.open_path(&target.book).unwrap();
    // A distinct existing global choice makes skipped replacement detectable.
    text_settings(&mut app, 81., 48);
    save_defaults(&mut app, false);
    text_settings(&mut app, 72., 96);
    app.activate(Action::Show(Screen::Contents)).unwrap();
    let dusk = app
        .items()
        .iter()
        .find(|item| item.title.to_lowercase().contains("dusk"))
        .unwrap()
        .title
        .clone();
    row(&mut app, &dusk);
    assert_eq!(app.active.as_ref().unwrap().reading.chapter, 1);
    add_note(&mut app);
    app.activate(Action::ToggleBookmark).unwrap();
    app.save().unwrap();
    let epub_id = app.open_book_id().unwrap();
    let epub_state = app.library.state(epub_id).unwrap().clone();
    assert_eq!(epub_state.preferences.font_size, Some(72.));
    app.open_path(&target_pdf).unwrap();
    app.activate(Action::SetFixedZoom(2)).unwrap();
    save_defaults(&mut app, true);
    app.show(Screen::Fixed);
    add_note(&mut app);
    app.input(Input::PageForward);
    app.activate(Action::SetFixedZoom(3)).unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    app.save().unwrap();
    let pdf_id = app.open_book_id().unwrap();
    let pdf_state = app.library.state(pdf_id).unwrap().clone();
    fs::write(&app.import_path, &bytes).unwrap();
    app.activate(Action::PreviewBackup).unwrap();
    assert_eq!(app.screen, Screen::RestoreBackup);
    let default_row = app
        .items()
        .into_iter()
        .find(|item| item.title == "New book defaults")
        .unwrap();
    assert!(default_row.detail.as_deref().unwrap().contains("Skipped"));
    row(&mut app, "New book defaults");
    assert!(
        app.items()
            .iter()
            .find(|item| item.title == "New book defaults")
            .unwrap()
            .detail
            .as_deref()
            .unwrap()
            .contains("replace")
    );
    row(&mut app, "Apply backup");
    assert_eq!(app.screen, Screen::ReadingData);
    assert_eq!(
        app.document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .font_size,
        Some(58.)
    );
    assert_eq!(
        app.library.state(epub_id).unwrap().position,
        epub_state.position
    );
    assert_eq!(
        app.library.state(epub_id).unwrap().preferences,
        epub_state.preferences
    );
    assert_eq!(
        serde_json::to_value(&app.library.state(epub_id).unwrap().highlights).unwrap(),
        serde_json::to_value(&epub_state.highlights).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&app.library.state(pdf_id).unwrap().pdf_highlights).unwrap(),
        serde_json::to_value(&pdf_state.pdf_highlights).unwrap()
    );
    assert_eq!(
        app.library.state(pdf_id).unwrap().fixed_preferences,
        pdf_state.fixed_preferences
    );
    assert_eq!(
        app.library.state(pdf_id).unwrap().position,
        pdf_state.position
    );
    assert_eq!(
        app.document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .render_dpi,
        Some(212)
    );
    let unseen_epub_id = identify(&app, &unseen_epub);
    let unseen_pdf_id = identify(&app, &unseen_pdf);
    assert_ne!(unseen_epub_id, epub_id);
    assert_ne!(unseen_pdf_id, pdf_id);
    assert!(app.library.state(unseen_epub_id).is_none());
    assert!(app.library.state(unseen_pdf_id).is_none());
    app.open_path(&unseen_epub).unwrap();
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        58.
    );
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .render_dpi,
        212
    );
    app.input(Input::PageForward);
    app.save().unwrap();
    let unseen_epub_position = app.current_position().unwrap();
    app.open_path(&unseen_pdf).unwrap();
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().zoom, 1);
    app.save().unwrap();
    let unseen_pdf_position = app.current_position().unwrap();
    drop(app);
    let mut reopened = reader(&target);
    reopened.open_path(&target.book).unwrap();
    assert_eq!(reopened.current_position(), epub_state.position);
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        72.
    );
    assert_eq!(
        reopened.library.state(epub_id).unwrap().highlights[0]
            .note
            .as_deref(),
        Some("local")
    );
    assert_eq!(
        reopened.library.state(epub_id).unwrap().bookmarks.len(),
        epub_state.bookmarks.len()
    );
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .render_dpi,
        96
    );
    reopened.open_path(&target_pdf).unwrap();
    assert_eq!(reopened.current_position(), pdf_state.position);
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences(),
        pdf_state.fixed_preferences
    );
    assert_eq!(
        reopened.library.state(pdf_id).unwrap().pdf_highlights[0]
            .note
            .as_deref(),
        Some("local")
    );
    assert_eq!(
        reopened.library.state(pdf_id).unwrap().bookmarks.len(),
        pdf_state.bookmarks.len()
    );
    reopened.open_path(&unseen_epub).unwrap();
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        58.
    );
    assert_eq!(reopened.current_position(), Some(unseen_epub_position));
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .render_dpi,
        212
    );
    reopened.open_path(&unseen_pdf).unwrap();
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences().zoom,
        1
    );
    assert_eq!(reopened.current_position(), Some(unseen_pdf_position));
    for (path, hash) in protected {
        assert_eq!(
            sha(&fs::read(path).unwrap()),
            hash,
            "A source publication changed"
        );
    }
}
