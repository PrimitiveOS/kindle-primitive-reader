use super::*;
use crate::app::tests::{Fixture, capture_test_screen};
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use primitive_app::{App, frame};

fn screen_tap(app: &mut Reader, area: Rect) -> Change {
    app.input(Input::Tap(
        (area.x + area.w / 2) as f32,
        (area.y + area.h / 2) as f32,
    ))
}

fn row(app: &mut Reader, title: &str) -> Change {
    let index = app
        .items()
        .iter()
        .position(|item| item.title == title)
        .unwrap();
    app.input(Input::Tap(
        116.0,
        (LIST_Y + ROW_HEIGHT * index as i32 + 46) as f32,
    ))
}

fn name_from_keyboard(app: &mut Reader, name: &str) {
    app.editor.text = name.chars().collect();
    app.editor.caret = app.editor.text.len();
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    let activation = (945..HEIGHT)
        .step_by(8)
        .find_map(|y| {
            (850..WIDTH).step_by(8).find_map(|x| {
                let key = keyboard_key(&layout, &app.editor, "Save", x as f32, y as f32)?;
                let mut editor = Editor::new(48);
                matches!(editor.press(key), Press::Go).then_some((x as f32, y as f32))
            })
        })
        .unwrap();
    assert_eq!(
        app.input(Input::Tap(activation.0, activation.1)),
        Change::Partial
    );
}

fn specimen(fixture: &Fixture, name: &str, width: u32, height: u32) -> PathBuf {
    let path = fixture.root.join(name);
    fs::write(&path, striped_pdf_sized_bytes(3, width, height)).unwrap();
    path
}

fn confirm(app: &mut Reader, action: Confirm, choice: SheetAction) -> Change {
    screen_tap(app, profile_confirmation_sheet("", action).button(choice))
}

