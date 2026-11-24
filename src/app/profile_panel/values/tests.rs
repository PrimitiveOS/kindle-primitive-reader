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
    assert_eq!(before.value(field), Some(120.0));
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
    assert_eq!(app.profiles[0].value(field), Some(137.0));
    assert_eq!(app.profiles[0].selection(), [Field::FontSize]);
    assert_eq!(
        app.items()
            .iter()
            .find(|item| item.title == field.label())
            .unwrap()
            .detail
            .as_deref(),
        Some("137% · Excluded")
    );
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
    assert_eq!(reopened.profiles[0].value(field), Some(137.0));
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
    assert_eq!(app.profiles[0].value(horizontal), Some(45.0));
    assert_eq!(app.profiles[0].value(vertical), Some(87.0));

    tap_contents_item(&mut app, vertical.label());
    assert_eq!(app.editor.string(), "87");
    type_value(&mut app, "91");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0% to 90%"));
    type_value(&mut app, "88");
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.profiles[0].value(vertical), Some(87.0));
    tap_contents_item(&mut app, vertical.label());
    type_value(&mut app, "88");
    press_key(&mut app, Key::Go);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(app.profiles[0].value(vertical), Some(88.0));
    assert_eq!(app.profiles[0].selection(), fields);
    assert!(
        app.items()
            .iter()
            .filter(|item| item.title.contains("overlap"))
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

#[test]
fn saved_font_size_and_content_zoom_keep_legacy_values_and_mask_until_explicit_apply() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 65.0,
        logical_font_size: None,
        render_dpi: 167,
        ..settings
    }))
    .unwrap();
    app.save_reading_profile("Harbor typography").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::LineSpacing).unwrap();
    app.save_profile_fields(0).unwrap();
    let profile_before = app.profiles[0].clone();
    app.activate(Action::Text(settings)).unwrap();
    let accepted = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(accepted, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeRight)
        .unwrap();
    let turn = LogicalTurn::Clockwise;
    let accepted = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(accepted, ViewportDecision::Commit);
    let book_before = app.active.as_ref().unwrap().reading.config.settings;
    let location = app.current_position().unwrap();

    let font = ValueField::FontSize;
    let zoom = ValueField::ContentZoom;
    show_editor(&mut app, font);
    assert_eq!(app.editor.string(), "30.5");
    let [minus, default, plus] = app.numeric_layout().steps(STEPS, false);
    tap(&mut app, default);
    assert_eq!(app.editor.string(), "22");
    tap(&mut app, plus);
    assert_eq!(app.editor.string(), "22.5");
    assert_eq!(
        app.input(Input::LongTap(
            (minus.x + minus.w / 2) as f32,
            (minus.y + minus.h / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "17.5");
    type_value(&mut app, "11.5");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("12 to 255"));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(
        app.items()[0].detail.as_deref(),
        Some("65 px (legacy) · Excluded")
    );
    assert_eq!(app.profiles[0], profile_before);

    tap_contents_item(&mut app, font.label());
    type_value(&mut app, "44.5");
    let drawn = draw_test_candidate(&mut app, 3, turn, "landscape-profile-font-size", false);
    app.finish_viewport(drawn, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(font), Some(44.5));
    assert_eq!(app.items()[0].detail.as_deref(), Some("44.5 · Excluded"));

    tap_contents_item(&mut app, zoom.label());
    assert_eq!(app.editor.string(), "167");
    tap(&mut app, default);
    assert_eq!(app.editor.string(), "96");
    type_value(&mut app, "125.5");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0 dpi to 300 dpi"));
    type_value(&mut app, "125");
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(zoom), Some(125.0));
    assert_eq!(app.items()[1].detail.as_deref(), Some("125 dpi · Excluded"));
    assert_eq!(app.profiles[0].selection(), [Field::LineSpacing]);
    let mut wanted = profile_before;
    let Profile::Text { preferences, .. } = &mut wanted else {
        panic!("Expected a reflow profile");
    };
    preferences.logical_font_size = Some(44.5);
    preferences.render_dpi = Some(125);
    assert_eq!(app.profiles[0], wanted);
    assert_eq!(app.current_position(), Some(location.clone()));
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.settings,
        book_before
    );

    app.apply_reading_profile(0).unwrap();
    let book = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(
        (book.logical_font_size, book.render_dpi),
        (book_before.logical_font_size, book_before.render_dpi)
    );
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::FontSize).unwrap();
    app.toggle_profile_field(Field::ContentZoom).unwrap();
    app.save_profile_fields(0).unwrap();
    app.apply_reading_profile(0).unwrap();
    let book = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!((book.logical_font_size, book.render_dpi), (Some(44.5), 125));
    assert_eq!(app.current_position().unwrap().anchor, location.anchor);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(open(&fixture, &fixture.book).profiles[0], app.profiles[0]);
}

