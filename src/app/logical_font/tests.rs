use super::*;
use crate::app::tests::Fixture;

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn setup() -> (Fixture, Reader, PathBuf) {
    let fixture = Fixture::new();
    let path = fixture.root.join("Logical font.xhtml");
    fs::write(&path,"<html><head><title>Font size</title></head><body><p id='passage'>Harbor lanterns return at dusk. <a href='#end'>Follow the light</a> across the water.</p><p id='end'>End.</p></body></html>").unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    (fixture, app, path)
}
fn current(app: &Reader) -> Option<f64> {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .logical_font_size
}
fn tap(app: &mut Reader, rect: Rect) {
    assert!(rect.w >= 100 && rect.h >= 100);
    app.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ));
}
fn editor(app: &mut Reader) {
    let id = app.active.as_ref().unwrap().id;
    app.show(Screen::NumberingValue(NumberField::LogicalFont(id)));
}
fn draft(app: &mut Reader, text: &str) {
    app.editor.text = text.chars().collect();
    app.editor.caret = app.editor.text.len();
}

fn keyboard(app: &mut Reader, key: primitive_ui::Key) {
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    if !layout
        .keyboard_keys_with(app.editor.layer, "Set")
        .iter()
        .any(|(item, _)| item.key == key)
    {
        let layer = [
            primitive_ui::Layer::Lower,
            primitive_ui::Layer::Symbols,
            primitive_ui::Layer::More,
        ]
        .into_iter()
        .find(|&layer| {
            layout
                .keyboard_keys_with(layer, "Set")
                .iter()
                .any(|(item, _)| item.key == key)
        })
        .unwrap();
        let rect = layout
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(item, _)| item.key == primitive_ui::Key::Layer(layer))
            .unwrap()
            .1;
        tap(app, rect);
    }
    let rect = layout
        .keyboard_keys_with(app.editor.layer, "Set")
        .into_iter()
        .find(|(item, _)| item.key == key)
        .unwrap()
        .1;
    tap(app, rect);
}
fn type_number(app: &mut Reader, value: &str) {
    while !app.editor.text.is_empty() {
        keyboard(app, primitive_ui::Key::Delete);
    }
    for ch in value.chars() {
        keyboard(app, primitive_ui::Key::Char(ch));
    }
    assert_eq!(
        app.editor.string(),
        value,
        "all decimal characters must pass the real keyboard capacity"
    );
}

#[test]
fn real_keyboard_accepts_fifth_decimal_character_and_applies_the_raw_logical_value() {
    let (_fixture, mut app, _) = setup();
    for (text, value) in [("12.25", 12.25), ("22.25", 22.25), ("255.0", 255.)] {
        editor(&mut app);
        type_number(&mut app, text);
        keyboard(&mut app, primitive_ui::Key::Go);
        assert_eq!(current(&app), Some(value));
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .font_size,
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .font_scale
                .effective(value)
                .unwrap()
        );
    }
}

#[test]
fn draft_steps_typed_decimals_saved_default_and_cancel_preserve_legacy_mode() {
    let (_fixture, mut app, path) = setup();
    let id = app.active.as_ref().unwrap().id;
    assert_eq!(current(&app), Some(22.));
    app.library.state_mut(id).unwrap().preferences = BookPreferences {
        font_size: Some(42.25),
        ..Default::default()
    };
    app.save().unwrap();
    app.open_path(&path).unwrap();
    assert_eq!(current(&app), None);
    let mut template = app.document_defaults.templates.clone();
    template.reflow = Some(BookPreferences {
        logical_font_size: Some(24.25),
        ..Default::default()
    });
    app.document_defaults.save(template).unwrap();
    editor(&mut app);
    assert_eq!(app.editor.string(), "22");
    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "24.25");
    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "24.75");
    app.input(Input::LongTap(
        (DECREASE.x + 50) as f32,
        (DECREASE.y + 50) as f32,
    ));
    assert_eq!(app.editor.string(), "20.75");
    app.input(Input::PageBack);
    assert_eq!(current(&app), None);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        42.25
    );
    editor(&mut app);
    type_number(&mut app, "22.25");
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(current(&app), Some(22.25));
    assert_eq!(
        app.library.state(id).unwrap().preferences.font_size,
        Some(42.25)
    );
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        render_dpi: 123,
        ..settings
    }))
    .unwrap();
    assert_eq!(
        app.library.state(id).unwrap().preferences.font_size,
        Some(42.25)
    );
    assert_eq!(current(&app), Some(22.25));
}

