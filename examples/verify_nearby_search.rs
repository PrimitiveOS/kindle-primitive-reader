//! Compare a directional search with exact source offsets in a late chapter.
//! The output excludes publication names, matched text and chapter paths.
use reader::{
    Engine, Probe, Result, Settings,
    epub::Book,
    search::{NearbySearch, Search, SearchDirection},
};
use std::{env, path::PathBuf, sync::atomic::AtomicBool};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Pass a document, chapter, literal query and font files")?,
    );
    let chapter = args
        .next()
        .and_then(|a| a.to_str()?.parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
        .ok_or("Select a 1-based chapter")?;
    let query = args
        .next()
        .and_then(|a| a.into_string().ok())
        .ok_or("Pass a UTF-8 literal")?;
    let fonts: Vec<PathBuf> = args.map(PathBuf::from).collect();
    let probe = Probe::open(
        &path,
        Engine::Parley,
        &fonts,
        Settings {
            width: 600,
            height: 800,
            margin: 24,
            font_size: 32.0,
            ..Settings::default()
        },
        chapter,
    )?;
    let hits = probe.matches(&query);
    if hits.len() < 3 {
        return Err("Select a chapter containing at least three literal occurrences".into());
    }
    let mut book = Book::open(&path)?;
    let mut all = Search::with_case(&query, true)?.at_most(100);
    let mut listed = Vec::new();
    while !all.is_finished() {
        listed.extend(all.step(&mut book)?);
    }
    let at = hits.len() / 2;
    let start = &hits[at];
    let mut end = start.clone();
    end.offset += query.len();
    let cancel = AtomicBool::new(false);
    for (direction, origin, expected) in [
        (SearchDirection::Next, &end, &hits[at + 1]),
        (SearchDirection::Previous, start, &hits[at - 1]),
    ] {
        let found = NearbySearch::new(&query, true, false, direction)?
            .find(&mut book, chapter, origin, &cancel)?
            .ok_or("No adjacent result")?;
        if &found.anchor != expected
            || found.chapter != chapter
            || found.end.offset != expected.offset + query.len()
            || &found.snippet[found.matched] != query
            || probe
                .select_range(expected, &found.end)
                .is_none_or(|s| s.text != query)
        {
            return Err("Nearby match disagrees with its expected source range".into());
        }
    }
    println!(
        "{}",
        serde_json::json!({"chapter":chapter+1,"original_chapter_matches":hits.len(),
        "all_results":listed.len(),"all_last_chapter":listed.last().map(|h|h.chapter+1),
        "pivot_in_all":listed.iter().any(|h|&h.anchor==start),
        "both_adjacent_anchors_and_quotes_match":true,"coverage":probe.coverage(),"pivot_page":probe.page_for(start).map(|p|p+1)})
    );
    Ok(())
}
