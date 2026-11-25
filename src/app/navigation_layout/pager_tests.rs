use super::super::*;
use crate::app::selection_viewport_tests::{adopt, canvas, capture, press, tap};
use crate::app::tests::Fixture;
use crate::session::ReadingViewport;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = ["NotoSans-Regular.ttf", "NotoSans-Bold.ttf"]
        .map(|name| crate::test_fixture(&format!("fonts/{name}")));
    Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap()
}

fn hold(reader: &mut Reader, area: Rect) -> Change {
    let x = (area.x + area.w / 2) as f32;
    let y = (area.y + area.h / 2) as f32;
    assert_eq!(reader.input(Input::HoldStart(x, y)), Change::None);
    reader.input(Input::HoldRelease {
        start_x: x,
        start_y: y,
        x,
        y,
        held_ms: 1_000,
    })
}

#[test]
fn menu_footer_hold_jumps_to_edges_without_moving_the_book() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let start = app.current_position();
    app.show(Screen::ReadingControls);
    let last = (app.list_window().0 - 1) / app.list_geometry().rows;
    assert!(last >= 2);

    let mut frame = canvas(&app, ReadingViewport::PORTRAIT);
    press(&app, &mut frame, NEXT, false);
    assert_eq!(hold(&mut app, PREV), Change::None);
    tap(&mut app, NEXT);
    assert_eq!(app.list_page, 1);
    assert_eq!(hold(&mut app, NEXT), Change::Partial);
    assert_eq!(app.list_page, last);
    assert_eq!(hold(&mut app, NEXT), Change::None);
    capture(&app, &mut frame, "menu-pager-last.png", false);

    assert_eq!(hold(&mut app, PREV), Change::Partial);
    assert_eq!(app.list_page, 0);
    capture(&app, &mut frame, "menu-pager-first.png", false);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.list_page, 1);
    tap(&mut app, PREV);
    assert_eq!(app.list_page, 0);
    assert_eq!(app.screen, Screen::ReadingControls);
    assert_eq!(app.current_position(), start);
}

#[test]
fn menu_footer_hold_uses_live_documents_bounds_and_stays_inside_the_folder() {
    let fixture = Fixture::new();
    let folder = fixture.root.join("Reports");
    fs::create_dir(&folder).unwrap();
    for number in 1..=25 {
        fs::copy(
            &fixture.book,
            folder.join(format!("Report {number:02}.epub")),
        )
        .unwrap();
    }
    let mut app = reader(&fixture);
    app.browse_to("Reports".into()).unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let layout = app.navigation_layout();
    assert_eq!(app.screen, Screen::Library);
    assert!(layout.next().x > WIDTH as i32);
    let first = primitive_app::frame(&app, &mut canvas, true);
    assert_eq!(hold(&mut app, layout.previous()), Change::None);

    tap(&mut app, layout.next());
    let second = primitive_app::frame(&app, &mut canvas, true);
    assert!(second != first);
    for _ in 0..2 {
        assert_eq!(app.input(Input::PageForward), Change::Partial);
    }
    assert_eq!(app.input(Input::PageForward), Change::None);
    let last = primitive_app::frame(&app, &mut canvas, true);
    assert!(last != second);

    assert_eq!(hold(&mut app, layout.previous()), Change::Partial);
    assert!(primitive_app::frame(&app, &mut canvas, true) == first);
    assert_eq!(hold(&mut app, layout.next()), Change::Partial);
    assert!(primitive_app::frame(&app, &mut canvas, true) == last);
    assert_eq!(hold(&mut app, layout.next()), Change::None);
    capture(&app, &mut canvas, "documents-pager-last-dark.png", true);
    assert_eq!(app.screen, Screen::Library);
    assert_eq!(app.browser.directory(), Path::new("Reports"));
    assert!(app.current_position().is_none());
}
