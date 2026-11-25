//! ODT-only bounded chapter partitioning and verified historical source maps.
use crate::epub::{Book, TocEntry};
use crate::kf8::Kf8Chapter;
use crate::source::Source;
use crate::{Anchor, Result};
use std::collections::BTreeMap;

const MAX_SECTION_TEXT: usize = 512 * 1024;

struct Span {
    original: std::ops::Range<usize>,
    chapter: usize,
    path: String,
    revision: u64,
    block: usize,
}

struct ImagePoint {
    chapter: usize,
    path: String,
    revision: u64,
    block: usize,
    ordinal: usize,
}

pub(crate) struct History {
    book_revision: u64,
    original_path: String,
    original: Source,
    current_revisions: BTreeMap<String, u64>,
    continued_before: Vec<bool>,
    blocks: BTreeMap<usize, Vec<Span>>,
    images: BTreeMap<usize, Vec<ImagePoint>>,
}

pub(crate) struct Split {
    pub chapters: Vec<Kf8Chapter>,
    pub toc: Vec<TocEntry>,
    pub history: History,
}

fn source(xhtml: &str) -> Result<Source> {
    Source::parse_odt_with_typography(xhtml, None, true, &Default::default())
}

/// A None result deliberately retains the existing single-chapter XHTML and
/// all its identifiers. Only documents too large for Parley are partitioned.
pub(crate) fn prepare(
    xhtml: &str,
    path: &str,
    title: &str,
    toc: &[TocEntry],
) -> Result<Option<Split>> {
    let original = source(xhtml)?;
    let text_size = original
        .blocks
        .iter()
        .try_fold(0usize, |sum, block| sum.checked_add(block.text.len()))
        .ok_or("ODT text size overflow")?;
    if text_size <= MAX_SECTION_TEXT {
        return Ok(None);
    }
    let chapter = Kf8Chapter {
        continued: false,
        path: path.to_owned(),
        label: title.to_owned(),
        xhtml: xhtml.to_owned(),
    };
    let mut chapters = crate::kindle_sections::partition_odt(chapter)
        .map_err(|_| "The OpenDocument text could not be safely partitioned")?;
    if chapters.len() < 2 {
        return Err("The OpenDocument text cannot fit a reading section".into());
    }
    let mut owners = BTreeMap::<String, String>::new();
    for part in &chapters {
        let xml = roxmltree::Document::parse(&part.xhtml)?;
        for element in xml.descendants().filter(roxmltree::Node::is_element) {
            if let Some(id) = element.attribute("id") {
                owners
                    .entry(id.to_owned())
                    .or_insert_with(|| part.path.clone());
            }
        }
    }
    // All generated ODT links are same-document fragments. Resolve the
    // decoded ID, preserve its original escaped spelling, and use the first
    // occurrence in document order even if an author duplicated an ID.
    for part in &mut chapters {
        let xml = roxmltree::Document::parse(&part.xhtml)?;
        let mut changes = Vec::new();
        for node in xml.descendants().filter(|node| node.has_tag_name("a")) {
            let Some(attribute) = node.attribute_node("href") else {
                continue;
            };
            let Some((_, fragment)) = attribute.value().split_once('#') else {
                continue;
            };
            let reference = format!("#{fragment}");
            let Some(id) = Book::resolve_link(path, &reference)
                .ok()
                .and_then(|target| target.fragment)
            else {
                continue;
            };
            let Some(destination) = owners.get(&id) else {
                continue;
            };
            let wanted = if destination == &part.path {
                ""
            } else {
                destination.as_str()
            };
            let range = attribute.range_value();
            let Some(hash) = part.xhtml[range.clone()].find('#') else {
                continue;
            };
            changes.push((range.start..range.start + hash, wanted.to_owned()));
        }
        drop(xml);
        for (range, value) in changes.into_iter().rev() {
            part.xhtml.replace_range(range, &value);
        }
    }
    let mut toc = toc.to_vec();
    for entry in &mut toc {
        if let Some(target) = &mut entry.destination
            && let Some(owner) = target.fragment.as_ref().and_then(|id| owners.get(id))
        {
            target.path.clone_from(owner);
        }
    }
    let mut parsed = Vec::with_capacity(chapters.len());
    for part in &chapters {
        let parsed_part = source(&part.xhtml)?;
        let count = parsed_part
            .blocks
            .iter()
            .try_fold(0usize, |sum, block| sum.checked_add(block.text.len()))
            .ok_or("ODT section text size overflow")?;
        if count > MAX_SECTION_TEXT {
            return Err("An OpenDocument section exceeds the reading limit".into());
        }
        parsed.push(parsed_part);
    }
    let current = parsed
        .iter()
        .enumerate()
        .flat_map(|(chapter, source)| {
            source
                .blocks
                .iter()
                .filter(|block| !block.text.is_empty() || !block.images.is_empty())
                .map(move |block| (chapter, source.revision, block))
        })
        .collect::<Vec<_>>();
    let mut blocks = BTreeMap::new();
    let mut images = BTreeMap::new();
    let mut index = 0;
    for old in original
        .blocks
        .iter()
        .filter(|block| !block.text.is_empty() || !block.images.is_empty())
    {
        let mut consumed = 0;
        let mut image_index = 0;
        let mut spans = Vec::new();
        let mut image_points = Vec::new();
        while consumed < old.text.len() || image_index < old.images.len() {
            let Some(&(chapter, revision, block)) = current.get(index) else {
                return Err("Partitioning lost OpenDocument content".into());
            };
            if !old.text[consumed..].starts_with(block.text.as_str()) {
                return Err("Partitioning changed OpenDocument text".into());
            }
            let end = consumed + block.text.len();
            for (ordinal, image) in block.images.iter().enumerate() {
                let Some(previous) = old.images.get(image_index) else {
                    return Err("Partitioning added an OpenDocument image".into());
                };
                if previous.href != image.href
                    || previous.inline_svg != image.inline_svg
                    || previous.offset != consumed + image.offset
                    || previous.space_after != image.space_after
                {
                    return Err("Partitioning changed an OpenDocument image position".into());
                }
                image_points.push(ImagePoint {
                    chapter,
                    path: chapters[chapter].path.clone(),
                    revision,
                    block: block.key,
                    ordinal,
                });
                image_index += 1;
            }
            spans.push(Span {
                original: consumed..end,
                chapter,
                path: chapters[chapter].path.clone(),
                revision,
                block: block.key,
            });
            consumed = end;
            index += 1;
        }
        blocks.insert(old.key, spans);
        images.insert(old.key, image_points);
    }
    if index != current.len() {
        return Err("Partitioning added OpenDocument content".into());
    }

    let mut continued_before = vec![false; chapters.len()];
    for spans in blocks.values() {
        for pair in spans.windows(2) {
            if pair[1].chapter == pair[0].chapter + 1 {
                continued_before[pair[1].chapter] = true;
            }
        }
    }
    let current_revisions = chapters
        .iter()
        .zip(&parsed)
        .map(|(part, source)| (part.path.clone(), source.revision))
        .collect();
    Ok(Some(Split {
        chapters,
        toc,
        history: History {
            book_revision: crate::revision(xhtml),
            original_path: path.to_owned(),
            current_revisions,
            continued_before,
            original,
            blocks,
            images,
        },
    }))
}

