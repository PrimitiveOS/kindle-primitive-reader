//! Source-verified handoff of actual shaped raw blocks to search workers.
//!
//! Styles alone are insufficient: layout may reject a raw candidate because
//! of fonts, width or unsupported content. Only an already shaped Probe can
//! produce this map. The worker revalidates it against its separately parsed
//! source before using any raw bytes.
#![allow(dead_code)]

use crate::{
    Anchor, Probe, Renderer, Result,
    source::{Block, Source},
};
use ring::digest::{SHA256, digest};
use std::collections::BTreeMap;
use std::ops::Range;

const MAX_RAW_BLOCKS: usize = 4096;

#[derive(Clone)]
pub(super) struct VerifiedRawLayout {
    book: u64,
    chapter: String,
    revision: u64,
    flow: super::FlowConfig,
    blocks: BTreeMap<usize, ([u8; 32], usize)>,
}

/// Construct raw anchors and canonical coverage with at most one grapheme
/// scan per bounded batch. A hit splitting a grapheme is rejected.
pub(super) fn matches_anchors(
    book: u64,
    chapter: &str,
    source: &Source,
    block: &Block,
    ranges: &[Range<usize>],
) -> Result<Vec<(Anchor, Anchor, Vec<Range<usize>>)>> {
    use crate::source::raw_whitespace::CaretAffinity;
    let bound = block
        .raw_whitespace
        .as_ref()
        .and_then(|s| s.bind(&block.text))
        .ok_or("The raw search source has changed")?;
    let anchor = |caret: crate::RawCaret| Anchor {
        offset: caret.canonical_offset,
        raw_caret: Some(caret),
        book,
        chapter: chapter.to_owned(),
        revision: source.revision,
        block: block.key,
        illustration: None,
    };
    let mut results = Vec::with_capacity(ranges.len());
    for chunk in ranges.chunks(4096) {
        let mut requests = Vec::with_capacity(chunk.len() * 2);
        for range in chunk {
            if range.start >= range.end || bound.raw().get(range.clone()).is_none() {
                return Err("The raw search result has invalid boundaries".into());
            }
            requests.push((range.start, CaretAffinity::After));
            requests.push((range.end, CaretAffinity::Before));
        }
        let carets = bound
            .carets_at(&requests)
            .ok_or("The raw search result splits a grapheme")?;
        for (range, pair) in chunk.iter().zip(carets.chunks_exact(2)) {
            let mut spans = Vec::new();
            let text = bound.text_spans();
            let start = text.partition_point(|span| span.raw.end <= range.start);
            for span in text[start..]
                .iter()
                .take_while(|span| span.raw.start < range.end)
            {
                let from = range.start.max(span.raw.start);
                let to = range.end.min(span.raw.end);
                if from < to {
                    spans.push(
                        (span.canonical.start + from - span.raw.start)
                            ..(span.canonical.start + to - span.raw.start),
                    );
                }
            }
            let gaps = bound.gaps();
            let start = gaps.partition_point(|gap| gap.raw.end <= range.start);
            for gap in gaps[start..]
                .iter()
                .take_while(|gap| gap.raw.start < range.end)
            {
                if !gap.canonical.is_empty()
                    && range.start < gap.raw.end
                    && range.end > gap.raw.start
                {
                    spans.push(gap.canonical.clone());
                }
            }
            spans.sort_unstable_by_key(|span| (span.start, span.end));
            let mut merged: Vec<Range<usize>> = Vec::new();
            for span in spans {
                if let Some(last) = merged.last_mut()
                    && span.start <= last.end
                {
                    last.end = last.end.max(span.end);
                } else {
                    merged.push(span);
                }
            }
            results.push((anchor(pair[0].clone()), anchor(pair[1].clone()), merged));
        }
    }
    Ok(results)
}

/// Search canonical blocks and only those virtual blocks proven installed by
/// the renderer, retaining document order without changing canonical indices.
pub(super) fn ordered_blocks<'a>(
    source: &'a Source,
    layout: Option<&VerifiedRawLayout>,
) -> Vec<&'a Block> {
    let mut blocks: Vec<_> = source.blocks.iter().collect();
    if let Some(layout) = layout {
        blocks.extend(
            source
                .raw_only_blocks
                .iter()
                .filter(|block| layout.contains(block.key)),
        );
        blocks.sort_unstable_by_key(|block| block.key);
    }
    blocks
}

