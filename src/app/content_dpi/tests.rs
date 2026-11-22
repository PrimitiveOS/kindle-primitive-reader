use super::*;
use crate::app::tests::Fixture;
use crate::reading_backup::{Envelope, Key as BackupKey};

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
}
fn setup() -> (Fixture, Reader, PathBuf) {
    let fixture = Fixture::new();
    let path = fixture.root.join("Content zoom.xhtml");
    fs::write(&path,"<html><head><title>Content zoom</title></head><body><p id='passage' style='padding:12pt 8mm;text-indent:2em'>The harbor keeps its own time. The boats return at dusk, and the reader keeps the same passage.</p><p style='padding:3% 12px'>Content zoom scales publisher lengths while the reading font and screen remain independent.</p></body></html>").unwrap();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&path).unwrap();
    (fixture, app, path)
}
fn dpi(app: &Reader) -> u16 {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .render_dpi
}
fn tap(app: &mut Reader, rect: Rect) {
    assert!(rect.w >= 100 && rect.h >= 100, "{rect:?}");
    assert_eq!(
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}
fn keyboard(app: &mut Reader, key: primitive_ui::Key) {
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    if !layout
        .keyboard_keys_with(app.editor.layer, "Set")
        .iter()
        .any(|(item, _)| item.key == key)
    {
        let target = [
            primitive_ui::Layer::Lower,
            primitive_ui::Layer::Symbols,
            primitive_ui::Layer::More,
        ]
        .into_iter()
        .find(|layer| {
            layout
                .keyboard_keys_with(*layer, "Set")
                .iter()
                .any(|(item, _)| item.key == key)
        })
        .unwrap();
        let switch = layout
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(item, _)| item.key == primitive_ui::Key::Layer(target))
            .unwrap()
            .1;
        tap(app, switch);
    }
    let rect = Layout::new(WIDTH, HEIGHT, 4, 5)
        .keyboard_keys_with(app.editor.layer, "Set")
        .into_iter()
        .find(|(item, _)| item.key == key)
        .unwrap()
        .1;
    tap(app, rect);
}
fn text(app: &mut Reader, value: &str) {
    while !app.editor.text.is_empty() {
        keyboard(app, primitive_ui::Key::Delete);
    }
    for ch in value.chars() {
        keyboard(app, primitive_ui::Key::Char(ch));
    }
    assert_eq!(app.editor.string(), value);
}
fn open_picker(app: &mut Reader) {
    app.open_reading_edge(false, WIDTH as f32 / 2.);
    if app.reading_panel != Some(BarPanel::TextRendering) {
        let rendering = app.reading_button("Rendering");
        tap(app, rendering);
    }
    let button = app.reading_button("Set value");
    tap(app, button);
    assert!(matches!(
        app.screen,
        Screen::NumberingValue(NumberField::ContentDpi(_))
    ));
    assert_eq!(app.editor.string(), dpi(app).to_string());
}
fn apply(app: &mut Reader, value: u16) {
    open_picker(app);
    text(app, &value.to_string());
    keyboard(app, primitive_ui::Key::Go);
    assert_eq!(dpi(app), value);
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
}
fn selection(app: &Reader) -> TextSelection {
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.fragment("passage").unwrap();
    let mut end = start.clone();
    end.offset += "The harbor".len();
    reading.probe.select_range(&start, &end).unwrap()
}

