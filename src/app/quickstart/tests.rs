use super::*;
use crate::app::selection_viewport_tests::{adopt, canvas, capture};
use crate::app::tests::{Fixture, tap_reading_control};
use crate::app::{Screen, WIDTH};
use crate::session::ReadingViewport;
use primitive_app::App;
use std::fs;

fn reader(fixture: &Fixture) -> Reader {
    let fonts = [
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap()
}

fn tap(reader: &mut Reader, rect: Rect) -> Change {
    reader.input(Input::Tap(
        (rect.x + rect.w / 2) as f32,
        (rect.y + rect.h / 2) as f32,
    ))
}

fn open_from_book(reader: &mut Reader) {
    let width = reader.reading_viewport().dimensions().0;
    assert_eq!(
        reader.input(Input::Tap(width as f32 / 2., 30.)),
        Change::Partial
    );
    assert_eq!(tap_reading_control(reader, "Settings"), Change::Partial);
    assert_eq!(tap_reading_control(reader, TITLE), Change::Full);
}

#[test]
fn quickstart_guide_returns_to_the_epub_menu_and_exact_passage() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(1, 0).unwrap();
    let initial_position = app.current_history_location();
    let book = app.open_book_id();
    let mut canvas = canvas(&app, ReadingViewport::PORTRAIT);
    open_from_book(&mut app);
    let menu = (
        app.screen,
        app.top_group,
        app.reading_bottom,
        app.reading_panel,
    );
    assert_eq!(app.quickstart.unwrap().page, 0);
    capture(&app, &mut canvas, "quickstart-topics.png", false);

    let layout = app.navigation_layout();
    let topic = Guide::topic_rows(layout, &Guide::index_rows())
        .row_rect(2)
        .unwrap();
    assert_eq!(tap(&mut app, topic), Change::Partial);
    assert_eq!(app.quickstart.unwrap().page, 3);
    assert_eq!(app.input(Input::PageForward), Change::Partial);
    assert_eq!(app.quickstart.unwrap().page, 4);
    assert_eq!(tap(&mut app, Guide::topics_button(layout)), Change::Partial);
    assert_eq!(app.quickstart.unwrap().page, 0);
    assert_eq!(tap(&mut app, layout.back()), Change::Full);

    assert!(app.quickstart.is_none());
    assert_eq!(app.open_book_id(), book);
    assert_eq!(app.current_history_location(), initial_position);
    assert_eq!(
        menu,
        (
            app.screen,
            app.top_group,
            app.reading_bottom,
            app.reading_panel
        )
    );
    assert!(app.reading_button(TITLE).w >= 100);
}

#[test]
fn quickstart_guide_works_over_the_landscape_pdf_menu_and_tracks_no_extra_book() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Guide reference.pdf");
    fs::write(&pdf, crate::fixed_session::tests::striped_pdf_bytes(4)).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&pdf).unwrap();
    app.follow_fixed(2, None).unwrap();
    app.save().unwrap();
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let initial_position = app.current_history_location();
    let library_bytes = fs::read(&fixture.state).unwrap();
    open_from_book(&mut app);
    assert_eq!(app.screen, Screen::FixedTools);
    let layout = app.navigation_layout();
    assert!(layout.back().x > WIDTH as i32);
    let gesture = Input::Swipe {
        dx: -340.,
        dy: 40.,
        start_y: 830.,
    };
    assert_eq!(app.swipe(gesture, 950.), Change::Partial);
    assert_eq!(app.quickstart.unwrap().page, 1);
    let pager = app.quickstart.unwrap().pager(layout);
    assert_eq!(
        tap(&mut app, pager.button(PageTurn::Next).unwrap()),
        Change::Partial
    );
    assert_eq!(app.quickstart.unwrap().page, 2);
    for _ in 0..2 {
        assert_eq!(app.input(Input::PageForward), Change::Partial);
    }
    assert_eq!(app.quickstart.unwrap().page, 4);
    capture(
        &app,
        &mut canvas,
        "quickstart-reading-view-landscape-dark.png",
        true,
    );
    assert_eq!(tap(&mut app, layout.back()), Change::Full);
    assert_eq!(app.screen, Screen::FixedTools);
    assert_eq!(app.current_history_location(), initial_position);
    assert_eq!(fs::read(&fixture.state).unwrap(), library_bytes);
    assert_eq!(app.library.recents().len(), 1);
}

#[test]
fn quickstart_guide_first_launch_and_documents_route_are_separate() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.show_first_run_quickstart(&fixture.state).unwrap();
    let layout = app.navigation_layout();
    assert_eq!(app.screen, Screen::Library);
    assert_eq!(app.quickstart.unwrap().page, 0);
    assert!(fixture.root.join(SEEN_FILE).is_file());
    assert_eq!(tap(&mut app, layout.back()), Change::Full);
    assert_eq!(app.screen, Screen::Library);

    let mut restarted = reader(&fixture);
    restarted.show_first_run_quickstart(&fixture.state).unwrap();
    assert!(restarted.quickstart.is_none());
    assert_eq!(
        restarted.input(Input::Tap(WIDTH as f32 / 2., 250.)),
        Change::Partial
    );
    assert_eq!(restarted.screen, Screen::ReadingControls);
    let entry = restarted.list_geometry().row(0);
    assert_eq!(tap(&mut restarted, entry), Change::Full);
    assert_eq!(tap(&mut restarted, layout.back()), Change::Full);
    assert_eq!(restarted.screen, Screen::ReadingControls);

    let existing = Fixture::new();
    fs::write(existing.root.join("reading-controls.json"), b"{}").unwrap();
    let mut existing_reader = reader(&existing);
    existing_reader
        .show_first_run_quickstart(&existing.state)
        .unwrap();
    assert!(existing_reader.quickstart.is_none());
    assert!(!existing.root.join(SEEN_FILE).exists());
}

#[test]
fn quickstart_guide_text_fits_both_supported_orientations() {
    let fixture = Fixture::new();
    let app = reader(&fixture);
    for viewport in [ReadingViewport::PORTRAIT, ReadingViewport::LANDSCAPE] {
        let (width, _) = viewport.dimensions();
        let mut canvas = canvas(&app, viewport);
        let paragraph = Rect::new(0, 0, width as i32 - 2 * LIST_X - 64, 148);
        for topic in &TOPICS {
            for (section, text) in topic.sections {
                assert_eq!(
                    canvas.text_pages(paragraph, text, BODY.lines(3)),
                    [0..text.len()],
                    "{section}",
                );
            }
        }
    }
}
