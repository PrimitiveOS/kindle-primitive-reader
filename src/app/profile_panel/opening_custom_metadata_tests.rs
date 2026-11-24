use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, Screen, Settings};
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use crate::reading_profiles::Field;
use crate::session::ReadingViewport;
use primitive_ui::Key;
use std::fs;

const STOCK: &[u8] = include_bytes!("../../../tests/fixtures/koreader-custom-metadata.lua");

#[test]
fn opening_custom_metadata_replaces_fields_only_for_profiles_and_the_current_book_shortcut() {
    let f = Fixture::new();
    let original = fs::read(&f.book).unwrap();
    let folder = f.book.with_extension("sdr");
    fs::create_dir(&folder).unwrap();
    let sidecar = folder.join("custom_metadata.lua");
    fs::write(&sidecar, STOCK).unwrap();
    let mut app = Reader::open(
        &f.root,
        &f.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&f.book).unwrap();
    let provider = app.active.as_ref().unwrap().reading.probe.title.clone();
    assert_eq!(provider, "Field Notes at Dusk (EPUB 3)");
    let effective = app.current_opening_metadata().unwrap();
    assert_eq!(
        effective.current_phrase(Title).as_deref(),
        Some("Coastal Ledger")
    );
    assert_eq!(
        effective.current_phrase(Authors).as_deref(),
        Some("Ada Scout")
    );
    assert_eq!(effective.current_phrase(Language).as_deref(), Some("en"));
    assert_eq!(
        effective.current_phrase(SeriesIndex).as_deref(),
        Some("2.5")
    );

    let normal = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        invert_night_images: true,
        ..normal
    }))
    .unwrap();
    app.save_reading_profile("Coastal notes").unwrap();
    let mut profiles = app.profiles.clone();
    profiles[0].set_fields(vec![Field::NightImages]).unwrap();
    app.commit_profiles(profiles).unwrap();
    app.activate(Action::Text(normal)).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply on book opening");
    tap_contents_item(&mut app, "Book metadata contains");
    tap_contents_item(&mut app, "Title");
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let position = app.current_position();
    let current = app.current_opening_book_button().unwrap();
    tap(&mut app, current);
    assert_eq!(app.editor.string(), "Coastal Ledger");
    capture(
        &app,
        &mut canvas,
        "landscape-profile-koreader-custom-title.png",
        false,
    );
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    assert_eq!(app.current_position(), position);
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    app.open_path(&f.book).unwrap();
    assert_eq!(app.current_position(), position);
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    assert_eq!(app.active.as_ref().unwrap().reading.probe.title, provider);
    assert_eq!(fs::read(&f.book).unwrap(), original);
    assert_eq!(fs::read(&sidecar).unwrap(), STOCK);

    // With a malformed custom field, the whole overlay falls back. Cached
    // doc_props cannot match either, even in a syntactically valid sidecar.
    app.activate(Action::Text(normal)).unwrap();
    for contents in [
        "return {custom_props={title='Coastal Ledger', language=false}}",
        "return {doc_props={title='Coastal Ledger'}}",
    ] {
        fs::write(&sidecar, contents).unwrap();
        app.open_path(&f.book).unwrap();
        assert!(
            !app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .invert_night_images
        );
        assert_eq!(
            app.current_opening_metadata()
                .unwrap()
                .current_phrase(Title)
                .as_deref(),
            Some(provider.as_str())
        );
    }
}

#[test]
fn opening_custom_metadata_follows_the_stock_stem_and_ignores_the_progress_table() {
    let f = Fixture::new();
    let pdf = f.book.with_extension("pdf");
    let original = striped_pdf_sized_bytes(3, 612, 820);
    fs::write(&pdf, &original).unwrap();
    let folder = f.book.with_extension("sdr");
    fs::create_dir(&folder).unwrap();
    fs::write(
        folder.join("metadata.pdf.lua"),
        b"return {custom_props={title='Decoy title'}}",
    )
    .unwrap();
    let mut app = f.app();
    app.open_path(&pdf).unwrap();
    assert_eq!(
        app.current_opening_metadata()
            .unwrap()
            .current_phrase(Title)
            .as_deref(),
        Some("Field Notes")
    );
    fs::write(folder.join("custom_metadata.lua"), STOCK).unwrap();
    assert_eq!(
        app.current_opening_metadata()
            .unwrap()
            .current_phrase(Title)
            .as_deref(),
        Some("Coastal Ledger")
    );
    assert_eq!(app.fixed.as_ref().unwrap().reading.title(), None);
    app.follow_fixed(1, None).unwrap();
    app.activate(Action::ScanGamma(23)).unwrap();
    app.save_reading_profile("Coastal scans").unwrap();
    let mut profiles = app.profiles.clone();
    profiles[0].set_fields(vec![Field::ScanContrast]).unwrap();
    let mut rules = crate::reading_profiles::opening::Conditions::default();
    rules.metadata.insert(Title, "coastal ledger".into());
    profiles[0].set_opening_conditions(rules);
    app.commit_profiles(profiles).unwrap();
    app.activate(Action::ScanGamma(10)).unwrap();
    app.open_path(&pdf).unwrap();
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), 1);
    assert_eq!(reading.preferences().scan_gamma_tenths, Some(23));
    assert_eq!(fs::read(&pdf).unwrap(), original);
    app.open_path(&f.book).unwrap();
    assert!(app.active.is_some());
    assert_eq!(
        app.current_opening_metadata()
            .unwrap()
            .current_phrase(Title)
            .as_deref(),
        Some("Coastal Ledger")
    );
}
