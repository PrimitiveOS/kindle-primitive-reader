use super::*;
use crate::Settings;
use crate::app::NumberField;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::library::BookPreferences;
use std::fs;
use std::path::PathBuf;

fn tap(app: &mut Reader, label: &str) {
    let rect = app.reading_button(label);
    assert!(rect.w >= 100 && rect.h >= 100, "{label}: {rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn settings(app: &Reader) -> Settings {
    app.active.as_ref().unwrap().reading.config.settings
}

fn tap_dpi(app: &mut Reader, dpi: u16) {
    let bars = app.reading_bars();
    let row = &bars
        .row_labels
        .iter()
        .find(|(_, name)| name == "Zoom (dpi)")
        .unwrap()
        .0;
    let name = if dpi == 0 {
        "Off".to_string()
    } else {
        dpi.to_string()
    };
    let rect = bars
        .buttons
        .iter()
        .find(|button| {
            button.label == name
                && (row.y..row.y + row.h).contains(&(button.rect.y + button.rect.h / 2))
        })
        .unwrap()
        .rect;
    assert!(rect.w >= 100 && rect.h >= 100, "DPI {dpi}: {rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

#[test]
fn dpi_live_controls_reflow_retain_source_selection_and_restore_book_and_profile_settings() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Content scale.xhtml");
    fs::write(&path, r#"<html><head><title>Content scale</title></head><body><p id="anchor" style="padding:3mm 12px;text-indent:1em">The harbor keeps its own time. The boats return at dusk.</p><p style="padding:8pt 3%;font-size:125%">Content zoom changes the publisher's absolute lengths while the chosen reading font stays the same.</p></body></html>"#).unwrap();
    let original = fs::read(&path).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&path).unwrap();
    let base_font = settings(&app).font_size;
    let anchor = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .fragment("anchor")
        .unwrap();
    let mut end = anchor.clone();
    end.offset += "The harbor".len();
    let selected = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .select_range(&anchor, &end)
        .unwrap();
    let initial = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    for dpi in [48, 167, 212, 300, 0, 96] {
        app.open_reading_edge(false, 500.);
        if app.reading_panel != Some(BarPanel::TextRendering) {
            tap(&mut app, "Rendering");
        }
        tap_dpi(&mut app, dpi);
        assert_eq!(settings(&app).render_dpi, dpi);
        assert_eq!(settings(&app).font_size, base_font);
        let reading = &app.active.as_ref().unwrap().reading;
        assert!(reading.probe.page_for(&anchor).is_some());
        assert_eq!(
            reading
                .probe
                .select_range(&selected.start, &selected.end)
                .unwrap()
                .text,
            selected.text
        );
        assert!(
            !reading
                .probe
                .selection_rects(reading.probe.page_for(&anchor).unwrap(), &selected)
                .is_empty()
        );
        assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
    }
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        initial
    );
    capture_test_screen(&app, &fonts, "READER_DPI_SCREENSHOT", false);
    app.controls.large_toolbar = true;
    capture_test_screen(&app, &fonts, "READER_DPI_LARGE_SCREENSHOT", false);
    app.controls.night_pages = true;
    app.system_theme = None;
    capture_test_screen(&app, &fonts, "READER_DPI_LARGE_DARK_SCREENSHOT", true);
    app.controls.large_toolbar = false;
    capture_test_screen(&app, &fonts, "READER_DPI_DARK_SCREENSHOT", true);
    app.activate(Action::Text(Settings {
        render_dpi: 212,
        ..settings(&app)
    }))
    .unwrap();
    app.save_reading_profile("Publisher zoom").unwrap();
    assert_eq!(
        app.profiles.last().unwrap().preferences.render_dpi,
        Some(212)
    );
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(settings(&reopened).render_dpi, 212);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn dpi_global_defaults_freeze_each_new_book_and_reject_invalid_backup_preferences() {
    use crate::app::default_settings::Command;
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Text(Settings {
        render_dpi: 167,
        ..settings(&app)
    }))
    .unwrap();
    app.activate_document_defaults(Command::SaveReflow).unwrap();
    assert_eq!(
        app.document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .render_dpi,
        Some(167)
    );
    let next = fixture.root.join("Unseen book.txt");
    fs::write(
        &next,
        "The newly opened book inherits the chosen content scale.",
    )
    .unwrap();
    app.open_path(&next).unwrap();
    assert_eq!(settings(&app).render_dpi, 167);
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Text(Settings {
        render_dpi: 300,
        ..settings(&app)
    }))
    .unwrap();
    app.activate_document_defaults(Command::SaveReflow).unwrap();
    app.open_path(&next).unwrap();
    assert_eq!(
        settings(&app).render_dpi,
        167,
        "existing book is unchanged by new defaults"
    );
    app.activate_document_defaults(Command::ResetReflow)
        .unwrap();
    assert_eq!(settings(&app).render_dpi, 300);
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&next).unwrap();
    assert_eq!(settings(&reopened).render_dpi, 300);
    for dpi in crate::RENDER_DPI_PRESETS {
        let prefs = BookPreferences {
            render_dpi: Some(dpi),
            ..Default::default()
        };
        crate::document_defaults::validate_preferences(&prefs).unwrap();
    }
    let prefs = BookPreferences {
        render_dpi: Some(65535),
        ..Default::default()
    };
    assert!(crate::document_defaults::validate_preferences(&prefs).is_err());
    let bytes = br#"{"version":1,"reflow":{"render_dpi":301},"fixed":null}"#;
    assert!(crate::document_defaults::validate_backup(bytes).is_err());
}

