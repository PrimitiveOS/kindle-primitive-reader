//! Verify a literal source passage or discover one spanning a laid-out page turn.
//! Reference wording is written only when a caller supplies an explicit private output file.
use reader::{
    Engine, Probe, Result, Settings,
    epub::Book,
    search::{NearbySearch, Search, SearchDirection},
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn source_page_phrase(probe: &Probe) -> Result<String> {
    for page in 1..probe.page_count() {
        let Some(at) = probe.page_anchor(page) else {
            continue;
        };
        if at.illustration.is_some() || at.offset < 60 {
            continue;
        }
        let around = |forward| {
            (28..60).rev().find_map(|delta| {
                let mut edge = at.clone();
                edge.offset = if forward {
                    at.offset + delta
                } else {
                    at.offset - delta
                };
                let (a, b) = if forward { (&at, &edge) } else { (&edge, &at) };
                let sel = probe.select_range(a, b)?;
                (sel.start == *a && sel.end == *b).then_some(sel.text)
            })
        };
        let (Some(left), Some(right)) = (around(false), around(true)) else {
            continue;
        };
        let begin = left
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .rev()
            .nth(2)
            .map(|(i, c)| i + c.len_utf8());
        let end = right
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .nth(2)
            .map(|(i, _)| i);
        if let (Some(begin), Some(end)) = (begin, end) {
            let query = format!("{}{}", &left[begin..], &right[..end]);
            if query.trim() == query && query.split_whitespace().count() >= 3 && query.len() < 200 {
                return Ok(query);
            }
        }
    }
    Err("No suitable interior page-turn text in this chapter".into())
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let file = PathBuf::from(args.next().ok_or("Supply a document")?);
    let font = PathBuf::from(args.next().ok_or("Supply a font")?);
    let chapter: usize = args
        .next()
        .ok_or("Supply a one-based chapter")?
        .into_string()
        .map_err(|_| "Invalid chapter")?
        .parse()?;
    let chapter = chapter.checked_sub(1).ok_or("Use a one-based chapter")?;
    let mode = args
        .next()
        .ok_or("Supply exact, soft, accents, absent, or page")?
        .into_string()
        .map_err(|_| "Invalid mode")?;
    if !matches!(
        mode.as_str(),
        "exact" | "soft" | "accents" | "absent" | "page"
    ) {
        return Err("Invalid comparison mode".into());
    }
    let probe = Probe::open(
        &file,
        Engine::Parley,
        &[font],
        Settings {
            width: 600,
            height: 800,
            font_size: 32.0,
            margin: 24,
            ..Settings::default()
        },
        chapter,
    )?;
    let query = if mode == "page" {
        source_page_phrase(&probe)?
    } else {
        args.next()
            .ok_or("Supply a search phrase")?
            .into_string()
            .map_err(|_| "Invalid phrase")?
    };
    let mut search = Search::with_options(&query, true, mode == "accents")?;
    let mut book = Book::open(&file)?;
    let mut all = Vec::new();
    while !search.is_finished() {
        all.extend(search.step(&mut book)?);
    }
    let mut target = None;
    for hit in all.iter().filter(|h| h.chapter == chapter) {
        if let Some(sel) = probe.select_range(&hit.anchor, &hit.end) {
            let from = probe.page_for(&hit.anchor).ok_or("Unmapped start")?;
            let to = probe.page_for(&hit.end).ok_or("Unmapped end")?;
            if (mode != "page" || from < to)
                && (mode != "soft" || sel.text.contains('\u{ad}'))
                && (mode != "accents" || sel.text != query)
            {
                target = Some((hit, sel.text, from, to));
                break;
            }
        }
    }
    let mut report = serde_json::json!({"count":all.len(),"chapter":chapter+1,"mode":mode,"query_bytes":query.len()});
    if mode == "absent" {
        if !all.is_empty() {
            return Err("Unexpected cross-paragraph match".into());
        }
    } else {
        let (hit, text, from, to) = target.ok_or("The expected original passage was not found")?;
        if text != hit.snippet[hit.matched.clone()] {
            return Err("Selected source wording differs".into());
        }
        for (d, anchor) in [
            (SearchDirection::Next, &hit.anchor),
            (SearchDirection::Previous, &hit.end),
        ] {
            let near = NearbySearch::new(&query, true, mode == "accents", d)?
                .find(&mut book, chapter, anchor, &AtomicBool::new(false))?
                .ok_or("Missing directional match")?;
            if (near.anchor, near.end) != (hit.anchor.clone(), hit.end.clone()) {
                return Err("Directional positions differ".into());
            }
        }
        report["from_page"] = serde_json::json!(from + 1);
        report["to_page"] = serde_json::json!(to + 1);
        report["quote_bytes"] = serde_json::json!(text.len());
    }
    if let Some(path) = args.next() {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
            .open(Path::new(&path))
            .and_then(|mut f| std::io::Write::write_all(&mut f, query.as_bytes()))?;
    }
    println!("{report}");
    Ok(())
}
