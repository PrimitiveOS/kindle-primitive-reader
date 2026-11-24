use super::*;
use crate::FontKerningMode;
use crate::app::tests::{Fixture, capture_test_screen};
use std::fs;

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

fn tap_kerning_choice(app: &mut Reader, label: &str) {
    let bars = app.reading_bars();
    let row = bars
        .row_labels
        .iter()
        .find(|(_, title)| title == "Font kerning")
        .unwrap()
        .0;
    let rect = bars
        .buttons
        .iter()
        .find(|button| {
            button.label == label
                && (row.y..row.y + row.h).contains(&(button.rect.y + button.rect.h / 2))
        })
        .unwrap()
        .rect;
    assert!(rect.w >= 100 && rect.h >= 100, "{label}: {rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

fn settings(app: &Reader) -> crate::Settings {
    app.active.as_ref().unwrap().reading.config.settings
}

#[test]
fn four_kerning_modes_are_selectable_and_persist_per_book() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();

    for (label, mode) in [
        ("Off", FontKerningMode::Off),
        ("Fast", FontKerningMode::Fast),
        ("Good", FontKerningMode::Good),
        ("Best", FontKerningMode::Best),
    ] {
        app.open_reading_edge(false, 500.0);
        if app.reading_panel != Some(BarPanel::TextRendering) {
            tap(&mut app, "Rendering");
        }
        tap_kerning_choice(&mut app, label);
        assert_eq!(settings(&app).effective_font_kerning_mode(), mode);
        let id = app.active.as_ref().unwrap().id;
        assert_eq!(
            app.library.state(id).unwrap().preferences.font_kerning_mode,
            Some(mode)
        );
        app.save().unwrap();

        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(settings(&reopened).effective_font_kerning_mode(), mode);
        app = reopened;
    }

    app.open_reading_edge(false, 500.0);
    if app.reading_panel != Some(BarPanel::TextRendering) {
        tap(&mut app, "Rendering");
    }
    capture_test_screen(&app, &fonts, "READER_KERNING_SCREENSHOT", false);
}

#[test]
fn fast_pair_kerning_reader_action_reflows_and_keeps_the_source_anchor() {
    use crate::session::Reading;

    let fixture = Fixture::new();
    let book = crate::test_fixture("fast-pair-kerning.epub");
    let source = fs::read(&book).unwrap();
    fs::copy(book, &fixture.book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/HarborPairTest-Regular.ttf"),
    ];
    let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let mut app = open();
    app.open_path(&fixture.book).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let anchor = reading.probe.matches("Lantern").remove(0);
    *reading = Reading::restore(reading.config.clone(), 0, 0, Some(&anchor)).unwrap();
    app.open_reading_edge(false, 500.0);
    if app.reading_panel != Some(BarPanel::TextRendering) {
        tap(&mut app, "Rendering");
    }
    tap_kerning_choice(&mut app, "Off");
    let reading = &mut app.active.as_mut().unwrap().reading;
    let before = reading.probe.capture(reading.page).unwrap();
    tap_kerning_choice(&mut app, "Fast");
    let reading = &mut app.active.as_mut().unwrap().reading;
    let after = reading.probe.capture(reading.page).unwrap();
    assert_ne!(before, after);
    assert_eq!(reading.anchor().as_ref(), Some(&anchor));
    app.save().unwrap();
    let mut reopened = open();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        settings(&reopened).effective_font_kerning_mode(),
        FontKerningMode::Fast
    );
    let saved = &mut reopened.active.as_mut().unwrap().reading;
    assert_eq!(saved.anchor().as_ref(), Some(&anchor));
    assert_eq!(after, saved.probe.capture(saved.page).unwrap());
    assert_eq!(fs::read(&fixture.book).unwrap(), source);
    capture_test_screen(&reopened, &fonts, "READER_FAST_KERNING_SCREENSHOT", false);
}

#[test]
#[ignore = "Requires READER_FAST_KERNING_ORIGINAL pointing to an authorized original EPUB"]
fn fast_pair_kerning_real_epub_reopens_at_the_original_source_anchor() {
    use crate::session::Reading;

    let fixture = Fixture::new();
    let original = std::env::var_os("READER_FAST_KERNING_ORIGINAL").unwrap();
    let original_bytes = fs::read(&original).unwrap();
    fs::copy(&original, &fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let mut app = open();
    app.open_path(&fixture.book).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let anchor = reading.probe.page_anchor(0).unwrap();
    *reading = Reading::restore(reading.config.clone(), 0, 0, Some(&anchor)).unwrap();
    app.open_reading_edge(false, 500.0);
    if app.reading_panel != Some(BarPanel::TextRendering) {
        tap(&mut app, "Rendering");
    }
    tap_kerning_choice(&mut app, "Fast");
    let reading = &mut app.active.as_mut().unwrap().reading;
    assert_eq!(reading.anchor().as_ref(), Some(&anchor));
    let saved_frame = reading.probe.capture(reading.page).unwrap();
    app.save().unwrap();
    let mut reopened = open();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        settings(&reopened).effective_font_kerning_mode(),
        FontKerningMode::Fast
    );
    let reading = &mut reopened.active.as_mut().unwrap().reading;
    assert_eq!(reading.anchor().as_ref(), Some(&anchor));
    assert_eq!(saved_frame, reading.probe.capture(reading.page).unwrap());
    assert_eq!(fs::read(&fixture.book).unwrap(), original_bytes);
    assert_eq!(fs::read(original).unwrap(), original_bytes);
}
