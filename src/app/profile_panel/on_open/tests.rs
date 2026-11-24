use super::*;
use crate::app::profile_panel::Name;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, Change, ReadingOrientation, Screen, Settings};
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use crate::library::FixedTileAxis;
use crate::reading_profiles::opening::MetadataField;
use crate::reading_profiles::{Field, ValueField};
use crate::session::ReadingViewport;
use primitive_app::App;
use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest};
use primitive_ui::Key;
use serde_json::json;
use std::fs;
use std::io::{Read, Write};

fn metadata_epub(path: &Path, fields: &str) {
    let bytes = include_bytes!("../../../../tests/fixtures/field-notes-epub3.epub");
    let mut source = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut book = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..source.len() {
        let mut member = source.by_index(index).unwrap();
        if member.is_dir() {
            book.add_directory(member.name(), options).unwrap();
        } else {
            book.start_file(member.name(), options).unwrap();
            if member.name() == "EPUB/package.opf" {
                let mut package = String::new();
                member.read_to_string(&mut package).unwrap();
                let old = package.find("<dc:title>").unwrap();
                let end = package.find("<dc:rights>").unwrap();
                package.replace_range(old..end, fields);
                book.write_all(package.as_bytes()).unwrap();
            } else {
                std::io::copy(&mut member, &mut book).unwrap();
            }
        }
    }
    book.finish().unwrap();
}

fn metadata_pdf(path: &Path) {
    use lopdf::{Object, dictionary};
    let mut file = lopdf::Document::load_mem(&striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    let id = file.add_object(dictionary! {
        "Title" => Object::string_literal("Tide log"),
        "Author" => Object::string_literal("Ada Scout"),
        "Subject" => Object::string_literal("Two coastal transects"),
        "Keywords" => Object::string_literal("Gauge, Shore"),
    });
    file.trailer.set("Info", id);
    file.catalog_mut()
        .unwrap()
        .set("Lang", Object::string_literal("en-GB"));
    file.save(path).unwrap();
}

fn metadata_editor(app: &mut Reader, index: usize, field: MetadataField) {
    tap_contents_item(app, field.label());
    assert_eq!(
        app.screen,
        Screen::ProfileName(Name::OpeningMetadata(index, field))
    );
}

fn save_metadata_editor(app: &mut Reader, value: &str) {
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let save = key(app, Key::Go, "Save");
    tap(app, save);
}

fn stock_metadata_fixture(source: &Path) {
    if let Some(output) = std::env::var_os("READER_METADATA_STOCK_FIXTURES") {
        let output = Path::new(&output);
        fs::create_dir_all(output).unwrap();
        fs::copy(source, output.join(source.file_name().unwrap())).unwrap();
    }
}

#[test]
fn opening_metadata_uses_publisher_fields_unicode_and_preserves_the_open_book() {
    use MetadataField::*;
    let f = Fixture::new();
    let edition = f.root.join("Publisher metadata.epub");
    let fallback = f.root.join("Seabird Survey.epub");
    metadata_epub(
        &edition,
        r#"
        <dc:title>ĐẶNG [A+B]</dc:title><dc:creator>Ada Scout</dc:creator>
        <dc:language>vi</dc:language><dc:subject>Estuary</dc:subject>
        <dc:subject>Circulation</dc:subject><dc:description>Lower river delta</dc:description>
        <meta name="calibre:series" content="Sounding Log"/>
        <meta name="calibre:series_index" content="2.5"/>
    "#,
    );
    metadata_epub(&fallback, "<dc:title> </dc:title>");
    stock_metadata_fixture(&edition);
    stock_metadata_fixture(&fallback);
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&edition).unwrap();
    let normal = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        invert_night_images: true,
        ..normal
    }))
    .unwrap();
    app.save_reading_profile("Evening contrast").unwrap();
    include(&mut app, 0, vec![Field::NightImages]);
    app.activate(Action::Text(normal)).unwrap();
    let metadata = app.current_opening_metadata().unwrap();
    let id = app.open_book_id().unwrap();
    let context = Book {
        id,
        path: &edition,
        library: &app.library,
        orientation: ReadingOrientation::Upright,
        is_new: true,
        metadata: Some(&metadata),
    };
    for (field, phrase) in [
        (Title, "waypoint, đặng [a+b]"),
        (Authors, "ADA SCOUT"),
        (Series, "sounding log"),
        (SeriesIndex, "2.5"),
        (Language, "vi"),
        (Keywords, "circulation"),
        (Description, "river DELTA"),
    ] {
        let mut rules = crate::reading_profiles::opening::Conditions::default();
        rules.metadata.insert(field, phrase.into());
        let mut profile = app.profiles[0].clone();
        profile.set_opening_conditions(rules);
        assert!(profile.matches_opening(&context), "{field:?}");
    }
    app.follow_reflow_page(1, 0).unwrap();
    let anchor = app.current_position().unwrap().anchor;
    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Book metadata contains");
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    capture(
        &app,
        &mut canvas,
        "landscape-profile-opening-metadata.png",
        false,
    );
    metadata_editor(&mut app, 0, Title);
    save_metadata_editor(&mut app, "invalid, , phrase");
    assert_eq!(
        app.screen,
        Screen::ProfileName(Name::OpeningMetadata(0, Title))
    );
    assert!(app.profiles[0].opening_conditions().is_none());
    let current_book = app.current_opening_book_button().unwrap();
    tap(&mut app, current_book);
    assert_eq!(app.editor.string(), "ĐẶNG [A+B]");
    capture(
        &app,
        &mut canvas,
        "landscape-profile-current-book-metadata.png",
        false,
    );
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    assert_eq!(app.current_position().unwrap().anchor, anchor);
    metadata_editor(&mut app, 0, Description);
    save_metadata_editor(&mut app, "river delta");
    app.save().unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&edition).unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    assert_eq!(app.current_position().unwrap().anchor, anchor);
    app.open_path(&fallback).unwrap();
    let metadata = app.current_opening_metadata().unwrap();
    assert_eq!(
        metadata.current_phrase(Title).as_deref(),
        Some("Seabird Survey")
    );
    assert_eq!(metadata.current_phrase(Authors), None);
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Book metadata contains");
    metadata_editor(&mut app, 0, Title);
    save_metadata_editor(&mut app, "");
    assert_eq!(
        app.profiles[0].opening_conditions().unwrap().metadata.len(),
        1
    );
    metadata_editor(&mut app, 0, Authors);
    assert!(app.current_opening_book_button().is_none());
}