impl History {
    pub(crate) fn continued_before(&self) -> &[bool] {
        &self.continued_before
    }

    pub(crate) fn migrate(&self, anchor: &Anchor, end: bool) -> Result<Option<(usize, Anchor)>> {
        if anchor.book != self.book_revision {
            return Err("The saved OpenDocument edition cannot be migrated safely".into());
        }
        if self.current_revisions.get(&anchor.chapter) == Some(&anchor.revision) {
            return Ok(None);
        }
        if anchor.chapter != self.original_path {
            return Err(
                "The saved OpenDocument section or revision cannot be migrated safely".into(),
            );
        }
        let Some(key) = self.original.current_key(anchor.block, anchor.revision) else {
            return Err("The saved OpenDocument block is unavailable".into());
        };
        let Some(block) = self.original.blocks.iter().find(|block| block.key == key) else {
            return Err("The saved OpenDocument block is unavailable".into());
        };
        let Some(offset) = self
            .original
            .project_offset(block, anchor.revision, anchor.offset)
        else {
            return Err(
                "The saved OpenDocument revision or offset cannot be migrated safely".into(),
            );
        };
        if let Some(ordinal) = anchor.illustration {
            let image = self
                .images
                .get(&key)
                .and_then(|images| images.get(ordinal))
                .ok_or("The saved OpenDocument image position cannot be migrated safely")?;
            let mut migrated = anchor.clone();
            migrated.chapter.clone_from(&image.path);
            migrated.revision = image.revision;
            migrated.block = image.block;
            migrated.offset = 0;
            migrated.illustration = Some(image.ordinal);
            return Ok(Some((image.chapter, migrated)));
        }
        let spans = self
            .blocks
            .get(&key)
            .ok_or("The saved OpenDocument position cannot be migrated safely")?;
        let found = if end {
            spans
                .iter()
                .find(|span| span.original.start < offset && offset <= span.original.end)
                .or_else(|| spans.iter().find(|span| span.original.start == offset))
        } else {
            spans
                .iter()
                .find(|span| span.original.start <= offset && offset < span.original.end)
                .or_else(|| spans.iter().find(|span| span.original.end == offset))
        }
        .ok_or("The saved OpenDocument offset cannot be migrated safely")?;
        let mut migrated = anchor.clone();
        migrated.chapter.clone_from(&found.path);
        migrated.revision = found.revision;
        migrated.block = found.block;
        migrated.offset = offset - found.original.start;
        Ok(Some((found.chapter, migrated)))
    }
}

#[cfg(test)]
mod tests;
