use super::*;
use crate::app::tests::Fixture;
use crate::reading_backup::{Envelope, Key as BackupKey};

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn setup() -> (Fixture, Reader, PathBuf) {
    let fixture = Fixture::new();
    let path = fixture.root.join("Spacing.xhtml");
    fs::write(&path,"<html><head><title>Spacing</title></head><body><p id='passage'>The harbor keeps its own time. A careful reader follows <a href='#end'>the returning boats</a> across a long passage of connected words and changing spaces.</p><p id='end'>The journey ends here.</p></body></html>").unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    (fixture, app, path)
}
fn current(app: &Reader) -> Option<WordSpacePercent> {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .percent_word_spacing
}
fn tap(app: &mut Reader, rect: Rect) {
    assert!(rect.w >= 100 && rect.h >= 100);
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}
fn key(app: &mut Reader, key: primitive_ui::Key) {
    let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Set")
        .into_iter()
        .find(|(item, _)| item.key == key)
        .unwrap()
        .1;
    tap(app, rect);
}
fn text(app: &mut Reader, text: &str) {
    while !app.editor.text.is_empty() {
        key(app, primitive_ui::Key::Delete);
    }
    for ch in text.chars() {
        key(app, primitive_ui::Key::Char(ch));
    }
    assert_eq!(app.editor.string(), text);
}
fn editor(app: &mut Reader) {
    app.open_reading_edge(false, WIDTH as f32 / 2.);
    if app.reading_panel != Some(BarPanel::TextSpacing) {
        let target = app.reading_button("Spacing");
        tap(app, target);
    }
    let target = app.reading_button("More");
    tap(app, target);
    assert!(matches!(
        app.screen,
        Screen::NumberingValue(NumberField::WordSpacing(_))
    ));
}
fn apply(app: &mut Reader, value: WordSpacePercent) {
    editor(app);
    text(app, &value.scaling.to_string());
    tap(app, MINIMUM);
    text(app, &value.minimum.to_string());
    key(app, primitive_ui::Key::Go);
    assert_eq!(current(app), Some(value));
    assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
}

#[test]
fn two_real_keyboard_fields_apply_atomically_preserve_other_preferences_and_reopen() {
    let (fixture, mut app, path) = setup();
    assert_eq!(current(&app), Some(WordSpacePercent::MEDIUM));
    let original = fs::read(&path).unwrap();
    let old_settings = app.active.as_ref().unwrap().reading.config.settings;
    let pair = WordSpacePercent {
        scaling: 123,
        minimum: 67,
    };
    apply(&mut app, pair);
    let now = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(
        Settings {
            percent_word_spacing: None,
            ..old_settings
        },
        Settings {
            percent_word_spacing: None,
            ..now
        }
    );
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(current(&reopened), Some(pair));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn pair_draft_steps_defaults_cancel_and_lifecycle_do_not_apply_early() {
    let (_fixture, mut app, _) = setup();
    apply(
        &mut app,
        WordSpacePercent {
            scaling: 167,
            minimum: 88,
        },
    );
    app.activate_document_defaults(default_settings::Command::SaveReflow)
        .unwrap();
    apply(&mut app, WordSpacePercent::SMALL);
    editor(&mut app);
    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "167");
    tap(&mut app, MINIMUM);
    assert_eq!(app.editor.string(), "88");
    assert_eq!(current(&app), Some(WordSpacePercent::SMALL));
    assert_eq!(
        app.input(Input::LongTap(
            (INCREASE.x + 10) as f32,
            (INCREASE.y + 10) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "98");
    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "99");
    tap(&mut app, INCREASE);
    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "100");
    app.input(Input::Sleep);
    app.input(Input::Wake);
    assert_eq!(current(&app), Some(WordSpacePercent::SMALL));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(current(&app), Some(WordSpacePercent::SMALL));
    assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
    editor(&mut app);
    tap(&mut app, DEFAULT);
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(
        current(&app),
        Some(WordSpacePercent {
            scaling: 167,
            minimum: 88
        })
    );
}

