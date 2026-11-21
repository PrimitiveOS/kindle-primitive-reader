//! Numeric cross-engine comparison. Keeps publication titles, text and paths out of its output.
use reader::{
    Engine, Probe, Result, Settings,
    epub::Book,
    search::{NearbySearch, Search, SearchDirection},
};
use std::{env, path::PathBuf, sync::atomic::AtomicBool};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let document = PathBuf::from(args.next().ok_or("Supply a reflow document")?);
    let pattern = args
        .next()
        .ok_or("Supply an ECMAScript pattern")?
        .into_string()
        .map_err(|_| "Invalid pattern")?;
    let case = args.next().ok_or("Supply exact or insensitive")? == "exact";
    let font = PathBuf::from(args.next().ok_or("Supply a font")?);
    let mut book = Book::open(&document)?;
    let mut all = Search::with_regular_expression(&pattern, case)?;
    let mut hits = Vec::new();
    while !all.is_finished() {
        hits.extend(all.step(&mut book)?);
    }
    if hits.len() < 3 {
        return Err("The verifier needs three occurrences".into());
    }
    let mut loaded = None;
    let mut digest = (1u32, 0u32);
    let mut chars = 0;
    for hit in &hits {
        if loaded
            .as_ref()
            .is_none_or(|(chapter, _)| *chapter != hit.chapter)
        {
            let probe = Probe::open(
                &document,
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
            loaded = Some((hit.chapter, probe));
        }
        let selection = loaded
            .as_ref()
            .unwrap()
            .1
            .select_range(&hit.anchor, &hit.end)
            .ok_or("Pattern match is missing from the layout")?;
        if selection.text != hit.snippet[hit.matched.clone()] {
            return Err("The original selected text differs from the search result".into());
        }
        chars += selection.text.chars().count();
        for b in selection.text.bytes().chain([0]) {
            digest.0 = (digest.0 + u32::from(b)) % 65521;
            digest.1 = (digest.1 + digest.0) % 65521;
        }
    }
    let pivot = hits.len() / 2;
    for (direction, origin, expected) in [
        (SearchDirection::Next, &hits[pivot].end, &hits[pivot + 1]),
        (
            SearchDirection::Previous,
            &hits[pivot].anchor,
            &hits[pivot - 1],
        ),
    ] {
        let result = NearbySearch::with_regular_expression(&pattern, case, direction)?
            .find(
                &mut book,
                hits[pivot].chapter,
                origin,
                &AtomicBool::new(false),
            )?
            .ok_or("No adjacent occurrence")?;
        if (result.anchor, result.end) != (expected.anchor.clone(), expected.end.clone()) {
            return Err("Directional pattern search returned different anchors".into());
        }
    }
    println!(
        "{}",
        serde_json::json!({"count":hits.len(),"characters":chars,"ordered_text_checksum":format!("{:04x}{:04x}",digest.1,digest.0),"mapped":hits.len(),"directions":2})
    );
    Ok(())
}
