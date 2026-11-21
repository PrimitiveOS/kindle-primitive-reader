//! Compare two reflows on one local book using only the public Reader interfaces.
//! Text and rendered pixels stay in the supplied private scratch directory.
//! The report contains numerical evidence. Each measured mode uses a fresh process.
use reader::{
    Anchor, Engine, Probe, Settings, epub::Book, search::Search, selection::TextSelection,
};
use serde_json::json;
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, &'static str>;

fn elapsed_ms(start: Instant) -> u128 {
    start.elapsed().as_millis()
}

fn peak_kib() -> Option<u64> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn cpu_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<_> = stat.rsplit_once(") ")?.1.split_whitespace().collect();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

fn xml_member(zip: &mut zip::ZipArchive<fs::File>, path: &str) -> Result<String> {
    use std::io::Read;
    let mut data = String::new();
    zip.by_name(path)
        .map_err(|_| "metadata member unavailable")?
        .take(1048577)
        .read_to_string(&mut data)
        .map_err(|_| "metadata unreadable")?;
    if data.len() > 1048576 {
        return Err("oversized metadata");
    }
    Ok(data)
}

// Check the publication independently, including inline declarations. Requiring
// every text node in the selected block to be English also covers split markup.
fn english_block(path: &Path, chapter: usize, block: usize, book: &mut Book) -> Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|_| "no original")?)
        .map_err(|_| "no ZIP")?;
    let container = xml_member(&mut zip, "META-INF/container.xml")?;
    let opf = roxmltree::Document::parse(&container).map_err(|_| "bad container")?;
    let package = opf
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("package unavailable")?;
    let metadata = xml_member(&mut zip, package)?;
    let package = roxmltree::Document::parse(&metadata).map_err(|_| "bad metadata")?;
    let primary = package
        .descendants()
        .find(|n| n.has_tag_name(("http://purl.org/dc/elements/1.1/", "language")))
        .and_then(|n| n.text())
        .map(str::trim);
    let xhtml = book
        .chapter(chapter)
        .map_err(|_| "missing source section")?;
    let doc = roxmltree::Document::parse(&xhtml).map_err(|_| "bad source section")?;
    let node = doc
        .descendants()
        .find(|n| n.is_element() && n.range().start == block)
        .ok_or("source block unavailable")?;
    let mut count = 0;
    for text in node
        .descendants()
        .filter(|n| n.is_text() && n.text().is_some_and(|s| !s.trim().is_empty()))
    {
        let tag = text
            .ancestors()
            .find_map(|n| {
                n.attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
                    .or_else(|| n.attribute("lang"))
            })
            .or(primary)
            .unwrap_or_default();
        if !matches!(
            tag.to_ascii_lowercase().as_str(),
            "en" | "en-us" | "en-latn" | "en-latn-us"
        ) {
            return Err("selected source contains a non-US-English run");
        }
        count += 1;
    }
    if count == 0 {
        return Err("empty source block");
    }
    Ok(())
}

fn negative(
    mut probe: Probe,
    path: &Path,
    chapter: usize,
    fonts: &[PathBuf],
    settings: Settings,
) -> Result<()> {
    let original = probe.page_anchor(0).ok_or("no source location")?;
    let automatic = probe.capture(0).map_err(|_| "automatic frame failed")?;
    let coverage = probe.coverage();
    let pages = probe.page_count();
    drop(probe);
    let mut authored = Probe::open(
        path,
        Engine::Parley,
        fonts,
        Settings {
            automatic_hyphenation: false,
            ..settings
        },
        chapter,
    )
    .map_err(|_| "authored negative control failed")?;
    let destination = authored
        .page_for(&original)
        .ok_or("unresolvable negative control position")?;
    if automatic
        != authored
            .capture(destination)
            .map_err(|_| "authored frame failed")?
        || coverage != authored.coverage()
        || pages != authored.page_count()
    {
        return Err("negative control changed");
    }
    println!(
        "{}",
        json!({"coverage":coverage,"pages":authored.page_count(),"different_frame_pixels":0,"negative_control":true})
    );
    Ok(())
}

