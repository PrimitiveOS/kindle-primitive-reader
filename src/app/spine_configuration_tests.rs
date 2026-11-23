//! The corpus verifier must reproduce the user's resumed session, not a baseline Probe.
use super::tests::Fixture;
use super::*;
use crate::WordSpacePercent;
use crate::document_check::spine::verify_spine;

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}

#[test]
fn verifier_snapshot_recreates_actual_custom_reader_frames_positions_and_spine_counts() {
    let fixture = Fixture::new();
    let original = fs::read(&fixture.book).unwrap();
    let mut rules = crate::user_hyphenation::Rules::default();
    rules.enabled = true;
    rules.set("en-US", "observation", "ob-ser-va-tion").unwrap();
    fs::write(
        fixture.root.join("user-hyphenation.json"),
        serde_json::to_vec(&rules).unwrap(),
    )
    .unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&fixture.book).unwrap();
    assert_eq!(
        app.verification_book_config()
            .unwrap()
            .settings
            .percent_word_spacing,
        Some(WordSpacePercent::MEDIUM)
    );
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 96.,
        logical_font_size: None,
        render_dpi: 123,
        percent_word_spacing: Some(WordSpacePercent {
            scaling: 123,
            minimum: 67,
        }),
        ..settings
    }))
    .unwrap();
    fs::create_dir(&app.fonts_directory).unwrap();
    fs::copy(
        crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        app.fonts_directory.join("Selected.ttf"),
    )
    .unwrap();
    app.activate(Action::SetFont(Some("Selected.ttf".into())))
        .unwrap();
    app.activate_style(style_tweaks::Command::Preset("paragraph-indent-em".into()))
        .unwrap();
    app.activate_style(style_tweaks::Command::Choose(
        "paragraph-indent-em".into(),
        Some(true),
    ))
    .unwrap();
    app.activate(Action::FacingPages(true)).unwrap();
    app.show(Screen::Book);
    app.save().unwrap();
    drop(app);

    // The same Continue action as the public verifier loads durable settings.
    let mut resumed = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    resumed.input(Input::Tap(410., 364.));
    let snapshot = resumed.verification_book_config().unwrap();
    assert_eq!(snapshot.settings.render_dpi, 123);
    assert_eq!(
        snapshot.settings.percent_word_spacing,
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
    assert!(!snapshot.user_styles.is_empty());
    assert!(snapshot.user_hyphenation.has_rules());
    assert_eq!(snapshot.fonts.len(), 2);
    assert!(snapshot.facing_pages);
    let active = &mut resumed.active.as_mut().unwrap().reading;
    let chapter = active.chapter;
    let saved_position = position(active);
    let active_anchors: Vec<_> = (0..active.probe.page_count())
        .map(|p| active.probe.page_anchor(p))
        .collect();
    let active_frames: Vec<_> = (0..active.probe.page_count())
        .map(|p| active.probe.capture(p).unwrap())
        .collect();
    let mut from_snapshot = snapshot.chapter(chapter).unwrap();
    assert_eq!(from_snapshot.settings, active.probe.settings);
    assert_eq!(from_snapshot.page_count(), active_frames.len());
    for (page, frame) in active_frames.iter().enumerate() {
        assert_eq!(from_snapshot.capture(page).unwrap(), *frame);
        assert_eq!(from_snapshot.page_anchor(page), active_anchors[page]);
        if let Some(anchor) = &active_anchors[page] {
            assert_eq!(from_snapshot.page_for(anchor), Some(page));
        }
    }
    if let Some(anchor) = &saved_position.anchor {
        assert_eq!(from_snapshot.page_for(anchor), Some(saved_position.page));
    }
    let mut baseline = Probe::open(
        &fixture.book,
        Engine::Parley,
        &fonts(),
        Settings {
            width: from_snapshot.settings.width,
            height: from_snapshot.settings.height,
            ..Settings::default()
        },
        chapter,
    )
    .unwrap();
    let default_frame = baseline.capture(0).unwrap();
    assert_eq!(default_frame.len(), active_frames[0].len());
    assert_ne!(
        default_frame, active_frames[0],
        "baseline must differ from the configured Reader"
    );

    // A later live style change must not rewrite the captured immutable snapshot.
    resumed
        .activate_style(style_tweaks::Command::Preset("paragraph-indent-em".into()))
        .unwrap();
    resumed
        .activate_style(style_tweaks::Command::Choose(
            "paragraph-indent-em".into(),
            Some(false),
        ))
        .unwrap();
    assert!(
        resumed
            .verification_book_config()
            .unwrap()
            .user_styles
            .is_empty()
    );
    drop(resumed);
    let mut retained = snapshot.chapter(chapter).unwrap();
    assert_eq!(retained.capture(0).unwrap(), active_frames[0]);

    let count = Book::open(&snapshot.book).unwrap().chapters.len();
    let mut expected = 0u64;
    let mut legacy = 0u64;
    for index in 0..count {
        expected += snapshot.chapter(index).unwrap().page_count() as u64;
        legacy += Probe::open(
            &fixture.book,
            Engine::Parley,
            &fonts(),
            Settings {
                width: WIDTH,
                height: PAGE_HEIGHT,
                ..Settings::default()
            },
            index,
        )
        .unwrap()
        .page_count() as u64;
    }
    assert_ne!(
        expected, legacy,
        "the corpus regression must detect a default-configuration fallback"
    );
    let audit = verify_spine(&fixture.book, &fonts(), "all", Some(&snapshot)).unwrap();
    let receipt = serde_json::to_value(audit).unwrap();
    assert_eq!(receipt["configuration_source"], "resumed_reader");
    assert_eq!(receipt["session_configuration"]["render_dpi"], 123);
    assert_eq!(
        receipt["session_configuration"]["percent_word_spacing"],
        serde_json::json!({"scaling":123,"minimum":67})
    );
    assert_eq!(
        receipt["session_configuration"]["user_style_snapshot"],
        true
    );
    assert_eq!(
        receipt["session_configuration"]["personal_hyphenation_rules"],
        true
    );
    assert_eq!(receipt["session_configuration"]["font_files"], 2);
    assert_eq!(receipt["pages_in_checked_sections"], expected);
    assert_eq!(receipt["failed_indices"], serde_json::json!([]));
    let encoded = serde_json::to_string(&receipt).unwrap();
    assert!(!encoded.contains(&fixture.root.display().to_string()));
    assert!(!encoded.contains("observation"));
    assert!(!encoded.contains("text-indent"));
    assert_eq!(fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn verifier_accessor_distinguishes_unopened_and_fixed_page_sessions() {
    let fixture = Fixture::new();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    assert!(app.verification_book_config().is_none());
    let pdf = fixture.root.join("Fixed.pdf");
    fs::copy(crate::test_fixture("cmyk-print-chart.pdf"), &pdf).unwrap();
    app.open_path(&pdf).unwrap();
    assert!(app.fixed.is_some());
    assert!(app.verification_book_config().is_none());
}
