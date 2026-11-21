//! Audit Kindle-format navigation and local resources without printing book metadata or text.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use reader::{Engine, Probe, Result, Settings, epub::Book};
use roxmltree::Document;
use serde::Serialize;

#[derive(Default, Serialize)]
struct Evidence {
    spine_sections: usize,
    sampled_sections: Vec<usize>,
    rendered_pages: usize,
    anchored_pages: usize,
    anchor_page_mismatches: usize,
    local_links: usize,
    linked_image_destinations: usize,
    hidden_image_links: usize,
    sampled_linked_artworks: usize,
    unavailable_linked_artworks: usize,
    unresolved_local_links: usize,
    sampled_fragment_destinations: usize,
    unresolved_sampled_fragments: usize,
    image_placements: usize,
    local_images: usize,
    image_decode_errors: usize,
    local_stylesheets: usize,
    publisher_page_templates: usize,
    missing_stylesheets: usize,
    sampled_artworks: usize,
    unavailable_sampled_artworks: usize,
    unavailable_artwork_sections: BTreeSet<usize>,
    covered_characters: usize,
    source_characters: usize,
    missing_glyphs: usize,
}

#[derive(Default)]
struct Section {
    ids: BTreeSet<String>,
    images: BTreeSet<String>,
    targets: BTreeSet<String>,
    linked_artworks: BTreeSet<String>,
}

