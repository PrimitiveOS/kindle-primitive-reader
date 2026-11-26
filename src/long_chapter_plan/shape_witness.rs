//! Bounded transcript for replaying one immutable long-chapter layout batch.
//!
//! A matching transcript proves only that a later batch matches this isolated
//! snapshot's shaped geometry and glyph stream. It does not prove equivalence
//! to the live Reader's font provider or authorize a reading handoff.
use crate::parley_layout::Paragraph;
use crate::{Line, Result};
use parley::PositionedLayoutItem;
use ring::digest::{Context, SHA256};
use std::ops::Range;

const MAX_RUNS: usize = 100_000;
const MAX_CLUSTERS: usize = 1_000_000;
const MAX_LINES: usize = 30_000;
const MAX_GLYPHS: usize = 2_000_000;
const MAX_VARIATIONS: usize = 64;

pub(crate) struct ShapeWitness {
    digest: Context,
    paragraphs: usize,
    lines: usize,
    runs: usize,
    clusters: usize,
    glyphs: usize,
}

impl ShapeWitness {
    pub(crate) fn new(blocks: Range<usize>) -> Self {
        let mut digest = Context::new(&SHA256);
        digest.update(b"PrimitiveOS-long-chapter-shape-v1\0");
        digest.update(&(blocks.start as u64).to_le_bytes());
        digest.update(&(blocks.end as u64).to_le_bytes());
        Self {
            digest,
            paragraphs: 0,
            lines: 0,
            runs: 0,
            clusters: 0,
            glyphs: 0,
        }
    }

    fn u64(&mut self, value: usize) -> Result<()> {
        let value = u64::try_from(value).map_err(|_| "The shape witness is too large")?;
        self.digest.update(&value.to_le_bytes());
        Ok(())
    }

    fn f32(&mut self, value: f32) -> Result<()> {
        if !value.is_finite() {
            return Err("The shaped chapter contains invalid geometry".into());
        }
        self.digest.update(&value.to_bits().to_le_bytes());
        Ok(())
    }

    fn range(&mut self, value: Range<usize>) -> Result<()> {
        self.u64(value.start)?;
        self.u64(value.end)
    }

    fn add_count(count: &mut usize, amount: usize, limit: usize) -> Result<()> {
        *count = count
            .checked_add(amount)
            .filter(|value| *value <= limit)
            .ok_or("The shaped chapter exceeds the replay witness limit")?;
        Ok(())
    }

    /// Add one complete batch in source order. Each shaped line must have its
    /// corresponding canonical source projection; inline boxes are declined.
    pub(crate) fn observe(
        &mut self,
        paragraphs: &[Paragraph],
        source_lines: &[Line],
        font_ids: &[u64],
        font_digests: &[[u8; 32]],
    ) -> Result<()> {
        if font_ids.is_empty() || font_ids.len() != font_digests.len() {
            return Err("The shaped chapter font proof is unavailable".into());
        }
        let mut source_iter = source_lines.iter();
        for paragraph in paragraphs {
            Self::add_count(&mut self.paragraphs, 1, 256)?;
            self.u64(paragraph.key)?;
            self.f32(paragraph.x)?;
            self.f32(paragraph.y)?;
            self.f32(paragraph.column.start)?;
            self.f32(paragraph.column.end)?;
            self.f32(paragraph.layout.width())?;
            self.f32(paragraph.layout.height())?;
            self.digest.update(&[u8::from(paragraph.signed_margin)]);

            for shaped in paragraph.layout.lines() {
                Self::add_count(&mut self.lines, 1, MAX_LINES)?;
                let source = source_iter
                    .next()
                    .ok_or("A shaped line has no source location")?;
                if source.block != paragraph.key || source.missing_glyphs != 0 {
                    return Err("The shaped line does not match its source".into());
                }
                self.range(shaped.text_range())?;
                self.range(source.bytes.clone())?;
                self.u64(source.block)?;
                self.f32(source.top)?;
                self.f32(source.bottom)?;
                self.f32(source.page_extension[0])?;
                self.f32(source.page_extension[1])?;

                let metrics = shaped.metrics();
                for value in [
                    metrics.line_height,
                    metrics.baseline,
                    metrics.offset,
                    metrics.indent,
                    metrics.advance,
                    metrics.hanging_advance,
                    metrics.inline_min_coord,
                    metrics.inline_max_coord,
                    metrics.block_min_coord,
                    metrics.block_max_coord,
                    metrics.content_block_min_coord,
                    metrics.content_block_max_coord,
                ] {
                    self.f32(value)?;
                }

                for item in shaped.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        return Err("The shaped chapter contains an unsupported inline box".into());
                    };
                    let run = glyph_run.run();
                    Self::add_count(&mut self.runs, 1, MAX_RUNS)?;
                    let font_slot = font_ids
                        .iter()
                        .position(|id| *id == run.font().data.id())
                        .ok_or("The shaped chapter used an uncaptured font")?;
                    self.u64(font_slot)?;
                    self.digest.update(&font_digests[font_slot]);
                    self.u64(run.font().index as usize)?;
                    self.u64(glyph_run.style_index() as usize)?;
                    self.range(run.text_range())?;
                    self.f32(run.font_size())?;
                    self.f32(glyph_run.offset())?;
                    self.f32(glyph_run.baseline())?;
                    self.f32(glyph_run.advance())?;

                    let synthesis = run.synthesis();
                    self.digest.update(&[
                        u8::from(synthesis.embolden()),
                        u8::from(synthesis.skew().is_some()),
                    ]);
                    self.f32(synthesis.skew().unwrap_or(0.0))?;
                    let variations = run.normalized_coords();
                    if variations.len() > MAX_VARIATIONS {
                        return Err("The shaped font has too many variation axes".into());
                    }
                    self.u64(variations.len())?;
                    for coordinate in variations {
                        self.digest.update(&coordinate.to_bits().to_le_bytes());
                    }
                    let settings = synthesis.variation_settings();
                    if settings.len() > MAX_VARIATIONS {
                        return Err("The shaped font has too many synthesized axes".into());
                    }
                    self.u64(settings.len())?;
                    for (tag, value) in settings {
                        self.digest.update(&tag.to_be_bytes());
                        self.f32(*value)?;
                    }

                    for cluster in run.clusters() {
                        Self::add_count(&mut self.clusters, 1, MAX_CLUSTERS)?;
                        self.range(cluster.text_range())?;
                        for glyph in cluster.glyphs() {
                            Self::add_count(&mut self.glyphs, 1, MAX_GLYPHS)?;
                            self.u64(glyph.id as usize)?;
                            self.f32(glyph.x)?;
                            self.f32(glyph.y)?;
                            self.f32(glyph.advance)?;
                        }
                    }
                    for glyph in glyph_run.positioned_glyphs() {
                        Self::add_count(&mut self.glyphs, 1, MAX_GLYPHS)?;
                        self.u64(glyph.id as usize)?;
                        self.f32(glyph.x)?;
                        self.f32(glyph.y)?;
                        self.f32(glyph.advance)?;
                    }
                }
            }
        }
        if source_iter.next().is_some() {
            return Err("A source line has no shaped layout".into());
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<[u8; 32]> {
        if self.paragraphs == 0 || self.lines == 0 {
            return Err("The shaped chapter batch is empty".into());
        }
        Ok(self.digest.finish().as_ref().try_into()?)
    }
}