#[test]
fn opening_metadata_reads_fixed_document_fields_without_changing_physical_page() {
    use MetadataField::*;
    let f = Fixture::new();
    let source = f.root.join("Source.pdf");
    let target = f.root.join("Survey print.pdf");
    fs::write(&source, striped_pdf_sized_bytes(3, 820, 612)).unwrap();
    metadata_pdf(&target);
    stock_metadata_fixture(&target);
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&target).unwrap();
    let fields = app.current_opening_metadata().unwrap();
    for (field, phrase) in [
        (Title, "Tide log"),
        (Authors, "Ada Scout"),
        (Description, "Two coastal transects"),
        (Keywords, "Gauge"),
        (Language, "en-GB"),
    ] {
        assert_eq!(fields.current_phrase(field).as_deref(), Some(phrase));
    }
    assert_eq!(fields.current_phrase(SeriesIndex), None);
    app.follow_fixed(1, None).unwrap();
    let page = app.fixed.as_ref().unwrap().reading.page();
    app.open_path(&source).unwrap();
    app.activate(Action::ScanGamma(23)).unwrap();
    app.save_reading_profile("Survey gamma").unwrap();
    include(&mut app, 0, vec![Field::ScanContrast]);
    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Book metadata contains");
    metadata_editor(&mut app, 0, Keywords);
    save_metadata_editor(&mut app, "absent, shore");
    app.open_path(&target).unwrap();
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), page);
    assert_eq!(reading.preferences().scan_gamma_tenths, Some(23));
    let invalid = serde_json::to_value(&app.profiles).unwrap();
    for metadata in [
        json!({"title":"a,,b"}),
        json!({"page_count":"3"}),
        json!({"authors":"x".repeat(161)}),
    ] {
        let mut bad = invalid.clone();
        bad[0]["opening"]["metadata"] = metadata;
        assert!(crate::reading_profiles::decode(bad).is_err());
    }
}