fn discover(probe: &Probe, state: &Path, settings: Settings) -> Result<()> {
    let mut choices = Vec::<TextSelection>::new();
    for page in 0..probe.page_count().min(8) {
        for y in (settings.margin..settings.height - settings.margin).step_by(18) {
            for x in (settings.width / 2..settings.width - settings.margin)
                .step_by(24)
                .rev()
            {
                let Some(word) = probe.word_at(page, x as f32, y as f32) else {
                    continue;
                };
                if !(5..=64).contains(&word.text.len())
                    || !word.text.bytes().all(|b| b.is_ascii_alphabetic())
                    || choices.iter().any(|old| old.start == word.start)
                {
                    continue;
                }
                let rects = probe.selection_rects(page, &word);
                if rects.len() == 2
                    && rects[1].y > rects[0].y
                    && rects[0].width > 20
                    && rects[1].width > 20
                {
                    choices.push(word);
                    if choices.len() >= 24 {
                        break;
                    }
                }
            }
            if choices.len() >= 24 {
                break;
            }
        }
        if choices.len() >= 12 {
            break;
        }
    }
    if choices.is_empty() {
        return Err("no split word found in sampled pages");
    }
    fs::write(
        state.join("candidates.json"),
        serde_json::to_vec(&choices).unwrap(),
    )
    .map_err(|_| "candidate write failed")?;
    println!("{}", json!({"split_candidates":choices.len()}));
    Ok(())
}

