use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

fn fonts() -> Vec<PathBuf> {
    vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]
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

fn tap_button(app: &mut Reader, label: &str) {
    let rect = app.reading_button(label);
    tap(app, rect);
}

fn open_layout(app: &mut Reader) {
    app.open_reading_edge(false, WIDTH as f32 / 2.);
    if app.reading_panel != Some(BarPanel::TextLayout) {
        tap_button(app, "Layout");
    }
    assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
}

fn value(app: &Reader) -> u16 {
    app.active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .effective_line_spacing_percent()
}

fn keyboard(app: &mut Reader, key: primitive_ui::Key) {
    let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
    if !layout
        .keyboard_keys_with(app.editor.layer, "Set")
        .iter()
        .any(|(item, _)| item.key == key)
    {
        let layer = [
            primitive_ui::Layer::Lower,
            primitive_ui::Layer::Symbols,
            primitive_ui::Layer::More,
        ]
        .into_iter()
        .find(|&layer| {
            layout
                .keyboard_keys_with(layer, "Set")
                .iter()
                .any(|(item, _)| item.key == key)
        })
        .unwrap();
        let rect = layout
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(item, _)| item.key == primitive_ui::Key::Layer(layer))
            .unwrap()
            .1;
        tap(app, rect);
    }
    let rect = layout
        .keyboard_keys_with(app.editor.layer, "Set")
        .into_iter()
        .find(|(item, _)| item.key == key)
        .unwrap()
        .1;
    tap(app, rect);
}

fn enter_number(app: &mut Reader, text: &str) {
    while !app.editor.text.is_empty() {
        keyboard(app, primitive_ui::Key::Delete);
    }
    for ch in text.chars() {
        keyboard(app, primitive_ui::Key::Char(ch));
    }
    assert_eq!(app.editor.string(), text);
}

#[test]
fn all_koreader_presets_apply_without_losing_anchor_and_survive_reopen() {
    let fixture = Fixture::new();
    let fonts = fonts();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let origin = position(&app.active.as_ref().unwrap().reading);

    open_layout(&mut app);
    for percent in crate::LINE_SPACING_PRESETS {
        if value(&app) != percent {
            tap_button(&mut app, &format!("{percent}%"));
        }
        assert_eq!(value(&app), percent);
        assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading).anchor,
            origin.anchor,
            "changing line spacing must retain the source passage"
        );
    }
    capture_test_screen(&app, &fonts, "READER_LINE_SPACING_SCREENSHOT", false);
    app.save().unwrap();

    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(value(&reopened), 130);
    assert_eq!(
        reopened
            .library
            .state(reopened.active.as_ref().unwrap().id)
            .unwrap()
            .preferences
            .line_spacing_percent,
        Some(130)
    );
}

#[test]
fn custom_editor_uses_one_and_five_percent_steps_and_clamps_to_reference_range() {
    let fixture = Fixture::new();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts()).unwrap();
    app.open_path(&fixture.book).unwrap();
    open_layout(&mut app);
    tap_button(&mut app, "Set value");
    assert!(matches!(
        app.screen,
        Screen::NumberingValue(NumberField::LineSpacing(_))
    ));
    assert_eq!(app.editor.string(), "100");

    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "101");
    assert_eq!(
        app.input(Input::LongTap(
            (INCREASE.x + INCREASE.w / 2) as f32,
            (INCREASE.y + INCREASE.h / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "106");

    tap(&mut app, DEFAULT);
    assert_eq!(app.editor.string(), "100");
    enter_number(&mut app, "135");
    keyboard(&mut app, primitive_ui::Key::Go);
    assert_eq!(value(&app), 135);
    assert_eq!(app.reading_panel, Some(BarPanel::TextLayout));

    let owner = app.active.as_ref().unwrap().id;
    app.show(Screen::NumberingValue(NumberField::LineSpacing(owner)));
    app.editor.text = "200".chars().collect();
    app.editor.caret = app.editor.text.len();
    tap(&mut app, INCREASE);
    assert_eq!(app.editor.string(), "200");
    app.editor.text = "50".chars().collect();
    app.editor.caret = app.editor.text.len();
    tap(&mut app, DECREASE);
    assert_eq!(app.editor.string(), "50");
    app.editor.text = "49".chars().collect();
    assert!(
        app.apply_numbering_input(NumberField::LineSpacing(owner))
            .is_err()
    );
}

#[test]
fn legacy_multiplier_is_preserved_until_a_percentage_is_explicitly_chosen() {
    let legacy = Settings {
        line_height: 1.8,
        ..Settings::default()
    };
    assert_eq!(legacy.line_spacing_percent, None);
    assert_eq!(legacy.effective_line_height(), 1.8);
    assert_eq!(legacy.effective_line_spacing_percent(), 133);
    let changed = legacy.with_line_spacing_percent(120);
    assert_eq!(changed.line_height, 1.8);
    assert_eq!(changed.effective_line_height(), 1.62);
}
