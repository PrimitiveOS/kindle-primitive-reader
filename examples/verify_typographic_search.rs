//! Verify a normal keyboard query against one actual publisher spelling.
//! Only numeric evidence leaves the process.
use reader::{
    Engine, Probe, Result, Settings,
    epub::Book,
    search::{NearbySearch, Search, SearchDirection, SearchHit},
};
use std::{env, path::PathBuf, sync::atomic::AtomicBool};

fn literal_pattern(text: &str) -> String {
    let mut escaped = String::new();
    for ch in text.chars() {
        if r".^$*+?()[]{}|\".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}
fn collect(search: &mut Search, book: &mut Book) -> Result<Vec<SearchHit>> {
    let mut result = Vec::new();
    while !search.is_finished() {
        result.extend(search.step(book)?);
    }
    Ok(result)
}
fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("Supply a document")?);
    let typed = args
        .next()
        .ok_or("Supply the ordinary keyboard wording")?
        .into_string()
        .map_err(|_| "Invalid input")?;
    let published = args
        .next()
        .ok_or("Supply the source spelling")?
        .into_string()
        .map_err(|_| "Invalid source wording")?;
    let font = PathBuf::from(args.next().ok_or("Supply a font")?);
    if typed == published {
        return Err("The control needs two different spellings".into());
    }
    let mut book = Book::open(&path)?;
    let exact = collect(
        &mut Search::with_regular_expression(&literal_pattern(&published), true)?,
        &mut book,
    )?;
    let ordinary = collect(&mut Search::with_case(&typed, true)?, &mut book)?;
    let unmodified = collect(
        &mut Search::with_regular_expression(&literal_pattern(&typed), true)?,
        &mut book,
    )?;
    let target = exact
        .iter()
        .find(|hit| {
            ordinary
                .iter()
                .any(|h| h.anchor == hit.anchor && h.end == hit.end)
        })
        .ok_or("Reference and ordinary search did not identify the same original range")?;
    if unmodified
        .iter()
        .any(|h| h.anchor == target.anchor && h.end == target.end)
    {
        return Err("Exact pattern mode matched different punctuation".into());
    }
    let probe = Probe::open(
        &path,
        Engine::Parley,
        std::slice::from_ref(&font),
        Settings {
            width: 600,
            height: 800,
            ..Settings::default()
        },
        target.chapter,
    )?;
    let selected = probe
        .select_range(&target.anchor, &target.end)
        .ok_or("Could not return to original passage")?;
    if selected.text != published {
        return Err("Selection did not preserve the source spelling".into());
    }
    let cancel = AtomicBool::new(false);
    for (d, anchor) in [
        (SearchDirection::Next, &target.anchor),
        (SearchDirection::Previous, &target.end),
    ] {
        let h = NearbySearch::new(&typed, true, false, d)?
            .find(&mut book, target.chapter, anchor, &cancel)?
            .ok_or("Directional lookup did not find the passage")?;
        if (h.anchor, h.end) != (target.anchor.clone(), target.end.clone())
            || &h.snippet[h.matched] != published
        {
            return Err("Directional source offsets or text differ".into());
        }
    }
    // Same ordered-byte checksum as the installed reference, with a NUL
    // separator after each quote. The private publication text stays local.
    let (mut a, mut b, mut characters) = (1u32, 0u32, 0usize);
    for hit in &ordinary {
        let quote = &hit.snippet[hit.matched.clone()];
        characters += quote.chars().count();
        for byte in quote.bytes().chain([0]) {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
    }
    println!(
        "{}",
        serde_json::json!({"chapter":target.chapter+1,"literal":ordinary.len(),"original_spelling":exact.len(),"exact_typed":unmodified.len(),"original_bytes":published.len(),"typed_bytes":typed.len(),"verified_directions":2,"exact_source_selection":true,"characters":characters,"ordered_text_checksum":format!("{b:04x}{a:04x}")})
    );
    Ok(())
}