fn audit(path: &Path, fonts: &[PathBuf]) -> Result<Evidence> {
    let mut book = Book::open(path)?;
    let mut evidence = Evidence {
        spine_sections: book.chapters.len(),
        ..Default::default()
    };
    let mut sections = Vec::new();
    let mut destinations = Vec::new();
    let mut images = BTreeSet::new();
    let mut sheets = BTreeSet::new();
    for index in 0..book.chapters.len() {
        let source = book.chapter(index)?;
        let base = &book.chapters[index].href;
        let xml = Document::parse(&source)?;
        let mut section = Section::default();
        for node in xml.descendants().filter(|node| node.is_element()) {
            if let Some(id) = node.attribute("id") {
                section.ids.insert(id.into());
            }
            if node.has_tag_name("a")
                && let Some(href) = node.attribute("href")
            {
                if href.contains(':') && !href.starts_with("kindle:") {
                    continue;
                }
                evidence.local_links += 1;
                match Book::resolve_link(base, href) {
                    Ok(destination) => {
                        let visible = !node.ancestors().any(|parent| {
                            parent.has_attribute("hidden")
                                || parent.attribute("aria-hidden") == Some("true")
                        });
                        destinations.push((index, destination, visible));
                    }
                    Err(_) => evidence.unresolved_local_links += 1,
                }
            }
            let image = if node.has_tag_name("img") {
                node.attribute("src")
            } else if node.has_tag_name(("http://www.w3.org/2000/svg", "image")) {
                node.attribute("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
            } else {
                None
            };
            if let Some(src) = image {
                evidence.image_placements += 1;
                let image = Book::resolve(base, src)?;
                section.images.insert(image.clone());
                images.insert(image);
            }
            if node.has_tag_name("link")
                && node.attribute("rel") == Some("stylesheet")
                && let Some(href) = node.attribute("href")
            {
                let path = Book::resolve(base, href)?;
                if path.ends_with(".xpgt") {
                    evidence.publisher_page_templates += 1;
                } else {
                    sheets.insert(path);
                }
            }
        }
        sections.push(section);
    }
    for (source, destination, visible) in destinations {
        let Some(index) = book.chapter_index(&destination.path) else {
            if destination.fragment.is_none()
                && destination
                    .path
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| matches!(ext, "jpg" | "png" | "svg"))
                && book.resource(&destination.path).is_ok()
            {
                evidence.linked_image_destinations += 1;
                if visible {
                    sections[source]
                        .linked_artworks
                        .insert(destination.path.clone());
                } else {
                    evidence.hidden_image_links += 1;
                }
                images.insert(destination.path);
                continue;
            }
            evidence.unresolved_local_links += 1;
            continue;
        };
        if let Some(id) = destination.fragment {
            if !sections[index].ids.contains(&id) {
                evidence.unresolved_local_links += 1;
            }
            sections[index].targets.insert(id);
        }
    }
    evidence.local_stylesheets = sheets.len();
    for sheet in sheets {
        if book.resource(&sheet).is_err() {
            evidence.missing_stylesheets += 1;
        }
    }
    evidence.local_images = images.len();
    for image in images {
        let ok = book.resource(&image).ok().is_some_and(|data| {
            if image.ends_with(".svg") {
                std::str::from_utf8(&data)
                    .ok()
                    .and_then(|s| Document::parse(s).ok())
                    .is_some()
            } else {
                let Ok(mut reader) =
                    image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()
                else {
                    return false;
                };
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(4096);
                limits.max_image_height = Some(4096);
                limits.max_alloc = Some(24 * 1024 * 1024);
                reader.limits(limits);
                reader.decode().is_ok()
            }
        });
        if !ok {
            evidence.image_decode_errors += 1;
        }
    }
    drop(book);
    let count = sections.len();
    if count == 0 {
        return Err("Empty reading spine".into());
    }
    let mut sampled = BTreeSet::from([0, count / 2, count - 1]);
    let mut illustrated: Vec<_> = (0..count)
        .filter(|&i| !sections[i].images.is_empty())
        .collect();
    illustrated.sort_by_key(|&i| (std::cmp::Reverse(sections[i].images.len()), i));
    sampled.extend(illustrated.into_iter().take(2));
    if let Some(index) = (0..count).max_by_key(|&i| sections[i].targets.len())
        && !sections[index].targets.is_empty()
    {
        sampled.insert(index);
    }
    sampled.extend(
        (0..count)
            .filter(|&i| !sections[i].linked_artworks.is_empty())
            .take(8),
    );
    for index in sampled {
        let mut probe = Probe::open(path, Engine::Parley, fonts, Settings::default(), index)?;
        evidence.sampled_sections.push(index + 1);
        let (covered, chars) = probe.coverage();
        evidence.covered_characters += covered;
        evidence.source_characters += chars;
        evidence.missing_glyphs += probe.missing_glyphs();
        let pages = probe.page_count();
        if pages == 0 {
            return Err("Empty chapter layout".into());
        }
        for page in BTreeSet::from([0, pages / 2, pages - 1]) {
            let frame = probe.capture(page)?;
            if frame.len() != (Settings::default().width * Settings::default().height) as usize {
                return Err("Unexpected page dimensions".into());
            }
            evidence.rendered_pages += 1;
            if let Some(anchor) = probe.page_anchor(page) {
                evidence.anchored_pages += 1;
                if probe.page_for(&anchor) != Some(page) {
                    evidence.anchor_page_mismatches += 1;
                }
            }
        }
        for id in &sections[index].targets {
            evidence.sampled_fragment_destinations += 1;
            if !probe
                .fragment(id)
                .is_some_and(|a| probe.page_for(&a).is_some())
            {
                evidence.unresolved_sampled_fragments += 1;
            }
        }
        for image in sections[index].linked_artworks.iter().take(4) {
            evidence.sampled_linked_artworks += 1;
            if probe.open_illustration(path, image).is_err() {
                evidence.unavailable_linked_artworks += 1;
            }
        }
        for image in sections[index].images.iter().take(2) {
            evidence.sampled_artworks += 1;
            if probe.open_illustration(path, image).is_err() {
                evidence.unavailable_sampled_artworks += 1;
                evidence.unavailable_artwork_sections.insert(index + 1);
            }
        }
    }
    Ok(evidence)
}

fn main() {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let Some(book) = args.next() else {
        eprintln!("Usage: verify_kindle_sections <book> <font> [font...]");
        std::process::exit(2)
    };
    let fonts: Vec<_> = args.collect();
    if fonts.is_empty() {
        eprintln!("A reading font is required");
        std::process::exit(2)
    }
    match audit(&book, &fonts) {
        Ok(report) => println!("{}", serde_json::to_string(&report).unwrap()),
        Err(_) => {
            eprintln!("The document or one of its sampled sections did not open");
            std::process::exit(1);
        }
    }
}
