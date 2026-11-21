//! Verify the ordered whole-book list and navigation on both sides of the old result cap.
use reader::{
    Engine, Probe, Result, Settings,
    epub::Book,
    search::{MAX_REFLOW_SEARCH_HITS, Search},
};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("Supply a document")?);
    let term = args
        .next()
        .ok_or("Supply a literal search term")?
        .into_string()
        .map_err(|_| "Invalid term")?;
    let font = PathBuf::from(args.next().ok_or("Supply a font")?);
    let mut book = Book::open(&path)?;
    let mut search = Search::with_case(&term, true)?;
    let mut hits = Vec::new();
    while !search.is_finished() {
        hits.extend(search.step(&mut book)?);
    }
    if hits.len() <= 101 || hits.len() > MAX_REFLOW_SEARCH_HITS || search.memory_limited() {
        return Err("The original must produce more than 101 hits within the result budget".into());
    }
    let (mut a, mut b, mut chars) = (1u32, 0u32, 0usize);
    for h in &hits {
        let found = &h.snippet[h.matched.clone()];
        chars += found.chars().count();
        for byte in found.bytes().chain([0]) {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
    }
    let mut pages = Vec::new();
    for index in [0, 99, 100, hits.len() / 2, hits.len() - 1] {
        let hit = &hits[index];
        let probe = Probe::open(
            &path,
            Engine::Parley,
            std::slice::from_ref(&font),
            Settings {
                width: 600,
                height: 800,
                margin: 24,
                font_size: 32.0,
                ..Settings::default()
            },
            hit.chapter,
        )?;
        let sel = probe
            .select_range(&hit.anchor, &hit.end)
            .ok_or("Unmapped result")?;
        if sel.text != hit.snippet[hit.matched.clone()] {
            return Err("Displayed text differs from source match".into());
        }
        pages.push(serde_json::json!({"result":index+1,"spine":hit.chapter+1,"local_page":probe.page_for(&hit.anchor).ok_or("Unmapped page")?+1}));
    }
    println!(
        "{}",
        serde_json::json!({"count":hits.len(),"characters":chars,"ordered_text_checksum":format!("{b:04x}{a:04x}"),"mapped":pages,"skipped":search.skipped_chapters()})
    );
    Ok(())
}
