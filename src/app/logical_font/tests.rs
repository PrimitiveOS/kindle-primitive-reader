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
    draft(&mut app, "22.25");
    app.apply_logical_font_input(id).unwrap();
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
