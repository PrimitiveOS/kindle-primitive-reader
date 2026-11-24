use super::*;
use crate::app::{Action, Destination, ReadingOrientation, tests::Fixture};
use primitive_app::{
    App, HEIGHT, WIDTH,
    viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest},
};
use primitive_ui::Rect;
use std::{fs, path::Path};

fn reader(fixture: &Fixture) -> Reader {
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

fn viewport(turn: LogicalTurn, generation: u64) -> Viewport {
    let logical_size = match turn {
        LogicalTurn::Clockwise | LogicalTurn::Counterclockwise => (HEIGHT, WIDTH),
        _ => (WIDTH, HEIGHT),
    };
    Viewport {
        turn,
        generation,
        logical_size,
    }
}

fn canvas(app: &Reader, (width, height): (u32, u32)) -> Canvas {
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    Canvas::new_viewport(width, height, WIDTH, &fonts).unwrap()
}

fn stage(app: &mut Reader, target: Viewport, decision: ViewportDecision) -> Vec<u8> {
    let mut canvas = canvas(app, target.logical_size);
    app.stage_viewport(target).unwrap();
    canvas.begin();
    app.draw_staged_viewport(&mut canvas).unwrap();
    let frame = canvas.finish(false);
    app.finish_viewport(target, decision);
    frame
}

fn view(app: &Reader) -> &FixedReading {
    app.illustration.as_ref().unwrap()
}

fn frame(app: &Reader) -> Vec<u8> {
    let mut canvas = canvas(app, app.reading_viewport().dimensions());
    primitive_app::frame(app, &mut canvas, false)
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn pan(app: &mut Reader, dx: f32, dy: f32) {
    assert_eq!(
        app.input(Input::Swipe {
            dx,
            dy,
            start_y: 640.0
        }),
        Change::Partial
    );
}

#[test]
fn illustration_viewport_retains_an_authored_image_focus_and_the_owning_epub() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("gif-signals.epub"), &fixture.book).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let opening = app.current_position().unwrap();
    let history = app.back.clone();
    let book = app.open_book_id();
    let destination = Destination {
        path: "EPUB/art/flags.gif".into(),
        fragment: None,
    };
    app.activate(Action::Jump(destination)).unwrap();
    assert_eq!(app.screen, Screen::Illustration);
    for _ in 0..3 {
        assert_eq!(app.input(tap(view(&app).zoom_buttons().1)), Change::Partial);
    }
    let initial = view(&app).page_center_source().unwrap().1;
    pan(&mut app, -112.0, 78.0);
    let focused = view(&app).page_center_source().unwrap().1;
    assert!((focused - initial).hypot() > 3.0);
    let portrait = frame(&app);

    let wide = viewport(LogicalTurn::Clockwise, 1);
    let candidate = stage(&mut app, wide, ViewportDecision::Abort);
    assert_eq!(view(&app).viewport().dimensions(), (WIDTH, HEIGHT));
    assert_eq!(frame(&app), portrait);
    assert_eq!(app.current_position(), Some(opening.clone()));
    assert_eq!(view(&app).page_center_source().unwrap().1, focused);

    app.controls.sticky_orientation = Some(ReadingOrientation::LandscapeRight);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(wide.turn))
    );
    assert_eq!(stage(&mut app, wide, ViewportDecision::Commit), candidate);
    assert_eq!(frame(&app), candidate);
    assert_eq!(view(&app).viewport().dimensions(), (HEIGHT, WIDTH));
    assert_eq!(view(&app).page_center_source().unwrap().1, focused);
    assert_eq!(app.current_position().unwrap().anchor, opening.anchor);
    let (out, plus) = view(&app).zoom_buttons();
    assert!(plus.x > WIDTH as i32);
    assert_eq!(app.input(tap(plus)), Change::Partial);
    assert_eq!(view(&app).page_center_source().unwrap().1, focused);
    assert_ne!(frame(&app), candidate);

    // A new pan invalidates an already prepared image and its owning-book transition.
    let upright = viewport(LogicalTurn::Upright, 2);
    app.stage_viewport(upright).unwrap();
    pan(&mut app, 78.0, -42.0);
    let revised = view(&app).page_center_source().unwrap().1;
    assert!((revised - focused).hypot() > 1.0);
    assert!(
        app.draw_staged_viewport(&mut canvas(&app, upright.logical_size))
            .is_err()
    );
    app.finish_viewport(upright, ViewportDecision::Commit);
    assert_eq!(app.accepted_viewport, Some(wide));
    assert_eq!(app.reading_viewport().dimensions(), (HEIGHT, WIDTH));
    assert_eq!(view(&app).page_center_source().unwrap().1, revised);
    let other = viewport(LogicalTurn::Counterclockwise, 3);
    stage(&mut app, other, ViewportDecision::Commit);
    assert_eq!(view(&app).page_center_source().unwrap().1, revised);
    for _ in 0..4 {
        assert_eq!(app.input(tap(out)), Change::Partial);
    }
    assert_eq!(view(&app).page_center_source().unwrap().1, revised);
    let art = frame(&app);
    let footer = view(&app).viewport().page_height() as usize * HEIGHT as usize;
    assert!(
        art[footer..]
            .chunks_exact(HEIGHT as usize)
            .any(|row| row[38..300].iter().any(|&ink| ink < 160))
    );
    if let Some(path) = std::env::var_os("READER_ILLUSTRATION_LANDSCAPE_CAPTURE") {
        primitive_app::write_png(Path::new(&path), HEIGHT, WIDTH, &art).unwrap();
    }
    let close = view(&app).viewport().done();
    assert!(close.x > WIDTH as i32);
    assert_eq!(app.input(tap(close)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);
    assert!(app.illustration.is_none());
    assert_eq!(app.open_book_id(), book);
    assert_eq!(app.current_position().unwrap().anchor, opening.anchor);
    assert_eq!(app.back, history);
    assert!(!app.viewport_needs_stage(other));
    stage(
        &mut app,
        viewport(LogicalTurn::Upright, 4),
        ViewportDecision::Commit,
    );
    assert_eq!(app.current_position(), Some(opening));
}

