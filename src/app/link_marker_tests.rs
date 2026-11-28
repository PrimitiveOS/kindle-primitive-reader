use super::*;
use crate::app::tests::Fixture;
use crate::epub::Destination;
use primitive_app::App;
use primitive_ui::Canvas;
use std::time::Duration;

#[test]
fn a_followed_link_marks_its_target_for_a_second_after_half_a_second() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    // "door" lives in chapter 1; land on the chapter's page first so the link is on screen.
    app.active.as_mut().unwrap().reading.jump(1, Some("door")).unwrap();
    let path = app.active.as_ref().unwrap().reading.probe.chapter.clone();
    let target = Destination {
        path,
        fragment: Some("door".into()),
    };
    let start = Instant::now();
    app.open_reflow_link(link_taps::ReflowLink::Internal(target), 0.0, 0.0);
    let marker = app.link_marker.expect("a followed link is marked");
    assert!(!marker.shown);
    assert!(marker.width > 0 && marker.height > 0);

    let font_paths: Vec<String> = fonts.iter().map(|font| font.display().to_string()).collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
    // Not yet: the bar waits for its delay.
    assert_eq!(app.tick(start + Duration::from_millis(100)), Change::None);
    assert!(!app.link_marker.unwrap().shown);

    // Shown after the delay: the bar is black at the left edge, at the target's height.
    assert_eq!(app.tick(start + Duration::from_millis(600)), Change::Partial);
    assert!(app.link_marker.unwrap().shown);
    let frame = primitive_app::frame(&app, &mut canvas, false);
    let (x, y) = (2_usize, (marker.y + marker.height / 2) as usize);
    assert_eq!(frame[y * WIDTH as usize + x], 0, "the bar is black");

    // Removed a second after it appeared.
    assert_eq!(app.tick(start + Duration::from_millis(1600)), Change::Partial);
    assert!(app.link_marker.is_none());
}
