use super::*;
use crate::app::{
    selection_viewport_tests::{adopt, capture},
    tests::{Fixture, tap_contents_item, tap_reading_control},
};
use crate::reading_controls::{MenuActivation, StatusBar};
use crate::session::ReadingViewport;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

#[test]
fn menu_activation_switches_preserve_one_trigger_and_save_auto_show() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::PORTRAIT, 1);
    let origin = app.current_history_location();
    let top = Input::Tap(WIDTH as f32 / 2.0, 32.0);
    assert_eq!(app.input(top), Change::Partial);
    assert!(app.reading_top && app.reading_bottom);
    tap_reading_control(&mut app, "Settings");
    tap_reading_control(&mut app, "Menu activation");
    assert_eq!(app.screen, Screen::MenuActivationSettings);
    capture(&app, &mut canvas, "reader-menu-activation.png", false);
    for (item, expected) in [
        ("With a tap", MenuActivation::Swipe),
        ("With a swipe", MenuActivation::Tap),
        ("With a swipe", MenuActivation::TapAndSwipe),
        ("With a swipe", MenuActivation::Tap),
        ("With a tap", MenuActivation::Swipe),
        ("With a tap", MenuActivation::TapAndSwipe),
    ] {
        tap_contents_item(&mut app, item);
        assert_eq!(app.controls.menu_activation, expected);
        assert!(expected.allows_tap() || expected.allows_swipe());
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).menu_activation,
            expected
        );
    }
    tap_contents_item(&mut app, "Auto-show bottom menu");
    assert!(!app.controls.auto_shows_bottom(true));
    app.show(Screen::Book);
    assert_eq!(app.input(top), Change::Partial);
    assert!(app.reading_top && !app.reading_bottom);
    capture(&app, &mut canvas, "reader-menu-top-only.png", false);
    app.show(Screen::Book);
    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert!(!reopened.controls.auto_shows_bottom(false));
    assert_eq!(reopened.input(top), Change::Partial);
    assert!(reopened.reading_top && !reopened.reading_bottom);
    assert_eq!(reopened.current_history_location(), origin);
}

#[test]
fn menu_activation_edges_and_direct_bottom_bar_work_in_landscape_pdf() {
    let fixture = Fixture::new();
    let file = fixture.root.join("Harbor survey.pdf");
    std::fs::write(&file, crate::fixed::tests::document()).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&file).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 1);
    let origin = app.current_history_location();
    let (width, height) = app.reading_viewport().dimensions();
    let (width, height) = (width as f32, height as f32);
    let left = width / 5.0;
    assert_eq!(
        app.input(Input::Tap(left, height / 8.0 - 3.0)),
        Change::Partial
    );
    assert_eq!(app.top_group, Some(TopGroup::Navigation));
    assert!(app.reading_top && app.reading_bottom);
    assert_eq!(app.reading_panel, Some(BarPanel::View));
    capture(
        &app,
        &mut canvas,
        "reader-menu-auto-bottom-landscape.png",
        false,
    );

    app.show(Screen::Fixed);
    app.change_menu_activation(ActivationOption::AutoBottom)
        .unwrap();
    assert_eq!(app.input(Input::Tap(left, height * 0.89)), Change::Partial);
    assert!(!app.reading_top && app.reading_bottom);
    assert_eq!(app.reading_panel, Some(BarPanel::View));
    app.show(Screen::Fixed);
    app.change_menu_activation(ActivationOption::Tap).unwrap();
    assert_eq!(app.controls.menu_activation, MenuActivation::Swipe);
    assert!(!app.reveal_reading_tools(left, height / 8.0 - 3.0, Screen::FixedTools));
    assert!(!app.reveal_reading_tools(left, height * 0.89, Screen::FixedTools));
    let down = Input::Swipe {
        dx: 2.0,
        dy: 178.0,
        start_y: height * 0.19,
    };
    assert!(app.reading_edge_swipe_direction(down, left).is_none());
    assert_eq!(app.swipe(down, width * 0.7), Change::Partial);
    assert_eq!(app.top_group, Some(TopGroup::Library));
    assert!(app.reading_top && !app.reading_bottom);
    app.show(Screen::Fixed);
    app.change_menu_activation(ActivationOption::Swipe).unwrap();
    assert_eq!(app.controls.menu_activation, MenuActivation::Tap);
    assert!(
        app.reading_edge_swipe_direction(down, width * 0.7)
            .is_none()
    );
    assert!(!app.reveal_reading_tools(left, height * 0.95, Screen::FixedTools));
    app.controls.footer_tap_locked = true;
    assert_eq!(app.input(Input::Tap(left, height * 0.95)), Change::None);
    assert_eq!(app.controls.status_bar, StatusBar::Chapter);
    assert_eq!(app.current_history_location(), origin);
}

#[test]
fn menu_activation_legacy_edges_keep_the_upper_center_shortcut() {
    let mut controls: ReadingControls = serde_json::from_str(r#"{"both_toolbars":false}"#).unwrap();
    assert!(controls.auto_shows_bottom(true));
    assert!(!controls.auto_shows_bottom(false));
    controls.auto_show_bottom_menu = Some(true);
    assert!(controls.auto_shows_bottom(false));
    let defaults: ReadingControls = serde_json::from_str("{}").unwrap();
    assert!(defaults.auto_shows_bottom(false));
    assert!(defaults.auto_shows_bottom(true));
}