#[test]
fn saved_margins_preserve_fallbacks_link_and_mask_until_landscape_apply() {
    use crate::reading_profiles::MarginEdge;

    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let original = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        margin: 63,
        margin_sides: Some(71),
        margin_right: Some(85),
        margin_bottom: Some(42),
        ..original
    }))
    .unwrap();
    app.save_reading_profile("Harbor margins").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::LineSpacing).unwrap();
    app.save_profile_fields(0).unwrap();
    let old = app.profiles[0].clone();
    app.activate(Action::Text(original)).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let accepted = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(accepted, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let turn = LogicalTurn::Counterclockwise;
    let accepted = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(accepted, ViewportDecision::Commit);
    let before = app.active.as_ref().unwrap().reading.config.settings;
    let location = app.current_position().unwrap();

    app.show(Screen::ReadingProfile(0));
    let edit_values = field_buttons(app.navigation_layout()).1;
    tap(&mut app, edit_values);
    tap_contents_item(&mut app, "Page margins");
    assert_eq!(app.screen, Screen::ReadingProfileMargins(0));
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    let rows = app.items();
    for (row, detail) in rows.iter().zip([71, 85, 63, 42]) {
        assert_eq!(
            row.detail.as_deref(),
            Some(format!("{detail} px · Excluded").as_str())
        );
    }

    let left = ValueField::Margin(MarginEdge::Left);
    let right = ValueField::Margin(MarginEdge::Right);
    let top = ValueField::Margin(MarginEdge::Top);
    let bottom = ValueField::Margin(MarginEdge::Bottom);
    tap_contents_item(&mut app, left.label());
    assert_eq!(app.editor.string(), "71");
    type_value(&mut app, "1025");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0 px to 1024 px"));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.profiles[0], old);
    for (field, value) in [(left, "170"), (right, "90"), (top, "83")] {
        tap_contents_item(&mut app, field.label());
        type_value(&mut app, value);
        press_key(&mut app, Key::Go);
        assert_eq!(app.screen, Screen::ReadingProfileMargins(0));
    }
    tap_contents_item(&mut app, "Link top and bottom");
    assert_eq!(app.profiles[0].margins_linked(), Some(true));
    assert_eq!(app.profiles[0].value(top), Some(63.0));
    assert_eq!(app.profiles[0].value(bottom), Some(63.0));

    tap_contents_item(&mut app, bottom.label());
    type_value(&mut app, "101");
    let shown = draw_test_candidate(&mut app, 3, turn, "landscape-profile-bottom-margin", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(top), Some(101.0));
    assert_eq!(app.profiles[0].value(bottom), Some(101.0));
    tap_contents_item(&mut app, "Link top and bottom");
    assert_eq!(app.profiles[0].margins_linked(), Some(false));
    tap_contents_item(&mut app, bottom.label());
    type_value(&mut app, "33");
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].selection(), [Field::LineSpacing]);
    let mut expected = old;
    let Profile::Text { preferences, .. } = &mut expected else {
        panic!("Expected a reflow profile");
    };
    preferences.margin_left = Some(170);
    preferences.margin_right = Some(90);
    preferences.margin_top = Some(101);
    preferences.margin_bottom = Some(33);
    assert_eq!(app.profiles[0], expected);
    assert_eq!(app.active.as_ref().unwrap().reading.config.settings, before);
    assert_eq!(app.current_position(), Some(location.clone()));

    let margins = |settings: Settings| {
        [
            settings.left_margin(),
            settings.right_margin(),
            settings.top_margin(),
            settings.bottom_margin(),
        ]
    };
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let excluded = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(margins(excluded), margins(before));
    assert_eq!(excluded.link_vertical_margins, before.link_vertical_margins);
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::Margins).unwrap();
    app.save_profile_fields(0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let reading = &app.active.as_ref().unwrap().reading;
    let book = reading.config.settings;
    assert!(!app.viewport_needs_stage(shown));
    assert_eq!(
        (reading.probe.settings.width, reading.probe.settings.height),
        (book.width, book.height)
    );
    assert_eq!(
        [
            book.left_margin(),
            book.right_margin(),
            book.top_margin(),
            book.bottom_margin()
        ],
        [170, 90, 101, 33]
    );
    assert_eq!((book.width, book.height), (before.width, before.height));
    assert_eq!(app.current_position().unwrap().anchor, location.anchor);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(open(&fixture, &fixture.book).profiles[0], app.profiles[0]);
}

