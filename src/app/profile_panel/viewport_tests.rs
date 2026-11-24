use super::*;
use crate::app::reading_viewport_stage::draw_test_candidate;
use crate::app::selection_viewport_tests::{canvas, key, press, tap};
use crate::app::tests::Fixture;
use crate::fixed_session::tests::striped_pdf_sized_bytes;
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, ViewportDecision, ViewportRequest};
use primitive_ui::Key;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

fn rotate(app: &mut Reader, generation: u64, orientation: ReadingOrientation, turn: LogicalTurn) {
    app.set_reading_orientation(orientation).unwrap();
    let candidate = draw_test_candidate(app, generation, turn, "", false);
    app.finish_viewport(candidate, ViewportDecision::Commit);
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
}

fn start_landscape(app: &mut Reader, turn: LogicalTurn) {
    let candidate = draw_test_candidate(app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(candidate, ViewportDecision::Commit);
    let orientation = match turn {
        LogicalTurn::Clockwise => ReadingOrientation::LandscapeRight,
        LogicalTurn::Counterclockwise => ReadingOrientation::LandscapeLeft,
        _ => unreachable!("a landscape turn"),
    };
    rotate(app, 2, orientation, turn);
}

fn assert_landscape(app: &Reader, turn: LogicalTurn) {
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(app.reading_viewport(), ReadingViewport::LANDSCAPE);
}

fn tap_row(app: &mut Reader, title: &str) {
    let (_, items) = app.list_window();
    let on_screen = items
        .iter()
        .position(|item| item.title == title)
        .unwrap_or_else(|| panic!("{title} is not visible on the current page"));
    let area = app.list_geometry().row(on_screen);
    assert!(area.right() - 80 > WIDTH as i32);
    assert_eq!(
        app.input(Input::Tap(
            (area.right() - 80) as f32,
            (area.y + area.h / 2) as f32,
        )),
        Change::Partial
    );
}

fn tap_key(app: &mut Reader, key_name: Key) {
    let area = key(app, key_name, "Save");
    tap(app, area);
}

fn type_name(app: &mut Reader, name: &str) {
    for character in name.chars() {
        tap_key(app, Key::Char(character));
    }
}

#[test]
fn landscape_profiles_create_and_apply_only_the_saved_epub_field_at_the_reading_anchor() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let turn = LogicalTurn::Clockwise;
    start_landscape(&mut app, turn);
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(1, 0)
        .unwrap();
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    app.activate(Action::Text(Settings {
        font_size: 65.,
        logical_font_size: None,
        margin: 44,
        ..settings
    }))
    .unwrap();
    let before = app.current_position().unwrap();
    let book = app.open_book_id();
    app.open_reading_edge(true, 840.);
    app.top_group = Some(TopGroup::Appearance);
    let profiles = app.reading_button("Reading profiles");
    tap(&mut app, profiles);
    assert_eq!(app.screen, Screen::ReadingProfiles);
    assert_landscape(&app, turn);
    tap_row(&mut app, "Save current settings");
    assert_eq!(app.screen, Screen::ProfileName(Name::Create));
    assert_landscape(&app, turn);
    type_name(&mut app, "harbor");
    let draft = app.editor.string();
    let staged = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(staged, ViewportDecision::Abort);
    assert_eq!(app.editor.string(), draft);
    assert_eq!(app.editor.caret, draft.len());
    assert!(app.profiles.is_empty());
    assert_landscape(&app, turn);
    let mut frame = canvas(&app, ReadingViewport::LANDSCAPE);
    press(&app, &mut frame, key(&app, Key::Char('h'), "Save"), false);
    tap_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].name(), "harbor");
    assert_eq!(app.screen, Screen::ReadingProfiles);
    assert_landscape(&app, turn);
    tap_row(&mut app, "harbor");
    tap_row(&mut app, "Included settings");
    assert_landscape(&app, turn);
    let (save, cancel) = field_buttons(app.navigation_layout());
    assert!(save.x > WIDTH as i32);
    assert!(cancel.right() < save.x);
    press(&app, &mut frame, save, false);
    press(&app, &mut frame, cancel, false);

    let original_profile = app.profiles[0].clone();
    tap_row(&mut app, "Clear all");
    tap(&mut app, save);
    assert!(app.notice.is_some());
    assert_eq!(app.profiles[0], original_profile);
    tap_row(&mut app, "Font size");
    assert_eq!(app.profile_fields, [Field::FontSize]);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let fields_page = app.list_page;
    let staged = draw_test_candidate(&mut app, 4, LogicalTurn::Upright, "", false);
    app.finish_viewport(staged, ViewportDecision::Abort);
    assert_eq!(app.list_page, fields_page);
    assert_eq!(app.profile_fields, [Field::FontSize]);
    tap(&mut app, cancel);
    assert_eq!(app.screen, Screen::ReadingProfile(0));
    assert_eq!(app.profiles[0], original_profile);
    tap_row(&mut app, "Included settings");
    tap_row(&mut app, "Clear all");
    tap_row(&mut app, "Font size");
    let shown = draw_test_candidate(&mut app, 5, turn, "landscape-reading-profile-fields", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    tap(&mut app, save);
    assert_eq!(app.profiles[0].selection(), [Field::FontSize]);
    assert_eq!(app.current_position(), Some(before.clone()));
    assert_landscape(&app, turn);

    app.activate(Action::Text(Settings {
        font_size: 48.,
        logical_font_size: None,
        margin: 93,
        ..settings
    }))
    .unwrap();
    let before_apply = app.current_position().unwrap();
    app.show(Screen::ReadingProfile(0));
    tap_row(&mut app, "Apply to this book");
    assert_eq!(app.screen, Screen::Book);
    assert_landscape(&app, turn);
    assert_eq!(app.open_book_id(), book);
    assert_eq!(app.current_position(), Some(before_apply));
    let settings = app.active.as_ref().unwrap().reading.config.settings;
    assert_eq!(settings.font_size, 65.);
    assert_eq!(settings.margin, 93);
    assert_eq!(reader(&fixture).profiles[0].selection(), [Field::FontSize]);
}

