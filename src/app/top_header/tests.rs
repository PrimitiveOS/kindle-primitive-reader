use super::*;
use crate::app::tests::Fixture;
use primitive_app::App;

#[test]
fn top_header_legacy_book_keeps_enabled_after_new_book_default_changes() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut original = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    original.open_path(&fixture.book).unwrap();
    let id = original.active.as_ref().unwrap().id;
    // Recreate library and controls written before the per-book key was introduced.
    original.library.state_mut(id).unwrap().top_status_header = None;
    original.save().unwrap();
    std::fs::write(
        &original.controls_path,
        r#"{"top_header":{"enabled":true}}"#,
    )
    .unwrap();
    drop(original);

    let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    assert!(app.controls.top_header.default_enabled());
    assert!(app.controls.top_header.legacy_enabled);
    app.activate(Action::SetReadingControls(ReadingControls {
        top_header: TopHeader {
            book_default: Some(false),
            ..app.controls.top_header
        },
        ..app.controls
    }))
    .unwrap();
    assert!(app.top_header_for_book(id));
    assert!(!app.controls.top_header.default_enabled());
    app.open_path(&fixture.book).unwrap();
    assert!(app.top_header_caption().is_some());
    assert_eq!(app.library.state(id).unwrap().top_status_header, Some(true));
}
