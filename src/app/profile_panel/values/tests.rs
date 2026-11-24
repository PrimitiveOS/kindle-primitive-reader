use super::*;
use crate::app::reading_viewport_stage::draw_test_candidate;
use crate::app::selection_viewport_tests::{key, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use crate::reading_profiles::Field;
use primitive_app::viewport::{LogicalTurn, ViewportDecision, ViewportRequest};
use primitive_ui::Key;

fn open(fixture: &Fixture, path: &Path) -> Reader {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    app.open_path(path).unwrap();
    app
}

fn show_editor(app: &mut Reader, field: ValueField) {
    app.show(Screen::ReadingProfile(0));
    let button = field_buttons(app.navigation_layout()).1;
    tap(app, button);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    tap_contents_item(app, field.label());
    assert_eq!(
        app.screen,
        Screen::NumberingValue(NumberField::ProfileValue(0, field))
    );
}

fn press_key(app: &mut Reader, name: Key) {
    let area = key(app, name, "Set");
    tap(app, area);
}

fn type_value(app: &mut Reader, value: &str) {
    while !app.editor.text.is_empty() {
        press_key(app, Key::Delete);
    }
    for character in value.chars() {
        press_key(app, Key::Char(character));
    }
}

#[test]
fn reflow_profile_edits_storage_and_keeps_selection_and_book_until_explicit_apply() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        line_height: 1.62,
        line_spacing_percent: None,
        ..settings
    }))
    .unwrap();
    app.save_reading_profile("Harbor").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::FontSize).unwrap();
    app.save_profile_fields(0).unwrap();
    let field = ValueField::LineSpacing;
    let before = app.profiles[0].clone();
    assert_eq!(before.value(field), Some(120));
    let bytes = fs::read(&app.profiles_path).unwrap();
    let location = app.current_position();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    show_editor(&mut app, field);
    assert_eq!(app.editor.string(), "120");
    type_value(&mut app, "49");
    press_key(&mut app, Key::Go);
    assert_eq!(
        app.screen,
        Screen::NumberingValue(NumberField::ProfileValue(0, field))
    );
    assert!(app.notice.as_deref().unwrap().contains("50% to 200%"));
    type_value(&mut app, "137");
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    assert_eq!(fs::read(&app.profiles_path).unwrap(), bytes);

    tap_contents_item(&mut app, field.label());
    type_value(&mut app, "137");
    let mut external = before.clone();
    external.set_name("An external edit");
    let replacement = serde_json::to_vec(&vec![external]).unwrap();
    fs::write(&app.profiles_path, &replacement).unwrap();
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("changed on disk"));
    assert_eq!(app.editor.string(), "137");
    assert_eq!(app.profiles[0], before);
    assert_eq!(fs::read(&app.profiles_path).unwrap(), replacement);
    fs::write(&app.profiles_path, bytes).unwrap();

    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    assert_eq!(app.profiles[0].value(field), Some(137));
    assert_eq!(app.profiles[0].selection(), [Field::FontSize]);
    assert_eq!(app.items()[0].detail.as_deref(), Some("137% · Excluded"));
    let mut expected = before;
    let Profile::Text { preferences, .. } = &mut expected else {
        panic!("Expected a reflow profile");
    };
    preferences.line_spacing_percent = Some(137);
    assert_eq!(app.profiles[0], expected);
    assert_eq!(app.current_position(), location);
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings,
        settings
    );

    app.apply_reading_profile(0).unwrap();
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .effective_line_spacing_percent(),
        settings.effective_line_spacing_percent()
    );
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::LineSpacing).unwrap();
    app.save_profile_fields(0).unwrap();
    app.apply_reading_profile(0).unwrap();
    assert_eq!(
        app.current_position().unwrap().anchor,
        location.unwrap().anchor
    );
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .effective_line_spacing_percent(),
        137
    );
    let reopened = open(&fixture, &fixture.book);
    assert_eq!(reopened.profiles[0].value(field), Some(137));
}

#[test]
fn fixed_profile_edits_each_overlap_in_landscape_and_keeps_older_values_above_84() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor scans.pdf");
    fs::write(&path, striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    let mut app = open(&fixture, &path);
    app.fixed.as_mut().unwrap().reading.jump(1).unwrap();
    app.set_fixed_auto_turn_overlap(true, 89).unwrap();
    app.set_fixed_auto_turn_overlap(false, 87).unwrap();
    app.activate(Action::ScanGamma(20)).unwrap();
    app.save_reading_profile("Scans").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::PanOverlap).unwrap();
    app.save_profile_fields(0).unwrap();
    let upright = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let turn = LogicalTurn::Counterclockwise;
    let landscape = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(landscape, ViewportDecision::Commit);
    let location = app.current_history_location();
    let settings = app.fixed.as_ref().unwrap().reading.preferences();
    let fields = app.profiles[0].selection();
    let old = app.profiles[0].clone();
    let horizontal = ValueField::HorizontalOverlap;
    let vertical = ValueField::VerticalOverlap;
    show_editor(&mut app, horizontal);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(app.editor.string(), "89");
    let [decrease, default, increase] = app.numeric_layout().steps(STEPS, false);
    tap(&mut app, default);
    assert_eq!(app.editor.string(), "36");
    assert_eq!(
        app.input(Input::LongTap(
            (increase.x + increase.w / 2) as f32,
            (increase.y + increase.h / 2) as f32,
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "41");
    tap(&mut app, decrease);
    assert_eq!(app.editor.string(), "40");
    type_value(&mut app, "45");
    let refused = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(refused, ViewportDecision::Abort);
    assert_eq!(app.editor.string(), "45");
    assert_eq!(app.profiles[0], old);
    let shown = draw_test_candidate(&mut app, 4, turn, "landscape-profile-overlap", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    assert_eq!(app.profiles[0].value(horizontal), Some(45));
    assert_eq!(app.profiles[0].value(vertical), Some(87));

    tap_contents_item(&mut app, vertical.label());
    assert_eq!(app.editor.string(), "87");
    type_value(&mut app, "91");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0% to 90%"));
    type_value(&mut app, "88");
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.profiles[0].value(vertical), Some(87));
    tap_contents_item(&mut app, vertical.label());
    type_value(&mut app, "88");
    press_key(&mut app, Key::Go);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(app.profiles[0].value(vertical), Some(88));
    assert_eq!(app.profiles[0].selection(), fields);
    assert!(
        app.items()
            .iter()
            .all(|item| item.detail.as_deref().unwrap().ends_with("Excluded"))
    );
    assert_eq!(app.current_history_location(), location);
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), settings);

    app.apply_reading_profile(0).unwrap();
    let excluded = app.fixed.as_ref().unwrap().reading.preferences();
    assert_eq!(
        (excluded.zoom_overlap_h(), excluded.zoom_overlap_v()),
        (89, 87)
    );
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::PanOverlap).unwrap();
    app.save_profile_fields(0).unwrap();
    app.apply_reading_profile(0).unwrap();
    assert_eq!(app.current_history_location(), location);
    let view = app.fixed.as_ref().unwrap().reading.preferences();
    assert_eq!((view.zoom_overlap_h(), view.zoom_overlap_v()), (45, 88));
    assert_eq!(view.scan_gamma_tenths, Some(20));
    let persisted = open(&fixture, &path);
    assert_eq!(persisted.profiles[0], app.profiles[0]);
}