#[test]
fn saves_word_spacing_as_a_pair_and_honors_expansion_and_spacing_masks_on_apply() {
    use crate::WordSpacePercent;
    use crate::app::word_spacing::Field as SpacingField;

    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let active_before_profile = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        word_spacing: 1.25,
        percent_word_spacing: None,
        word_expansion: 5,
        ..active_before_profile
    }))
    .unwrap();
    app.save_reading_profile("Harbor paragraphs").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::LineSpacing).unwrap();
    app.save_profile_fields(0).unwrap();
    let old_profile = app.profiles[0].clone();
    app.activate(Action::Text(active_before_profile)).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let upright = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeRight)
        .unwrap();
    let turn = LogicalTurn::Clockwise;
    let landscape = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(landscape, ViewportDecision::Commit);
    let reading = &app.active.as_ref().unwrap().reading;
    let before = reading.config.settings;
    let location = app.current_position().unwrap();
    let profiles_before = fs::read(&app.profiles_path).unwrap();

    app.show(Screen::ReadingProfile(0));
    let edit_values = field_buttons(app.navigation_layout()).1;
    tap(&mut app, edit_values);
    let legacy_detail = app
        .items()
        .into_iter()
        .find(|item| item.title == "Word spacing")
        .unwrap()
        .detail;
    assert_eq!(
        legacy_detail.as_deref(),
        Some("+1.25 px (legacy) · Excluded")
    );
    tap_contents_item(&mut app, "Word spacing");
    let [scaling, reduction] = app.numeric_layout().choices([Rect::default(); 2]);
    assert_eq!(app.editor.string(), "95");
    type_value(&mut app, "123");
    tap(&mut app, reduction);
    assert_eq!(app.editor.string(), "75");
    type_value(&mut app, "24");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("25 to 100"));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    assert_eq!(app.profiles[0], old_profile);
    assert_eq!(fs::read(&app.profiles_path).unwrap(), profiles_before);

    tap_contents_item(&mut app, "Word spacing");
    type_value(&mut app, "123");
    tap(&mut app, reduction);
    type_value(&mut app, "67");
    let refused = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(refused, ViewportDecision::Abort);
    assert_eq!(app.editor.string(), "67");
    assert!(
        matches!(app.screen, Screen::NumberingValue(NumberField::WordSpacing(draft))
        if draft.field == SpacingField::Minimum && draft.value.scaling == 123)
    );
    assert_eq!(app.profiles[0], old_profile);
    tap(&mut app, scaling);
    assert_eq!(app.editor.string(), "123");
    tap(&mut app, reduction);
    assert_eq!(app.editor.string(), "67");
    let shown = draw_test_candidate(&mut app, 4, turn, "landscape-profile-word-spacing", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::ReadingProfileValues(0));
    let pair = WordSpacePercent {
        scaling: 123,
        minimum: 67,
    };
    let pair_detail = app
        .items()
        .into_iter()
        .find(|item| item.title == "Word spacing")
        .unwrap()
        .detail;
    assert_eq!(pair_detail.as_deref(), Some("123% / 67% · Excluded"));

    tap_contents_item(&mut app, "Word expansion");
    assert_eq!(app.editor.string(), "5");
    let [_, default, increase] = app.numeric_layout().steps(STEPS, false);
    tap(&mut app, default);
    assert_eq!(app.editor.string(), "0");
    assert_eq!(
        app.input(Input::LongTap(
            (increase.x + 12) as f32,
            (increase.y + 12) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "4");
    type_value(&mut app, "21");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0% to 20%"));
    type_value(&mut app, "7");
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].selection(), [Field::LineSpacing]);
    let mut expected = old_profile;
    let Profile::Text { preferences, .. } = &mut expected else {
        panic!("Expected a reflow profile")
    };
    preferences.percent_word_spacing = Some(pair);
    preferences.word_expansion = Some(7);
    assert_eq!(app.profiles[0], expected);
    assert_eq!(app.active.as_ref().unwrap().reading.config.settings, before);
    assert_eq!(app.current_position(), Some(location.clone()));

    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.percent_word_spacing, before.percent_word_spacing);
    assert_eq!(settings.word_expansion, before.word_expansion);
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::WordSpacing).unwrap();
    app.toggle_profile_field(Field::WordExpansion).unwrap();
    app.save_profile_fields(0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let reading = &app.active.as_ref().unwrap().reading;
    let applied = reading.config.settings;
    assert_eq!(applied.percent_word_spacing, Some(pair));
    assert_eq!(applied.word_expansion, 7);
    assert_eq!(applied.word_spacing, 1.25);
    assert_eq!(
        (applied.width, applied.height),
        (before.width, before.height)
    );
    assert_eq!(
        (reading.probe.settings.width, reading.probe.settings.height),
        (before.width, before.height)
    );
    assert_eq!(app.current_position().unwrap().anchor, location.anchor);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert!(!app.viewport_needs_stage(shown));
    assert_eq!(open(&fixture, &fixture.book).profiles[0], app.profiles[0]);
}