#[test]
fn illustration_viewport_works_for_a_cover_without_an_active_book() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("gif-signals.epub"), &fixture.book).unwrap();
    let mut app = reader(&fixture);
    let book = app.library.resolve(&fixture.book).unwrap();
    app.organize_book(book, Screen::Library).unwrap();
    app.activate(Action::ViewCover).unwrap();
    assert_eq!(app.screen, Screen::Illustration);
    assert!(app.open_book_id().is_none());
    let wide = viewport(LogicalTurn::Clockwise, 1);
    app.controls.sticky_orientation = Some(ReadingOrientation::LandscapeRight);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(wide.turn))
    );
    assert_eq!(stage(&mut app, wide, ViewportDecision::Commit), frame(&app));
    assert_eq!(view(&app).viewport().dimensions(), (HEIGHT, WIDTH));
    assert_eq!(
        app.input(tap(view(&app).viewport().done())),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::BookDetails);
    assert!(app.open_book_id().is_none());
    assert!(app.library.recently_read().next().is_none());
}

#[test]
fn illustration_viewport_opens_a_comic_panel_already_wide() {
    use crate::fixed_session::tests::ComicFixture;

    let fixture = Fixture::new();
    let path = fixture.root.join("Four panels.cbz");
    fs::copy(&ComicFixture::panels().0, &path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    let wide = viewport(LogicalTurn::Counterclockwise, 1);
    stage(&mut app, wide, ViewportDecision::Commit);
    let original = app.current_position();
    let (width, _) = wide.logical_size;
    let page_height = app.reading_viewport().page_height() as f32;
    let scale = (width as f32 / 320.0).min(page_height / 480.0);
    let x = (width as f32 - 320.0 * scale) / 2.0 + 230.0 * scale;
    let y = (page_height - 480.0 * scale) / 2.0 + 60.0 * scale;
    assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    assert_eq!(app.screen, Screen::Illustration);
    assert_eq!(view(&app).viewport().dimensions(), (HEIGHT, WIDTH));
    assert!(!app.viewport_needs_stage(wide));
    assert_eq!(
        app.input(tap(view(&app).viewport().done())),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Fixed);
    assert_eq!(app.current_position(), original);
}
