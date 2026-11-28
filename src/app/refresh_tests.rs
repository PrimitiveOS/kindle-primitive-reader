use super::*;
use crate::app::chapter_navigation::Direction;
use crate::app::tests::Fixture;

/// Reflow chapter jumps run on a worker; wait until the position has moved.
fn settle_chapter_jump(app: &mut Reader) {
    let stop = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while app.chapter_navigation_job.is_some() {
        assert!(std::time::Instant::now() < stop, "chapter jump did not finish");
        let due = app
            .chapter_navigation_job
            .as_ref()
            .map_or_else(std::time::Instant::now, |job| job.deadline());
        app.poll_chapter_navigation(due + std::time::Duration::from_secs(1));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn chapter_boundary_refresh_is_full_only_when_the_option_is_on() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    // Picture pages refresh by default; this test checks only the chapter rule.
    app.controls.no_refresh_on_pictures = true;
    // The rule starts from the chapter the book opened in.
    assert_eq!(app.reading_update(true), Change::Partial);

    app.activate(Action::Chapter(Direction::Next)).unwrap();
    settle_chapter_jump(&mut app);
    assert_eq!(app.reading_update(true), Change::Partial, "off by default");

    app.controls.refresh_on_chapter_boundaries = true;
    app.activate(Action::Chapter(Direction::Previous)).unwrap();
    settle_chapter_jump(&mut app);
    assert_eq!(app.reading_update(true), Change::Full, "crossing back refreshes");
    assert_eq!(app.reading_update(true), Change::Partial, "same chapter stays partial");
}

#[test]
fn a_page_with_a_picture_refreshes_fully_unless_the_option_is_off() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.open_path(&fixture.book).unwrap();
    let pages = app.active.as_ref().unwrap().reading.probe.page_count();
    let picture = (0..pages)
        .find(|&page| {
            let reading = &mut app.active.as_mut().unwrap().reading;
            reading.go_to_page(0, page).unwrap();
            reading.page_has_images()
        })
        .expect("the field notes fixture has a picture page");

    // The rule looks at the page the turn lands on.
    app.active.as_mut().unwrap().reading.go_to_page(0, picture).unwrap();
    assert_eq!(app.reading_update(true), Change::Full, "on by default");
    app.controls.no_refresh_on_pictures = true;
    assert_eq!(app.reading_update(true), Change::Partial, "off when chosen");
}