#[test]
fn reading_profiles_save_pdf_and_text_share_management_and_restore_compatible_book_settings() {
    let f = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let pdf = specimen(&f, "Scanned journal.pdf", 612, 820);
    let other = specimen(&f, "Wide scan.pdf", 820, 612);
    let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    app.open_path(&f.book).unwrap();
    app.show(Screen::ReadingProfiles);
    assert_eq!(row(&mut app, "Save current settings"), Change::Partial);
    name_from_keyboard(&mut app, "Text at dusk");
    assert_eq!(app.screen, Screen::ReadingProfiles);
    app.open_path(&pdf).unwrap();
    assert!(
        app.top_config_items(TopGroup::Appearance)
            .iter()
            .any(|item| { matches!(item.action, Action::Show(Screen::ReadingProfiles)) })
    );
    app.activate(Action::ScanGamma(20)).unwrap();
    app.activate(Action::SetFixedRotation(1)).unwrap();
    app.activate(Action::SetFixedTileZoom(FixedTileAxis::Rows, 24))
        .unwrap();
    let source_view = app.fixed.as_ref().unwrap().reading.preferences();
    app.show(Screen::ReadingProfiles);
    assert_eq!(row(&mut app, "Save current settings"), Change::Partial);
    name_from_keyboard(&mut app, "Large scans");
    assert_eq!(app.screen, Screen::ReadingProfiles);
    capture_test_screen(&app, &fonts, "READER_PROFILES_LIST", false);
    assert_eq!(row(&mut app, "Text at dusk"), Change::Partial);
    assert_eq!(row(&mut app, "Apply to this book"), Change::Partial);
    assert_eq!(app.screen, Screen::ReadingProfile(0));
    assert!(
        app.items().iter().any(
            |item| item.title == "Apply to this book" && matches!(item.action, Action::Heading)
        )
    );
    assert!(app.activate(Action::ApplyProfile(0)).is_err());
    assert!(app.activate(Action::UpdateReadingProfile(0)).is_err());
    assert_eq!(
        screen_tap(&mut app, primitive_ui::back_button(WIDTH as i32)),
        Change::Partial
    );
    assert_eq!(row(&mut app, "Large scans"), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_PROFILES_DETAILS", true);
    assert_eq!(row(&mut app, "Rename"), Change::Partial);
    assert_eq!(app.editor.string(), "Large scans");
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(row(&mut app, "Rename"), Change::Partial);
    name_from_keyboard(&mut app, "Scanned journals");
    assert_eq!(app.profiles[1].name(), "Scanned journals");
    assert_eq!(row(&mut app, "Duplicate"), Change::Partial);
    name_from_keyboard(&mut app, "Evening scans");
    assert_eq!(app.profiles.len(), 3);
    assert_eq!(row(&mut app, "Evening scans"), Change::Partial);
    assert_eq!(row(&mut app, "Delete"), Change::Partial);
    capture_test_screen(&app, &fonts, "READER_PROFILES_CONFIRM", false);
    assert_eq!(
        confirm(&mut app, Confirm::Delete, SheetAction::Cancel),
        Change::Partial
    );
    assert_eq!(app.profiles.len(), 3);
    assert_eq!(row(&mut app, "Delete"), Change::Partial);
    assert_eq!(
        confirm(&mut app, Confirm::Delete, SheetAction::Confirm),
        Change::Partial
    );
    assert_eq!(app.profiles.len(), 2);
    assert_eq!(row(&mut app, "Scanned journals"), Change::Partial);
    assert_eq!(
        row(&mut app, "Replace with current settings"),
        Change::Partial
    );
    assert_eq!(
        confirm(&mut app, Confirm::Replace, SheetAction::Cancel),
        Change::Partial
    );

    let old_defaults = app.document_defaults.templates.clone();
    let old_controls = app.controls;
    app.open_path(&other).unwrap();
    app.fixed.as_mut().unwrap().reading.jump(1).unwrap();
    let target = app.fixed.as_mut().unwrap();
    let crop = crate::fixed_text::PdfTextRect {
        x0: 0.12,
        y0: 0.10,
        x1: 0.89,
        y1: 0.90,
    };
    target
        .reading
        .restore_view(FixedPreferences {
            crop: Some(crop),
            force_ocr: true,
            ..target.reading.preferences()
        })
        .unwrap();
    app.activate(Action::ToggleBookmark).unwrap();
    let target = app.fixed.as_ref().unwrap();
    let id = target.id;
    let position = fixed_position(&target.reading);
    let marks = app.library.state(id).unwrap().bookmarks.clone();
    app.show(Screen::ReadingProfile(1));
    assert_eq!(row(&mut app, "Apply to this book"), Change::Partial);
    assert_eq!(app.screen, Screen::Fixed);
    let target = app.fixed.as_ref().unwrap();
    let applied = target.reading.preferences();
    assert_eq!(fixed_position(&target.reading), position);
    assert_eq!(target.reading.preferences().crop, Some(crop));
    assert!(applied.force_ocr);
    assert_eq!(applied.turns, 1);
    assert_eq!(applied.scan_gamma_tenths, Some(20));
    let tile = applied.tile_zoom.unwrap();
    assert_eq!(tile.axis, FixedTileAxis::Rows);
    assert_eq!(tile.count_tenths, 24);
    assert_ne!(
        tile.width_factor,
        source_view.tile_zoom.unwrap().width_factor
    );
    assert_eq!(app.library.state(id).unwrap().bookmarks, marks);
    assert_eq!(app.document_defaults.templates, old_defaults);
    assert_eq!(
        serde_json::to_value(app.controls).unwrap(),
        serde_json::to_value(old_controls).unwrap()
    );

    app.activate(Action::FitPage(false)).unwrap();
    app.show(Screen::ReadingProfile(1));
    assert_eq!(
        row(&mut app, "Replace with current settings"),
        Change::Partial
    );
    assert_eq!(
        confirm(&mut app, Confirm::Replace, SheetAction::Confirm),
        Change::Partial
    );
    let bytes = app.reading_backup_bytes().unwrap();
    let mut reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    assert_eq!(reopened.profiles, app.profiles);
    assert!(reopened.prepare_reading_backup(&bytes).is_ok());
    reopened.open_path(&other).unwrap();
    assert_eq!(
        fixed_position(&reopened.fixed.as_ref().unwrap().reading),
        position
    );
    assert!(
        reopened
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .tile_zoom
            .is_none()
    );
    assert!(reopened.activate(Action::ApplyProfile(0)).is_err());
}

#[test]
fn reading_profiles_keep_saved_files_frames_and_positions_when_storage_changes() {
    let f = Fixture::new();
    let pdf = specimen(&f, "Scanned journal.pdf", 612, 820);
    let mut app = f.app();
    app.open_path(&pdf).unwrap();
    app.activate(Action::ScanGamma(20)).unwrap();
    app.save_reading_profile("Scans").unwrap();
    let saved = fs::read(&app.profiles_path).unwrap();
    fs::write(&app.profiles_path, b"{broken").unwrap();
    assert!(
        app.edit_reading_profile_name(Name::Rename(0), "New name")
            .is_err()
    );
    assert_eq!(app.profiles[0].name(), "Scans");
    assert_eq!(fs::read(&app.profiles_path).unwrap(), b"{broken");
    fs::write(&app.profiles_path, saved).unwrap();
    app.activate(Action::ScanGamma(10)).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let before = fixed.reading.preferences();
    let position = fixed_position(&fixed.reading);
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let pixels = frame(&app, &mut canvas, false);
    let source = fs::read(&pdf).unwrap();
    fs::write(&pdf, striped_pdf_sized_bytes(3, 408, 622)).unwrap();
    assert!(app.activate(Action::ApplyProfile(0)).is_err());
    fs::write(&pdf, source).unwrap();
    app.save().unwrap();
    let original = f.root.join("saved-library.json");
    fs::rename(&f.state, &original).unwrap();
    fs::create_dir(&f.state).unwrap();
    assert!(app.activate(Action::ApplyProfile(0)).is_err());
    let fixed = app.fixed.as_ref().unwrap();
    assert_eq!(fixed.reading.preferences(), before);
    assert_eq!(fixed_position(&fixed.reading), position);
    assert_eq!(frame(&app, &mut canvas, false), pixels);
    fs::remove_dir(&f.state).unwrap();
    fs::rename(original, &f.state).unwrap();
    app.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .scan_gamma_tenths,
        Some(20)
    );
}