#[test]
fn saved_font_weight_and_gamma_replace_only_their_legacy_flags_on_set() {
    let fixture = Fixture::new();
    let mut app = open(&fixture, &fixture.book);
    let initial = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        bold_text: true,
        text_contrast: true,
        ..initial
    }))
    .unwrap();
    app.save_reading_profile("Harbor font rendering").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::LineSpacing).unwrap();
    app.save_profile_fields(0).unwrap();
    let original_profile = app.profiles[0].clone();
    app.activate(Action::Text(initial)).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let portrait = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(portrait, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let turn = LogicalTurn::Counterclockwise;
    let landscape = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(landscape, ViewportDecision::Commit);
    let before = app.active.as_ref().unwrap().reading.config.settings;
    let position = app.current_position().unwrap();

    app.show(Screen::ReadingProfile(0));
    let edit_values = field_buttons(app.navigation_layout()).1;
    tap(&mut app, edit_values);
    tap_contents_item(&mut app, "Font rendering");
    assert_eq!(app.screen, Screen::ReadingProfileFontRendering(0));
    assert_eq!(
        app.items()[0].detail.as_deref(),
        Some("Bold (legacy) · Excluded")
    );
    assert_eq!(
        app.items()[1].detail.as_deref(),
        Some("High contrast (legacy) · Excluded")
    );

    let weight = ValueField::FontWeight;
    let gamma = ValueField::FontContrast;
    tap_contents_item(&mut app, gamma.label());
    let [minus, default, plus] = app.numeric_layout().steps(STEPS, false);
    assert_eq!(app.editor.string(), "1");
    tap(&mut app, plus);
    assert_eq!(app.editor.string(), "1.02");
    assert_eq!(
        app.input(Input::LongTap((plus.x + 12) as f32, (plus.y + 12) as f32)),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "1.25");
    type_value(&mut app, "1.03");
    press_key(&mut app, Key::Go);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("supported gamma level")
    );
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::ReadingProfileFontRendering(0));
    assert_eq!(app.profiles[0], original_profile);

    tap_contents_item(&mut app, weight.label());
    tap(&mut app, default);
    tap(&mut app, minus);
    assert_eq!(app.editor.string(), "-0.25");
    assert_eq!(
        app.input(Input::LongTap((plus.x + 12) as f32, (plus.y + 12) as f32)),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "0.75");
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(weight), Some(0.75));
    assert_eq!(app.profiles[0].legacy_value_label(weight), None);
    assert_eq!(
        app.profiles[0].legacy_value_label(gamma),
        Some("High contrast (legacy)")
    );

    tap_contents_item(&mut app, gamma.label());
    type_value(&mut app, "1.35");
    let refused = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(refused, ViewportDecision::Abort);
    assert_eq!(app.editor.string(), "1.35");
    assert!(app.profiles[0].legacy_value_label(gamma).is_some());
    let shown = draw_test_candidate(&mut app, 4, turn, "landscape-profile-font-contrast", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(gamma), Some(1.35));
    assert_eq!(app.profiles[0].selection(), [Field::LineSpacing]);
    let mut expected = original_profile;
    let Profile::Text { preferences, .. } = &mut expected else {
        panic!("Expected a reflow profile")
    };
    preferences.font_weight_delta = Some(0.75);
    preferences.font_gamma_index = Some(23);
    preferences.bold_text = false;
    preferences.text_contrast = false;
    assert_eq!(app.profiles[0], expected);
    assert_eq!(app.active.as_ref().unwrap().reading.config.settings, before);
    assert_eq!(app.current_position(), Some(position.clone()));

    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::FontWeight).unwrap();
    app.save_profile_fields(0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let only_weight = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(only_weight.font_weight_delta, 0.75);
    assert_eq!(only_weight.font_gamma_index, before.font_gamma_index);
    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::FontContrast).unwrap();
    app.save_profile_fields(0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let reading = &app.active.as_ref().unwrap().reading;
    assert_eq!(reading.config.settings.font_gamma_index, 23);
    assert_eq!(
        (reading.probe.settings.width, reading.probe.settings.height),
        (before.width, before.height)
    );
    assert_eq!(app.current_position().unwrap().anchor, position.anchor);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert!(!app.viewport_needs_stage(shown));
    assert_eq!(open(&fixture, &fixture.book).profiles[0], app.profiles[0]);
}

