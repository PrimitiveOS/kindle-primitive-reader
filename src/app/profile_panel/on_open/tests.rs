use super::*;
use crate::app::profile_panel::Name;
use crate::app::selection_viewport_tests::{adopt, capture};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, ReadingOrientation, Screen, Settings};
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use crate::library::FixedTileAxis;
use crate::reading_profiles::{Field, ValueField};
use crate::session::ReadingViewport;
use primitive_app::App;
use std::fs;

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