#[test]
fn logical_apply_failure_stale_edition_and_invalid_draft_leave_frame_state_anchor_intact() {
    let (fixture, mut app, path) = setup();
    let id = app.active.as_ref().unwrap().id;
    editor(&mut app);
    app.save().unwrap();
    let before = app.library.bytes().unwrap();
    let anchor = app.active.as_ref().unwrap().reading.anchor();
    let frame = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    for bad in ["11.9", "255.1", "NaN", "inf"] {
        draft(&mut app, bad);
        assert!(app.apply_logical_font_input(id).is_err());
        assert_eq!(app.library.bytes().unwrap(), before);
    }
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.replace("Harbor", "Market")).unwrap();
    assert!(app.apply_logical_font(id, 80.).is_err());
    assert_eq!(app.library.bytes().unwrap(), before);
    fs::write(&path, original).unwrap();
    let saved = fixture.root.join("Saved state");
    fs::rename(&fixture.state, &saved).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    assert!(app.apply_logical_font(id, 160.).is_err());
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        frame
    );
    fs::remove_dir(&fixture.state).unwrap();
    fs::rename(&saved, &fixture.state).unwrap();
    app.apply_logical_font(id, 160.).unwrap();
    assert_eq!(current(&app), Some(160.));
}

#[test]
fn logical_profiles_defaults_backups_roundtrip_raw_size_while_legacy_remains_exact() {
    let (fixture, mut app, path) = setup();
    let id = app.active.as_ref().unwrap().id;
    app.apply_logical_font(id, 255.).unwrap();
    let prefs = app.current_text_preferences().unwrap();
    assert_eq!(prefs.logical_font_size, Some(255.));
    assert_ne!(prefs.font_size, Some(340.));
    app.save_reading_profile("Large logical").unwrap();
    let mut templates = app.document_defaults.templates.clone();
    templates.reflow = Some(prefs.clone());
    app.document_defaults.save(templates).unwrap();
    let bytes = app.reading_backup_bytes().unwrap();
    crate::reading_backup::Envelope::parse(&bytes).unwrap();
    let target = Fixture::new();
    let copy = target.root.join(path.file_name().unwrap());
    fs::copy(&path, &copy).unwrap();
    let mut restored = Reader::open(&target.root, &target.state, fonts()).unwrap();
    fs::write(&restored.import_path, &bytes).unwrap();
    restored.activate(Action::PreviewBackup).unwrap();
    tap(
        &mut restored,
        Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, ROW_HEIGHT),
    );
    assert_eq!(
        restored
            .document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .logical_font_size,
        Some(255.)
    );
    restored.open_path(&copy).unwrap();
    assert_eq!(current(&restored), Some(255.));
    restored
        .apply_text_profile(BookPreferences {
            font_size: Some(42.25),
            ..Default::default()
        })
        .unwrap();
    restored.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(current(&restored), Some(255.));
    for bad in [f64::NAN, f64::INFINITY, 11., 256.] {
        let prior = app.document_defaults.templates.clone();
        let mut invalid = app.document_defaults.templates.clone();
        invalid.reflow.as_mut().unwrap().logical_font_size = Some(bad);
        assert!(app.document_defaults.save(invalid).is_err());
        assert_eq!(app.document_defaults.templates, prior);
        let mut state = app.library.state(id).unwrap().clone();
        state.preferences.logical_font_size = Some(bad);
        let valid = app.library.state(id).unwrap().clone();
        *app.library.state_mut(id).unwrap() = state;
        assert!(app.library.bytes().is_err());
        *app.library.state_mut(id).unwrap() = valid;
    }
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(current(&reopened), Some(255.));
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size,
        340.
    );
    reopened
        .apply_text_profile(BookPreferences {
            font_size: Some(42.25),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(current(&reopened), None);
    reopened.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(current(&reopened), Some(255.));
}

#[test]
fn logical_all_487_half_steps_are_valid_live_sizes_with_ink_and_stable_source() {
    let (_fixture, mut app, _) = setup();
    let id = app.active.as_ref().unwrap().id;
    let anchor = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .fragment("passage")
        .unwrap();
    for half in 24..=510 {
        let size = f64::from(half) / 2.;
        app.apply_logical_font(id, size).unwrap();
        let active = app.active.as_mut().unwrap();
        assert_eq!(active.reading.probe.book, anchor.book);
        let page = active.reading.probe.page_for(&anchor).unwrap();
        let pixels = active.reading.probe.capture(page).unwrap();
        assert!(pixels.iter().any(|&n| n < 128), "logical={size}");
        assert_eq!(
            active.reading.config.settings.font_size,
            active
                .reading
                .config
                .settings
                .font_scale
                .effective(size)
                .unwrap()
        );
    }
}

#[test]
fn logical_controls_capture_compact_large_light_dark_without_losing_presets() {
    let (_fixture, mut app, _) = setup();
    let id = app.active.as_ref().unwrap().id;
    app.apply_logical_font(id, 22.25).unwrap();
    for large in [false, true] {
        for dark in [false, true] {
            app.controls.large_toolbar = large;
            app.controls.night_pages = dark;
            app.system_theme = None;
            app.open_reading_edge(false, WIDTH as f32 / 2.);
            app.reading_panel = Some(BarPanel::Text);
            for preset in crate::logical_font::PRESETS {
                let rect = app.reading_button(&preset.to_string());
                assert!(rect.w >= 100 && rect.h >= 100);
            }
            let suffix = format!(
                "{}{}",
                if large { "_LARGE" } else { "" },
                if dark { "_DARK" } else { "" }
            );
            crate::app::tests::capture_test_screen(
                &app,
                &fonts(),
                &format!("READER_LOGICAL_FONT_PANEL{suffix}_SCREENSHOT"),
                dark,
            );
            editor(&mut app);
            crate::app::tests::capture_test_screen(
                &app,
                &fonts(),
                &format!("READER_LOGICAL_FONT_PICKER{suffix}_SCREENSHOT"),
                dark,
            );
            app.input(Input::PageBack);
        }
    }
}

#[test]
#[ignore = "Supply an authorized original EPUB, AZW3 or MOBI"]
fn logical_original_publication_keeps_canonical_selection_and_resumes_large_size() {
    let path = PathBuf::from(std::env::var_os("READER_LOGICAL_FONT_ORIGINAL").unwrap());
    let original = fs::read(&path).unwrap();
    let fixture = Fixture::new();
    let copy = fixture.root.join(path.file_name().unwrap());
    fs::write(&copy, &original).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&copy).unwrap();
    let id = app.active.as_ref().unwrap().id;
    let (chapter, probe) = (0..app.active.as_ref().unwrap().reading.probe.chapter_count())
        .find_map(|chapter| {
            let probe = app.active.as_ref()?.reading.config.chapter(chapter).ok()?;
            probe
                .source
                .blocks
                .iter()
                .any(|b| b.text.split_whitespace().count() > 3)
                .then_some((chapter, probe))
        })
        .unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.split_whitespace().count() > 3)
        .unwrap();
    let word = crate::selection::word_ranges(&block.text)
        .find(|r| r.len() > 3)
        .unwrap();
    let expected = block.text[word.clone()].to_string();
    let start = probe.anchor(block.key, word.start);
    let end = probe.anchor(block.key, word.end);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump_to_anchor(chapter, &start)
        .unwrap();
    for size in [12., 22.25, 80., 160., 255.] {
        app.apply_logical_font(id, size).unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        let selection = reading.probe.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, expected);
        let page = reading.probe.page_for(&start).unwrap();
        assert!(!reading.probe.selection_rects(page, &selection).is_empty());
        assert!(
            reading
                .probe
                .capture(page)
                .unwrap()
                .iter()
                .any(|&n| n < 128)
        );
    }
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&copy).unwrap();
    assert_eq!(current(&reopened), Some(255.));
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(fs::read(copy).unwrap(), original);
}

