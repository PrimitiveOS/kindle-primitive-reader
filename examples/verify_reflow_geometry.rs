//! Verify publisher reading geometry without printing book text, excerpts or filenames.
use reader::{Anchor, Engine, Probe, Settings, epub::Book, search::Search};
use serde::Serialize;
use std::{
    env,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, &'static str>;

fn terms(path: &Path, count: usize) -> Result<Vec<String>> {
    let mut s = String::new();
    File::open(path)
        .map_err(|_| "query file unavailable")?
        .take(4097)
        .read_to_string(&mut s)
        .map_err(|_| "invalid query file")?;
    let items: Vec<_> = s.lines().map(str::trim).map(str::to_owned).collect();
    if s.len() > 4096 || items.len() != count || items.iter().any(|s| s.is_empty() || s.len() > 512)
    {
        return Err("invalid number or length of query lines");
    }
    Ok(items)
}
fn open(path: &Path, chapter: usize, fonts: &[PathBuf]) -> Result<Probe> {
    Probe::open(path, Engine::Parley, fonts, Settings::default(), chapter)
        .map_err(|_| "publication or section unavailable")
}
#[derive(Serialize)]
struct ListPage {
    page: usize,
    pages: usize,
    source_characters: usize,
    covered_characters: usize,
    missing_glyphs: usize,
    layout_matches: usize,
}
fn list_page(
    path: &Path,
    chapter: usize,
    fonts: &[PathBuf],
    term: &str,
) -> Result<(ListPage, Vec<u8>)> {
    let mut probe = open(path, chapter, fonts)?;
    let matches = probe.matches(term);
    let [anchor] = matches.as_slice() else {
        return Err("list query must identify exactly one passage in the section");
    };
    let page = probe.page_for(anchor).ok_or("no page for list passage")?;
    let pixels = probe
        .capture(page)
        .map_err(|_| "list page could not be painted")?;
    let (covered_characters, source_characters) = probe.coverage();
    Ok((
        ListPage {
            page: page + 1,
            pages: probe.page_count(),
            source_characters,
            covered_characters,
            missing_glyphs: probe.missing_glyphs(),
            layout_matches: matches.len(),
        },
        pixels,
    ))
}
fn differences(reference: &[u8], other: &[u8]) -> Result<usize> {
    if reference.len() != other.len() {
        return Err("incomparable frame dimensions");
    }
    Ok(reference.iter().zip(other).filter(|(a, b)| a != b).count())
}
fn list(args: &[PathBuf], chapter: usize) -> Result<serde_json::Value> {
    // [list, chapter, terms, original, start-control, type-control, fonts...]
    if args.len() < 7 {
        return Err("list requires three publications and at least one font");
    }
    let query = &terms(&args[2], 1)?[0];
    let fonts = &args[6..];
    let (original, reference) = list_page(&args[3], chapter, fonts, query)?;
    let (start, start_pixels) = list_page(&args[4], chapter, fonts, query)?;
    let start_changed = differences(&reference, &start_pixels)?;
    drop(start_pixels);
    let (kind, kind_pixels) = list_page(&args[5], chapter, fonts, query)?;
    let kind_changed = differences(&reference, &kind_pixels)?;
    drop(kind_pixels);
    let mut book = Book::open(&args[3]).map_err(|_| "publication unavailable for book search")?;
    let mut search = Search::new(query).map_err(|_| "invalid book search query")?;
    let mut source_matches = 0;
    for n in 0..=chapter {
        let hits = search.step(&mut book).map_err(|_| "book search failed")?;
        if n == chapter {
            source_matches = hits.len();
        }
    }
    Ok(
        serde_json::json!({"mode":"list","original":original,"start_control":start,"type_control":kind,"source_search_hits_in_section":source_matches,"frame_pixels":reference.len(),"start_changed_pixels":start_changed,"type_changed_pixels":kind_changed}),
    )
}
#[derive(Serialize)]
struct Cell {
    page: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
fn table(args: &[PathBuf], chapter: usize) -> Result<serde_json::Value> {
    // [table, chapter, terms, original, fonts...]
    if args.len() < 5 {
        return Err("table requires a publication and at least one font");
    }
    let terms = terms(&args[2], 3)?;
    let mut probe = open(&args[3], chapter, &args[4..])?;
    let hits: Vec<_> = terms.iter().map(|t| probe.matches(t)).collect();
    let mut closest = None;
    // Header labels may also occur in body text. Select the nearest three ordered source blocks.
    for first in &hits[0] {
        let Some(second) = hits[1].get(hits[1].partition_point(|a| a.block <= first.block)) else {
            continue;
        };
        let Some(third) = hits[2].get(hits[2].partition_point(|a| a.block <= second.block)) else {
            continue;
        };
        let span = third.block - first.block;
        if closest.as_ref().is_none_or(|(best, _)| span < *best) {
            closest = Some((span, [first, second, third]));
        }
    }
    let (_, row) = closest.ok_or("no row with the three ordered source headers")?;
    let mut cells = Vec::new();
    for (anchor, term) in row.into_iter().zip(&terms) {
        let page = probe.page_for(anchor).ok_or("no page for table cell")?;
        let end = Anchor {
            raw_caret: None,
            offset: anchor.offset + term.len(),
            ..anchor.clone()
        };
        let selection = probe
            .select_range(anchor, &end)
            .ok_or("table text could not be selected")?;
        let rects = probe.selection_rects(page, &selection);
        let rect = rects.first().ok_or("table cell has no visible rectangle")?;
        std::hint::black_box(
            probe
                .capture(page)
                .map_err(|_| "table page could not be painted")?,
        );
        cells.push(Cell {
            page: page + 1,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
    }
    let shared_row = cells.windows(2).all(|pair| {
        pair[0].page == pair[1].page
            && pair[0].x < pair[1].x
            && pair[0].y < pair[1].y + pair[1].height
            && pair[1].y < pair[0].y + pair[0].height
    });
    Ok(serde_json::json!({"mode":"table","cells":cells,"shared_row":shared_row}))
}
fn run() -> Result<serde_json::Value> {
    let args: Vec<_> = env::args_os().skip(1).map(PathBuf::from).collect();
    let chapter = args
        .get(1)
        .and_then(|v| v.to_str())
        .and_then(|v| v.parse::<usize>().ok())
        .and_then(|v| v.checked_sub(1))
        .ok_or("provide a 1-based section number")?;
    match args.first().and_then(|v| v.to_str()) {
        Some("list") => list(&args, chapter),
        Some("table") => table(&args, chapter),
        _ => Err("choose list or table"),
    }
}
fn main() {
    match run() {
        Ok(data) => println!("{data}"),
        Err(error) => {
            eprintln!("Reader geometry verification failed: {error}");
            std::process::exit(1);
        }
    }
}
