use std::fs;
use std::path::PathBuf;

use primitive_app::{App, HEIGHT, WIDTH, frame, write_png};
use primitive_ui::{Canvas, Input};
use reader::Result;
use reader::app::Reader;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let usage = "Usage: desktop <books directory> <book filename> <output directory> <Noto Sans font> [fallback fonts...]";
    let documents = args.next().ok_or(usage)?;
    let book = args.next().ok_or(usage)?;
    let output = args.next().ok_or(usage)?;
    let fonts: Vec<_> = args.collect();
    let canvas_fonts = fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    fs::create_dir_all(&output)?;
    let mut app = Reader::open(&documents, &output.join("preview-state.json"), fonts)?;
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &canvas_fonts)?;
    write_png(
        &output.join("library.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    app.open_path(&book)?;
    write_png(
        &output.join("book.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    app.input(Input::Tap(WIDTH as f32 / 2.0, 28.0));
    write_png(
        &output.join("reading-controls.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    app.input(Input::Tap(112.0, HEIGHT as f32 - 45.0));
    write_png(
        &output.join("contents.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    app.open_path(&book)?;
    app.swipe(
        Input::Swipe {
            dx: 0.0,
            dy: 280.0,
            start_y: 28.0,
        },
        45.0,
    );
    write_png(
        &output.join("navigation.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    write_png(
        &output.join("navigation-dark.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, true),
    )?;
    app.swipe(
        Input::Swipe {
            dx: 0.0,
            dy: -280.0,
            start_y: HEIGHT as f32 - 28.0,
        },
        WIDTH as f32 / 2.0,
    );
    write_png(
        &output.join("page-settings.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    Ok(())
}