#[test]
fn opening_metadata_reads_djvu_publisher_values_and_comic_title_fallback() {
    use MetadataField::*;
    use djvu_rs::{djvu_mut::DjVuDocumentMut, metadata::DjVuMetadata};
    let f = Fixture::new();
    let path = f.root.join("Scanned sequence.djvu");
    let mut djvu = DjVuDocumentMut::from_bytes(include_bytes!(
        "../../../../tests/fixtures/beacon-atlas.djvu"
    ))
    .unwrap();
    djvu.set_metadata(&DjVuMetadata {
        title: Some("Beacon Atlas".into()),
        author: Some("Ada Scout".into()),
        subject: Some("A weathered scan".into()),
        keywords: Some("Beacon, Ice".into()),
        extra: vec![
            ("Series".into(), "North archive #2.5".into()),
            ("Language".into(), "en".into()),
        ],
        ..DjVuMetadata::default()
    })
    .unwrap();
    fs::write(&path, djvu.into_bytes()).unwrap();
    stock_metadata_fixture(&path);
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    let metadata = app.current_opening_metadata().unwrap();
    for (field, value) in [
        (Title, "Beacon Atlas"),
        (Authors, "Ada Scout"),
        (Description, "A weathered scan"),
        (Keywords, "Beacon"),
        (Series, "North archive"),
        (SeriesIndex, "2.5"),
        (Language, "en"),
    ] {
        assert_eq!(metadata.current_phrase(field).as_deref(), Some(value));
    }
    let path = f.root.join("Drawn sequence.cbz");
    let bytes = include_bytes!("../../../../tests/fixtures/field-notes-epub3.epub");
    let mut source = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("Page 01.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::copy(
        &mut source.by_name("EPUB/images/shore.png").unwrap(),
        &mut archive,
    )
    .unwrap();
    archive.finish().unwrap();
    stock_metadata_fixture(&path);
    app.open_path(&path).unwrap();
    let metadata = app.current_opening_metadata().unwrap();
    assert_eq!(
        metadata.current_phrase(Title).as_deref(),
        Some("Drawn sequence")
    );
    for field in &MetadataField::ALL[1..] {
        assert!(metadata.current_phrase(*field).is_none());
    }
}

fn fonts() -> Vec<std::path::PathBuf> {
    vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ]
}

fn include(app: &mut Reader, index: usize, fields: Vec<Field>) {
    let mut profiles = app.profiles.clone();
    profiles[index].set_fields(fields).unwrap();
    app.commit_profiles(profiles).unwrap();
}

fn toggle(app: &mut Reader, index: usize) {
    app.show(Screen::ReadingProfile(index));
    tap_contents_item(app, "Apply on book opening");
    tap_contents_item(app, "Always apply");
    let back = app.navigation_layout().back();
    tap(app, back);
}

fn open_rules(app: &mut Reader, index: usize) {
    app.show(Screen::ReadingProfile(index));
    tap_contents_item(app, "Apply on book opening");
}

fn set_path(app: &mut Reader, index: usize, value: &str) {
    tap_contents_item(app, "File path contains");
    assert_eq!(app.screen, Screen::ProfileName(Name::OpeningPath(index)));
    app.editor.text = value.chars().collect();
    app.editor.caret = app.editor.text.len();
    let save = key(app, Key::Go, "Save");
    tap(app, save);
}

fn stage_opening(app: &mut Reader, generation: u64, turn: LogicalTurn) -> (Viewport, Vec<u8>) {
    use crate::app::selection_viewport_tests::canvas;
    let size = if matches!(turn, LogicalTurn::Clockwise | LogicalTurn::Counterclockwise) {
        ReadingViewport::LANDSCAPE
    } else {
        ReadingViewport::PORTRAIT
    };
    let candidate = Viewport {
        generation,
        turn,
        logical_size: size.dimensions(),
    };
    let mut display = canvas(app, size);
    app.stage_viewport(candidate).unwrap();
    display.begin();
    app.draw_staged_viewport(&mut display).unwrap();
    (candidate, display.finish(false))
}

