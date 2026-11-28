use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{App, BarPanel, Settings};
use crate::session::{Reading, ReadingViewport};
use std::fs;

fn layout(app: &mut Reader) {
    let width = app.reading_viewport().dimensions().0;
    app.open_reading_edge(false, width as f32 / 2.0);
    if app.reading_panel != Some(BarPanel::TextLayout) {
        let button = app.reading_button("Layout");
        tap(app, button);
    }
    assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
}

fn mode(app: &mut Reader, value: PublisherRenderMode) {
    let button = app.reading_button_in_row("Render mode", value.label());
    tap(app, button);
    assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
    assert_eq!(app.book_publisher_render_mode(), value);
}

fn settings_mode(app: &Reader) -> Option<PublisherRenderMode> {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .probe
        .settings
        .publisher_render_mode
}

#[test]
fn publisher_render_settings_live_mode_keeps_passage_reopens_and_resets() {
    let fixture = Fixture::new();
    let authored = crate::test_fixture("render-mode-composition.epub");
    fs::copy(authored, &fixture.book).unwrap();
    let original_bytes = fs::read(&fixture.book).unwrap();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let open = || Reader::open(&fixture.root, &fixture.state, vec![font.clone()]).unwrap();
    let mut app = open();
    app.open_path(&fixture.book).unwrap();
    assert_eq!(settings_mode(&app), None);
    let book = app.active.as_mut().unwrap();
    let anchor = book.reading.probe.matches("Neighbor starts here").remove(0);
    let fallback = book.reading.probe.page_for(&anchor).unwrap();
    book.reading =
        Reading::restore(book.reading.config.clone(), 0, fallback, Some(&anchor)).unwrap();
    let first = book.reading.probe.capture(book.reading.page).unwrap();

    layout(&mut app);
    mode(&mut app, PublisherRenderMode::Legacy);
    mode(&mut app, PublisherRenderMode::Flat);
    mode(&mut app, PublisherRenderMode::Book);
    mode(&mut app, PublisherRenderMode::Web);
    let book = app.active.as_mut().unwrap();
    let web = book.reading.probe.capture(book.reading.page).unwrap();
    assert_ne!(web, first);
    assert_eq!(book.reading.anchor(), Some(anchor.clone()));
    let mut portrait = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    capture(&app, &mut portrait, "render-mode-reader-layout.png", false);

    let mut landscape = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let row = app.reading_button_in_row("Render mode", "Web");
    assert!(row.y >= 0 && row.right() <= ReadingViewport::LANDSCAPE.dimensions().0 as i32);
    capture(
        &app,
        &mut landscape,
        "render-mode-reader-layout-landscape.png",
        false,
    );
    app.save().unwrap();
    let mut reopened = open();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(settings_mode(&reopened), Some(PublisherRenderMode::Web));
    assert_eq!(
        reopened.active.as_ref().unwrap().reading.anchor(),
        Some(anchor)
    );
    layout(&mut reopened);
    mode(&mut reopened, PublisherRenderMode::Reader);
    let book = reopened.active.as_mut().unwrap();
    assert_eq!(
        book.reading.probe.capture(book.reading.page).unwrap(),
        first
    );
    assert_eq!(settings_mode(&reopened), None);
    assert_eq!(fs::read(&fixture.book).unwrap(), original_bytes);
}

#[test]
fn publisher_render_settings_default_applies_to_new_books_and_respects_eligibility() {
    let fixture = Fixture::new();
    let sample = fixture.root.join("Second book.epub");
    fs::copy(crate::test_fixture("render-mode-composition.epub"), &sample).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    let mut app = open();
    app.open_path(&fixture.book).unwrap();
    layout(&mut app);
    let default = app.reading_button_in_row("New-book mode", "Reader");
    tap(&mut app, default);
    assert_eq!(app.screen, Screen::PublisherRenderDefaults);
    tap_contents_item(&mut app, "Web");
    assert_eq!(settings_mode(&app), None);
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 3);
    capture(
        &app,
        &mut canvas,
        "render-mode-reader-new-book-default.png",
        false,
    );

    app.save().unwrap();
    let mut reopened = open();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(settings_mode(&reopened), None);
    reopened.open_path(&sample).unwrap();
    assert_eq!(settings_mode(&reopened), Some(PublisherRenderMode::Web));

    let active_settings = reopened.active.as_ref().unwrap().reading.config.settings;
    reopened
        .activate(Action::Text(Settings {
            publisher_styles: false,
            ..active_settings
        }))
        .unwrap();
    // KOReader keeps render-mode flags with the embedded stylesheet off, so the mode stays.
    assert_eq!(settings_mode(&reopened), Some(PublisherRenderMode::Web));
    layout(&mut reopened);
    assert_ne!(
        reopened.reading_information_in_row("Render mode"),
        Some("Turn on Publisher CSS")
    );
    let active_settings = reopened.active.as_ref().unwrap().reading.config.settings;
    reopened
        .activate(Action::Text(Settings {
            publisher_styles: true,
            ..active_settings
        }))
        .unwrap();
    assert_eq!(settings_mode(&reopened), Some(PublisherRenderMode::Web));
    reopened.activate(Action::ReadingMode(true)).unwrap();
    assert_eq!(settings_mode(&reopened), None);
    layout(&mut reopened);
    assert_eq!(
        reopened.reading_information_in_row("Render mode"),
        Some("Requires one-page reading")
    );
    reopened.activate(Action::ReadingMode(false)).unwrap();
    assert_eq!(settings_mode(&reopened), Some(PublisherRenderMode::Web));
}