#[test]
fn reading_profile_fields_are_drafts_and_apply_only_selected_text_settings() {
    let f = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    app.open_path(&f.book).unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 65.,
        logical_font_size: None,
        margin: 44,
        ..settings
    }))
    .unwrap();
    app.save_reading_profile("Large text").unwrap();
    let legacy = fs::read(&app.profiles_path).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("\"fields\""));
    app.show(Screen::ReadingProfile(0));
    assert_eq!(row(&mut app, "Included settings"), Change::Partial);
    assert_eq!(row(&mut app, "Clear all"), Change::Partial);
    let (save, cancel) = field_buttons();
    screen_tap(&mut app, save);
    assert_eq!(app.screen, Screen::ReadingProfileFields(0));
    assert_eq!(fs::read(&app.profiles_path).unwrap(), legacy);
    assert_eq!(row(&mut app, "Font size"), Change::Partial);
    assert!(app.notice.is_none());
    capture_test_screen(&app, &fonts, "READER_PROFILE_FIELDS_TEXT", false);
    assert_eq!(screen_tap(&mut app, cancel), Change::Partial);
    assert_eq!(fs::read(&app.profiles_path).unwrap(), legacy);
    assert_eq!(row(&mut app, "Included settings"), Change::Partial);
    assert_eq!(row(&mut app, "Clear all"), Change::Partial);
    assert_eq!(row(&mut app, "Font size"), Change::Partial);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(screen_tap(&mut app, save), Change::Partial);
    assert_eq!(app.profiles[0].selection(), [Field::FontSize]);
    assert!(app.profile_fields.is_empty());

    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 49.,
        logical_font_size: None,
        margin: 93,
        ..settings
    }))
    .unwrap();
    app.activate(Action::ReadingMode(true)).unwrap();
    let active = app.active.as_ref().unwrap();
    let anchor = active.reading.anchor();
    let id = active.id;
    let mut before = app.library.state(id).unwrap().preferences.clone();
    app.activate(Action::ApplyProfile(0)).unwrap();
    before.font_size = Some(65.);
    assert_eq!(app.library.state(id).unwrap().preferences, before);
    let active = app.active.as_ref().unwrap();
    assert_eq!(active.reading.anchor(), anchor);
    assert!(active.reading.config.scroll);

    let settings = active.reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 48.,
        ..settings
    }))
    .unwrap();
    app.activate(Action::UpdateReadingProfile(0)).unwrap();
    let saved = app.profiles[0].text_preferences();
    assert_eq!(saved.font_size, Some(48.));
    assert_eq!(saved.margin, Some(44));
    let bytes = app.reading_backup_bytes().unwrap();
    assert!(app.prepare_reading_backup(&bytes).is_ok());
    let reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    assert_eq!(reopened.profiles[0].selection(), [Field::FontSize]);
}