#[test]
fn accepted_opening_turn_composes_epub_rules_on_same_turn_and_keeps_anchor() {
    let f = Fixture::new();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let basis = app.current_text_preferences().unwrap();
    let night_images = basis.invert_night_images;
    for (name, size, inverted) in [
        ("Alpha upside down", 32.0, !night_images),
        ("Zulu EPUB title", 26.0, night_images),
    ] {
        app.apply_text_profile(BookPreferences {
            logical_font_size: Some(size),
            invert_night_images: inverted,
            ..basis.clone()
        })
        .unwrap();
        app.save_reading_profile(name).unwrap();
    }
    include(&mut app, 0, vec![Field::FontSize, Field::NightImages]);
    include(&mut app, 1, vec![Field::FontSize]);
    let mut profiles = app.profiles.clone();
    profiles[0].set_opening_conditions(crate::reading_profiles::opening::Conditions {
        orientations: vec![ReadingOrientation::UpsideDown],
        ..Default::default()
    });
    profiles[1].set_opening_conditions(crate::reading_profiles::opening::Conditions {
        metadata: [(MetadataField::Title, "at dusk (epub 3)".into())].into(),
        ..Default::default()
    });
    app.commit_profiles(profiles).unwrap();
    app.apply_text_profile(basis.clone()).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let saved = app.current_position().unwrap();
    let id = app.open_book_id().unwrap();
    let (prior, _) = stage_opening(&mut app, 1, LogicalTurn::UpsideDown);
    app.finish_viewport(prior, ViewportDecision::Commit);

    app.negotiated_opening_profiles = true;
    app.open_path(&f.book).unwrap();
    assert_eq!(
        app.reading_orientation(),
        ReadingOrientation::FollowLauncher
    );
    assert!(app.viewport_needs_stage(prior));
    assert_eq!(app.library.state(id).unwrap().preferences, basis);
    let (stale, _) = stage_opening(&mut app, 2, LogicalTurn::UpsideDown);
    app.open_path(&f.book).unwrap();
    app.finish_viewport(stale, ViewportDecision::Abort);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::FollowLauncher)
    );
    app.finish_viewport(stale, ViewportDecision::Commit);
    assert_eq!(app.library.state(id).unwrap().preferences, basis);
    let (accepted, staged) = stage_opening(&mut app, 3, LogicalTurn::UpsideDown);
    assert_eq!(app.library.state(id).unwrap().preferences, basis);
    assert_eq!(app.current_position(), Some(saved.clone()));
    app.finish_viewport(accepted, ViewportDecision::Commit);
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.logical_font_size, Some(26.0));
    assert_eq!(settings.invert_night_images, !night_images);
    assert_eq!(app.current_position().unwrap().anchor, saved.anchor);
    assert!(!app.viewport_needs_stage(accepted));
    let mut display = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    assert!(primitive_app::frame(&app, &mut display, false) == staged);
    app.save().unwrap();
    let mut headless = Reader::open(&f.root, &f.state, fonts()).unwrap();
    headless.open_path(&f.book).unwrap();
    assert!(headless.pending_opening_profiles.is_none());
    assert_eq!(headless.current_position().unwrap().anchor, saved.anchor);
}

#[test]
fn accepted_opening_turn_rolls_back_pdf_and_uses_a_verified_fallback() {
    let f = Fixture::new();
    let source = f.root.join("Tall profile source.pdf");
    let target = f.root.join("Survey print target.pdf");
    fs::write(&source, striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    let source_bytes = striped_pdf_sized_bytes(3, 820, 612);
    fs::write(&target, &source_bytes).unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&source).unwrap();
    app.activate(Action::SetFixedTileZoom(FixedTileAxis::Rows, 24))
        .unwrap();
    app.activate(Action::ScanGamma(27)).unwrap();
    app.save_reading_profile("Alpha wide view").unwrap();
    include(&mut app, 0, vec![Field::FitAndZoom, Field::ScanContrast]);
    app.activate(Action::ScanGamma(19)).unwrap();
    app.save_reading_profile("Zulu print contrast").unwrap();
    include(&mut app, 1, vec![Field::ScanContrast]);
    let mut profiles = app.profiles.clone();
    profiles[0].set_opening_conditions(crate::reading_profiles::opening::Conditions {
        orientations: vec![ReadingOrientation::LandscapeRight],
        ..Default::default()
    });
    profiles[1].set_opening_conditions(crate::reading_profiles::opening::Conditions {
        path: Some("print target".into()),
        ..Default::default()
    });
    app.open_path(&target).unwrap();
    app.activate(Action::FixedPage(2)).unwrap();
    let id = app.open_book_id().unwrap();
    let base_view = app.fixed.as_ref().unwrap().reading.preferences();
    let (prior, _) = stage_opening(&mut app, 1, LogicalTurn::Upright);
    app.finish_viewport(prior, ViewportDecision::Commit);
    app.library.state_mut(id).unwrap().screen_orientation =
        Some(ReadingOrientation::LandscapeRight);
    app.commit_profiles(profiles).unwrap();
    app.negotiated_opening_profiles = true;
    app.open_path(&target).unwrap();
    let (rejected, _) = stage_opening(&mut app, 2, LogicalTurn::Clockwise);
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), base_view);
    app.finish_viewport(rejected, ViewportDecision::Abort);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(LogicalTurn::Upright))
    );
    assert_eq!(app.library.state(id).unwrap().fixed_preferences, base_view);
    app.input(primitive_ui::Input::PageBack);
    if let Some(deadline) = app.fixed.as_ref().unwrap().reading.deadline() {
        app.tick(deadline);
    }
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    assert!(app.pending_opening_profiles.is_some());
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .position
            .as_ref()
            .unwrap()
            .page,
        2
    );
    let (fallback, staged) = stage_opening(&mut app, 3, LogicalTurn::Upright);
    app.finish_viewport(fallback, ViewportDecision::Commit);
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), 1);
    assert!(reading.preferences().tile_zoom.is_none());
    assert_eq!(reading.preferences().scan_gamma_tenths, Some(19));
    let mut display = crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::PORTRAIT);
    assert!(primitive_app::frame(&app, &mut display, false) == staged);
    assert!(app.pending_opening_profiles.is_none());

    app.open_path(&target).unwrap();
    let (wide, staged) = stage_opening(&mut app, 4, LogicalTurn::Clockwise);
    assert!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .tile_zoom
            .is_none()
    );
    app.finish_viewport(wide, ViewportDecision::Commit);
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), 1);
    assert_eq!(reading.preferences().scan_gamma_tenths, Some(19));
    let tile = reading.preferences().tile_zoom.unwrap();
    assert_eq!((tile.axis, tile.count_tenths), (FixedTileAxis::Rows, 24));
    let mut display =
        crate::app::selection_viewport_tests::canvas(&app, ReadingViewport::LANDSCAPE);
    assert!(primitive_app::frame(&app, &mut display, false) == staged);
    assert_eq!(fs::read(&target).unwrap(), source_bytes);
}

