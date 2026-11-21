//! Exercise the normal opener, first frame, page button and durable state without modifying a book.
use primitive_app::{App, HEIGHT, WIDTH, frame};
use primitive_ui::{Canvas, Input};
use reader::{
    Engine, Probe, Result, Settings,
    app::Reader,
    library::{BrowseKind, DocumentFormat, Library},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn select_in_picker(app: &mut Reader, library: &Library, relative: &Path) -> Result<()> {
    // The fresh validation profile has no "Continue reading" row. Drive the same ten-row
    // library and physical page-key handlers that a user follows from Launcher.
    let mut folder = PathBuf::new();
    for component in relative.components() {
        let listing = library.browse(&folder)?;
        folder.push(component);
        let (index, entry) = listing
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| e.path == folder)
            .ok_or("Document or directory is missing from Reader's picker")?;
        if matches!(entry.kind, BrowseKind::KindleProtected { .. }) {
            return Err("Protected Kindle book: open in Kindle Library".into());
        }
        if let BrowseKind::Unavailable { too_large, .. } = entry.kind {
            return Err(if too_large {
                "Picker reports this file exceeds its format limit"
            } else if entry
                .path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("kfx"))
            {
                "KFX: open in Kindle Library"
            } else {
                "Picker reports an unsupported format"
            }
            .into());
        }
        for _ in 0..index / 10 {
            app.input(Input::PageForward);
        }
        app.input(Input::Tap(410.0, (364 + 116 * (index % 10)) as f32));
    }
    Ok(())
}

/// Exercise later chapters without persisting a page or collecting any book text.
fn verify_spine(path: &Path, fonts: &[PathBuf], all: bool) -> Result<serde_json::Value> {
    let package = reader::epub::Book::open(path)?;
    let count = package.chapters.len();
    let fixed = package.is_fixed_layout();
    drop(package);
    let mut indices = if all {
        (0..count).collect::<Vec<_>>()
    } else {
        vec![0, count / 2, count.saturating_sub(1)]
    };
    indices.dedup();
    let fixed = if fixed {
        reader::fixed_epub::FixedEpub::open(path, fonts)?
    } else {
        None
    };
    let settings = Settings {
        width: WIDTH,
        height: HEIGHT - 74,
        ..Settings::default()
    };
    let mut failed = Vec::new();
    for &index in &indices {
        let result = (|| -> Result<()> {
            if let Some(book) = &fixed {
                book.decode_page(index)?;
            } else {
                let mut chapter = Probe::open(path, Engine::Parley, fonts, settings, index)?;
                chapter.capture(0)?;
                if chapter.page_count() > 1 {
                    chapter.capture(chapter.page_count() - 1)?;
                }
            }
            Ok(())
        })();
        if result.is_err() {
            failed.push(index);
        }
    }
    Ok(serde_json::json!({"chapters":count, "checked":indices.len(), "failed_indices":failed}))
}

fn verify() -> Result<()> {
    let mut args = env::args_os().skip(1).map(PathBuf::from);
    let usage =
        "verify_document <documents root> <book path> <new scratch directory> <font> [font...]";
    let root = args.next().ok_or(usage)?;
    let path = args.next().ok_or(usage)?;
    let scratch = args.next().ok_or(usage)?;
    let fonts: Vec<_> = args.collect();
    if fonts.is_empty() {
        return Err(usage.into());
    }
    fs::create_dir(&scratch)?;
    let state = scratch.join("library.json");
    let paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths)?;
    let start = Instant::now();
    let mut app = Reader::open(&root, &state, fonts.clone())?;
    let picker = env::var_os("READER_VERIFY_PICKER").is_some();
    if picker {
        let library = Library::open(&root, &state)?;
        select_in_picker(
            &mut app,
            &library,
            path.strip_prefix(&root).unwrap_or(&path),
        )?;
    } else {
        app.open_path(&path)?;
    }
    let pixels = frame(&app, &mut canvas, false);
    if let Some(destination) = env::var_os("READER_VERIFY_FRAME") {
        primitive_app::write_png(Path::new(&destination), WIDTH, HEIGHT, &pixels)?;
    }
    let first_ms = start.elapsed().as_millis();
    let ink = pixels.iter().filter(|&&gray| gray < 128).count();
    app.input(Input::PageForward);
    app.tick(Instant::now() + Duration::from_secs(2));
    let second = frame(&app, &mut canvas, false);
    let changed = pixels.iter().zip(&second).any(|(a, b)| a != b);
    app.save()?;
    let index = Library::open(&root, &state)?;
    let selected = index.resolve(&path)?;
    let entry = index
        .publication_for_chapter(&selected)
        .map(|(publication, _, _)| publication)
        .unwrap_or(selected);
    let located = index
        .state(index.identify(&entry)?)
        .and_then(|book| book.position.as_ref());
    if located.is_none() {
        // Report the underlying opener's diagnostic if the picker showed an inline error.
        if picker {
            app.open_path(&path)?;
        }
        return Err("Reader did not enter a document (a password prompt may be open)".into());
    }
    let position = located.cloned();
    drop(app);
    // Restart from the persisted user-facing Continue reading action. An ephemeral
    // first-frame success would otherwise mask resume failures in large documents.
    let mut resumed = Reader::open(&root, &state, fonts.clone())?;
    resumed.input(Input::Tap(410.0, 364.0));
    let resumed_frame = frame(&resumed, &mut canvas, false);
    let same_page_pixels = second
        .chunks_exact(WIDTH as usize)
        .zip(resumed_frame.chunks_exact(WIDTH as usize))
        .skip(180)
        .take(HEIGHT as usize - 400)
        .all(|(left, right)| left == right);
    if !same_page_pixels {
        return Err("Continue reading did not restore the same page image".into());
    }
    resumed.save()?;
    let saved = Library::open(&root, &state)?;
    if saved
        .state(saved.identify(&entry)?)
        .and_then(|s| s.position.as_ref())
        != position.as_ref()
    {
        return Err("Continue reading changed the saved position".into());
    }
    drop(resumed);
    drop(canvas);
    drop(resumed_frame);
    drop(second);
    drop(pixels);
    let spine = match env::var("READER_VERIFY_SPINE") {
        Ok(mode) if matches!(entry.format, DocumentFormat::Epub | DocumentFormat::Mobi) => Some(
            verify_spine(&saved.absolute_path(&entry.path)?, &fonts, mode == "all")?,
        ),
        _ => None,
    };
    let peak_kib = fs::read_to_string("/proc/self/status").ok().and_then(|s| {
        s.lines().find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
    });
    println!(
        "{}",
        serde_json::json!({"opened":true,"picker":picker,"first_frame_ms": first_ms, "first_frame_ink_pixels":ink, "next_frame_changed":changed, "resumed":true, "peak_kib":peak_kib, "spine":spine})
    );
    Ok(())
}

fn main() {
    if let Err(error) = verify() {
        println!(
            "{}",
            serde_json::json!({"opened":false,"error":error.to_string()})
        );
        std::process::exit(1);
    }
}
