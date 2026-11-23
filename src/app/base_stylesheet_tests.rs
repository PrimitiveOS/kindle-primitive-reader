use super::*;
use crate::BaseStylesheet;
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

fn open_stylesheet_row(app: &mut Reader) {
    app.open_reading_edge(false, WIDTH as f32 / 2.);
    if app.reading_panel != Some(BarPanel::TextRendering) {
        tap(app, "Rendering");
    }
    let rect = app.reading_button("Auto");
    assert!(rect.w >= 100 && rect.h >= 100, "{rect:?}");
}

fn settings(app: &Reader) -> crate::Settings {
    app.active.as_ref().unwrap().reading.config.settings
}

#[test]
fn base_stylesheet_choices_apply_and_persist_for_each_book() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();

    assert_eq!(settings(&app).base_stylesheet, BaseStylesheet::Auto);
    assert_eq!(
        app.library
            .state(app.active.as_ref().unwrap().id)
            .unwrap()
            .preferences
            .base_stylesheet,
        None
    );

    for (label, stylesheet) in [
        ("None", BaseStylesheet::None),
        ("Traditional", BaseStylesheet::Traditional),
        ("HTML", BaseStylesheet::HtmlStandard),
        ("Auto", BaseStylesheet::Auto),
    ] {
        open_stylesheet_row(&mut app);
        if settings(&app).base_stylesheet != stylesheet {
            tap(&mut app, label);
        }
        assert_eq!(settings(&app).base_stylesheet, stylesheet);
        let id = app.active.as_ref().unwrap().id;
        assert_eq!(
            app.library.state(id).unwrap().preferences.base_stylesheet,
            Some(stylesheet)
        );
        app.save().unwrap();

        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(settings(&reopened).base_stylesheet, stylesheet);
        app = reopened;
    }

    open_stylesheet_row(&mut app);
    capture_test_screen(&app, &fonts, "READER_BASE_STYLESHEET_SCREENSHOT", false);
}
