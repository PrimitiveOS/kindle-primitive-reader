use super::tests::{Fixture, capture_test_screen};
use super::*;

fn setup(fixture: &Fixture) -> Reader {
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap()
}

#[test]
fn legacy_font_size_taps_use_half_logical_steps_and_preserve_the_reading_anchor() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let book_id = app.open_book_id().unwrap();
    let initial = app.active.as_ref().unwrap().reading.config.settings;
    let legacy = Settings {
        font_size: 58.0,
        logical_font_size: None,
        ..initial
    };
    app.activate(Action::Text(legacy)).unwrap();
    let origin = app.active.as_ref().unwrap().reading.anchor();
    let step = legacy.font_scale.physical_delta(0.5);

    let larger = text_size_items(legacy, book_id)
        .into_iter()
        .find(|item| item.title == "Larger text")
        .unwrap()
        .action;
    let Action::Text(larger_settings) = larger else {
        panic!("legacy font-size controls keep pixel settings");
    };
    assert!((larger_settings.font_size - (legacy.font_size + step)).abs() < 0.0001);
    assert_eq!(larger_settings.logical_font_size, None);

    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    choose(&mut app, "Type");
    choose(&mut app, "Larger text");
    let reading = &app.active.as_ref().unwrap().reading;
    assert!((reading.config.settings.font_size - (legacy.font_size + step)).abs() < 0.0001);
    assert_eq!(reading.config.settings.logical_font_size, None);
    assert_eq!(reading.anchor(), origin);

    capture_test_screen(
        &app,
        &app.fonts,
        "READER_LEGACY_FONT_STEPS_SCREENSHOT",
        false,
    );

    choose(&mut app, "Smaller text");
    let reading = &app.active.as_ref().unwrap().reading;
    assert!((reading.config.settings.font_size - legacy.font_size).abs() < 0.0001);
    assert_eq!(reading.config.settings.logical_font_size, None);
    assert_eq!(reading.anchor(), origin);

    app.show(Screen::Typography);
    let larger = app
        .items()
        .into_iter()
        .find(|item| item.title == "Larger text")
        .unwrap()
        .action;
    app.activate(larger).unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    assert!((reading.config.settings.font_size - (legacy.font_size + step)).abs() < 0.0001);
    assert_eq!(reading.config.settings.logical_font_size, None);
    assert_eq!(reading.anchor(), origin);
}

fn choose(app: &mut Reader, label: &str) {
    let rect = app.reading_button(label);
    let input = Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32);
    assert_eq!(app.input(input), Change::Partial, "{label}");
}

fn style_panel(app: &mut Reader) {
    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    choose(app, "Style");
}

#[test]
fn numeric_gamma_repaints_glyphs_keeps_layout_and_survives_a_profile_and_reopen() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.open_path(&fixture.book).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    reading.jump(1, Some("door")).unwrap();
    let start = reading.probe.fragment("door").unwrap();
    let mut end = start.clone();
    end.offset += "At dusk the door".len();
    let selected = reading.probe.select_range(&start, &end).unwrap();
    let origin = reading.anchor();
    let page = reading.page;
    let layout_address = reading.probe.lines.as_ptr();
    let boxes = reading.probe.selection_rects(page, &selected);
    let before = reading.probe.capture(page).unwrap();
    assert!(!boxes.is_empty());
    style_panel(&mut app);
    choose(&mut app, "1.45");
    assert_eq!(app.reading_panel, Some(BarPanel::TextStyle));
    let reading = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(reading.config.settings.font_gamma_index, 25);
    assert_eq!(reading.probe.lines.as_ptr(), layout_address);
    assert_eq!(reading.anchor(), origin);
    assert_eq!(reading.probe.selection_rects(page, &selected), boxes);
    let darkened = reading.probe.capture(page).unwrap();
    assert!(before.iter().zip(&darkened).any(|(a, b)| a > b));
    assert!(
        before
            .iter()
            .zip(&darkened)
            .all(|(&a, &b)| a >= b && (a != 255 || b == 255))
    );
    capture_test_screen(&app, &app.fonts, "READER_FONT_GAMMA_SCREENSHOT", false);
    choose(&mut app, "+");
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_gamma_index,
        26
    );
    choose(&mut app, "-");
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_gamma_index,
        25
    );

    for index in 0..57 {
        let settings = app.active.as_ref().unwrap().reading.config.settings;
        app.activate(Action::Text(Settings {
            font_gamma_index: index,
            ..settings
        }))
        .unwrap();
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.probe.lines.as_ptr(), layout_address);
        assert_eq!(reading.anchor(), origin);
        assert_eq!(reading.probe.selection_rects(page, &selected), boxes);
    }
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_gamma_index: 15,
        ..settings
    }))
    .unwrap();
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(page)
            .unwrap(),
        before
    );
    app.save_reading_profile("Neutral contrast").unwrap();
    assert_eq!(
        app.profiles.last().unwrap().preferences.font_gamma_index,
        Some(15)
    );
    app.save().unwrap();
    let mut reopened = setup(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    let reading = reopened.active.as_ref().unwrap();
    assert_eq!(reading.reading.anchor(), origin);
    assert_eq!(reading.reading.config.settings.font_gamma_index, 15);
}