#[test]
fn saved_scan_values_keep_adaptive_pdf_and_crop_position_until_apply() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Survey.pdf");
    let source = crate::pdf_crop::source_margin_tests::fixture("edge-specks");
    fs::write(&path, &source).unwrap();
    let mut app = open(&fixture, &path);
    app.activate(Action::FixedScroll(true)).unwrap();
    app.activate(Action::SemiAutoCrop).unwrap();
    tap(&mut app, crate::app::semi_auto_crop::APPLY);
    assert_eq!(app.screen, Screen::Fixed);
    app.activate(Action::ScanContrast(true)).unwrap();
    app.save_reading_profile("Survey scans").unwrap();
    app.edit_profile_fields(0).unwrap();
    app.set_profile_fields(false).unwrap();
    app.toggle_profile_field(Field::ScanContrast).unwrap();
    app.save_profile_fields(0).unwrap();

    let upright = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let turn = LogicalTurn::Counterclockwise;
    let shown = draw_test_candidate(&mut app, 2, turn, "", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    app.input(Input::Swipe {
        dx: 0.0,
        dy: -140.0,
        start_y: 800.0,
    });
    app.tick(Instant::now() + Duration::from_secs(1));
    let before = app.fixed.as_ref().unwrap().reading.preferences();
    let fixed = app.fixed.as_ref().unwrap();
    assert!(before.continuous && before.semi_auto_crop && before.scan_contrast);
    let document = fixed.id;
    let seeds = app.library.state(document).unwrap().pdf_crop_seeds.clone();
    let page = fixed.reading.page();
    let position = fixed.reading.history_point().unwrap();
    let profile = app.profiles[0].clone();

    let gamma = ValueField::ScanGamma;
    let margin = ValueField::CropMargin;
    show_editor(&mut app, gamma);
    assert_eq!(app.profiles[0].legacy_value_label(gamma), Some("Adaptive"));
    let [minus, reset, plus] = app.numeric_layout().steps(STEPS, false);
    tap(&mut app, reset);
    tap(&mut app, minus);
    assert_eq!(app.editor.string(), "0.9");
    assert_eq!(
        app.input(Input::LongTap((plus.x + 12) as f32, (plus.y + 12) as f32)),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "1.9");
    type_value(&mut app, "1.95");
    press_key(&mut app, Key::Go);
    assert!(app.notice.as_deref().unwrap().contains("0.8 to 50.0"));
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.profiles[0], profile);
    tap_contents_item(&mut app, gamma.label());
    type_value(&mut app, "1.9");
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(gamma), Some(1.9));
    assert_eq!(app.profiles[0].legacy_value_label(gamma), None);

    tap_contents_item(&mut app, margin.label());
    tap(&mut app, minus);
    assert_eq!(app.editor.string(), "0.09");
    assert_eq!(
        app.input(Input::LongTap((plus.x + 12) as f32, (plus.y + 12) as f32)),
        Change::Partial
    );
    assert_eq!(app.editor.string(), "0.19");
    type_value(&mut app, "1.501");
    press_key(&mut app, Key::Go);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("0.00 in to 1.50 in")
    );
    tap(&mut app, reset);
    type_value(&mut app, "0.37");
    let aborted = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(aborted, ViewportDecision::Abort);
    assert_eq!(app.editor.string(), "0.37");
    assert_eq!(app.profiles[0].value(margin), Some(0.10));
    let shown = draw_test_candidate(&mut app, 4, turn, "landscape-profile-scan-margin", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    press_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].value(margin), Some(0.37));
    assert_eq!(app.profiles[0].selection(), [Field::ScanContrast]);
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences(), before);
    assert_eq!(
        app.fixed.as_ref().unwrap().reading.history_point(),
        Some(position)
    );

    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let active = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(active.preferences().scan_gamma_tenths, Some(19));
    assert!(!active.preferences().scan_contrast);
    assert_eq!(active.preferences().crop, before.crop);
    assert_eq!(active.preferences().crop_margin(), 10);

    app.edit_profile_fields(0).unwrap();
    app.toggle_profile_field(Field::CropMargin).unwrap();
    app.save_profile_fields(0).unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");
    let active = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(active.preferences().crop_margin(), 37);
    assert_ne!(active.preferences().crop, before.crop);
    assert_eq!(active.viewport().dimensions(), (1680, 1264));
    assert_eq!(active.page(), page);
    assert!((active.history_point().unwrap().y - position.y).abs() < 1.0);
    assert_eq!(app.library.state(document).unwrap().pdf_crop_seeds, seeds);
    assert_eq!(fs::read(&path).unwrap(), source);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert!(!app.viewport_needs_stage(shown));
    assert_eq!(open(&fixture, &path).profiles[0], app.profiles[0]);
}