#[test]
fn unfit_imported_layout_requires_explicit_reset_backs_up_choices_and_rolls_back_failures() {
    let (fixture, mut app, path) = setup();
    let id = app.active.as_ref().unwrap().id;
    let anchor = app.active.as_ref().unwrap().reading.anchor();
    let source = fs::read(&path).unwrap();
    let old = BookPreferences {
        logical_font_size: Some(255.),
        line_height: Some(3.),
        margin_top: Some(350),
        margin_bottom: Some(350),
        ..Default::default()
    };
    crate::document_defaults::validate_preferences(&old).unwrap();
    app.library.state_mut(id).unwrap().preferences = old.clone();
    app.save().unwrap();
    let bytes = fs::read(&fixture.state).unwrap();
    let model = app.library.bytes().unwrap();
    app.open_path(&path).unwrap();
    assert!(app.logical_recovery.is_some());
    crate::app::tests::capture_test_screen(
        &app,
        &fonts(),
        "READER_LOGICAL_FONT_RECOVERY_SCREENSHOT",
        false,
    );
    crate::app::tests::capture_test_screen(
        &app,
        &fonts(),
        "READER_LOGICAL_FONT_RECOVERY_DARK_SCREENSHOT",
        true,
    );
    assert_eq!(app.library.bytes().unwrap(), model);
    app.input(Input::PageBack);
    assert!(app.logical_recovery.is_none());
    assert_eq!(fs::read(&fixture.state).unwrap(), bytes);
    app.open_path(&path).unwrap();
    app.input(Input::Sleep);
    assert!(app.logical_recovery.is_some());
    app.input(Input::Wake);
    let folder = app.profiles_path.parent().unwrap().join("layout-recovery");
    fs::write(&folder, b"blocked").unwrap();
    assert!(app.reset_unopened_logical_layout().is_err());
    assert_eq!(fs::read(&fixture.state).unwrap(), bytes);
    assert_eq!(app.library.bytes().unwrap(), model);
    fs::remove_file(&folder).unwrap();
    let saved = fixture.root.join("saved-before-recovery");
    fs::rename(&fixture.state, &saved).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    assert!(app.reset_unopened_logical_layout().is_err());
    assert_eq!(app.library.bytes().unwrap(), model);
    fs::remove_dir(&fixture.state).unwrap();
    fs::rename(&saved, &fixture.state).unwrap();
    app.reset_unopened_logical_layout().unwrap();
    assert!(app.logical_recovery.is_none());
    assert_eq!(current(&app), Some(22.));
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    let files = fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let archive: serde_json::Value = serde_json::from_slice(&fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_value::<BookPreferences>(archive["preferences"].clone()).unwrap(),
        old
    );
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(current(&reopened), Some(22.));
    assert!(reopened.logical_recovery.is_none());
    assert_eq!(fs::read(path).unwrap(), source);
}

