use super::*;
use crate::app::{
    Action, position,
    tests::{Fixture, tap_contents_item},
};
use primitive_app::App;

fn source_point(app: &mut Reader, label: &str) -> (f32, f32) {
    let reading = &app.active.as_ref().unwrap().reading;
    let first = reading.probe.matches(label).into_iter().next().unwrap();
    let page = reading.probe.page_for(&first).unwrap();
    let budget = reading.probe.page_count().saturating_add(1);
    // Prepare the actual page through ordinary input. A direct jump installs a
    // transient passage caret even when the displayed page has not changed.
    for _ in 0..budget {
        let current = app.active.as_ref().unwrap().reading.page;
        if current == page {
            break;
        }
        app.input(if current < page {
            Input::PageForward
        } else {
            Input::PageBack
        });
    }
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.page, page);
    let mut end = first.clone();
    end.offset += label.len();
    let selected = reading.probe.select_range(&first, &end).unwrap();
    let rect = reading.highlight_rects(&selected)[0];
    (
        (rect.x + rect.width / 2) as f32,
        (rect.y + rect.height / 2) as f32,
    )
}
fn screenshot(app: &Reader, case: &str, fonts: &[std::path::PathBuf], palettes: &[bool]) {
    let Some(folder) = std::env::var_os("READER_FOOTNOTE_ROOT_SCREENS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let paths = fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
    for &dark in palettes {
        let pixels = primitive_app::frame(app, &mut canvas, dark);
        primitive_app::write_png(
            &folder.join(format!(
                "footnote-{case}-{}.png",
                if dark { "dark" } else { "light" }
            )),
            WIDTH,
            HEIGHT,
            &pixels,
        )
        .unwrap();
    }
}

#[test]
fn native_oracle_popup_roots_render_inline_styles_and_preserve_source_page_and_exit() {
    let fixture = Fixture::new();
    std::fs::copy(
        crate::test_fixture("footnote-export-roots.epub"),
        &fixture.book,
    )
    .unwrap();
    let original = std::fs::read(&fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(
        app.controls.footnote_popups,
        "Fresh profiles must enable stock note previews"
    );
    for (number, case, word, visible) in [
        ("1", "A", "INLINE INNER NOTE A", true),
        ("2", "B", "INLINE ASIDE NOTE B", false),
        ("3", "C", "LINKED INNER NOTE C", true),
        ("4", "D", "LINKED ASIDE NOTE D", true),
        ("5", "E", "VISIBLE INLINE TARGET E", true),
        ("6", "F", "HIDDEN BLOCK TARGET F", true),
        ("7", "G", "HIDDEN INLINE TARGET G", false),
        ("8", "H", "VISIBLE DIV TARGET H", true),
    ] {
        let point = source_point(&mut app, number);
        let reading = &mut app.active.as_mut().unwrap().reading;
        let before = position(reading);
        let frame = reading.probe.capture(reading.page).unwrap();
        let history = app.back.len();
        app.input(Input::Tap(point.0, point.1));
        assert_eq!(app.screen, Screen::Footnote, "{case}");
        let popup = app.footnote.as_ref().unwrap();
        assert_eq!(!popup.preview.matches(word).is_empty(), visible, "{case}");
        if case == "B" {
            assert!(
                popup.frame.pixels.iter().all(|p| *p == 255),
                "inline display:none must remain effective"
            );
        }
        if matches!(case, "E" | "G" | "H") {
            assert!(
                !popup.preview.matches("Visible before").is_empty(),
                "{case}"
            );
        }
        if case == "F" {
            assert!(popup.preview.matches("Hidden before").is_empty());
        }
        if case != "B" {
            assert!(popup.frame.pixels.iter().any(|p| *p < 200), "{case}");
        }
        screenshot(&app, case, &fonts, &[false, true]);
        let close = popup.close_button();
        app.input(Input::Tap(
            (close.x + close.w / 2) as f32,
            (close.y + close.h / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading),
            before,
            "{case}"
        );
        assert_eq!(app.back.len(), history, "{case}");
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(
            reading.probe.capture(reading.page).unwrap(),
            frame,
            "{case}"
        );
    }
    let saved = position(&app.active.as_ref().unwrap().reading);
    let reading = &mut app.active.as_mut().unwrap().reading;
    let saved_frame = reading.probe.capture(reading.page).unwrap();
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(position(&reopened.active.as_ref().unwrap().reading), saved);
    let reading = &mut reopened.active.as_mut().unwrap().reading;
    assert_eq!(reading.probe.capture(reading.page).unwrap(), saved_frame);
    assert_eq!(std::fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn footnote_book_font_toggle_keeps_the_same_document_location_and_other_settings() {
    use std::fs;

    let fixture = Fixture::new();
    fs::copy(
        crate::test_fixture("footnote-export-roots.epub"),
        &fixture.book,
    )
    .unwrap();
    let original = fs::read(&fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let first = open();
    assert!(first.controls.footnote_popups);
    assert!(!first.controls.footnote_book_font);
    fs::write(&first.controls_path, br#"{"footnote_popups":true,"prefer_footnotes":true,"swipe_to_follow_nearest_link":true,"search_rows":4}"#).unwrap();
    drop(first);

    let open_note = |app: &mut Reader| {
        let (x, y) = source_point(app, "1");
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Footnote);
        let popup = app.footnote.as_ref().unwrap();
        assert!(!popup.preview.matches("INLINE INNER NOTE A").is_empty());
        popup.frame.pixels.clone()
    };
    let close_note = |app: &mut Reader| {
        let close = app.footnote.as_ref().unwrap().close_button();
        assert_eq!(
            app.input(Input::Tap(
                (close.x + close.w / 2) as f32,
                (close.y + close.h / 2) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
    };
    let toggle = |app: &mut Reader| {
        app.activate(Action::Show(Screen::LinkSettings)).unwrap();
        tap_contents_item(app, "Use book font in popups");
        app.activate(Action::Show(Screen::Book)).unwrap();
    };

    let mut app = open();
    let before = app.controls;
    assert!(before.prefer_footnotes && before.swipe_to_follow_nearest_link);
    assert_eq!(before.search_rows, Some(4));
    assert!(!before.footnote_book_font);
    app.open_path(&fixture.book).unwrap();
    let default_popup = open_note(&mut app);
    close_note(&mut app);
    fs::create_dir_all(&app.fonts_directory).unwrap();
    let custom = "Silkscreen-Regular.ttf";
    let custom_path = app.fonts_directory.join(custom);
    fs::copy(
        crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        &custom_path,
    )
    .unwrap();
    app.activate(Action::SetFont(Some(custom.into()))).unwrap();
    source_point(&mut app, "1");
    let location = app.current_history_location();
    let history = (app.back.len(), app.forward.len());
    let reading = &mut app.active.as_mut().unwrap().reading;
    let document_settings = reading.config.settings;
    let screen = reading.probe.capture(reading.page).unwrap();
    assert_eq!(reading.config.fonts[0], custom_path);

    assert_eq!(open_note(&mut app), default_popup);
    screenshot(&app, "font-default", &fonts, &[false]);
    close_note(&mut app);
    assert_eq!(app.current_history_location(), location);
    toggle(&mut app);
    let mut expected = before;
    expected.footnote_book_font = true;
    assert_eq!(app.controls, expected);
    let custom_popup = open_note(&mut app);
    assert_ne!(custom_popup, default_popup);
    screenshot(&app, "font-book", &fonts, &[false]);
    app.controls.night_pages = true;
    screenshot(&app, "font-book", &fonts, &[true]);
    app.controls = expected;
    close_note(&mut app);
    assert_eq!(app.current_history_location(), location);
    assert_eq!((app.back.len(), app.forward.len()), history);
    let reading = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(reading.config.settings, document_settings);
    assert_eq!(reading.probe.capture(reading.page).unwrap(), screen);
    app.save().unwrap();
    drop(app);

    let mut reopened = open();
    assert_eq!(reopened.controls, expected);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.current_history_location(), location);
    assert_eq!(open_note(&mut reopened), custom_popup);
    close_note(&mut reopened);
    toggle(&mut reopened);
    assert_eq!(reopened.controls, before);
    assert_eq!(open_note(&mut reopened), default_popup);
    close_note(&mut reopened);
    assert_eq!(reopened.current_history_location(), location);
    toggle(&mut reopened);
    fs::remove_file(&custom_path).unwrap();
    assert_eq!(reopened.controls, expected);
    assert_eq!(open_note(&mut reopened), default_popup);
    close_note(&mut reopened);
    assert_eq!(reopened.current_history_location(), location);
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn footnote_font_sizes_follow_book_and_fixed_settings_without_clipping_the_popup() {
    use crate::footnotes::font_size::FontSize;
    use std::io::{Read, Write};

    let fixture = Fixture::new();
    let original = std::fs::File::open(crate::test_fixture("footnote-export-roots.epub")).unwrap();
    let mut original = zip::ZipArchive::new(original).unwrap();
    let mut authored = zip::ZipWriter::new(std::fs::File::create(&fixture.book).unwrap());
    for index in 0..original.len() {
        let mut entry = original.by_index(index).unwrap();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        if entry.name() == "OPS/chapter.xhtml" {
            content = String::from_utf8(content)
                .unwrap()
                .replace(
                    "<p id=\"noteC\">LINKED INNER NOTE C",
                    "<p id=\"noteC\" style=\"line-height:3\">LINKED INNER NOTE C",
                )
                .into_bytes();
        }
        authored
            .start_file(entry.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        authored.write_all(&content).unwrap();
    }
    authored.finish().unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let id = app.active.as_ref().unwrap().id;
    app.activate(Action::ApplyLogicalFont(id, 32.0)).unwrap();
    app.activate(Action::Show(Screen::Book)).unwrap();
    let document = app.active.as_ref().unwrap().reading.config.settings;

    for (label, value, effective, screenshot_name) in [
        ("1", FontSize::default(), document.font_size - 2.0, None),
        (
            "1",
            FontSize::Relative(-10),
            document.font_size - 10.0,
            None,
        ),
        ("1", FontSize::Relative(5), document.font_size + 5.0, None),
        (
            "1",
            FontSize::Fixed(12),
            document.font_scale.scaled_pixels(12.0) as f32,
            Some("size-small"),
        ),
        (
            "1",
            FontSize::Fixed(255),
            MAX_FONT_PIXELS,
            Some("size-large"),
        ),
        ("3", FontSize::Fixed(255), MAX_FONT_PIXELS, None),
    ] {
        app.activate(Action::SetReadingControls(
            crate::reading_controls::ReadingControls {
                footnote_font_size: value,
                ..app.controls
            },
        ))
        .unwrap();
        let (x, y) = source_point(&mut app, label);
        let location = app.current_history_location();
        let history = (app.back.len(), app.forward.len());
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Footnote, "{label} {value:?}");
        let preview = app.footnote.as_ref().unwrap();
        assert_eq!(
            preview.preview.settings.font_size, effective,
            "{label} {value:?}"
        );
        assert!(preview.frame.pixels.iter().any(|pixel| *pixel < 160));
        assert!(preview.panel().h <= HEIGHT as i32 / 3);
        assert!(preview.panel().bottom() <= HEIGHT as i32);
        if value == FontSize::Fixed(12) {
            assert_eq!(preview.preview.page_count(), 1);
            assert!(preview.content_height() < CONTENT_HEIGHT / 2);
        }
        if let Some(name) = screenshot_name {
            screenshot(&app, name, &fonts, &[false]);
        }
        if value == FontSize::Fixed(255) {
            assert!(preview.preview.page_count() > 1);
            assert_eq!(app.input(Input::PageForward), Change::Partial);
            let following = app.footnote.as_ref().unwrap();
            assert_eq!(following.page, 1);
            assert!(following.frame.pixels.iter().any(|pixel| *pixel < 160));
        }
        let close = app.footnote.as_ref().unwrap().close_button();
        assert_eq!(
            app.input(Input::Tap(
                (close.x + close.w / 2) as f32,
                (close.y + close.h / 2) as f32
            )),
            Change::Partial
        );
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), location);
        assert_eq!((app.back.len(), app.forward.len()), history);
        assert_eq!(
            app.active.as_ref().unwrap().reading.config.settings,
            document
        );
    }
    app.activate(Action::Text(Settings {
        font_size: 12.0,
        logical_font_size: None,
        ..document
    }))
    .unwrap();
    app.activate(Action::SetReadingControls(
        crate::reading_controls::ReadingControls {
            footnote_font_size: FontSize::Relative(-10),
            ..app.controls
        },
    ))
    .unwrap();
    let (x, y) = source_point(&mut app, "1");
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    let preview = app.footnote.as_ref().unwrap();
    assert_eq!(preview.preview.settings.font_size, MIN_FONT_PIXELS);
    assert!(preview.frame.pixels.iter().any(|pixel| *pixel < 160));
}

#[test]
#[ignore = "Requires an authorized unchanged EPUB via READER_FOOTNOTE_ORIGINAL"]
fn original_document_note_links_open_by_default_and_preserve_source_frame_and_progress() {
    use std::collections::BTreeSet;
    let original_path =
        std::path::PathBuf::from(std::env::var_os("READER_FOOTNOTE_ORIGINAL").unwrap());
    let original = std::fs::read(&original_path).unwrap();
    let fixture = Fixture::new();
    std::fs::copy(&original_path, &fixture.book).unwrap();
    let fonts = std::env::var_os("READER_FOOTNOTE_ORIGINAL_FONTS")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]);
    let mut book = Book::open(&fixture.book).unwrap();
    let mut references = Vec::new();
    let mut destinations = BTreeSet::new();
    for chapter in 0..book.chapters.len() {
        let href = book.chapters[chapter].href.clone();
        let xml = book.chapter(chapter).unwrap();
        let source = book.chapter_source(&href, &xml).unwrap();
        for block in &source.blocks {
            for link in &block.links {
                if link.note_reference != crate::footnotes::Reference::Explicit
                    || link.bytes.is_empty()
                {
                    continue;
                }
                let target = Book::resolve_link(&href, &link.href).unwrap();
                let Some(fragment) = target.fragment.as_ref() else {
                    continue;
                };
                if book.chapter_index(&target.path).is_none()
                    || !destinations.insert((target.path.clone(), fragment.clone()))
                {
                    continue;
                }
                references.push((chapter, block.key, link.bytes.clone(), target));
            }
        }
    }
    assert!(
        !references.is_empty(),
        "The unchanged publication must have real semantic note references"
    );
    drop(book);
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    assert!(app.controls.footnote_popups);
    app.open_path(&fixture.book).unwrap();
    for (chapter, key, bytes, target) in &references {
        let probe = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .chapter(*chapter)
            .unwrap();
        let start = probe.anchor(*key, bytes.start);
        let end = probe.anchor(*key, bytes.end);
        let page = probe.page_for(&start).unwrap();
        app.follow_reflow_page(*chapter, page).unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        let selected = reading.probe.select_range(&start, &end).unwrap();
        let rect = reading.highlight_rects(&selected)[0];
        let before = position(reading);
        let source_frame = reading.probe.capture(reading.page).unwrap();
        let history = app.back.len();
        app.input(Input::Tap(
            (rect.x + rect.width / 2) as f32,
            (rect.y + rect.height / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Footnote);
        let popup = app.footnote.as_ref().unwrap();
        assert_eq!(&popup.target, target);
        assert!(popup.frame.pixels.iter().any(|p| *p < 200));
        let close = popup.close_button();
        app.input(Input::Tap(
            (close.x + close.w / 2) as f32,
            (close.y + close.h / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.back.len(), history);
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(position(reading), before);
        assert_eq!(reading.probe.capture(reading.page).unwrap(), source_frame);
    }
    let reading = &mut app.active.as_mut().unwrap().reading;
    let saved = position(reading);
    let frame = reading.probe.capture(reading.page).unwrap();
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(position(&reopened.active.as_ref().unwrap().reading), saved);
    let reading = &mut reopened.active.as_mut().unwrap().reading;
    assert_eq!(reading.probe.capture(reading.page).unwrap(), frame);
    assert_eq!(std::fs::read(&original_path).unwrap(), original);
    assert_eq!(std::fs::read(&fixture.book).unwrap(), original);
    println!(
        "Original semantic note popups verified: {}",
        references.len()
    );
}