#[test]
fn accepted_opening_turn_startup_retry_retains_new_book_status_until_commit() {
    let f = Fixture::new();
    let target = f.root.join("Rain log.txt");
    fs::write(&target, "Rain filled the water tank by nightfall.").unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let basis = app.current_text_preferences().unwrap();
    let color = basis.invert_night_images;
    app.apply_text_profile(BookPreferences {
        invert_night_images: !color,
        ..basis.clone()
    })
    .unwrap();
    app.save_reading_profile("Only first inverted view")
        .unwrap();
    include(&mut app, 0, vec![Field::NightImages]);
    let mut profiles = app.profiles.clone();
    profiles[0].set_opening_conditions(crate::reading_profiles::opening::Conditions {
        orientations: vec![ReadingOrientation::UpsideDown],
        new_only: true,
        ..Default::default()
    });
    app.commit_profiles(profiles).unwrap();
    app.controls.reflow_orientation_default = Some(ReadingOrientation::LandscapeRight);
    app.negotiated_opening_profiles = true;
    app.open_path(&target).unwrap();
    let id = app.open_book_id().unwrap();
    assert!(app.library.state(id).unwrap().position.is_none());
    let (first, _) = stage_opening(&mut app, 1, LogicalTurn::Clockwise);
    app.finish_viewport(first, ViewportDecision::Abort);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::FollowLauncher)
    );
    assert!(app.library.state(id).unwrap().position.is_none());
    let (fallback, _) = stage_opening(&mut app, 2, LogicalTurn::UpsideDown);
    app.finish_viewport(fallback, ViewportDecision::Commit);
    assert!(app.library.state(id).unwrap().position.is_some());
    assert_eq!(
        app.current_text_preferences().unwrap().invert_night_images,
        !color
    );
    let mut prefs = app.current_text_preferences().unwrap();
    prefs.invert_night_images = color;
    app.apply_text_profile(prefs).unwrap();
    app.open_path(&target).unwrap();
    assert!(app.pending_opening_profiles.is_none());
    assert_eq!(
        app.current_text_preferences().unwrap().invert_night_images,
        color
    );
}