#[test]
fn numeric_weight_changes_fractional_glyphs_and_keeps_each_saved_source_location() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Weight checks.xhtml");
    fs::write(&path, "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Weight checks</title></head><body><p id=\"passage\">Harbor alpha beta. The boats return at dusk. The tide keeps its own time.</p></body></html>").unwrap();
    let original = fs::read(&path).unwrap();
    let mut app = setup(&fixture);
    app.open_path(&path).unwrap();
    let origin = app.active.as_ref().unwrap().reading.anchor();
    let baseline = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    let ink = |frame: &[u8]| frame.iter().map(|&v| u64::from(255 - v)).sum::<u64>();
    style_panel(&mut app);
    choose(&mut app, "+0.25");
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_weight_delta,
        0.25
    );
    assert!(
        ink(&app
            .active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap())
            > ink(&baseline)
    );
    capture_test_screen(&app, &app.fonts, "READER_FONT_WEIGHT_SCREENSHOT", false);

    for quarters in -12..=22 {
        let delta = quarters as f32 / 4.0;
        let settings = app.active.as_ref().unwrap().reading.config.settings;
        app.activate(Action::Text(Settings {
            font_weight_delta: delta,
            ..settings
        }))
        .unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(reading.anchor(), origin, "weight {delta}");
        assert!(reading.probe.capture(0).unwrap().iter().any(|&v| v < 255));
    }
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_weight_delta: -0.25,
        ..settings
    }))
    .unwrap();
    assert!(
        ink(&app
            .active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap())
            < ink(&baseline)
    );
    app.save_reading_profile("Lighter type").unwrap();
    assert_eq!(
        app.profiles.last().unwrap().preferences.font_weight_delta,
        Some(-0.25)
    );
    app.save().unwrap();
    let mut reopened = setup(&fixture);
    reopened.open_path(&path).unwrap();
    let reading = &reopened.active.as_ref().unwrap().reading;
    assert_eq!(reading.anchor(), origin);
    assert_eq!(reading.config.settings.font_weight_delta, -0.25);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn old_preferences_keep_their_bold_and_contrast_without_adding_new_adjustments() {
    let old: BookPreferences =
        serde_json::from_str("{\"bold_text\":true,\"text_contrast\":true}").unwrap();
    let settings = old.apply_to(Settings::default());
    assert!(settings.bold_text && settings.text_contrast);
    assert_eq!(settings.font_weight_delta, 0.0);
    assert_eq!(settings.font_gamma_index, 15);
    let invalid: BookPreferences =
        serde_json::from_str("{\"font_weight_delta\":15,\"font_gamma_index\":200}").unwrap();
    let settings = invalid.apply_to(Settings::default());
    assert_eq!(settings.font_weight_delta, 0.0);
    assert_eq!(settings.font_gamma_index, 15);
}

