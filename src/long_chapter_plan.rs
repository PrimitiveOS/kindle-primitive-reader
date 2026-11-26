//! Read-only planning for oversized EPUB chapters. No reading path uses this yet.
//!
//! These ranges are work batches over the original Source, not pages or new
//! spine items. They do not establish that independent layouts can be joined;
//! a future continuation/index pass must prove that separately.
#![allow(dead_code)]

#[cfg(feature = "kindle")]
pub(crate) mod font_snapshot;
pub(crate) mod index;
pub(crate) mod pagination;

use crate::book_style::{CssLength, Style, WhiteSpace, WritingMode, flow::Display};
use crate::epub::Book;
use crate::flow_projection::{ElementFlow, SourceBinding};
use crate::source::Source;
use crate::{Anchor, Result, Settings};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

const NORMAL_TEXT_LIMIT: usize = 512 * 1024;
const SOURCE_LIMIT: usize = 1024 * 1024;
const BATCH_TEXT_LIMIT: usize = 128 * 1024;
pub(crate) const MAX_BATCH_BLOCKS: usize = 256;
const MAX_BATCHES: usize = 32;
const MAX_BLOCKS: usize = 8192;
const MAX_ELEMENTS: usize = 8192;
const MAX_CHAPTERS: usize = 4096;
const MAX_HREF_BYTES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutMode {
    pub scroll: bool,
    pub facing_pages: bool,
    pub hidden_flows: bool,
    pub hide_nonlinear: bool,
    pub in_page_notes: bool,
}

pub(crate) struct Batch {
    /// Indices into the original Source's ordered blocks, never a page range.
    pub blocks: Range<usize>,
    pub first_key: usize,
    pub last_key: usize,
    pub text_bytes: usize,
}

pub(crate) struct LongChapterPlan {
    book: u64,
    href: String,
    revision: u64,
    source_digest: [u8; 32],
    canonical_digest: [u8; 32],
    binding: SourceBinding,
    batches: Vec<Batch>,
}

pub(crate) fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        Err("Long chapter planning was cancelled".into())
    } else {
        Ok(())
    }
}

fn simple_styles(styles: &[Style]) -> bool {
    styles.iter().all(|style| {
        style.flow == ElementFlow::default()
            && matches!(style.display, Display::Inline | Display::Block)
            && style.writing_mode == WritingMode::Horizontal
            && style.white_space == WhiteSpace::Normal
            && !style.preformatted_spaces
            && style.generated.is_none()
            && style.first_letter.is_none()
            && style.first_line.is_none()
            && style.float_side.is_none()
            && style.clear == Default::default()
            && style.page_breaks == Default::default()
            && style.orphans <= 1
            && style.widows <= 1
            && style.borders.is_none()
            && style.background.is_none()
            && style.min_width.is_none()
            && style.min_height.is_none()
            && !style.box_sizing_explicit
            && !style.inline_nowrap
            && style.padding == [CssLength::default(); 2]
            && style.vertical_padding == [CssLength::default(); 2]
            && style.vertical_margin == [CssLength::default(); 2]
    })
}

/// Recheck the structural gates before consuming a plan. The plan's text
/// digest alone does not bind these fields or the current computed styles.
pub(crate) fn simple_source(source: &Source, styles: &[Style]) -> bool {
    !source.blocks.is_empty()
        && source.blocks.len() <= MAX_BLOCKS
        && source.elements.len() <= MAX_ELEMENTS
        && source.fragments.len() <= MAX_ELEMENTS
        && styles.len() == source.elements.len()
        && source.stylesheets.is_empty()
        && source.table_cells.is_empty()
        && source.list_counters.is_empty()
        && source.raw_only_blocks.is_empty()
        && source.elements.iter().all(|element| {
            element.inline_style.is_none()
                && !element.anonymous
                && matches!(
                    element.tag.as_str(),
                    "html"
                        | "head"
                        | "title"
                        | "body"
                        | "p"
                        | "span"
                        | "a"
                        | "em"
                        | "strong"
                        | "b"
                        | "i"
                        | "br"
                )
        })
        && source.blocks.iter().all(|block| {
            block.element < styles.len()
                && !block.text.is_empty()
                && block.text.len() <= BATCH_TEXT_LIMIT
                && block.table_cell.is_none()
                && block.list_item.is_none()
                && block.raw_whitespace.is_none()
                && block.images.is_empty()
                && block.ruby.is_empty()
        })
        && simple_styles(styles)
}

/// The binding token identifies the parsed Source lifetime; this digest also
/// detects in-place edits to its canonical block ordering or text.
fn canonical_digest(source: &Source, binding: &SourceBinding) -> Option<[u8; 32]> {
    if source.blocks.is_empty() || source.blocks.len() > MAX_BLOCKS {
        return None;
    }
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut total = 0usize;
    let mut previous = None;
    for block in &source.blocks {
        let len = block.text.len();
        total = total.checked_add(len)?;
        if !binding.matches(block)
            || previous.is_some_and(|key| key >= block.key)
            || len == 0
            || len > BATCH_TEXT_LIMIT
            || total > SOURCE_LIMIT
        {
            return None;
        }
        digest.update(&block.key.to_le_bytes());
        digest.update(&len.to_le_bytes());
        digest.update(block.text.as_bytes());
        previous = Some(block.key);
    }
    digest.finish().as_ref().try_into().ok()
}