#[test]
fn opening_conditions_use_orientation_paths_and_collections_as_alternatives() {
    let f = Fixture::new();
    let enrolled = f.root.join("Outside inventory.txt");
    let matched = f.root.join("WAyPoint.txt");
    let unmatched = f.root.join("Rain notes.txt");
    let first_time = f.root.join("Ridge survey.txt");
    let no_matchers = f.root.join("Second field report.txt");
    for (path, contents) in [
        (&enrolled, "Survey the eastern shelf."),
        (&matched, "The waypoint is along the coast."),
        (&unmatched, "Rain filled the water tank."),
        (&first_time, "Record samples from the ridge."),
        (&no_matchers, "Ship the results to the office."),
    ] {
        fs::write(path, contents).unwrap();
    }
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&enrolled).unwrap();
    let id = app.open_book_id().unwrap();
    app.library.create_collection("Sea surveys").unwrap();
    app.library
        .set_collection_membership("Sea surveys", id, true)
        .unwrap();
    app.open_path(&f.book).unwrap();
    let normal = app.active.as_ref().unwrap().reading.config.settings;
    let pos = app.current_position();
    app.activate(Action::Text(Settings {
        invert_night_images: true,
        ..normal
    }))
    .unwrap();
    app.save_reading_profile("Evening contrast").unwrap();
    include(&mut app, 0, vec![Field::NightImages]);
    app.activate(Action::Text(normal)).unwrap();

    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Preferred opening orientation");
    tap_contents_item(&mut app, "Upside down");
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    set_path(&mut app, 0, "waypoint, ridge");
    tap_contents_item(&mut app, "Book is in a collection");
    tap_contents_item(&mut app, "Sea surveys");
    tap(&mut app, back);
    assert_eq!(app.current_position(), pos);
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    capture(
        &app,
        &mut canvas,
        "landscape-profile-opening-conditions.png",
        false,
    );
    app.save().unwrap();

    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    let inverted = |app: &Reader| {
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    };
    app.open_path(&enrolled).unwrap();
    assert!(inverted(&app));
    app.open_path(&matched).unwrap();
    assert!(inverted(&app));
    app.open_path(&unmatched).unwrap();
    assert!(!inverted(&app));
    app.set_reading_orientation(ReadingOrientation::UpsideDown)
        .unwrap();
    assert_eq!(app.applied_change(), Change::Full);
    app.open_path(&unmatched).unwrap();
    assert!(inverted(&app));

    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Only for new books");
    app.open_path(&enrolled).unwrap();
    let applied = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        invert_night_images: false,
        ..applied
    }))
    .unwrap();
    app.open_path(&enrolled).unwrap();
    assert!(!inverted(&app));
    app.open_path(&first_time).unwrap();
    assert!(inverted(&app));

    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Clear conditions");
    tap_contents_item(&mut app, "Only for new books");
    app.open_path(&no_matchers).unwrap();
    assert!(!inverted(&app));
    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "Always apply");
    assert!(app.profiles[0].applies_on_open());
    assert!(app.profiles[0].opening_conditions().is_none());
}

#[test]
fn opening_path_rules_apply_to_fixed_profiles_and_validate_backups() {
    let f = Fixture::new();
    let source = f.root.join("Source.pdf");
    let target = f.root.join("Survey print.pdf");
    fs::write(&source, striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    fs::write(&target, striped_pdf_sized_bytes(3, 820, 612)).unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&source).unwrap();
    app.activate(Action::ScanGamma(23)).unwrap();
    app.save_reading_profile("Survey gamma").unwrap();
    include(&mut app, 0, vec![Field::ScanContrast]);
    open_rules(&mut app, 0);
    set_path(&mut app, 0, " survey ,,");
    assert_eq!(app.screen, Screen::ProfileName(Name::OpeningPath(0)));
    assert!(app.notice.as_deref().unwrap().contains("nonempty phrase"));
    assert!(app.profiles[0].opening_conditions().is_none());
    app.editor.text = "Survey".chars().collect();
    app.editor.caret = app.editor.text.len();
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    app.open_path(&target).unwrap();
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .scan_gamma_tenths,
        Some(23)
    );
    app.open_path(&f.book).unwrap();
    assert!(app.active.is_some());
    assert!(app.notice.is_none());

    let value = serde_json::to_value(&app.profiles).unwrap();
    assert_eq!(
        crate::reading_profiles::decode(value.clone()).unwrap(),
        app.profiles
    );
    for bad in [
        json!({"path":"a,,b"}),
        json!({"orientations":["upright","upright"]}),
        json!({"path": "a".repeat(161)}),
        json!({"collections":["Sea surveys", "sea SURVEYS"]}),
    ] {
        let mut corrupt = value.clone();
        corrupt[0]["opening"] = bad;
        assert!(crate::reading_profiles::decode(corrupt).is_err());
    }
    let mut contradictory = value;
    contradictory[0]["apply_on_open"] = json!(true);
    assert!(crate::reading_profiles::decode(contradictory).is_err());
}

