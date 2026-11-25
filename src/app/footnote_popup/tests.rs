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
fn foreign_note_popup_reference_uses_its_canonical_chapter() {
    let path = crate::test_fixture("harbor-continuation.fb3");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let host = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 0).unwrap();
    let target = Probe::open(&path, Engine::Parley, &fonts, Settings::default(), 1).unwrap();
    let start = target.matches("Return to the boat").pop().unwrap();
    let mut end = start.clone();
    end.offset += "Return to the boat".len();
    let selected = target.select_range(&start, &end).unwrap();
    assert_eq!(reference_chapter(&host, 0, Some(&selected)), Some(1));
    assert_eq!(reference_chapter(&host, 0, None), Some(0));

    let mut invalid = selected.clone();
    invalid.start.chapter = "missing.xhtml".into();
    invalid.end.chapter = "missing.xhtml".into();
    assert_eq!(reference_chapter(&host, 0, Some(&invalid)), None);
    let mut mixed = selected.clone();
    mixed.end.chapter = "document.xhtml".into();
    assert_eq!(reference_chapter(&host, 0, Some(&mixed)), None);
    let mut wrong_book = selected;
    wrong_book.start.book ^= 1;
    assert_eq!(reference_chapter(&host, 0, Some(&wrong_book)), None);
}

