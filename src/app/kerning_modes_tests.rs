use super::*;
use crate::FontKerningMode;
use crate::app::tests::{Fixture, capture_test_screen};

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
