use std::fs;
use std::path::{Path, PathBuf};

use primitive_app::{App, HEIGHT, WIDTH, frame, write_png};
use primitive_ui::{Canvas, Input};
use reader::Result;
use reader::app::Reader;

fn main() -> Result<()> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--lookup-worker")) {
        return reader::online_lookup::worker();
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--wiki-images-worker")) {
        return reader::online_lookup::image_worker();
    }
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
    app.open_path(&book)?;
    app.input(Input::Tap(WIDTH as f32 / 2.0, 30.0));
    app.input(Input::Tap(525.0, 152.0)); // Settings tab.
    app.input(Input::Tap(170.0, 285.0)); // Reading gestures.
    write_png(
        &output.join("reading-gestures.png"),
        WIDTH,
        HEIGHT,
        &frame(&app, &mut canvas, false),
    )?;
    capture_footer(&mut app, &mut canvas, &book, &output)?;
    Ok(())
}

fn capture_footer(app: &mut Reader, canvas: &mut Canvas, book: &Path, output: &Path) -> Result<()> {
    app.open_path(book)?;
    let tap = Input::Tap(200.0, HEIGHT as f32 - 36.0);
    for name in ["footer-progress", "footer-chapter"] {
        app.input(tap);
        write_png(
            &output.join(format!("{name}.png")),
            WIDTH,
            HEIGHT,
            &frame(app, canvas, false),
        )?;
    }
    // Complete the cycle so a repeated capture retains the same starting indicator.
    for _ in 0..3 {
        app.input(tap);
    }
    Ok(())
}
