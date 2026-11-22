use super::*;
use crate::app::tests::{Fixture, capture_test_screen};

#[test]
fn live_cjk_width_adjusts_by_one_or_five_restores_source_and_reopens() {
    let fixture = Fixture::new();
    std::fs::copy(crate::test_fixture("harbour-cjk-width.epub"), &fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let current = |app: &Reader| {
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .cjk_width_percent
    };
    let control = |app: &mut Reader, label: &str| {
        // The CJK controls are the bottom row. Word expansion above also uses ±1%.
        let r = app
            .reading_bars()
            .buttons
            .into_iter()
            .filter(|button| button.label == label)
            .max_by_key(|button| button.rect.y)
            .unwrap_or_else(|| panic!("Missing CJK width control {label}"))
            .rect;
        app.input(Input::Tap((r.x + r.w / 2) as f32, (r.y + r.h / 2) as f32))
    };
    let position = app.current_history_location().unwrap().position;
    assert_eq!(app.open_reading_edge(false, 500.), Change::Partial);
    control(&mut app, "Spacing");
    for (name, width) in [("-1%", 100), ("+5%", 105), ("+1%", 106), ("-1%", 105)] {
        let was = current(&app);
        assert_eq!(
            control(&mut app, name),
            if was == width {
                Change::None
            } else {
                Change::Partial
            },
            "{name}"
        );
        assert_eq!(current(&app), width);
        assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
    }
    capture_test_screen(&app, &fonts, "READER_CJK_UI", false);
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(current(&reopened), 105);
    assert_eq!(
        reopened.current_history_location().unwrap().position,
        position
    );
    reopened.open_reading_edge(false, 500.);
    if reopened.reading_panel != Some(BarPanel::TextSpacing) {
        control(&mut reopened, "Spacing");
    }
    control(&mut reopened, "100%");
    assert_eq!(current(&reopened), 100);
    let id = reopened.active.as_ref().unwrap().id;
    assert!(
        reopened
            .library
            .state(id)
            .unwrap()
            .preferences
            .cjk_width_percent
            .is_none()
    );
}
