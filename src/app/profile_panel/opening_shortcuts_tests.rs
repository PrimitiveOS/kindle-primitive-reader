use super::*;
use crate::app::selection_viewport_tests::{adopt, capture, key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, Settings};
use crate::reading_profiles::Field;
use crate::session::ReadingViewport;
use primitive_app::App;
use primitive_ui::Key;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn opening_shortcuts_preview_and_save_the_raw_rule_and_preserve_the_position() {
    let f = Fixture::new();
    let incoming = f.root.join("Incoming");
    fs::create_dir(&incoming).unwrap();
    let book = incoming.join("Book.txt");
    fs::write(&book, "Follow the early coast across the channels.").unwrap();
    let leave = f.root.join("Book.txt");
    fs::write(&leave, "Research in the valley instead of at the inlet.").unwrap();
    let mut app = Reader::open(
        &f.root,
        &f.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.browse_to(PathBuf::from("Incoming")).unwrap();
    app.open_path(&f.book).unwrap();
    let normal = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        invert_night_images: true,
        ..normal
    }))
    .unwrap();
    app.save_reading_profile("Opening shortcuts").unwrap();
    let mut profiles = app.profiles.clone();
    profiles[0].set_fields(vec![Field::NightImages]).unwrap();
    app.commit_profiles(profiles).unwrap();
    app.activate(Action::Text(normal)).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply on book opening");
    tap_contents_item(&mut app, "File path contains");
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let position = app.current_position();
    let (help, preview) = app.opening_path_buttons().unwrap();
    tap(&mut app, help);
    assert!(matches!(app.opening_path_overlay, Some(Overlay::Shortcuts)));
    capture(
        &app,
        &mut canvas,
        "landscape-profile-folder-shortcuts.png",
        false,
    );
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::ProfileName(Name::OpeningPath(0)));
    let draft = "%D%/Book, %W%/Ridge";
    app.editor.text = draft.chars().collect();
    app.editor.caret = app.editor.text.len();
    tap(&mut app, preview);
    let expected = format!(
        "{}/Book, {}/Wikipedia/Ridge",
        incoming.display(),
        f.root.display()
    );
    assert!(
        matches!(app.opening_path_overlay.as_ref(), Some(Overlay::Preview(text)) if text == &expected)
    );
    let (_, done) = app.opening_path_overlay_rects();
    tap(&mut app, done);
    assert_eq!(app.editor.string(), draft);
    assert!(app.profiles[0].opening_conditions().is_none());
    let save = key(&app, Key::Go, "Save");
    tap(&mut app, save);
    assert_eq!(app.current_position(), position);
    assert_eq!(
        app.profiles[0]
            .opening_conditions()
            .unwrap()
            .path
            .as_deref(),
        Some(draft)
    );
    let restored =
        crate::reading_profiles::decode(serde_json::to_value(&app.profiles).unwrap()).unwrap();
    assert_eq!(restored, app.profiles);
    app.open_path(&leave).unwrap();
    assert!(
        !app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
    app.open_path(&book).unwrap();
    assert!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .invert_night_images
    );
}

#[test]
fn opening_shortcuts_match_both_physical_and_library_relative_paths() {
    use crate::reading_controls::ReadingOrientation;
    use crate::reading_profiles::opening::{Book, Conditions};
    let f = Fixture::new();
    let app = f.app();
    let mut conditions = Conditions::default();
    let last_folder = Path::new("Charts, atlas");
    let canonical = f.root.canonicalize().unwrap().join("SURVEY/ĐẶNG.txt");
    for (pattern, path, expected) in [
        ("%H%/survey/đặng", canonical.as_path(), true),
        ("%H%/survey/đặng", Path::new("SURVEY/ĐẶNG.txt"), true),
        ("%D%/Book", Path::new("Elsewhere/atlas/Book.txt"), true),
        ("%D%/Book", Path::new("Charts/Unrelated.txt"), true),
        (
            "%H%/Survey, %W%/Archive",
            Path::new("Wikipedia/Archive.pdf"),
            true,
        ),
        (
            "%H%/Survey, %W%/Archive",
            Path::new("Elsewhere/Archive.pdf"),
            false,
        ),
        ("\"%H%/Survey\"", Path::new("Survey/file.epub"), false),
        ("%Q%", Path::new("%q%.pdf"), true),
        ("%H%%D%", Path::new("Unrelated.txt"), false),
    ] {
        conditions.path = Some(pattern.to_owned());
        let mut profile = crate::reading_profiles::Profile::Text {
            name: "Reminder".into(),
            preferences: Default::default(),
            fields: None,
            apply_on_open: false,
            opening: None,
            opening_time: None,
            opening_execution: None,
            apply_on_close: None,
        };
        profile.set_opening_conditions(conditions.clone());
        assert_eq!(
            profile.matches_opening(&Book {
                id: 1,
                path,
                library: &app.library,
                orientation: ReadingOrientation::Upright,
                is_new: true,
                metadata: None,
                local_minute: None,
                download_folder: last_folder,
            }),
            expected,
            "{pattern} on {}",
            path.display()
        );
    }
}