#[test]
fn custom_dpi_keyboard_applies_real_72_and_123_values_retains_source_and_reopens() {
    let (fixture, mut app, path) = setup();
    let original = fs::read(&path).unwrap();
    let selected = selection(&app);
    let base_font = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .font_size;
    let baseline = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    app.selection = Some(selected.clone());
    apply(&mut app, 72);
    let scaled = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    assert_ne!(scaled, baseline);
    for value in [123, 0, 96, 72] {
        apply(&mut app, value);
        let reading = &app.active.as_ref().unwrap().reading;
        assert_eq!(reading.config.settings.font_size, base_font);
        assert_eq!(
            reading
                .probe
                .select_range(&selected.start, &selected.end)
                .unwrap()
                .text,
            selected.text
        );
        assert!(
            !reading
                .probe
                .selection_rects(reading.probe.page_for(&selected.start).unwrap(), &selected)
                .is_empty()
        );
        if value == 96 {
            assert_eq!(
                app.active
                    .as_mut()
                    .unwrap()
                    .reading
                    .probe
                    .capture(0)
                    .unwrap(),
                baseline
            );
        }
    }
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    reopened.open_path(&path).unwrap();
    assert_eq!(dpi(&reopened), 72);
    assert_eq!(selection(&reopened).text, selected.text);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn custom_dpi_steps_hold_default_cancel_and_lifecycle_keep_the_draft_separate() {
    let (_fixture, mut app, _path) = setup();
    apply(&mut app, 167);
    app.activate_document_defaults(default_settings::Command::SaveReflow)
        .unwrap();
    apply(&mut app, 123);
    open_picker(&mut app);
    tap(&mut app, DECREASE);
    assert_eq!(app.editor.string(), "122");
    assert_eq!(
        app.input(Input::LongTap(
            (INCREASE.x + 20) as f32,
            (INCREASE.y + 20) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "142");
    text(&mut app, "300");
    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "300");
    text(&mut app, "0");
    tap(&mut app, DECREASE);
    assert_eq!(app.editor.string(), "0");
    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "167");
    assert_eq!(dpi(&app), 123);
    app.input(Input::Sleep);
    app.input(Input::Wake);
    assert!(matches!(
        app.screen,
        Screen::NumberingValue(NumberField::ContentDpi(_))
    ));
    assert_eq!(app.editor.string(), "167");
    assert_eq!(dpi(&app), 123);
    tap(&mut app, primitive_ui::back_button(WIDTH as i32));
    assert_eq!(dpi(&app), 123);
    assert_eq!(app.reading_panel, Some(BarPanel::TextRendering));
    open_picker(&mut app);
    tap(&mut app, DEFAULT);
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 167);
    apply(&mut app, 0);
    app.activate_document_defaults(default_settings::Command::SaveReflow)
        .unwrap();
    apply(&mut app, 123);
    open_picker(&mut app);
    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "0");
    assert_eq!(dpi(&app), 123);
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 0);
}

#[test]
fn invalid_or_stale_custom_dpi_cannot_change_model_or_reading() {
    let (fixture, mut app, _path) = setup();
    app.save().unwrap();
    open_picker(&mut app);
    let before = app.library.bytes().unwrap();
    for bad in ["301", "-1", "72.5"] {
        open_picker(&mut app);
        text(&mut app, bad);
        keyboard(&mut app, primitive_ui::Key::Go);
        assert_eq!(dpi(&app), 96);
        assert_eq!(app.editor.string(), bad);
        assert_eq!(app.library.bytes().unwrap(), before);
        assert!(app.notice.as_deref().unwrap().contains("0 to 300"));
    }
    let stale = app.screen;
    let other = fixture.root.join("Another book.txt");
    fs::write(&other, "A different edition keeps its own content scale.").unwrap();
    app.open_path(&other).unwrap();
    app.save().unwrap();
    let other_before = app.library.bytes().unwrap();
    app.show(stale);
    text(&mut app, "72");
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 96);
    assert_eq!(app.library.bytes().unwrap(), other_before);
    assert!(app.notice.as_deref().unwrap().contains("book changed"));
}