#[test]
fn landscape_footnote_stages_repaginated_note_and_returns_to_the_exact_reading_anchor() {
    use crate::app::{ReadingOrientation, reading_viewport_stage::draw_test_candidate};
    use primitive_app::viewport::{LogicalTurn, ViewportDecision, ViewportRequest};

    let fixture = Fixture::new();
    std::fs::copy(crate::test_fixture("night-landing.epub"), &fixture.book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(&fixture.book).unwrap();
    let portrait = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(portrait, ViewportDecision::Commit);
    let (x, y) = source_point(&mut app, "[2]");
    let origin = app.active.as_ref().unwrap().reading.anchor().unwrap();
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::Footnote);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let popup = app.footnote.as_ref().unwrap();
    let cursor = popup.cursor.clone().unwrap();
    let portrait_panel = popup.panel();
    assert_eq!(popup.page, 1);

    app.set_reading_orientation(ReadingOrientation::LandscapeRight)
        .unwrap();
    let attempted = draw_test_candidate(&mut app, 3, LogicalTurn::Clockwise, "", false);
    assert_eq!(app.footnote.as_ref().unwrap().panel(), portrait_panel);
    app.finish_viewport(attempted, ViewportDecision::Abort);
    assert_eq!(app.reading_viewport(), ReadingViewport::PORTRAIT);
    assert_eq!(
        app.footnote.as_ref().unwrap().cursor.as_ref(),
        Some(&cursor)
    );

    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let landscape = draw_test_candidate(
        &mut app,
        5,
        LogicalTurn::Counterclockwise,
        "landscape-footnote",
        false,
    );
    app.finish_viewport(landscape, ViewportDecision::Commit);
    assert_eq!(app.screen, Screen::Footnote);
    let popup = app.footnote.as_ref().unwrap();
    let panel = popup.panel();
    assert_eq!(popup.viewport, ReadingViewport::LANDSCAPE);
    assert_eq!(popup.cursor.as_ref(), Some(&cursor));
    assert_eq!(popup.preview.page_for(&cursor), Some(popup.page));
    assert!(
        panel.y >= 1264 * 2 / 3 && panel.right() == 1680 - INSET && panel.bottom() == 1264 - INSET
    );
    assert_eq!(popup.frame.width, 1680 - 4 * INSET as u32);

    let paths: Vec<_> = app.fonts.iter().map(|p| p.display().to_string()).collect();
    let mut canvas = Canvas::new_viewport(1680, 1264, 1264, &paths).unwrap();
    let frame = primitive_app::frame(&app, &mut canvas, false);
    assert!((1230..1264).all(|y| {
        frame[y * 1680 + 1673..(y + 1) * 1680]
            .iter()
            .all(|&ink| ink > 230)
    }));

    let close = popup.close_button();
    assert_eq!(
        app.input(Input::Tap(
            (close.right() - 20) as f32,
            (close.y + 15) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(LogicalTurn::Counterclockwise))
    );
    assert_eq!(
        app.active.as_ref().unwrap().reading.anchor().as_ref(),
        Some(&origin)
    );

    let (x, y) = source_point(&mut app, "[2]");
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    assert_eq!(
        app.footnote.as_ref().unwrap().viewport,
        ReadingViewport::LANDSCAPE
    );
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert!(app.back.is_empty());
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
            assert!(preview.content_height() < preview.frame.height / 2);
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

#[test]
fn footnote_justification_is_independent_and_keeps_rtl_and_source_selection() {
    use crate::TextAlignment;
    use crate::app::selection_viewport_tests::{adopt, capture, tap};
    use std::io::{Read, Write};

    let fixture = Fixture::new();
    let base = std::fs::File::open(crate::test_fixture("footnote-export-roots.epub")).unwrap();
    let mut base = zip::ZipArchive::new(base).unwrap();
    let mut book = zip::ZipWriter::new(std::fs::File::create(&fixture.book).unwrap());
    let ltr = "The sheltered harbor contains a tide gauge, four narrow channels, and a marker beside the eastern approach. ".repeat(4);
    let rtl = "تبدأ الرحلة من الميناء ثم تمر بالقناة الضيقة قبل الوصول إلى الرصيف. ".repeat(4);
    for index in 0..base.len() {
        let mut entry = base.by_index(index).unwrap();
        let mut source = Vec::new();
        entry.read_to_end(&mut source).unwrap();
        if entry.name() == "OPS/chapter.xhtml" {
            source = format!(r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
                <head><title>Harbor notes</title></head><body><p>Read the harbor entry <a epub:type="noteref" href="#left">[1]</a>
                and the parallel entry <a epub:type="noteref" href="#right">[2]</a>.</p>
                <aside epub:type="footnote"><p id="left">{ltr}</p><p dir="rtl" lang="ar" id="right">{rtl}</p></aside>
                </body></html>"##).into_bytes();
        }
        book.start_file(entry.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        book.write_all(&source).unwrap();
    }
    book.finish().unwrap();

    let fonts = [
        "fonts/NotoSans-Regular.ttf",
        "fonts/NotoSansArabic-Regular.ttf",
    ]
    .into_iter()
    .map(crate::test_fixture)
    .collect::<Vec<_>>();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    assert_eq!(app.controls.footnote_justify, None);
    app.open_path(&fixture.book).unwrap();
    let document = Settings {
        alignment: TextAlignment::Center,
        ..app.active.as_ref().unwrap().reading.config.settings
    };
    app.activate(Action::Text(document)).unwrap();
    app.show(Screen::Book);
    let origin = app.current_history_location();
    let history = app.back.clone();
    let open = |app: &mut Reader, n| {
        let (x, y) = source_point(app, n);
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Footnote);
    };
    let close = |app: &mut Reader| {
        let button = app.footnote.as_ref().unwrap().close_button();
        tap(app, button);
    };
    let word = |app: &mut Reader| {
        let note = app.footnote.as_mut().unwrap();
        let start = note
            .preview
            .matches("sheltered")
            .into_iter()
            .next()
            .unwrap();
        let mut end = start.clone();
        end.offset += "sheltered".len();
        let quote = note.preview.select_range(&start, &end).unwrap();
        let rect = note.preview.selection_rects(note.page, &quote)[0];
        (quote, rect)
    };

    open(&mut app, "[1]");
    let ltr_on = app.footnote.as_ref().unwrap().frame.pixels.clone();
    let (quote, rect_on) = word(&mut app);
    screenshot(&app, "justify-on", &fonts, &[false]);
    let note = app.footnote.as_mut().unwrap();
    let body = note.content();
    assert_eq!(
        note.select_at(
            (body.x + rect_on.x + rect_on.width / 2) as f32,
            (body.y + rect_on.y + rect_on.height / 2) as f32,
            false
        )
        .as_deref(),
        Some("sheltered")
    );
    let selection = note.selection.clone();
    let mut wide = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    assert_eq!(app.footnote.as_ref().unwrap().selection, selection);
    capture(&app, &mut wide, "footnote-justify-landscape.png", false);
    adopt(&mut app, ReadingViewport::PORTRAIT, 3);
    close(&mut app);
    open(&mut app, "[2]");
    let rtl_on = app.footnote.as_ref().unwrap().frame.pixels.clone();
    close(&mut app);

    app.show(Screen::LinkSettings);
    tap_contents_item(&mut app, "Justify text in popups");
    assert_eq!(app.controls.footnote_justify, Some(false));
    app.show(Screen::Book);
    open(&mut app, "[1]");
    let (same_quote, rect_off) = word(&mut app);
    assert_eq!(same_quote, quote);
    assert_ne!(rect_on.x, rect_off.x);
    assert_ne!(app.footnote.as_ref().unwrap().frame.pixels, ltr_on);
    screenshot(&app, "justify-off", &fonts, &[false]);
    close(&mut app);
    open(&mut app, "[2]");
    assert_eq!(app.footnote.as_ref().unwrap().frame.pixels, rtl_on);
    close(&mut app);
    assert_eq!(app.current_history_location(), origin);
    assert_eq!(app.back, history);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .alignment,
        TextAlignment::Center
    );
    app.save().unwrap();
    let reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    assert_eq!(reopened.controls.footnote_justify, Some(false));
}

#[test]
fn fb2_named_note_sections_open_from_real_links_and_preserve_the_source() {
    let fixture = Fixture::new();
    let name = "Workshop notes.fb2";
    let path = fixture.root.join(name);
    let original = std::fs::read(crate::test_fixture("workshop-notes.fb2")).unwrap();
    std::fs::write(&path, &original).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();

    let point = |reader: &mut Reader, label: &str| {
        let reading = &mut reader.active.as_mut().unwrap().reading;
        let anchor = reading.probe.matches(label).remove(0);
        reading.jump_to_anchor(reading.chapter, &anchor).unwrap();
        source_point(reader, label)
    };

    for (label, fragment, second_paragraph) in [
        (
            "Earlier workshop note",
            "earlier-note",
            "A second entry records the final measurement.",
        ),
        (
            "Afterword commentary",
            "later-comment",
            "The next entry explains how the panes dried.",
        ),
    ] {
        let (x, y) = point(&mut app, label);
        let location = app.current_history_location();
        let history = app.back.clone();
        let reading = &mut app.active.as_mut().unwrap().reading;
        let frame = reading.probe.capture(reading.page).unwrap();
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Footnote, "{label}");
        let note = app.footnote.as_ref().unwrap();
        assert_eq!(note.target.fragment.as_deref(), Some(fragment));
        assert_eq!(note.reference.as_ref().unwrap().text, label);
        assert!(note.preview.matches(second_paragraph).len() == 1);
        assert!(note.preview.matches("Unrelated account").is_empty());
        assert!(note.preview.matches("wooden crates").is_empty());
        assert_eq!(app.current_history_location(), location);
        assert_eq!(app.back, history);
        screenshot(&app, fragment, &fonts, &[false]);
        let close = note.close_button();
        app.input(Input::Tap(
            (close.x + close.w / 2) as f32,
            (close.y + close.h / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), location);
        assert_eq!(app.back, history);
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(reading.probe.capture(reading.page).unwrap(), frame);
    }

    let (x, y) = point(&mut app, "Earlier workshop note");
    let workshop = app.current_history_location();
    assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
    let note = app.footnote.as_ref().unwrap();
    let start = note.preview.matches("Return to the workshop.").remove(0);
    let mut end = start.clone();
    end.offset += "Return to the workshop.".len();
    let link = note.preview.select_range(&start, &end).unwrap();
    let content = note.content();
    let rect = note.preview.selection_rects(note.page, &link)[0];
    assert_eq!(
        app.input(Input::Tap(
            (content.x + rect.x + rect.width / 2) as f32,
            (content.y + rect.y + rect.height / 2) as f32,
        )),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.back.last(), workshop.as_ref());
    let reading = &app.active.as_ref().unwrap().reading;
    let main = reading.probe.fragment("main-start").unwrap();
    assert_eq!(reading.probe.page_for(&main), Some(reading.page));
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.current_history_location(), workshop);

    // Merely belonging to a named body does not make a paragraph a trusted
    // FB2 section. The author's ordinary forward link still follows the target.
    for label in ["Separate paragraph link", "Open the supply catalog"] {
        let (x, y) = point(&mut app, label);
        let before = app.back.len();
        assert_eq!(app.input(Input::Tap(x, y)), Change::Partial);
        assert_eq!(app.screen, Screen::Book, "{label}");
        assert!(app.footnote.is_none());
        assert_eq!(app.back.len(), before + 1);
        app.activate(Action::ReturnToLocation).unwrap();
    }

    // The original fictionbook bytes and canonical position survive a reopen.
    point(&mut app, "Earlier workshop note");
    app.record_position();
    let saved = app.current_history_location();
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(reopened.current_history_location(), saved);
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