#[test]
#[ignore = "requires the user's authorized original EPUB copies"]
fn original_publications_keep_their_words_hit_boxes_and_independent_font_preferences() {
    let root = PathBuf::from(
        std::env::var_os("READER_FONT_RENDER_ROOT")
            .expect("Set READER_FONT_RENDER_ROOT to the private original corpus"),
    );
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
    let mut saved = Vec::new();
    for name in ["035.epub", "042.epub"] {
        let path = root.join(name);
        let original = fs::read(&path).unwrap();
        app.open_path(&path).unwrap();
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .font_gamma_index,
            15
        );
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .font_weight_delta,
            0.0
        );
        let chapter = app.active.as_ref().unwrap().reading.probe.chapter_count() / 2;
        app.follow_reflow_page(chapter, 0).unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        let word = (80..1_400)
            .step_by(32)
            .find_map(|y| {
                (70..1_180).step_by(32).find_map(|x| {
                    reading
                        .word_at(x as f32, y as f32)
                        .filter(|word| word.text.chars().count() >= 3)
                })
            })
            .expect("An original page has a selectable word");
        let origin = reading.anchor();
        let layout = reading.probe.lines.as_ptr();
        let page = reading.page;
        let before = reading.probe.capture(page).unwrap();
        let boxes = reading.probe.selection_rects(page, &word);
        let settings = reading.config.settings;
        app.activate(Action::Text(Settings {
            font_gamma_index: 25,
            ..settings
        }))
        .unwrap();
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(reading.probe.lines.as_ptr(), layout);
        assert_eq!(reading.anchor(), origin);
        assert_eq!(reading.probe.selection_rects(page, &word), boxes);
        assert_ne!(reading.probe.capture(page).unwrap(), before);
        for delta in [-0.25, 0.25, 1.0] {
            let settings = app.active.as_ref().unwrap().reading.config.settings;
            app.activate(Action::Text(Settings {
                font_weight_delta: delta,
                ..settings
            }))
            .unwrap();
            let reading = &app.active.as_ref().unwrap().reading;
            assert_eq!(reading.anchor(), origin);
            assert_eq!(
                reading.probe.select_range(&word.start, &word.end).unwrap(),
                word
            );
            let rects = reading.probe.selection_rects(reading.page, &word);
            assert!(!rects.is_empty());
            let rect = rects[0];
            let hit = reading
                .word_at(
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            assert_eq!(hit.text, word.text);
        }
        app.save().unwrap();
        saved.push((path.clone(), origin));
        assert_eq!(fs::read(path).unwrap(), original);
    }
    let mut reopened = Reader::open(&root, &fixture.state, fonts).unwrap();
    for (path, origin) in saved {
        reopened.open_path(&path).unwrap();
        let reading = &reopened.active.as_ref().unwrap().reading;
        assert_eq!(reading.anchor(), origin);
        assert_eq!(reading.config.settings.font_gamma_index, 25);
        assert_eq!(reading.config.settings.font_weight_delta, 1.0);
    }
}

#[test]
fn numeric_controls_remain_tappable_in_compact_large_light_and_dark_overlays() {
    let fixture = Fixture::new();
    let mut app = setup(&fixture);
    app.system_theme = None;
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(1, Some("door"))
        .unwrap();
    let origin = app.active.as_ref().unwrap().reading.anchor();
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
    let mut frames = Vec::new();
    for large in [false, true] {
        for dark in [false, true] {
            app.controls.large_toolbar = large;
            app.controls.night_pages = dark;
            let settings = app.active.as_ref().unwrap().reading.config.settings;
            app.activate(Action::Text(Settings {
                font_gamma_index: crate::font_render::NORMAL_GAMMA,
                font_weight_delta: 0.0,
                ..settings
            }))
            .unwrap();
            style_panel(&mut app);
            choose(&mut app, "1.45");
            choose(&mut app, "+0.25");
            let current = &app.active.as_ref().unwrap().reading;
            assert_eq!(current.anchor(), origin);
            assert_eq!(current.config.settings.font_gamma_index, 25);
            assert_eq!(current.config.settings.font_weight_delta, 0.25);
            let mut controls = Vec::new();
            for label in [
                "-1.00", "-0.50", "+0.00", "+0.50", "+1.00", "+1.50", "+3.00", "0.80", "1.00",
                "1.45", "1.90", "2.50", "4.00", "8.00", "15.00",
            ] {
                let rect = app.reading_button(label);
                assert!(rect.x >= 0 && rect.y >= 0);
                assert!(rect.right() <= WIDTH as i32 && rect.bottom() <= HEIGHT as i32);
                assert!(rect.w >= 90 && rect.h >= 90, "{label}");
                controls.push(
                    serde_json::json!({"label":label,"x":rect.x,"y":rect.y,"w":rect.w,"h":rect.h}),
                );
            }
            if let Some(directory) = std::env::var_os("READER_FONT_CONTROLS_REVIEW") {
                let file = format!(
                    "font-controls-{}-{}.png",
                    if large { "large" } else { "compact" },
                    if dark { "dark" } else { "light" }
                );
                let pixels = primitive_app::frame(&app, &mut canvas, dark);
                primitive_app::write_png(
                    &PathBuf::from(&directory).join(&file),
                    WIDTH,
                    HEIGHT,
                    &pixels,
                )
                .unwrap();
                frames.push(serde_json::json!({"file":file,"controls":controls}));
            }
        }
    }
    if let Some(directory) = std::env::var_os("READER_FONT_CONTROLS_REVIEW") {
        fs::write(
            PathBuf::from(directory).join("controls.json"),
            serde_json::to_vec_pretty(&frames).unwrap(),
        )
        .unwrap();
    }
}