#[test]
fn landscape_profiles_edit_replace_and_delete_a_pdf_profile_without_moving_the_document() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Harbor scans.pdf");
    fs::write(&path, striped_pdf_sized_bytes(3, 612, 820)).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    let turn = LogicalTurn::Counterclockwise;
    start_landscape(&mut app, turn);
    app.fixed.as_mut().unwrap().reading.jump(1).unwrap();
    let position = app.current_history_location();
    let book = app.open_book_id();
    app.activate(Action::ScanGamma(20)).unwrap();
    app.save_reading_profile("scans").unwrap();
    tap_row(&mut app, "scans");
    tap_row(&mut app, "Rename");
    assert_eq!(app.editor.string(), "scans");
    assert_landscape(&app, turn);
    tap_key(&mut app, Key::Char('x'));
    let back = app.navigation_layout().back();
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::ReadingProfile(0));
    assert_eq!(app.profiles[0].name(), "scans");
    tap_row(&mut app, "Rename");
    type_name(&mut app, "x");
    tap_key(&mut app, Key::Go);
    assert_eq!(app.profiles[0].name(), "scansx");

    tap_row(&mut app, "Duplicate");
    type_name(&mut app, "copies");
    tap_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::ReadingProfiles);
    assert_landscape(&app, turn);
    tap_row(&mut app, "copies");
    let tap_delete = |app: &mut Reader| {
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        tap_row(app, "Delete");
    };
    tap_delete(&mut app);
    let deletion = profile_confirmation_sheet(app.navigation_layout(), "copies", Confirm::Delete);
    let cancel = deletion.button(SheetAction::Cancel);
    let delete = deletion.button(SheetAction::Confirm);
    let portrait = profile_confirmation_sheet(NavigationLayout::PORTRAIT, "", Confirm::Delete)
        .button(SheetAction::Confirm);
    assert_eq!((delete.w, delete.h), (portrait.w, portrait.h));
    assert_ne!((delete.x, delete.y), (portrait.x, portrait.y));
    let mut frame = canvas(&app, ReadingViewport::LANDSCAPE);
    press(&app, &mut frame, cancel, false);
    press(&app, &mut frame, delete, false);
    assert_landscape(&app, turn);

    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.profiles.len(), 2);
    tap_delete(&mut app);
    tap(&mut app, cancel);
    assert_eq!(app.profiles.len(), 2);
    tap_delete(&mut app);
    tap(&mut app, delete);
    assert_eq!(app.screen, Screen::ReadingProfiles);
    assert_eq!(app.profiles.len(), 1);
    tap_row(&mut app, "scansx");
    tap_row(&mut app, "Replace with current settings");
    let replace = profile_confirmation_sheet(app.navigation_layout(), "", Confirm::Replace)
        .button(SheetAction::Confirm);
    tap(&mut app, replace);
    assert_eq!(app.screen, Screen::ReadingProfile(0));
    assert_landscape(&app, turn);
    tap(&mut app, back);
    tap(&mut app, back);
    assert_eq!(app.screen, Screen::Fixed);
    assert_landscape(&app, turn);
    assert_eq!(app.open_book_id(), book);
    assert_eq!(app.current_history_location(), position);
}

#[test]
fn profile_pages_retain_the_same_visible_window_through_a_viewport_round_trip() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let viewport = draw_test_candidate(&mut app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(viewport, ViewportDecision::Commit);
    for index in 1..=14 {
        app.save_reading_profile(&format!("profile {index:02}"))
            .unwrap();
    }
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    let (_, portrait_rows) = app.list_window();
    let first = portrait_rows[0].title.clone();
    app.notice = None;
    let mut portrait_canvas = canvas(&app, ReadingViewport::PORTRAIT);
    let portrait_frame = primitive_app::frame(&app, &mut portrait_canvas, false);
    rotate(
        &mut app,
        2,
        ReadingOrientation::LandscapeRight,
        LogicalTurn::Clockwise,
    );
    assert!(app.list_window().1.iter().any(|row| row.title == first));
    rotate(
        &mut app,
        3,
        ReadingOrientation::Upright,
        LogicalTurn::Upright,
    );
    assert_eq!(app.list_window().1[0].title, first);
    assert!(
        primitive_app::frame(&app, &mut portrait_canvas, false) == portrait_frame,
        "portrait list frame changed after the viewport round trip"
    );
}