impl LongChapterPlan {
    /// Prepare only a bounded, simple candidate against the original chapter.
    /// The caller must hold the appropriate storage lease during book access.
    /// No source, anchor, saved state, spine or reading view is changed.
    pub(crate) fn prepare(
        book: &mut Book,
        chapter: usize,
        source: &Source,
        styles: &[Style],
        user_styles: &crate::user_styles::Snapshot,
        settings: Settings,
        mode: LayoutMode,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        cancelled(cancel)?;
        if !book.is_reflowable_epub()
            || book.spine_rtl
            || book.chapters.len() > MAX_CHAPTERS
            || mode.scroll
            || mode.facing_pages
            || mode.hidden_flows
            || mode.hide_nonlinear
            || mode.in_page_notes
            || settings.in_page_footnotes
            || !user_styles.is_empty()
        {
            return Err("This long chapter needs a different reading layout".into());
        }
        let entry = book
            .chapters
            .get(chapter)
            .ok_or("The chapter is unavailable")?;
        if entry.href.is_empty()
            || entry.href.len() > MAX_HREF_BYTES
            || entry.is_svg
            || entry.nonlinear
            || book.recovered_placeholder(entry)
            || book.is_manifest_fallback(entry)
            || book
                .chapters
                .iter()
                .filter(|other| other.href == entry.href)
                .take(2)
                .count()
                != 1
        {
            return Err("The long chapter is unsupported or ambiguous".into());
        }
        let href = entry.href.clone();
        let publication = book.revision();
        let xhtml = book.chapter(chapter)?;
        cancelled(cancel)?;
        if xhtml.len() > SOURCE_LIMIT || !source.matches_input(&xhtml) {
            return Err("The original long chapter cannot be verified".into());
        }
        if !simple_source(source, styles) {
            return Err("This long chapter uses unsupported layout features".into());
        }
        let first = &source.blocks[0];
        let binding = SourceBinding::new(first);
        let mut batches = Vec::new();
        let mut start = 0usize;
        let mut bytes = 0usize;
        let mut total = 0usize;
        let mut previous = None;
        for (index, block) in source.blocks.iter().enumerate() {
            cancelled(cancel)?;
            let len = block.text.len();
            if !binding.matches(block)
                || previous.is_some_and(|key| key >= block.key)
                || block.element >= styles.len()
                || len == 0
                || len > BATCH_TEXT_LIMIT
                || block.table_cell.is_some()
                || block.list_item.is_some()
                || block.raw_whitespace.is_some()
                || !block.images.is_empty()
                || !block.ruby.is_empty()
            {
                return Err("The long chapter cannot be divided safely".into());
            }
            total = total
                .checked_add(len)
                .ok_or("The long chapter is too large")?;
            if total > SOURCE_LIMIT {
                return Err("The long chapter is too large".into());
            }
            if bytes
                .checked_add(len)
                .is_none_or(|size| size > BATCH_TEXT_LIMIT)
                || index - start >= MAX_BATCH_BLOCKS
            {
                batches.push(Batch {
                    blocks: start..index,
                    first_key: source.blocks[start].key,
                    last_key: source.blocks[index - 1].key,
                    text_bytes: bytes,
                });
                if batches.len() >= MAX_BATCHES {
                    return Err("The long chapter has too many batches".into());
                }
                start = index;
                bytes = 0;
            }
            bytes += len;
            previous = Some(block.key);
        }
        if total <= NORMAL_TEXT_LIMIT {
            return Err("This chapter does not need long-chapter planning".into());
        }
        batches.push(Batch {
            blocks: start..source.blocks.len(),
            first_key: source.blocks[start].key,
            last_key: source.blocks.last().ok_or("The chapter is empty")?.key,
            text_bytes: bytes,
        });
        cancelled(cancel)?;
        let source_digest = ring::digest::digest(&ring::digest::SHA256, xhtml.as_bytes())
            .as_ref()
            .try_into()
            .map_err(|_| "The chapter digest is unavailable")?;
        let canonical_digest =
            canonical_digest(source, &binding).ok_or("The canonical chapter cannot be verified")?;
        cancelled(cancel)?;
        Ok(Self {
            book: publication,
            href,
            revision: source.revision,
            source_digest,
            canonical_digest,
            binding,
            batches,
        })
    }

    /// Resolve only an exact canonical source anchor from this same Source.
    /// No page number or new position is fabricated.
    pub(crate) fn batch_for_anchor(&self, source: &Source, anchor: &Anchor) -> Option<usize> {
        if anchor.book != self.book
            || anchor.chapter != self.href
            || anchor.revision != self.revision
            || anchor.raw_caret.is_some()
            || anchor.illustration.is_some()
            || source.revision != self.revision
            || canonical_digest(source, &self.binding) != Some(self.canonical_digest)
        {
            return None;
        }
        let index = source
            .blocks
            .binary_search_by_key(&anchor.block, |block| block.key)
            .ok()?;
        let block = &source.blocks[index];
        if !self.binding.matches(block)
            || anchor.offset > block.text.len()
            || !block.text.is_char_boundary(anchor.offset)
        {
            return None;
        }
        let batch = self
            .batches
            .partition_point(|batch| batch.blocks.end <= index);
        self.batches
            .get(batch)?
            .blocks
            .contains(&index)
            .then_some(batch)
    }

    pub(crate) fn matches_source(&self, source: &Source, xhtml: &str) -> bool {
        xhtml.len() <= SOURCE_LIMIT
            && source.revision == self.revision
            && source
                .blocks
                .first()
                .is_some_and(|block| self.binding.matches(block))
            && source.matches_input(xhtml)
            && canonical_digest(source, &self.binding) == Some(self.canonical_digest)
            && ring::digest::digest(&ring::digest::SHA256, xhtml.as_bytes()).as_ref()
                == self.source_digest.as_slice()
    }

    pub(crate) fn batches(&self) -> &[Batch] {
        &self.batches
    }
}