#[test]
fn recovering_a_different_book_resets_navigation_and_restores_that_books_numbering_and_menus() {
    let (fixture, mut app, a) = setup();
    let b = fixture.root.join("Recovered book.epub");
    fs::copy(crate::test_fixture("field-notes-epub3.epub"), &b).unwrap();
    app.open_path(&b).unwrap();
    let id = app.active.as_ref().unwrap().id;
    let saved = position(&app.active.as_ref().unwrap().reading);
    app.add_bookmark().unwrap();
    let marks = serde_json::to_value(&app.library.state(id).unwrap().bookmarks).unwrap();
    let mut book = Book::open(&b).unwrap();
    let mut map = crate::synthetic_pages::CharacterPages::new(book.revision(), 500).unwrap();
    while !map.step(&mut book).unwrap() {}
    fs::create_dir_all(&app.numbering_directory).unwrap();
    map.save(&app.numbering_path(id)).unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.character_page_stride = Some(500);
    state.menus.bottom = Some(BarPanel::TextMargins);
    state.menus.top = Some(TopGroup::Navigation);
    state.preferences = BookPreferences {
        logical_font_size: Some(255.),
        line_height: Some(3.),
        margin_top: Some(350),
        margin_bottom: Some(350),
        ..Default::default()
    };
    app.save().unwrap();
    app.open_path(&a).unwrap();
    let previous = position(&app.active.as_ref().unwrap().reading);
    app.back.push(previous.clone().into());
    app.forward.push(previous.into());
    app.auto_turn
        .configure(Some(Duration::from_secs(10)), Instant::now());
    app.open_path(&b).unwrap();
    assert!(app.logical_recovery.is_some());
    app.reset_unopened_logical_layout().unwrap();
    assert_eq!(app.active.as_ref().unwrap().id, id);
    assert!(app.back.is_empty() && app.forward.is_empty());
    assert_eq!(app.auto_turn.interval(), None);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .numbering
            .as_ref()
            .unwrap()
            .stride,
        500
    );
    assert_eq!(app.last_text_panel, BarPanel::TextMargins);
    assert_eq!(
        position(&app.active.as_ref().unwrap().reading).anchor,
        saved.anchor
    );
    assert_eq!(
        serde_json::to_value(&app.library.state(id).unwrap().bookmarks).unwrap(),
        marks
    );
}

