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
    blocks: BTreeMap<usize, RawBlock>,
}

#[derive(Clone)]
struct RawBlock {
    hash: [u8; 32],
    length: usize,
    pre_line: Option<std::sync::Arc<crate::source::pre_line_projection::PreLineProjection>>,
}

/// Construct raw anchors and canonical coverage with at most one grapheme
/// scan per bounded batch. A hit splitting a grapheme is rejected.
pub(super) fn matches_anchors(
    book: u64,
    chapter: &str,
    source: &Source,
    block: &Block,
    ranges: &[Range<usize>],
    projection: Option<&crate::source::pre_line_projection::PreLineProjection>,
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
        side_bias: None,
    };
    let mut results = Vec::with_capacity(ranges.len());
    for chunk in ranges.chunks(4096) {
        let mut visible_requests = Vec::with_capacity(chunk.len() * 2);
        for range in chunk {
            let visible = projection.map_or(bound.raw(), |projection| projection.text());
            if range.start >= range.end || visible.get(range.clone()).is_none() {
                return Err("The raw search result has invalid boundaries".into());
            }
            visible_requests.push((range.start, crate::flow_projection::Affinity::After));
            visible_requests.push((range.end, crate::flow_projection::Affinity::Before));
        }
        let offsets = if let Some(projection) = projection {
            projection
                .visible_carets_at(&visible_requests)
                .ok_or("The search result splits a visible grapheme")?
        } else {
            visible_requests.iter().map(|(offset, _)| *offset).collect()
        };
        let mut requests = Vec::with_capacity(offsets.len());
        let mut raw_ranges = Vec::with_capacity(chunk.len());
        for pair in offsets.chunks_exact(2) {
            if pair[0] >= pair[1] {
                return Err("The raw search result has invalid source boundaries".into());
            }
            requests.push((pair[0], CaretAffinity::After));
            requests.push((pair[1], CaretAffinity::Before));
            raw_ranges.push(pair[0]..pair[1]);
        }
        let carets = bound
            .carets_at(&requests)
            .ok_or("The raw search result splits a grapheme")?;
        for (range, pair) in raw_ranges.iter().zip(carets.chunks_exact(2)) {
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
            let visible_requests: Vec<_> = paragraph
                .layout
                .lines()
                .flat_map(|line| {
                    let range = line.text_range();
                    [
                        (range.start, crate::flow_projection::Affinity::After),
                        (range.end, crate::flow_projection::Affinity::Before),
                    ]
                })
                .collect();
            let offsets = if let Some(projection) = &paragraph.raw_pre_line {
                projection
                    .visible_carets_at(&visible_requests)
                    .ok_or("A raw search line splits a visible grapheme")?
            } else {
                visible_requests.iter().map(|(offset, _)| *offset).collect()
            };
            for ((raw, pair), expected) in actual
                .into_iter()
                .zip(verified.chunks_exact(2))
                .zip(offsets.chunks_exact(2))
            {
                if raw.start != pair[0]
                    || raw.end != pair[1]
                    || raw.start.raw_offset != expected[0]
                    || raw.end.raw_offset != expected[1]
                {
                    return Err("A raw search line differs from its shaped text".into());
                }
            }
            let hash: [u8; 32] = digest(&SHA256, bound.raw().as_bytes())
                .as_ref()
                .try_into()?;
            if blocks
                .insert(
                    block.key,
                    RawBlock {
                        hash,
                        length: bound.raw().len(),
                        pre_line: paragraph.raw_pre_line.clone(),
                    },
                )
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
        for (key, saved) in &self.blocks {
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
            if bound.raw().len() != saved.length
                || digest(&SHA256, bound.raw().as_bytes()).as_ref() != saved.hash.as_slice()
                || saved.pre_line.as_ref().is_some_and(|projection| {
                    projection
                        .rebuild(bound.raw(), bound.hard_breaks())
                        .as_ref()
                        != Some(projection.as_ref())
                })
            {
                return Err("A raw search source has changed".into());
            }
        }
        Ok(())
    }

    pub(super) fn projection(
        &self,
        key: usize,
    ) -> Option<&crate::source::pre_line_projection::PreLineProjection> {
        self.blocks.get(&key)?.pre_line.as_deref()
    }

    pub(super) fn contains(&self, key: usize) -> bool {
        self.blocks.contains_key(&key)
    }

    /// A verified layout for a chapter with no raw-whitespace blocks. Source-only search fixtures use it
    /// because they have no probe to verify raw spans; it orders the blocks exactly as ordinary search does.
    pub(super) fn verified_without_raw_blocks(
        book: u64,
        chapter: &str,
        revision: u64,
        flow: super::FlowConfig,
    ) -> Self {
        Self {
            book,
            chapter: chapter.into(),
            revision,
            flow,
            blocks: BTreeMap::new(),
        }
    }
}