#[test]
fn saved_scan_values_follow_the_fixed_provider_and_keep_djvu_adaptive() {
    use crate::reading_profiles::{FixedProfile, FixedProvider};

    for provider in [
        FixedProvider::Pdf,
        FixedProvider::Comic,
        FixedProvider::Djvu,
    ] {
        let fixed = FixedProfile::from_view(
            provider,
            FixedPreferences {
                scan_contrast: true,
                ..Default::default()
            },
        );
        let mut profile = Profile::Fixed {
            name: "Scans".into(),
            fixed,
            fields: None,
            apply_on_open: false,
            opening: None,
            opening_time: None,
        };
        let numeric_gamma = provider != FixedProvider::Djvu;
        let crop_margin = provider == FixedProvider::Pdf;
        for (field, supported) in [
            (ValueField::ScanGamma, numeric_gamma),
            (ValueField::CropMargin, crop_margin),
        ] {
            let before = profile.clone();
            assert_eq!(profile.editable_values().contains(&field), supported);
            assert_eq!(profile.value(field).is_some(), supported);
            assert_eq!(profile.set_value(field, field.default()).is_ok(), supported);
            if !supported {
                assert_eq!(profile, before);
            }
        }
        if provider == FixedProvider::Djvu {
            assert_eq!(
                profile,
                Profile::Fixed {
                    name: "Scans".into(),
                    fixed,
                    fields: None,
                    apply_on_open: false,
                    opening: None,
                    opening_time: None,
                }
            );
        }
    }
}