fn measured(
    mode: &str,
    probe: &mut Probe,
    state: &Path,
    start: Instant,
    setup: u128,
    path: &Path,
    chapter: usize,
    settings: Settings,
) -> Result<()> {
    let automatic = mode == "automatic";
    let picked_path = state.join("passage.json");
    let picked: TextSelection = if automatic {
        serde_json::from_slice(&fs::read(&picked_path).map_err(|_| "saved passage unavailable")?)
            .map_err(|_| "saved passage invalid")?
    } else {
        let data = fs::read(state.join("candidates.json")).map_err(|_| "no candidates")?;
        let candidates: Vec<TextSelection> =
            serde_json::from_slice(&data).map_err(|_| "invalid candidates")?;
        let chosen = candidates
            .into_iter()
            .find(|word| {
                probe
                    .page_for(&word.start)
                    .is_some_and(|page| probe.selection_rects(page, word).len() == 1)
            })
            .ok_or("no intact authored-mode control for sampled words")?;
        fs::write(&picked_path, serde_json::to_vec(&chosen).unwrap())
            .map_err(|_| "could not save passage")?;
        chosen
    };
    let page = probe
        .page_for(&picked.start)
        .ok_or("saved bookmark did not resolve")?;
    let restored = probe
        .select_range(&picked.start, &picked.end)
        .ok_or("selection failed")?;
    if restored != picked || !probe.matches(&picked.text).contains(&picked.start) {
        return Err("source text or anchor mismatch");
    }
    let rects = probe.selection_rects(page, &picked);
    if rects.len() != if automatic { 2 } else { 1 } {
        return Err("unexpected line count for selection");
    }
    for r in &rects {
        let hit = probe
            .word_at(
                page,
                (r.x + r.width / 2) as f32,
                (r.y + r.height / 2) as f32,
            )
            .ok_or("hit test missed")?;
        if hit != picked {
            return Err("tap did not return original complete source word");
        }
    }
    let paint = Instant::now();
    let pixels = probe.capture(page).map_err(|_| "rasterization failed")?;
    let paint_time = elapsed_ms(paint);
    let frame_time = elapsed_ms(start);
    let frame_rss = peak_kib();
    let cpu = cpu_ticks();
    let (mapped, source) = probe.coverage();
    if mapped != source {
        return Err("source coverage mismatch");
    }
    let mut frame_delta = 0;
    let mut end_delta = 0;
    let gray = state.join("authored.gray");
    if automatic {
        let other = fs::read(&gray).map_err(|_| "control frame unavailable")?;
        if other.len() != pixels.len() {
            return Err("frame dimensions differ");
        }
        frame_delta = pixels.iter().zip(&other).filter(|(a, b)| a != b).count();
        let r = &rects[0];
        let right = (r.x + r.width).clamp(0, settings.width as i32) as usize;
        let left = right.saturating_sub(24).max(r.x.max(0) as usize);
        for y in r.y.max(0) as usize..(r.y + r.height).clamp(0, settings.height as i32) as usize {
            let row = y * settings.width as usize;
            end_delta += pixels[row + left..row + right]
                .iter()
                .zip(&other[row + left..row + right])
                .filter(|(a, b)| a != b)
                .count();
        }
        if end_delta == 0 || frame_delta == 0 {
            return Err("expected a changed line-end image");
        }
    } else {
        fs::write(&gray, &pixels).map_err(|_| "control frame write failed")?;
    }
    let mut search = Search::new(&picked.text).map_err(|_| "could not create search")?;
    let mut book = Book::open(path).map_err(|_| "search could not reopen book")?;
    english_block(path, chapter, picked.start.block, &mut book)?;
    let old: Anchor = serde_json::from_slice(
        &fs::read(state.join("legacy.json")).map_err(|_| "legacy saved position missing")?,
    )
    .map_err(|_| "invalid saved position")?;
    let old_page = probe
        .page_for(&old)
        .ok_or("v0.1.8 saved position failed to restore")?;
    let mut search_match = false;
    for _ in 0..=chapter {
        let hits = search.step(&mut book).map_err(|_| "search failed")?;
        search_match |= hits
            .iter()
            .any(|h| h.chapter == chapter && h.anchor == picked.start && h.end == picked.end);
    }
    if !search_match {
        return Err("source search endpoint mismatch");
    }
    println!(
        "{}",
        json!({"mode":mode,"chapter_one_based":chapter+1,"pages":probe.page_count(),"sample_page_one_based":page+1,
      "source_characters":source,"covered_characters":mapped,"missing_glyphs":probe.missing_glyphs(),
      "selected_bytes":picked.text.len(),"selected_lines":rects.len(),"verified_taps":rects.len(),
      "all_source_runs_us_english":true,"legacy_anchor_page_one_based":old_page+1,"saved_bookmark_resolved":true,"original_search_endpoints":true,"different_frame_pixels":frame_delta,"line_end_different_pixels":end_delta,
      "layout_ms":setup,"paint_ms":paint_time,"setup_selection_and_first_frame_ms":frame_time,"frame_peak_kib":frame_rss,
      "process_peak_kib":peak_kib(),"process_cpu_ticks_at_frame":cpu})
    );
    Ok(())
}

fn run() -> Result<()> {
    let argv: Vec<_> = env::args_os().skip(1).collect();
    if argv.len() < 5 {
        return Err(
            "usage: verify_english_hyphenation <discover|authored|automatic|negative> <private-dir> <book> <chapter-from-one> <fonts...>",
        );
    }
    let mode = argv[0].to_str().ok_or("invalid mode")?;
    if !matches!(mode, "discover" | "authored" | "automatic" | "negative") {
        return Err("invalid mode");
    }
    let state = Path::new(&argv[1]);
    let book = Path::new(&argv[2]);
    let chapter = argv[3]
        .to_str()
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|v| v.checked_sub(1))
        .ok_or("invalid chapter")?;
    let fonts: Vec<_> = argv[4..].iter().map(PathBuf::from).collect();
    let settings = Settings {
        font_size: 43.0,
        margin: 55,
        automatic_hyphenation: mode != "authored",
        ..Settings::default()
    };
    let start = Instant::now();
    let mut probe = Probe::open(book, Engine::Parley, &fonts, settings, chapter)
        .map_err(|_| "chapter did not open")?;
    let setup = elapsed_ms(start);
    if mode == "discover" {
        discover(&probe, state, settings)
    } else if mode == "negative" {
        negative(probe, book, chapter, &fonts, settings)
    } else {
        measured(
            mode, &mut probe, state, start, setup, book, chapter, settings,
        )
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
