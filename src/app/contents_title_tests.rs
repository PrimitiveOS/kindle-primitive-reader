use super::*;
use crate::app::tests::Fixture;

#[test]
fn long_press_shows_the_full_contents_title_until_a_tap_closes_it() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    app.activate(Action::Show(Screen::Contents)).unwrap();

    let (_, rows) = app.contents_items(0, 1);
    let expected = rows[0].title.clone();
    let rect = app.contents_row_rect(0);
    let (x, y) = (
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    );
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert_eq!(app.contents_title.as_deref(), Some(expected.as_str()));

    assert_eq!(app.input(Input::Tap(10.0, 10.0)), Change::Partial);
    assert_eq!(app.contents_title, None);
}
