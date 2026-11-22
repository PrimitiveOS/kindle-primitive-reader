use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn open(fixture: &Fixture) -> Reader {
    Reader::open(&fixture.root, &fixture.state, fonts()).unwrap()
}
fn frame(app: &Reader) -> Vec<u8> {
    let paths: Vec<_> = fonts()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
    primitive_app::frame(app, &mut canvas, false)
}
fn size(app: &Reader) -> f32 {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .font_size
}
fn text_size(app: &mut Reader, size: f32) {
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: size,
        logical_font_size: None,
        ..settings
    }))
    .unwrap();
}
fn pdf(f: &Fixture) -> PathBuf {
    let path = f.root.join("Page.pdf");
    fs::write(&path, crate::fixed::tests::document()).unwrap();
    path
}

#[test]
fn templates_apply_only_to_unseen_books_and_freeze_across_changes_and_restart() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    let old_book = f.root.join("Known.txt");
    fs::write(&old_book, "An already known short book.").unwrap();
    app.open_path(&old_book).unwrap();
    let old = size(&app);
    app.save().unwrap();
    app.open_path(&f.book).unwrap();
    text_size(&mut app, 58.);
    app.activate_document_defaults(Command::SaveReflow).unwrap();
    let next = f.root.join("New.txt");
    fs::write(&next, "A newly opened book gets a distinct saved snapshot.").unwrap();
    app.open_path(&next).unwrap();
    assert_eq!(size(&app), 58.);
    app.save().unwrap();
    app.open_path(&old_book).unwrap();
    assert_eq!(size(&app), old);
    text_size(&mut app, 64.);
    app.activate_document_defaults(Command::SaveReflow).unwrap();
    app.save().unwrap();
    let mut app = open(&f);
    app.open_path(&next).unwrap();
    assert_eq!(size(&app), 58.);
    app.open_path(&old_book).unwrap();
    assert_eq!(size(&app), 64.);
    app.activate_document_defaults(Command::ClearReflow)
        .unwrap();
    let last = f.root.join("After clearing.txt");
    fs::write(&last, "A later book uses the device baseline.").unwrap();
    app.open_path(&last).unwrap();
    assert_eq!(size(&app), app.device_reading_settings().font_size);
    app.open_path(&next).unwrap();
    assert_eq!(size(&app), 58.);
}

#[test]
fn reset_reflow_keeps_source_annotations_and_missing_default_fonts_use_device_fonts() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let before = position(&app.active.as_ref().unwrap().reading);
    let id = app.active.as_ref().unwrap().id;
    let probe = &app.active.as_ref().unwrap().reading.probe;
    let start = probe
        .page_anchor(app.active.as_ref().unwrap().reading.page)
        .unwrap();
    let mut end = start.clone();
    end.offset += 1;
    let mark = probe.select_range(&start, &end).unwrap();
    app.library
        .state_mut(id)
        .unwrap()
        .highlights
        .push(Highlight {
            selection: mark,
            note: Some("Keep this note".into()),
            style: crate::highlight_style::HighlightStyle::Underline,
            color: Default::default(),
            created_at: None,
        });
    let marks = serde_json::to_value(&app.library.state(id).unwrap().highlights).unwrap();
    let mut prefs = app.current_text_preferences().unwrap();
    prefs.font_size = Some(59.);
    prefs.logical_font_size = None;
    prefs.font_file = Some("Gone.otf".into());
    let mut templates = app.document_defaults.templates.clone();
    templates.reflow = Some(prefs);
    app.document_defaults.save(templates).unwrap();
    app.activate_document_defaults(Command::ResetReflow)
        .unwrap();
    assert_eq!(size(&app), 59.);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), before.anchor);
    assert!(
        app.library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .is_none()
    );
    assert_eq!(
        serde_json::to_value(&app.library.state(id).unwrap().highlights).unwrap(),
        marks
    );
    assert!(app.notice.as_deref().unwrap().contains("font unavailable"));
    let later = f.root.join("New missing font.txt");
    fs::write(
        &later,
        "A new book remains readable when its default font was removed.",
    )
    .unwrap();
    app.open_path(&later).unwrap();
    assert_eq!(size(&app), 59.);
    assert!(
        app.library
            .state(app.active.as_ref().unwrap().id)
            .unwrap()
            .preferences
            .font_file
            .is_none()
    );
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&later).unwrap();
    assert_eq!(size(&reopened), 59.);
}