#[test]
fn current_book_inserts_a_path_the_keyboard_cannot_type() {
    let f = Fixture::new();
    let title = "Sổ tay bờ biển.txt";
    let book = f.root.join(title);
    fs::write(&book, "The tide rose beyond the last marker.").unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&book).unwrap();
    app.save_reading_profile("Coastal notes").unwrap();
    open_rules(&mut app, 0);
    tap_contents_item(&mut app, "File path contains");
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let button = app.current_opening_book_button().unwrap();
    tap(&mut app, button);
    assert_eq!(app.editor.string(), title);
    assert!(app.profiles[0].opening_conditions().is_none());
    capture(
        &app,
        &mut canvas,
        "landscape-profile-current-book-path.png",
        false,
    );
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    assert_eq!(
        app.profiles[0]
            .opening_conditions()
            .unwrap()
            .path
            .as_deref(),
        Some(title)
    );
}

#[test]
fn opening_profiles_compose_reflow_settings_and_keep_the_saved_passage() {
    let f = Fixture::new();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    let original = fs::read(&f.book).unwrap();
    app.open_path(&f.book).unwrap();
    let basis = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: basis.font_scale.effective(28.0).unwrap(),
        logical_font_size: Some(28.0),
        invert_night_images: true,
        ..basis
    }))
    .unwrap();
    app.save_reading_profile("Zulu").unwrap();
    include(&mut app, 0, vec![Field::FontSize, Field::NightImages]);
    app.activate(Action::Text(Settings {
        font_size: basis.font_scale.effective(20.0).unwrap(),
        logical_font_size: Some(20.0),
        smooth_images: !basis.smooth_images,
        ..basis
    }))
    .unwrap();
    app.save_reading_profile("Alpha").unwrap();
    include(&mut app, 1, vec![Field::FontSize, Field::ImageScaling]);
    app.activate(Action::Text(basis)).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let anchor = app.current_position().unwrap().anchor;
    let original_layout = app.active.as_ref().unwrap().reading.config.settings;

    toggle(&mut app, 0);
    assert!(app.profiles[0].applies_on_open());
    capture(
        &app,
        &mut canvas,
        "landscape-profile-apply-on-opening.png",
        false,
    );
    toggle(&mut app, 1);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings,
        original_layout
    );
    app.edit_reading_profile_name(Name::Duplicate(0), "Copy")
        .unwrap();
    assert!(!app.profiles[2].applies_on_open());
    app.edit_reading_profile_name(Name::Rename(0), "Zulu renamed")
        .unwrap();
    assert!(app.profiles[0].applies_on_open());
    app.save().unwrap();

    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.logical_font_size, Some(28.0));
    assert!(settings.invert_night_images);
    assert_eq!(settings.smooth_images, !basis.smooth_images);
    assert_eq!(app.current_position().unwrap().anchor, anchor);
    assert_eq!(fs::read(&f.book).unwrap(), original);

    let fresh = f.root.join("Earlier in the season.txt");
    fs::write(
        &fresh,
        b"Walk the outer path to the lighthouse before dusk.",
    )
    .unwrap();
    app.open_path(&fresh).unwrap();
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .logical_font_size,
        Some(28.0)
    );
    toggle(&mut app, 0);
    assert!(!app.profiles[0].applies_on_open());
    app.open_path(&fresh).unwrap();
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .logical_font_size,
        Some(20.0)
    );
}

#[test]
fn opening_fixed_profiles_preserve_the_physical_page_and_project_tiled_fit() {
    let f = Fixture::new();
    let source = f.root.join("Tall survey.pdf");
    let target = f.root.join("Wide survey.pdf");
    let bytes = striped_pdf_sized_bytes(3, 820, 612);
    fs::write(&source, striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    fs::write(&target, &bytes).unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&source).unwrap();
    app.activate(Action::SetFixedTileZoom(FixedTileAxis::Rows, 24))
        .unwrap();
    app.activate(Action::ScanGamma(19)).unwrap();
    app.save_reading_profile("A tiled scan").unwrap();
    include(&mut app, 0, vec![Field::FitAndZoom, Field::ScanContrast]);
    app.save_reading_profile("Z overlap").unwrap();
    include(&mut app, 1, vec![Field::PanOverlap]);
    let mut profiles = app.profiles.clone();
    profiles[1]
        .set_value(ValueField::VerticalOverlap, 57.0)
        .unwrap();
    app.commit_profiles(profiles).unwrap();

    app.open_path(&target).unwrap();
    app.activate(Action::FixedPage(2)).unwrap();
    let prior = app.fixed.as_ref().unwrap().reading.preferences();
    assert!(prior.tile_zoom.is_none());
    toggle(&mut app, 0);
    toggle(&mut app, 1);
    app.open_path(&target).unwrap();
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), 2);
    let profile_view = reading.preferences();
    let zoom = profile_view.tile_zoom.unwrap();
    assert_eq!(zoom.axis, FixedTileAxis::Rows);
    assert_eq!(zoom.count_tenths, 24);
    assert_eq!(profile_view.scan_gamma_tenths, Some(19));
    assert_eq!(profile_view.zoom_overlap_v(), 57);
    assert_eq!(fs::read(&target).unwrap(), bytes);

    app.open_path(&f.book).unwrap();
    assert!(app.active.is_some() && app.fixed.is_none());
    assert!(app.notice.is_none());
}

