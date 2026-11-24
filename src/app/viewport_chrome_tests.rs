use super::tests::{Fixture, capture_test_screen};
use super::*;
use crate::reading_controls::{FooterTrackPosition, ProgressScope, SkimPosition};
use crate::session::ReadingViewport;

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn open_app(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap()
}

fn capture(app: &Reader, key: &str) {
    let Some(path) = std::env::var_os(key) else {
        return;
    };
    let (width, height) = app.reading_viewport().dimensions();
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new_viewport(width, height, WIDTH, &fonts).unwrap();
    let page = primitive_app::frame(app, &mut canvas, false);
    primitive_app::write_png(Path::new(&path), width, height, &page).unwrap();
}

#[test]
fn reading_footer_and_skim_follow_the_prepared_reflow_viewport() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let mut app = open_app(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(0, 2)
        .unwrap();
    app.controls.footer_scope = ProgressScope::Book;
    app.controls.footer_track_position = FooterTrackPosition::Below;
    let original = app.current_position().unwrap();
    capture_test_screen(&app, &app.fonts, "READER_VIEWPORT_CHROME_PORTRAIT", false);

    let viewport = ReadingViewport::LANDSCAPE;
    let reading = &mut app.active.as_mut().unwrap().reading;
    *reading = reading.prepare_viewport(viewport).unwrap();
    assert_eq!(app.reading_viewport(), viewport);
    let (width, height) = viewport.dimensions();
    let exit = viewport.done();
    assert!(exit.right() as u32 <= width && exit.bottom() as u32 <= height);
    let track = app.footer_track_rect(180).unwrap();
    assert_eq!(track.y, height as i32 - 12);
    assert!(track.right() < exit.x);
    assert_eq!(app.page_paint().visible, 0..viewport.page_height());
    capture(&app, "READER_VIEWPORT_CHROME_LANDSCAPE");

    let lower_swipe = Input::Swipe {
        dx: 0.0,
        dy: -145.0,
        start_y: height as f32 - 22.0,
    };
    assert_eq!(
        app.reading_edge_swipe(lower_swipe, width as f32 / 2.0),
        Some(Change::Partial)
    );
    let toolbar_exit = app.reading_button("Exit");
    assert!(toolbar_exit.x > 1264 && toolbar_exit.bottom() as u32 <= height);
    capture(&app, "READER_VIEWPORT_CHROME_BARS");
    app.show(Screen::Book);

    app.controls.skim_position = SkimPosition::Bottom;
    app.activate(Action::Skim).unwrap();
    let skim = app.skim_layout();
    assert!(skim.panel.bottom() <= viewport.page_height() as i32);
    assert!(skim.track.right() > 1264);
    capture(&app, "READER_VIEWPORT_CHROME_SKIM");
    assert_eq!(app.input(tap(skim.cancel)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);

    let reading = &mut app.active.as_mut().unwrap().reading;
    *reading = reading.prepare_viewport(ReadingViewport::PORTRAIT).unwrap();
    assert_eq!(app.current_position().as_ref(), Some(&original));
    assert_eq!(app.reading_viewport().done(), DONE);
}

#[test]
fn fixed_chrome_and_footer_use_the_resized_zoom_and_exit_hit_areas() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Tilted Scan.pdf");
    fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
    let mut app = open_app(&fixture);
    app.open_path(&pdf).unwrap();
    let reading = &mut app.fixed.as_mut().unwrap().reading;
    let viewport = ReadingViewport::LANDSCAPE;
    let candidate = reading.prepare_viewport(viewport).unwrap();
    reading.commit_viewport(candidate).unwrap();
    let zoom = reading.zoom_buttons().0;
    assert_eq!(zoom.y, viewport.done().y);
    assert!(zoom.x > 1264 - 20);
    let track = app.footer_track_rect(168).unwrap();
    assert!(track.right() < zoom.x);
    capture(&app, "READER_VIEWPORT_CHROME_FIXED");
    assert_eq!(app.input(tap(viewport.done())), Change::Exit);
}