#[test]
fn failed_persistence_or_reflow_keeps_previous_preferences_position_and_visible_frame() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    app.save().unwrap();
    let mut template = app.current_text_preferences().unwrap();
    template.font_size = Some(71.);
    let mut templates = app.document_defaults.templates.clone();
    templates.reflow = Some(template);
    app.document_defaults.save(templates).unwrap();
    let id = app.active.as_ref().unwrap().id;
    let old = serde_json::to_value(app.library.state(id).unwrap()).unwrap();
    let pixels = frame(&app);
    let saved = fs::read(&f.state).unwrap();
    fs::remove_file(&f.state).unwrap();
    fs::create_dir(&f.state).unwrap();
    assert!(
        app.activate_document_defaults(Command::ResetReflow)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(app.library.state(id).unwrap()).unwrap(),
        old
    );
    assert_eq!(frame(&app), pixels);
    fs::remove_dir(&f.state).unwrap();
    fs::write(&f.state, saved).unwrap();
    let original = fs::read(&f.book).unwrap();
    fs::write(&f.book, b"not an EPUB").unwrap();
    assert!(
        app.activate_document_defaults(Command::ResetReflow)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(app.library.state(id).unwrap()).unwrap(),
        old
    );
    assert_eq!(frame(&app), pixels);
    fs::write(&f.book, original).unwrap();
}

#[test]
fn fixed_defaults_are_distinct_retain_book_geometry_and_rollback_before_replacing_the_frame() {
    let f = Fixture::new();
    let path = pdf(&f);
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    text_size(&mut app, 61.);
    app.activate_document_defaults(Command::SaveReflow).unwrap();
    app.open_path(&path).unwrap();
    app.activate(Action::SetFixedZoom(1)).unwrap();
    app.activate_document_defaults(Command::SaveFixed).unwrap();
    assert_eq!(
        app.document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .font_size,
        Some(61.)
    );
    app.activate(Action::SetFixedZoom(3)).unwrap();
    let mut geometry = app.fixed.as_ref().unwrap().reading.preferences();
    geometry.crop = Some(crate::fixed_text::PdfTextRect {
        x0: 0.1,
        y0: 0.1,
        x1: 0.9,
        y1: 0.9,
    });
    geometry.offset_x = 24.;
    geometry.offset_y = -18.;
    app.fixed
        .as_mut()
        .unwrap()
        .reading
        .restore_view(geometry)
        .unwrap();
    app.show(Screen::Fixed);
    app.record_position();
    app.save().unwrap();
    let id = app.fixed.as_ref().unwrap().id;
    let before = serde_json::to_value(app.library.state(id).unwrap()).unwrap();
    let pixels = frame(&app);
    let saved = fs::read(&f.state).unwrap();
    fs::remove_file(&f.state).unwrap();
    fs::create_dir(&f.state).unwrap();
    assert!(app.activate_document_defaults(Command::ResetFixed).is_err());
    assert_eq!(
        serde_json::to_value(app.library.state(id).unwrap()).unwrap(),
        before
    );
    assert_eq!(frame(&app), pixels);
    fs::remove_dir(&f.state).unwrap();
    fs::write(&f.state, saved).unwrap();
    let view = app.fixed.as_ref().unwrap().reading.preferences();
    let page = app.fixed.as_ref().unwrap().reading.page();
    app.activate_document_defaults(Command::ResetFixed).unwrap();
    let reset = app.fixed.as_ref().unwrap().reading.preferences();
    assert_eq!(reset.zoom, 1);
    assert_eq!(reset.crop, view.crop);
    assert_eq!(reset.offset_x, view.offset_x);
    // The lower zoom fits the entire cropped page vertically. The existing
    // renderer clamps unavailable overflow; it must not retain an invalid pan.
    assert_eq!(reset.offset_y, 0.);
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), page);
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&path).unwrap();
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences().zoom,
        1
    );
    app.activate_document_defaults(Command::ClearFixed).unwrap();
    assert!(app.document_defaults.templates.fixed.is_none());
    assert!(app.document_defaults.templates.reflow.is_some());
}

