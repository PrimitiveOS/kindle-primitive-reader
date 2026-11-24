//! Exercise interactive selection chrome through the negotiated viewport contract.
use super::{App, Canvas, Change, Input, Layout, Reader, Rect, WIDTH, fs};
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision};
use primitive_ui::Key;
use std::path::Path;

pub(super) fn canvas(app: &Reader, viewport: ReadingViewport) -> Canvas {
    let (width, height) = viewport.dimensions();
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    Canvas::new_viewport(width, height, WIDTH, &fonts).unwrap()
}

pub(super) fn adopt(app: &mut Reader, viewport: ReadingViewport, generation: u64) -> Canvas {
    let mut canvas = canvas(app, viewport);
    let applied = Viewport {
        generation,
        turn: if viewport == ReadingViewport::LANDSCAPE {
            LogicalTurn::Clockwise
        } else {
            LogicalTurn::Upright
        },
        logical_size: viewport.dimensions(),
    };
    app.stage_viewport(applied).unwrap();
    canvas.begin();
    app.draw_staged_viewport(&mut canvas).unwrap();
    assert_eq!(
        canvas.finish(false).len(),
        (canvas.bounds().w * canvas.bounds().h) as usize
    );
    app.finish_viewport(applied, ViewportDecision::Commit);
    assert_eq!(app.reading_viewport(), viewport);
    assert_eq!(canvas.px(100), 100);
    canvas
}

pub(super) fn tap(app: &mut Reader, area: Rect) {
    assert_eq!(
        app.input(Input::Tap(
            (area.x + area.w / 2) as f32,
            (area.y + area.h / 2) as f32
        )),
        Change::Partial
    );
}

pub(super) fn key(app: &Reader, key: Key, action: &str) -> Rect {
    let (width, height) = app.reading_viewport().dimensions();
    Layout::new(width, height, 4, 5)
        .keyboard_keys_with(app.editor.layer, action)
        .into_iter()
        .find(|(candidate, _)| candidate.key == key)
        .unwrap()
        .1
}

pub(super) fn press(app: &Reader, canvas: &mut Canvas, rect: Rect, dark: bool) {
    let before = primitive_app::frame(app, canvas, dark);
    assert!(canvas.press_at((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32));
    let after = primitive_app::frame(app, canvas, dark);
    canvas.clear_press();
    let width = canvas.bounds().w as usize;
    assert!(
        before.iter().zip(after).enumerate().any(|(index, (a, b))| {
            *a != b && rect.contains((index % width) as f32, (index / width) as f32)
        }),
        "missing press feedback at {rect:?}"
    );
}

pub(super) fn capture(app: &Reader, canvas: &mut Canvas, name: &str, dark: bool) {
    if let Some(dir) = std::env::var_os("READER_SELECTION_VIEWPORT_CAPTURES") {
        fs::create_dir_all(&dir).unwrap();
        let (width, height) = app.reading_viewport().dimensions();
        primitive_app::write_png(
            &Path::new(&dir).join(name),
            width,
            height,
            &primitive_app::frame(app, canvas, dark),
        )
        .unwrap();
    }
}