#[test]
fn fit_recovery_keeps_restorable_css_hidden_focus_but_rejects_invalid_source_points() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Hidden focus.xhtml");
    let original = "<html><body><p>Before <span style='display:none'>secret focus</span> after visible text.</p></body></html>";
    fs::write(&path, original).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    let active = app.active.as_ref().unwrap();
    let id = active.id;
    let block = active
        .reading
        .probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.contains("secret focus"))
        .unwrap();
    let anchor = active
        .reading
        .probe
        .anchor(block.key, block.text.find("secret focus").unwrap());
    assert_eq!(
        active.reading.probe.source_presentation(&anchor),
        Some(crate::SourcePresentation::CssOmitted)
    );
    assert!(active.reading.probe.restore_anchor(&anchor).is_some());
    let bad_preferences = BookPreferences {
        logical_font_size: Some(255.),
        line_height: Some(3.),
        margin_top: Some(350),
        margin_bottom: Some(350),
        ..Default::default()
    };
    let state = app.library.state_mut(id).unwrap();
    state.preferences = bad_preferences.clone();
    state.position = Some(ReadingPosition {
        chapter: 0,
        page: 0,
        anchor: Some(anchor.clone()),
    });
    app.library.persist().unwrap();
    app.open_path(&path).unwrap();
    assert!(app.logical_recovery.is_some());
    app.reset_unopened_logical_layout().unwrap();
    assert_eq!(
        app.active.as_ref().unwrap().reading.anchor(),
        Some(anchor.clone())
    );
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .position
            .as_ref()
            .unwrap()
            .anchor,
        Some(anchor.clone())
    );
    let mut invalid = anchor;
    invalid.offset = usize::MAX;
    let state = app.library.state_mut(id).unwrap();
    state.preferences = bad_preferences;
    state.position = Some(ReadingPosition {
        chapter: 0,
        page: 0,
        anchor: Some(invalid),
    });
    app.library.persist().unwrap();
    let before = app.library.bytes().unwrap();
    assert!(app.open_path(&path).is_err());
    assert!(app.logical_recovery.is_none());
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