#[test]
fn fixed_templates_apply_to_unseen_content_without_changing_saved_documents() {
    let f = Fixture::new();
    let first = pdf(&f);
    let original = fs::read(&first).unwrap();
    let second = f.root.join("New PDF.pdf");
    let mut second_bytes = original.clone();
    second_bytes.extend_from_slice(b"\n% second authored edition\n");
    fs::write(&second, &second_bytes).unwrap();
    let third = f.root.join("After clearing.pdf");
    let mut third_bytes = original;
    third_bytes.extend_from_slice(b"\n% third authored edition\n");
    fs::write(&third, third_bytes).unwrap();
    let mut app = open(&f);
    app.open_path(&first).unwrap();
    app.activate(Action::SetFixedZoom(2)).unwrap();
    app.activate_document_defaults(Command::SaveFixed).unwrap();
    app.open_path(&second).unwrap();
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().zoom, 2);
    app.save().unwrap();
    app.open_path(&first).unwrap();
    app.activate(Action::SetFixedZoom(3)).unwrap();
    app.activate_document_defaults(Command::SaveFixed).unwrap();
    app.save().unwrap();
    let mut reopened = open(&f);
    reopened.open_path(&second).unwrap();
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences().zoom,
        2
    );
    reopened
        .activate_document_defaults(Command::ClearFixed)
        .unwrap();
    reopened.open_path(&third).unwrap();
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences().zoom,
        0
    );
    reopened.open_path(&first).unwrap();
    assert_eq!(
        reopened.fixed.as_ref().unwrap().reading.preferences().zoom,
        3
    );
}

#[test]
fn explicit_default_controls_use_icons_and_pressed_feedback() {
    let f = Fixture::new();
    let mut app = open(&f);
    app.open_path(&f.book).unwrap();
    let pdf = pdf(&f);
    let paths: Vec<_> = fonts()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for fixed in [false, true] {
        if fixed {
            app.open_path(&pdf).unwrap();
            app.activate_document_defaults(Command::SaveFixed).unwrap();
        } else {
            app.activate_document_defaults(Command::SaveReflow).unwrap();
        }
        capture_test_screen(
            &app,
            &fonts(),
            if fixed {
                "READER_FIXED_DEFAULTS_CAPTURE"
            } else {
                "READER_DEFAULTS_CAPTURE"
            },
            false,
        );
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        let plain = primitive_app::frame(&app, &mut canvas, false);
        for (rect, command, _) in controls(fixed) {
            assert!(app.default_command_available(command));
            canvas.press_at((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
            let pressed = primitive_app::frame(&app, &mut canvas, false);
            assert_ne!(plain, pressed);
            for (i, (a, b)) in plain.iter().zip(&pressed).enumerate() {
                if a != b {
                    assert!(
                        rect.contains((i % WIDTH as usize) as f32, (i / WIDTH as usize) as f32)
                    );
                }
            }
            canvas.clear_press();
        }
    }
}

#[test]
#[ignore = "Requires privately selected authorized original EPUB/PDF copies"]
fn original_epub_and_pdf_defaults_keep_source_positions_and_bytes() {
    let root = PathBuf::from(std::env::var_os("READER_DEFAULTS_ORIGINAL_ROOT").unwrap());
    let list = PathBuf::from(std::env::var_os("READER_DEFAULTS_ORIGINAL_LIST").unwrap());
    let paths: Vec<String> = serde_json::from_slice(&fs::read(list).unwrap()).unwrap();
    assert!(!paths.is_empty() && paths.len() <= 4);
    for path in paths {
        let source = root.join(path);
        let bytes = fs::read(&source).unwrap();
        let f = Fixture::new();
        let copy = f.root.join(format!(
            "Publication.{}",
            source.extension().unwrap().to_string_lossy()
        ));
        fs::write(&copy, &bytes).unwrap();
        let mut app = open(&f);
        app.open_path(&copy).unwrap();
        if app.active.is_some() {
            let initial = app.active.as_ref().unwrap().reading.anchor();
            text_size(&mut app, 56.);
            app.activate_document_defaults(Command::SaveReflow).unwrap();
            text_size(&mut app, 41.);
            app.activate_document_defaults(Command::ResetReflow)
                .unwrap();
            assert_eq!(size(&app), 56.);
            assert_eq!(app.active.as_ref().unwrap().reading.anchor(), initial);
        } else {
            let page = app.fixed.as_ref().unwrap().reading.page();
            app.activate(Action::SetFixedZoom(1)).unwrap();
            app.activate_document_defaults(Command::SaveFixed).unwrap();
            app.activate(Action::SetFixedZoom(2)).unwrap();
            app.activate_document_defaults(Command::ResetFixed).unwrap();
            assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().zoom, 1);
            assert_eq!(app.fixed.as_ref().unwrap().reading.page(), page);
        }
        let id = app.open_book_id().unwrap();
        let position = app.library.state(id).unwrap().position.clone();
        app.save().unwrap();
        let mut reopened = open(&f);
        reopened.open_path(&copy).unwrap();
        assert_eq!(reopened.library.state(id).unwrap().position, position);
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(fs::read(copy).unwrap(), bytes);
    }
}