#[test]
fn invalid_ranges_stale_picker_and_replaced_edition_cannot_change_saved_book() {
    let (fixture, mut app, path) = setup();
    app.save().unwrap();
    editor(&mut app);
    // Opening Spacing legitimately remembers the selected lower panel. Freeze
    // that navigation state before checking rejected mutation requests.
    app.save().unwrap();
    let before = app.library.bytes().unwrap();
    for invalid in ["9", "501"] {
        text(&mut app, invalid);
        key(&mut app, primitive_ui::Key::Go);
        assert_eq!(app.library.bytes().unwrap(), before);
        assert_eq!(app.editor.string(), invalid);
    }
    text(&mut app, "123");
    tap(&mut app, MINIMUM);
    for invalid in ["24", "101"] {
        text(&mut app, invalid);
        key(&mut app, primitive_ui::Key::Go);
        assert_eq!(app.library.bytes().unwrap(), before);
    }
    text(&mut app, "67");
    let stale = app.screen;
    let original = fs::read_to_string(&path).unwrap();
    let replacement = original.replace("harbor", "market");
    assert_eq!(original.len(), replacement.len());
    fs::write(&path, &replacement).unwrap();
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(app.library.bytes().unwrap(), before);
    assert!(app.notice.as_deref().unwrap().contains("edition changed"));
    fs::write(&path, &original).unwrap();
    let other = fixture.root.join("Other.txt");
    fs::write(&other, "A separate edition keeps its own preferences.").unwrap();
    app.open_path(&other).unwrap();
    app.save().unwrap();
    let other_before = app.library.bytes().unwrap();
    app.show(stale);
    text(&mut app, "67");
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(app.library.bytes().unwrap(), other_before);
    assert!(app.notice.as_deref().unwrap().contains("book changed"));
}

