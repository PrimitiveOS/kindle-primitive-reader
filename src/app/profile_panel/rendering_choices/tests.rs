use super::*;
use crate::app::profile_panel::field_buttons;
use crate::app::selection_viewport_tests::{adopt, capture, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::session::ReadingViewport;
use std::fs;

#[test]
fn profile_rendering_choices_preserve_legacy_preferences_masks_and_the_active_book() {
    let fixture = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let source = fs::read(&fixture.book).unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.save_reading_profile("Harbor").unwrap();
    let mut profiles = app.profiles.clone();
    let Profile::Text { preferences, .. } = &mut profiles[0] else {
        unreachable!();
    };
    preferences.font_kerning = Some(false);
    preferences.font_kerning_mode = None;
    preferences.publisher_styles = None;
    preferences.smooth_images = None;
    let available = [
        Field::Kerning,
        Field::Hinting,
        Field::PublisherStyles,
        Field::PublisherFonts,
        Field::NightImages,
    ];
    profiles[0].set_fields(available.to_vec()).unwrap();
    app.commit_profiles(profiles).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let initial = app.active.as_ref().unwrap().reading.config.settings;
    let position = app.current_position().unwrap();
    let before = app.profiles[0].clone();
    app.show(Screen::ReadingProfile(0));
    let edit = field_buttons(app.navigation_layout()).1;
    let back = app.navigation_layout().back();
    tap(&mut app, edit);
    tap_contents_item(&mut app, "Font rendering");
    tap_contents_item(&mut app, "Kerning");
    assert_eq!(app.items()[0].detail.as_deref(), Some("Selected"));
    tap(&mut app, back);
    assert_eq!(app.profiles[0], before);

    tap_contents_item(&mut app, "Kerning");
    let original = fs::read(&app.profiles_path).unwrap();
    let mut external = before.clone();
    external.set_name("External");
    let external = serde_json::to_vec(&[external]).unwrap();
    fs::write(&app.profiles_path, &external).unwrap();
    tap_contents_item(&mut app, "Good");
    assert_eq!(app.screen, Screen::ReadingProfileChoice(0, Field::Kerning));
    assert!(app.notice.as_deref().unwrap().contains("changed on disk"));
    assert_eq!(app.profiles[0], before);
    assert_eq!(fs::read(&app.profiles_path).unwrap(), external);
    fs::write(&app.profiles_path, original).unwrap();
    tap_contents_item(&mut app, "Good");
    tap_contents_item(&mut app, "Font hinting");
    tap_contents_item(&mut app, "Auto");
    assert_eq!(app.screen, Screen::ReadingProfileFontRendering(0));
    capture(
        &app,
        &mut canvas,
        "landscape-profile-rendering-choices.png",
        false,
    );
    tap(&mut app, back);
    tap_contents_item(&mut app, "Book rendering");
    assert!(app.items().iter().any(|item| {
        item.title == "Image scaling" && item.detail.as_deref() == Some("Default · Excluded")
    }));
    for (field, label) in [
        (Field::PublisherStyles, "Off"),
        (Field::PublisherFonts, "Off"),
        (
            Field::ImageScaling,
            if initial.smooth_images {
                "Fast"
            } else {
                "Best"
            },
        ),
        (Field::NightImages, "Inverted"),
    ] {
        tap_contents_item(&mut app, field.label());
        assert_eq!(app.screen, Screen::ReadingProfileChoice(0, field));
        tap_contents_item(&mut app, label);
    }
    assert_eq!(app.screen, Screen::ReadingProfileBookRendering(0));
    capture(
        &app,
        &mut canvas,
        "landscape-profile-book-rendering.png",
        false,
    );
    let profile = &app.profiles[0];
    assert_eq!(profile.selection(), before.selection());
    let preferences = profile.text_preferences();
    assert_eq!(preferences.font_kerning, Some(true));
    assert_eq!(preferences.font_kerning_mode, Some(FontKerningMode::Good));
    assert_eq!(preferences.font_hinting, FontHinting::Auto);
    assert_eq!(preferences.publisher_styles, Some(false));
    assert!(preferences.override_fonts);
    assert_eq!(preferences.smooth_images, Some(!initial.smooth_images));
    assert!(preferences.invert_night_images);
    assert_eq!(
        Reader::open(&fixture.root, &fixture.state, fonts)
            .unwrap()
            .profiles[0],
        *profile
    );
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings,
        initial
    );
    assert_eq!(app.current_position(), Some(position.clone()));

    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let active = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(active.effective_font_kerning_mode(), FontKerningMode::Good);
    assert_eq!(active.font_hinting, FontHinting::Auto);
    assert!(!active.publisher_styles);
    assert!(active.override_fonts && active.invert_night_images);
    assert_eq!(active.smooth_images, initial.smooth_images);
    assert_eq!(app.reading_viewport(), ReadingViewport::LANDSCAPE);
    assert_eq!(app.current_position().unwrap().anchor, position.anchor);
    assert_eq!(fs::read(&fixture.book).unwrap(), source);
}