#[test]
fn reading_profile_fields_apply_provider_specific_comic_and_djvu_settings() {
    let f = Fixture::new();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let djvu = f.root.join("Beacon atlas.djvu");
    let comic = f.root.join("Signals.cbz");
    fs::copy(crate::test_fixture("beacon-atlas.djvu"), &djvu).unwrap();
    fs::copy(crate::test_fixture("harbor-signals.cbz"), &comic).unwrap();
    let mut app = Reader::open(&f.root, &f.state, fonts.clone()).unwrap();
    app.open_path(&djvu).unwrap();
    app.activate(Action::ScanContrast(true)).unwrap();
    app.save_reading_profile("Atlas contrast").unwrap();
    let original = serde_json::to_value(&app.profiles).unwrap();
    let mut invalid = original.clone();
    invalid[0]["fixed"]["settings"]["scan_gamma_tenths"] = 20.into();
    assert!(reading_profiles::decode(invalid).is_err());
    let mut invalid = original;
    invalid[0]["fields"] = serde_json::json!(["scan_contrast", "crop_margin"]);
    assert!(reading_profiles::decode(invalid).is_err());
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::ScanContrast).unwrap();
    assert!(app.toggle_profile_field(Field::CropMargin).is_err());
    capture_test_screen(&app, &fonts, "READER_PROFILE_FIELDS_DJVU", true);
    assert_eq!(screen_tap(&mut app, field_buttons().0), Change::Partial);
    app.activate(Action::ScanContrast(false)).unwrap();
    app.activate(Action::SetFixedTileZoom(FixedTileAxis::Columns, 21))
        .unwrap();
    let current = app.fixed.as_ref().unwrap();
    let page = current.reading.page();
    let mut before = current.reading.preferences();
    app.activate(Action::ApplyProfile(0)).unwrap();
    before.scan_contrast = true;
    let current = app.fixed.as_ref().unwrap();
    assert_eq!(current.reading.page(), page);
    assert_eq!(current.reading.preferences(), before);

    app.open_path(&comic).unwrap();
    assert!(app.activate(Action::ApplyProfile(0)).is_err());
    app.activate(Action::ReversePageDirection).unwrap();
    app.save_reading_profile("Comic direction").unwrap();
    app.edit_profile_fields(1).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::ReadingDirection).unwrap();
    assert!(app.toggle_profile_field(Field::Straighten).is_err());
    assert_eq!(screen_tap(&mut app, field_buttons().0), Change::Partial);
    app.activate(Action::ReversePageDirection).unwrap();
    app.activate(Action::SetFixedRotation(1)).unwrap();
    let current = app.fixed.as_ref().unwrap();
    let page = current.reading.page();
    let mut before = current.reading.preferences();
    app.activate(Action::ApplyProfile(1)).unwrap();
    before.right_to_left = true;
    let current = app.fixed.as_ref().unwrap();
    assert_eq!(current.reading.page(), page);
    assert_eq!(current.reading.preferences(), before);
    let reopened = Reader::open(&f.root, &f.state, fonts).unwrap();
    assert_eq!(reopened.profiles, app.profiles);
}