#[test]
fn custom_dpi_failed_layout_and_atomic_save_keep_frame_anchor_preferences_and_draft() {
    let (fixture, mut app, path) = setup();
    let temporary_font = fixture.root.join("Private test font.ttf");
    fs::copy(
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        &temporary_font,
    )
    .unwrap();
    app.active.as_mut().unwrap().reading.config.fonts = vec![temporary_font.clone()];
    app.save().unwrap();
    open_picker(&mut app);
    text(&mut app, "123");
    let before = app.library.bytes().unwrap();
    let anchor = app.active.as_ref().unwrap().reading.anchor();
    let frame = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    let original = fs::read(&path).unwrap();
    let unavailable = fixture.root.join("temporarily-unavailable.ttf");
    fs::rename(&temporary_font, &unavailable).unwrap();
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 96);
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(app.editor.string(), "123");
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        frame
    );
    fs::rename(&unavailable, &temporary_font).unwrap();
    let rollback = fixture.root.join("saved-library-before-dpi.json");
    fs::rename(&fixture.state, &rollback).unwrap();
    fs::create_dir(&fixture.state).unwrap();
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 96);
    assert_eq!(app.library.bytes().unwrap(), before);
    assert_eq!(app.editor.string(), "123");
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        frame
    );
    fs::remove_dir(&fixture.state).unwrap();
    fs::rename(&rollback, &fixture.state).unwrap();
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 123);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn a_valid_same_length_replacement_cannot_commit_into_the_old_edition() {
    let (_fixture, mut app, path) = setup();
    open_picker(&mut app);
    text(&mut app, "123");
    app.save().unwrap();
    let model = app.library.bytes().unwrap();
    let anchor = app.active.as_ref().unwrap().reading.anchor();
    let frame = app
        .active
        .as_mut()
        .unwrap()
        .reading
        .probe
        .capture(0)
        .unwrap();
    let original = fs::read(&path).unwrap();
    let replaced = String::from_utf8(original.clone())
        .unwrap()
        .replace("harbor", "market");
    assert_eq!(replaced.len(), original.len());
    assert_ne!(replaced.as_bytes(), original);
    fs::write(&path, replaced).unwrap();
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 96);
    assert_eq!(app.editor.string(), "123");
    assert!(app.notice.as_deref().unwrap().contains("edition changed"));
    assert_eq!(app.library.bytes().unwrap(), model);
    assert_eq!(app.active.as_ref().unwrap().reading.anchor(), anchor);
    assert_eq!(
        app.active
            .as_mut()
            .unwrap()
            .reading
            .probe
            .capture(0)
            .unwrap(),
        frame
    );
    fs::write(&path, &original).unwrap();
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(dpi(&app), 123);
}

#[test]
fn custom_dpi_profiles_defaults_and_real_v2_restore_preserve_values_and_reject_301() {
    let (_fixture, mut app, path) = setup();
    apply(&mut app, 72);
    app.save_reading_profile("Custom content scale").unwrap();
    apply(&mut app, 123);
    app.activate_document_defaults(default_settings::Command::SaveReflow)
        .unwrap();
    let backup = app.reading_backup_bytes().unwrap();
    let target = Fixture::new();
    let copy = target.root.join(path.file_name().unwrap());
    fs::copy(&path, &copy).unwrap();
    let mut restored = Reader::open(&target.root, &target.state, fonts()).unwrap();
    let mut envelope = Envelope::parse(&backup).unwrap();
    let profiles = envelope
        .components
        .iter_mut()
        .find(|c| c.key == BackupKey::Profiles)
        .unwrap();
    profiles.data[0]["preferences"]["render_dpi"] = serde_json::json!(301);
    let before = restored.library.bytes().unwrap();
    assert!(
        restored
            .prepare_reading_backup(&envelope.bytes().unwrap())
            .is_err()
    );
    assert_eq!(restored.library.bytes().unwrap(), before);
    let mut envelope = Envelope::parse(&backup).unwrap();
    let defaults = envelope
        .components
        .iter_mut()
        .find(|c| c.key == BackupKey::DocumentDefaults)
        .unwrap();
    defaults.data["reflow"]["render_dpi"] = serde_json::json!(301);
    assert!(
        restored
            .prepare_reading_backup(&envelope.bytes().unwrap())
            .is_err()
    );
    assert_eq!(restored.library.bytes().unwrap(), before);
    fs::write(&restored.import_path, &backup).unwrap();
    restored.activate(Action::PreviewBackup).unwrap();
    assert_eq!(restored.screen, Screen::RestoreBackup);
    tap(
        &mut restored,
        Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, ROW_HEIGHT),
    );
    assert_eq!(restored.screen, Screen::ReadingData);
    assert_eq!(
        restored
            .document_defaults
            .templates
            .reflow
            .as_ref()
            .unwrap()
            .render_dpi,
        Some(123)
    );
    assert_eq!(restored.profiles[0].preferences.render_dpi, Some(72));
    restored.open_path(&copy).unwrap();
    assert_eq!(dpi(&restored), 123);
    restored.activate(Action::ApplyProfile(0)).unwrap();
    assert_eq!(dpi(&restored), 72);
    let unseen = target.root.join("New book.txt");
    fs::write(
        &unseen,
        "A new edition receives its frozen global content DPI.",
    )
    .unwrap();
    restored.open_path(&unseen).unwrap();
    assert_eq!(dpi(&restored), 123);
    restored.save().unwrap();
    let mut reopened = Reader::open(&target.root, &target.state, fonts()).unwrap();
    reopened.open_path(&copy).unwrap();
    assert_eq!(dpi(&reopened), 72);
    reopened.open_path(&unseen).unwrap();
    assert_eq!(dpi(&reopened), 123);
}
