//! Measure a paragraph's selected text on the normal reflow engine without printing book content.
use reader::{Anchor, Engine, Probe, Result, Settings, epub::Book};
use std::{collections::BTreeMap, env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Pass a document, 1-based chapter and paragraph, then font files")?,
    );
    let mut number = || -> Result<usize> {
        args.next()
            .and_then(|v| v.to_str()?.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .ok_or_else(|| "Expected a positive index".into())
    };
    let chapter = number()?;
    let paragraph = number()?;
    let fonts: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if fonts.is_empty() {
        return Err("Supply at least one font".into());
    }
    let markup = Book::open(&path)?.chapter(chapter)?;
    let document = roxmltree::Document::parse_with_options(
        &markup,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )?;
    let node = document
        .descendants()
        .filter(|n| n.has_tag_name("p"))
        .nth(paragraph)
        .ok_or("Paragraph index unavailable")?;
    let text: String = node
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
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
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let hits = probe.matches(&text);
    let start = hits
        .iter()
        .find(|a| a.offset == 0 && a.block == node.range().start)
        .or_else(|| (hits.len() == 1 && hits[0].offset == 0).then(|| &hits[0]))
        .ok_or("Could not uniquely match the expected paragraph at a source boundary")?;
    let end = Anchor {
        offset: start.offset + text.len(),
        ..start.clone()
    };
    let selected = probe
        .select_range(start, &end)
        .ok_or("Missing source selection")?;
    if selected.text != text {
        return Err("Selection disagrees with source".into());
    }
    let first = probe.page_for(start).ok_or("Missing source anchor")?;
    let last = probe.page_for(&end).ok_or("Missing end anchor")?;
    let mut left = BTreeMap::new();
    for page in first..=last {
        for rect in probe.selection_rects(page, &selected) {
            left.entry((page, rect.y))
                .and_modify(|x: &mut i32| *x = (*x).min(rect.x))
                .or_insert(rect.x);
        }
        if left.len() >= 3 {
            break;
        }
    }
    let lines: Vec<_> = left.into_values().take(3).collect();
    if lines.len() < 2 {
        return Err("At least two wrapped lines are required".into());
    }
    let coverage = probe.coverage();
    if coverage.0 != coverage.1 {
        return Err("Source coverage is incomplete".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "chapter":chapter+1, "paragraph":paragraph+1, "first_line_page":first+1,
            "source_bytes":text.len(), "selected_line_x":lines, "coverage":coverage,
            "missing_glyphs":probe.missing_glyphs()
        })
    );
    Ok(())
}
