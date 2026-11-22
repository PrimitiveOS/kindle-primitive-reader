use std::env;
use std::path::{Path, PathBuf};

use reader::Result;
use reader::app::{run_probe, run_reader};

const USAGE: &str = "Usage: reader-kindle [--open <document> | --check-document <document> <new scratch directory>]";

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let first = args.next();
    if first
        .as_deref()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--lookup-worker")) && args.len() == 0 {
        return reader::online_lookup::worker();
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--sync-worker")) && args.len() == 0 {
        return reader::progress_sync::worker();
    }
    let home = env::var_os("PRIMITIVE_READER_HOME")
        .map_or_else(|| PathBuf::from("/mnt/us/reader"), PathBuf::from);
    let documents = env::var_os("PRIMITIVE_READER_DOCUMENTS")
        .map_or_else(|| PathBuf::from("/mnt/us/documents"), PathBuf::from);
    if first.as_deref() == Some(std::ffi::OsStr::new("--check-document")) {
        let document = args.next().map(PathBuf::from).ok_or(USAGE)?;
        let scratch = args.next().map(PathBuf::from).ok_or(USAGE)?;
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        return reader::document_check::verify(
            &documents,
            &document,
            &scratch,
            reading_fonts(&home),
            true,
        );
    }
    if env::var_os("KL_NATIVE_APP_RUN").is_none() {
        return Err("Open Reader from PrimitiveOS Launcher".into());
    }
    if let Some(engine) = first
        .as_deref()
        .and_then(|arg| arg.to_str())
        .and_then(|arg| arg.parse().ok())
    {
        let book = args.next().map(PathBuf::from).ok_or(USAGE)?;
        let fonts = args.map(PathBuf::from).collect::<Vec<_>>();
        if fonts.is_empty() {
            return Err("Provide at least one reading font".into());
        }
        return run_probe(engine, book, fonts, &home);
    }
    let file = match first.as_deref() {
        None => None,
        Some(arg) if arg == "--open" => Some(args.next().map(PathBuf::from).ok_or(USAGE)?),
        _ => return Err(USAGE.into()),
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    run_reader(&home, &documents, reading_fonts(&home), file.as_deref())
}

fn reading_fonts(home: &Path) -> Vec<PathBuf> {
    let directory = [
        home.join("fonts"),
        PathBuf::from("/var/tmp/kl/fonts"),
        PathBuf::from("/mnt/us/launcher/fonts"),
    ]
    .into_iter()
    .find(|directory| directory.join("NotoSans-Regular.ttf").is_file());
    directory
        .into_iter()
        .flat_map(|path| {
            [
                path.join("NotoSans-Regular.ttf"),
                path.join("NotoSans-Bold.ttf"),
            ]
        })
        .chain([
            PathBuf::from("/usr/java/lib/fonts/NotoNaskh-Regular.ttf"),
            PathBuf::from("/usr/java/lib/fonts/TBGothicMed_213.ttf"),
            PathBuf::from("/usr/java/lib/fonts/code2000.ttf"),
        ])
        .filter(|path| path.is_file())
        .collect()
}