#[test]
fn failed_layout_and_save_keep_old_frame_anchor_model_and_both_draft_fields() {
    let (fixture, mut app, _) = setup();
    let font = fixture.root.join("Private font.ttf");
    fs::copy(crate::test_fixture("fonts/NotoSans-Regular.ttf"), &font).unwrap();
    app.active.as_mut().unwrap().reading.config.fonts = vec![font.clone()];
    app.save().unwrap();
    editor(&mut app);
    text(&mut app, "123");
    tap(&mut app, MINIMUM);
    text(&mut app, "67");
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
    let hidden = fixture.root.join("Unavailable.ttf");
    fs::rename(&font, &hidden).unwrap();
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(app.editor.string(), "67");
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
    fs::rename(&hidden, &font).unwrap();
    let saved = fixture.root.join("Saved-state.json");
    fs::rename(&fixture.state, &saved).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(current(&app), Some(WordSpacePercent::MEDIUM));
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    fs::remove_dir(&fixture.state).unwrap();
    fs::rename(&saved, &fixture.state).unwrap();
    key(&mut app, primitive_ui::Key::Go);
    assert_eq!(
        current(&app),
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
}

#[test]
fn legacy_records_profiles_defaults_and_typed_backup_preserve_both_modes() {
    let (fixture, mut app, path) = setup();
    let id = app.active.as_ref().unwrap().id;
    let old = crate::library::BookPreferences {
        word_spacing: 4.0,
        ..Default::default()
    };
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("percent_word_spacing")
    );
    assert_eq!(
        old.apply_to(Settings {
            percent_word_spacing: Some(WordSpacePercent::MEDIUM),
            ..Settings::default()
        })
        .percent_word_spacing,
        None
    );
    app.library.state_mut(id).unwrap().preferences = old;
    app.save().unwrap();
    app.open_path(&path).unwrap();
    assert_eq!(current(&app), None);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .word_spacing,
        4.0
    );
    editor(&mut app);
    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "95");
    tap(&mut app, MINIMUM);
    assert_eq!(app.editor.string(), "75");
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(current(&app), None);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .word_spacing,
        4.0
    );
    editor(&mut app);
    tap(&mut app, DEFAULT);
    tap(&mut app, primitive_ui::back_button(WIDTH as i32));
    assert_eq!(current(&app), None);
    app.save_reading_profile("Legacy pixel spacing").unwrap();
    apply(
        &mut app,
        WordSpacePercent {
            scaling: 123,
            minimum: 67,
        },
    );
    app.save_reading_profile("Percent spacing").unwrap();
    app.activate_document_defaults(default_settings::Command::SaveReflow)
        .unwrap();
    let backup = app.reading_backup_bytes().unwrap();
    let mut envelope = Envelope::parse(&backup).unwrap();
    envelope
        .components
        .iter_mut()
        .find(|c| c.key == BackupKey::Profiles)
        .unwrap()
        .data[1]["preferences"]["percent_word_spacing"]["minimum"] = serde_json::json!(24);
    assert!(
        app.prepare_reading_backup(&envelope.bytes().unwrap())
            .is_err()
    );
    let other = fixture.root.join("New.txt");
    fs::write(&other, "A new book follows explicit saved defaults.").unwrap();
    app.open_path(&other).unwrap();
    assert_eq!(
        current(&app),
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
    app.open_path(&path).unwrap();
    assert_eq!(
        current(&app),
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
    assert_eq!(
        app.library.state(id).unwrap().preferences.word_spacing,
        4.0,
        "legacy setting retained while inactive"
    );
    app.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(current(&app), None);
    let target = Fixture::new();
    let copy = target.root.join(path.file_name().unwrap());
    fs::copy(&path, &copy).unwrap();
    let mut restored = Reader::open(&target.root, &target.state, fonts()).unwrap();
    let mut invalid = Envelope::parse(&backup).unwrap();
    invalid
        .components
        .iter_mut()
        .find(|c| c.key == BackupKey::DocumentDefaults)
        .unwrap()
        .data["reflow"]["percent_word_spacing"]["scaling"] = serde_json::json!(501);
    assert!(
        restored
            .prepare_reading_backup(&invalid.bytes().unwrap())
            .is_err()
    );
    fs::write(&restored.import_path, &backup).unwrap();
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
            .percent_word_spacing,
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
    restored.open_path(&copy).unwrap();
    assert_eq!(
        current(&restored),
        Some(WordSpacePercent {
            scaling: 123,
            minimum: 67
        })
    );
    restored.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(current(&restored), None);
    assert_eq!(
        restored
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .word_spacing,
        4.0
    );
}

#[test]
fn compact_large_day_night_pair_controls_have_large_targets_and_capture_both_fields() {
    let (_fixture, mut app, _) = setup();
    apply(
        &mut app,
        WordSpacePercent {
            scaling: 123,
            minimum: 67,
        },
    );
    for large in [false, true] {
        for dark in [false, true] {
            app.controls.large_toolbar = large;
            app.controls.night_pages = dark;
            app.system_theme = None;
            app.open_reading_edge(false, WIDTH as f32 / 2.);
            if app.reading_panel != Some(BarPanel::TextSpacing) {
                let button = app.reading_button("Spacing");
                tap(&mut app, button);
            }
            let suffix = format!(
                "{}{}",
                if large { "_LARGE" } else { "" },
                if dark { "_DARK" } else { "" }
            );
            for label in ["Small", "Medium", "Large", "More"] {
                let rect = app.reading_button(label);
                assert!(rect.w >= 100 && rect.h >= 100);
            }
            crate::app::tests::capture_test_screen(
                &app,
                &fonts(),
                &format!("READER_WORD_SPACING_PANEL{suffix}_SCREENSHOT"),
                dark,
            );
            let button = app.reading_button("More");
            tap(&mut app, button);
            tap(&mut app, MINIMUM);
            crate::app::tests::capture_test_screen(
                &app,
                &fonts(),
                &format!("READER_WORD_SPACING_PICKER{suffix}_SCREENSHOT"),
                dark,
            );
            assert_eq!(app.input(Input::PageBack), Change::Partial);
        }
    }
}

#[test]
#[ignore = "Supply an authorized original EPUB, AZW3 or MOBI path"]
fn original_publication_pairs_keep_real_source_selections_and_resume() {
    let path = PathBuf::from(std::env::var_os("READER_WORD_SPACING_ORIGINAL").unwrap());
    let original = fs::read(&path).unwrap();
    let fixture = Fixture::new();
    let copy = fixture.root.join(path.file_name().unwrap());
    fs::write(&copy, &original).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&copy).unwrap();
    let owner = app.active.as_ref().unwrap().id;
    let reading = &mut app.active.as_mut().unwrap().reading;
    let (chapter, first) = (0..reading.probe.chapter_count())
        .find_map(|chapter| {
            let probe = reading.config.chapter(chapter).ok()?;
            probe
                .source
                .blocks
                .iter()
                .any(|b| b.text.split_whitespace().count() > 3)
                .then_some((chapter, probe))
        })
        .unwrap();
    let first_block = first
        .source
        .blocks
        .iter()
        .find(|b| b.text.split_whitespace().count() > 3)
        .unwrap();
    let first_anchor = first.anchor(first_block.key, 0);
    reading.jump_to_anchor(chapter, &first_anchor).unwrap();
    let source = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .source
        .blocks
        .iter()
        .map(|b| (b.key, b.text.clone()))
        .collect::<Vec<_>>();
    let selected = source
        .iter()
        .find_map(|(key, text)| {
            crate::selection::word_ranges(text)
                .find(|r| r.len() >= 5)
                .map(|range| (*key, range, text.clone()))
        })
        .unwrap();
    for pair in [
        WordSpacePercent::SMALL,
        WordSpacePercent::MEDIUM,
        WordSpacePercent::LARGE,
        WordSpacePercent {
            scaling: 123,
            minimum: 67,
        },
        WordSpacePercent {
            scaling: 100,
            minimum: 100,
        },
    ] {
        app.apply_word_spacing(owner, pair).unwrap();
        let probe = &app.active.as_ref().unwrap().reading.probe;
        assert!(
            source
                .iter()
                .zip(&probe.source.blocks)
                .all(|((key, text), block)| *key == block.key && *text == block.text)
        );
        let start = probe.anchor(selected.0, selected.1.start);
        let end = probe.anchor(selected.0, selected.1.end);
        let selection = probe.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, selected.2[selected.1.clone()]);
        let page = probe.page_for(&start).unwrap();
        assert!(!probe.selection_rects(page, &selection).is_empty());
    }
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&copy).unwrap();
    assert_eq!(
        current(&reopened),
        Some(WordSpacePercent {
            scaling: 100,
            minimum: 100
        })
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&copy).unwrap(), original);
}