impl VerifiedRawLayout {
    pub(super) fn from_probe(probe: &Probe) -> Result<Self> {
        let Renderer::Parley(renderer) = &probe.renderer else {
            if probe.lines.iter().any(|line| line.raw_whitespace.is_some()) {
                return Err("Raw search lines have no horizontal layout".into());
            }
            return Ok(Self {
                book: probe.book,
                chapter: probe.chapter.clone(),
                revision: probe.source.revision,
                flow: probe.search_flow_config.clone(),
                blocks: BTreeMap::new(),
            });
        };
        let mut blocks = BTreeMap::new();
        let mut line_groups = BTreeMap::<usize, Vec<&crate::Line>>::new();
        for line in &probe.lines {
            line_groups.entry(line.block).or_default().push(line);
        }
        for paragraph in renderer.paragraphs() {
            if paragraph.raw_binding.is_none() {
                continue;
            }
            let block = probe
                .raw_source_block(paragraph.key)
                .ok_or("A raw search block is unavailable")?;
            if !paragraph.is_verified_raw(block) || blocks.len() >= MAX_RAW_BLOCKS {
                return Err("A raw search layout cannot be verified".into());
            }
            let bound = block
                .raw_whitespace
                .as_ref()
                .and_then(|s| s.bind(&block.text))
                .ok_or("A raw search source is unavailable")?;
            let mut requests = Vec::new();
            let mut actual = Vec::new();
            for line in line_groups.get(&block.key).into_iter().flatten() {
                let raw = line
                    .raw_whitespace
                    .as_ref()
                    .ok_or("A raw search block has mixed line types")?;
                requests.push((raw.start.raw_offset, raw.start.affinity));
                requests.push((raw.end.raw_offset, raw.end.affinity));
                actual.push(raw);
            }
            if actual.len() != paragraph.layout.len() || requests.len() > 8192 {
                return Err("A raw search block has incomplete line metadata".into());
            }
            let verified = bound
                .carets_at(&requests)
                .ok_or("A raw search line has an invalid caret")?;
            for ((line, raw), pair) in paragraph
                .layout
                .lines()
                .zip(actual)
                .zip(verified.chunks_exact(2))
            {
                let bytes = line.text_range();
                if raw.start != pair[0]
                    || raw.end != pair[1]
                    || raw.start.raw_offset != bytes.start
                    || raw.end.raw_offset != bytes.end
                {
                    return Err("A raw search line differs from its shaped text".into());
                }
            }
            let hash: [u8; 32] = digest(&SHA256, bound.raw().as_bytes())
                .as_ref()
                .try_into()?;
            if blocks
                .insert(block.key, (hash, bound.raw().len()))
                .is_some()
            {
                return Err("A raw search block has ambiguous layouts".into());
            }
        }
        if probe
            .lines
            .iter()
            .any(|line| line.raw_whitespace.is_some() && !blocks.contains_key(&line.block))
        {
            return Err("A raw search line has no verified paragraph".into());
        }
        Ok(Self {
            book: probe.book,
            chapter: probe.chapter.clone(),
            revision: probe.source.revision,
            flow: probe.search_flow_config.clone(),
            blocks,
        })
    }

    pub(super) fn verify(
        &self,
        book: u64,
        chapter: &str,
        source: &Source,
        flow: &super::FlowConfig,
    ) -> Result<()> {
        if self.book != book
            || self.chapter != chapter
            || self.revision != source.revision
            || &self.flow != flow
        {
            return Err("The raw search publication or chapter changed".into());
        }
        for (key, (hash, length)) in &self.blocks {
            let block = source
                .blocks
                .binary_search_by_key(key, |block| block.key)
                .ok()
                .map(|index| &source.blocks[index])
                .or_else(|| {
                    source
                        .raw_only_blocks
                        .binary_search_by_key(key, |block| block.key)
                        .ok()
                        .map(|index| &source.raw_only_blocks[index])
                })
                .ok_or("A raw search block has moved")?;
            let bound = block
                .raw_whitespace
                .as_ref()
                .and_then(|s| s.bind(&block.text))
                .ok_or("A raw search source has changed")?;
            if bound.raw().len() != *length
                || digest(&SHA256, bound.raw().as_bytes()).as_ref() != hash
            {
                return Err("A raw search source has changed".into());
            }
        }
        Ok(())
    }

    pub(super) fn contains(&self, key: usize) -> bool {
        self.blocks.contains_key(&key)
    }
}