#[test]
fn opening_profile_with_a_missing_font_falls_back_to_the_book_settings() {
    let f = Fixture::new();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let original = app.current_position();
    let initial = app.active.as_ref().unwrap().reading.config.settings;
    app.save_reading_profile("Missing font").unwrap();
    include(&mut app, 0, vec![Field::Font]);
    let mut profiles = app.profiles.clone();
    let Profile::Text { preferences, .. } = &mut profiles[0] else {
        unreachable!();
    };
    preferences.font_file = Some("Unavailable.ttf".into());
    app.commit_profiles(profiles).unwrap();
    toggle(&mut app, 0);
    app.open_path(&f.book).unwrap();
    assert_eq!(app.current_position(), original);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings,
        initial
    );
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("Opening profile could not be applied")
    );
    assert!(app.profiles[0].applies_on_open());
}

#[test]
fn opening_profile_resolves_legacy_screen_numbers_before_changing_the_layout() {
    let f = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &f.book).unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let basis = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: basis.font_scale.effective(72.0).unwrap(),
        logical_font_size: Some(72.0),
        ..basis
    }))
    .unwrap();
    app.save_reading_profile("Large text").unwrap();
    include(&mut app, 0, vec![Field::FontSize]);
    app.activate(Action::Text(basis)).unwrap();
    app.follow_reflow_page(0, 6).unwrap();
    let original = app.current_position().unwrap();
    assert_eq!(original.page, 6);
    assert!(original.anchor.is_some());
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().position = Some(ReadingPosition {
        anchor: None,
        ..original.clone()
    });
    app.library.persist().unwrap();
    toggle(&mut app, 0);
    let mut reopened = Reader::open(&f.root, &f.state, fonts()).unwrap();
    reopened.open_path(&f.book).unwrap();
    let result = reopened.current_position().unwrap();
    assert_ne!(result.page, original.page);
    assert_eq!(result.anchor, original.anchor);
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .logical_font_size,
        Some(72.0)
    );
}

#[test]
fn opening_profile_uses_the_saved_landscape_viewport_to_check_fit() {
    let f = Fixture::new();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let basis = app.current_text_preferences().unwrap();
    app.apply_text_profile(BookPreferences {
        logical_font_size: Some(200.0),
        line_spacing_percent: Some(200),
        margin_top: Some(310),
        margin_bottom: Some(310),
        ..basis.clone()
    })
    .unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .is_err()
    );
    app.save_reading_profile("Portrait large print").unwrap();
    include(
        &mut app,
        0,
        vec![Field::FontSize, Field::Margins, Field::LineSpacing],
    );
    app.apply_text_profile(basis.clone()).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    state.screen_orientation = Some(ReadingOrientation::LandscapeRight);
    let position = state.position.clone();
    toggle(&mut app, 0);
    app.open_path(&f.book).unwrap();
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("Opening profile could not be applied")
    );
    assert_eq!(app.library.state(id).unwrap().preferences, basis);
    assert_eq!(app.current_position(), position);
    let _ = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    assert_eq!(
        app.current_position().unwrap().anchor,
        position.unwrap().anchor
    );
}

#[test]
fn opening_profile_retains_the_existing_font_fallback_when_font_is_excluded() {
    let f = Fixture::new();
    let mut app = Reader::open(&f.root, &f.state, fonts()).unwrap();
    app.open_path(&f.book).unwrap();
    let initial = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        invert_night_images: true,
        ..initial
    }))
    .unwrap();
    app.save_reading_profile("Invert images").unwrap();
    include(&mut app, 0, vec![Field::NightImages]);
    app.activate(Action::Text(initial)).unwrap();
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().preferences.font_file = Some("Missing.ttf".into());
    toggle(&mut app, 0);
    app.open_path(&f.book).unwrap();
    let active = &app.active.as_ref().unwrap().reading.config;
    assert!(active.settings.invert_night_images);
    assert_eq!(active.fonts, app.fonts);
    assert!(app.notice.is_none());
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .as_deref(),
        Some("Missing.ttf")
    );
}