#[test]
fn dpi_legacy_records_inherit_and_invalid_preferences_do_not_reset_other_valid_settings() {
    let old: BookPreferences = serde_json::from_str("{\"font_size\":54}").unwrap();
    let base = Settings {
        render_dpi: 167,
        ..Settings::default()
    };
    assert_eq!(old.apply_to(base).render_dpi, 167);
    assert!(!serde_json::to_string(&old).unwrap().contains("render_dpi"));
    let bad = BookPreferences {
        render_dpi: Some(65535),
        ..old
    };
    assert_eq!(bad.apply_to(base).render_dpi, 167);
    assert_eq!(bad.apply_to(base).font_size, 54.);
    for dpi in 0..=300 {
        assert!(
            Settings {
                render_dpi: dpi,
                ..base
            }
            .validate()
            .is_ok()
        );
    }
    for dpi in [301, 600, 65535] {
        assert!(
            Settings {
                render_dpi: dpi,
                ..base
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
#[ignore = "requires READER_DPI_ORIGINAL pointing to an unchanged authorized original EPUB"]
fn dpi_original_publication_keeps_text_anchors_and_selection_across_presets_and_custom_values() {
    let fixture = Fixture::new();
    let path = PathBuf::from(std::env::var_os("READER_DPI_ORIGINAL").unwrap());
    let original = fs::read(&path).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(path.parent().unwrap(), &fixture.state, fonts).unwrap();
    app.open_path(&path).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let (chapter, probe) = (0..reading.probe.chapter_count())
        .find_map(|chapter| {
            let p = reading.config.chapter(chapter).ok()?;
            p.source
                .blocks
                .iter()
                .any(|b| b.text.split_whitespace().count() > 3)
                .then_some((chapter, p))
        })
        .unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.split_whitespace().count() > 3)
        .unwrap();
    let word = block.text.split_whitespace().next().unwrap();
    let offset = word.as_ptr() as usize - block.text.as_ptr() as usize;
    let anchor = probe.anchor(block.key, offset);
    let end = probe.anchor(block.key, offset + word.len());
    let selected = probe.select_range(&anchor, &end).unwrap();
    reading.jump_to_anchor(chapter, &anchor).unwrap();
    let baseline = reading
        .probe
        .capture(reading.probe.page_for(&anchor).unwrap())
        .unwrap();
    for dpi in [0, 48, 72, 123, 167, 212, 300, 96] {
        app.activate(Action::Text(Settings {
            render_dpi: dpi,
            ..settings(&app)
        }))
        .unwrap();
        let r = &mut app.active.as_mut().unwrap().reading;
        let page = r.probe.page_for(&anchor).unwrap();
        assert_eq!(
            r.probe.select_range(&anchor, &end).unwrap().text,
            selected.text
        );
        assert!(!r.probe.selection_rects(page, &selected).is_empty());
        let frame = r.probe.capture(page).unwrap();
        assert!(frame.iter().any(|v| *v < 128));
        if dpi == 96 {
            assert_eq!(frame, baseline);
        }
    }
    app.save().unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn custom_dpi_controls_keep_preset_targets_and_current_custom_value_legible() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Text(Settings {
        render_dpi: 123,
        ..settings(&app)
    }))
    .unwrap();
    for large in [false, true] {
        for dark in [false, true] {
            app.controls.large_toolbar = large;
            app.controls.night_pages = dark;
            app.system_theme = None;
            app.open_reading_edge(false, WIDTH as f32 / 2.);
            if app.reading_panel != Some(BarPanel::TextRendering) {
                tap(&mut app, "Rendering");
            }
            let bars = app.reading_bars();
            let row = bars
                .row_labels
                .iter()
                .find(|(_, label)| label == "Zoom (dpi)")
                .unwrap()
                .0;
            for preset in crate::RENDER_DPI_PRESETS {
                let label = if preset == 0 {
                    "Off".into()
                } else {
                    preset.to_string()
                };
                let button = bars
                    .buttons
                    .iter()
                    .find(|button| {
                        button.label == label
                            && (row.y..row.y + row.h).contains(&(button.rect.y + button.rect.h / 2))
                    })
                    .unwrap();
                assert!(button.rect.w >= 100 && button.rect.h >= 100);
            }
            let custom = bars
                .buttons
                .iter()
                .find(|button| button.label == "123 dpi")
                .unwrap();
            assert!(custom.rect.w >= 100 && custom.rect.h >= 100 && custom.selected);
            let suffix = format!(
                "{}{}",
                if large { "_LARGE" } else { "" },
                if dark { "_DARK" } else { "" }
            );
            capture_test_screen(
                &app,
                &fonts,
                &format!("READER_CUSTOM_DPI_PANEL{suffix}_SCREENSHOT"),
                dark,
            );
            tap(&mut app, "Set value");
            assert!(matches!(
                app.screen,
                Screen::NumberingValue(NumberField::ContentDpi(_))
            ));
            capture_test_screen(
                &app,
                &fonts,
                &format!("READER_CUSTOM_DPI_PICKER{suffix}_SCREENSHOT"),
                dark,
            );
        }
    }
}
