//! Bounded Parley continuation for supported oversized EPUB chapters.
//!
//! A batch retains its original Source blocks and absolute y coordinates. It is
//! neither a page nor a replacement Probe. The live reader binds the stream's
//! page index to the current source and replays only the batches needed to
//! paint a verified page.
use super::*;
use crate::long_chapter_plan::{
    LayoutMode, LongChapterPlan, MAX_BATCH_BLOCKS, cancelled, simple_source,
};
use std::sync::atomic::AtomicBool;

// Bound the single synchronous shaping operation as well as the parent plan's
// 128 KiB work batch. The shaping library cannot be interrupted mid-paragraph.
const MAX_PARAGRAPH_BYTES: usize = 16 * 1024;

/// Exact scalar layout state after one complete work batch. This is valid only
/// with the same immutable plan, source, styles, settings, rules and font set.
#[derive(Clone)]
pub(crate) struct SimpleLayoutCheckpoint {
    next_batch: usize,
    line_count: usize,
    y: f32,
    maximum: f32,
    normal_gap: f32,
    trailing_gap: f32,
    painted_bottom: f32,
    previous_top: Option<f32>,
    row_flow: crate::table_layout::RowFlow,
}

pub(crate) struct SimpleLayoutStream<'a> {
    plan: &'a LongChapterPlan,
    source: &'a Source,
    styles: &'a [Style],
    rules: &'a crate::user_hyphenation::Rules,
    cancel: &'a AtomicBool,
    settings: Settings,
    flow: ChapterFlow,
    context: LayoutContext<PaintBrush>,
    fonts: FontContext,
    row_flow: crate::table_layout::RowFlow,
    next_batch: usize,
    line_count: usize,
    y: f32,
    maximum: f32,
    normal_gap: f32,
    trailing_gap: f32,
    painted_bottom: f32,
    previous_top: Option<f32>,
    failed: bool,
}

impl<'a> SimpleLayoutStream<'a> {
    /// The supplied font context and complete styles must later be bound to a
    /// verified source/settings/font snapshot before any reading handoff.
    pub(crate) fn new(
        plan: &'a LongChapterPlan,
        source: &'a Source,
        xhtml: &str,
        styles: &'a [Style],
        user_styles: &crate::user_styles::Snapshot,
        rules: &'a crate::user_hyphenation::Rules,
        settings: Settings,
        mode: LayoutMode,
        fonts: FontContext,
        cancel: &'a AtomicBool,
    ) -> Result<Self> {
        cancelled(cancel)?;
        settings.validate()?;
        if !plan.matches_source(source, xhtml)
            || !simple_source(source, styles)
            || !user_styles.supports_windowed_simple_chapter()
            || mode.scroll
            || mode.facing_pages
            || mode.hidden_flows
            || mode.in_page_notes
            || settings.in_page_footnotes
            || settings.block_margins != crate::BlockMarginMode::Ignore
            || settings.block_width != crate::BlockWidthMode::Ignore
            || settings.inline_block_mode != crate::InlineBlockMode::Ignore
            || settings.fit_content != crate::FitContentMode::Ignore
            || settings.float_mode != crate::FloatMode::Ignore
            || settings.empty_break_mode != crate::EmptyBreakMode::Current
            || settings.empty_break_geometry
            || settings.publisher_horizontal_margins
            || settings.publisher_signed_horizontal_margins
            || settings.publisher_block_height
            || source.elements.iter().enumerate().any(|(index, element)| {
                element.direction_hint.is_some()
                    || element.parent.is_some_and(|parent| parent >= index)
            })
            || styles.iter().any(|style| {
                style.paragraph_edges != [false; 2]
                    || style.cjk_tailored
                    || !style.size_scale.is_finite()
                    || style.size_scale <= 0.0
            })
            || source.blocks.iter().any(|block| {
                block.text.len() > MAX_PARAGRAPH_BYTES
                    || block
                        .styled_text
                        .windows(2)
                        .any(|runs| runs[0].bytes.end > runs[1].bytes.start)
                    || block.styled_text.iter().any(|run| {
                        run.element >= styles.len()
                            || run.preserve_whitespace
                            || run.bytes.start > run.bytes.end
                            || run.bytes.end > block.text.len()
                            || !block.text.is_char_boundary(run.bytes.start)
                            || !block.text.is_char_boundary(run.bytes.end)
                    })
            })
        {
            return Err("This long chapter layout is not supported by the simple stream".into());
        }
        cancelled(cancel)?;
        let flow = ChapterFlow::from_styles(source, styles)?;
        if !flow.is_identity() {
            return Err("This long chapter has a projected text flow".into());
        }
        let normal_gap = settings.font_size * 0.32;
        Ok(Self {
            plan,
            source,
            styles,
            rules,
            cancel,
            settings,
            flow,
            context: LayoutContext::new(),
            fonts,
            row_flow: Default::default(),
            next_batch: 0,
            line_count: 0,
            y: 0.0,
            maximum: 0.0,
            normal_gap,
            trailing_gap: normal_gap,
            painted_bottom: 0.0,
            previous_top: None,
            failed: false,
        })
    }

    /// Recreate a stream at a previously captured boundary, validating that
    /// the checkpoint still fits the same plan and settings.
    pub(crate) fn new_at_checkpoint(
        plan: &'a LongChapterPlan,
        source: &'a Source,
        xhtml: &str,
        styles: &'a [Style],
        user_styles: &crate::user_styles::Snapshot,
        rules: &'a crate::user_hyphenation::Rules,
        settings: Settings,
        mode: LayoutMode,
        fonts: FontContext,
        cancel: &'a AtomicBool,
        checkpoint: &SimpleLayoutCheckpoint,
    ) -> Result<Self> {
        let mut stream = Self::new(
            plan,
            source,
            xhtml,
            styles,
            user_styles,
            rules,
            settings,
            mode,
            fonts,
            cancel,
        )?;
        if checkpoint.next_batch >= plan.batches().len()
            || checkpoint.line_count > MAX_LINES
            || !checkpoint.y.is_finite()
            || checkpoint.y < 0.0
            || !checkpoint.maximum.is_finite()
            || checkpoint.maximum < 0.0
            || !checkpoint.normal_gap.is_finite()
            || checkpoint.normal_gap.to_bits() != stream.normal_gap.to_bits()
            || !checkpoint.trailing_gap.is_finite()
            || !checkpoint.painted_bottom.is_finite()
            || checkpoint.previous_top.is_some_and(|top| !top.is_finite())
        {
            return Err("The long chapter checkpoint is invalid".into());
        }
        stream.next_batch = checkpoint.next_batch;
        stream.line_count = checkpoint.line_count;
        stream.y = checkpoint.y;
        stream.maximum = checkpoint.maximum;
        stream.trailing_gap = checkpoint.trailing_gap;
        stream.painted_bottom = checkpoint.painted_bottom;
        stream.previous_top = checkpoint.previous_top;
        stream.row_flow = checkpoint.row_flow.clone();
        Ok(stream)
    }

    /// Capture the continuation state after a completed batch.
    pub(crate) fn checkpoint(&self) -> Result<SimpleLayoutCheckpoint> {
        if self.failed
            || self.next_batch > self.plan.batches().len()
            || !self.y.is_finite()
            || !self.maximum.is_finite()
            || !self.trailing_gap.is_finite()
            || !self.painted_bottom.is_finite()
        {
            return Err("The long chapter checkpoint is unavailable".into());
        }
        Ok(SimpleLayoutCheckpoint {
            next_batch: self.next_batch,
            line_count: self.line_count,
            y: self.y,
            maximum: self.maximum,
            normal_gap: self.normal_gap,
            trailing_gap: self.trailing_gap,
            painted_bottom: self.painted_bottom,
            previous_top: self.previous_top,
            row_flow: self.row_flow.clone(),
        })
    }

    /// Give a consumer borrowed layouts and lines for exactly one work batch.
    /// They are dropped after the callback; work-batch boundaries are not pages.
    pub(crate) fn next_batch(
        &mut self,
        consume: impl FnOnce(&[Paragraph], &[Line]) -> Result<()>,
    ) -> Result<bool> {
        self.next_batch_owned(|paragraphs, lines| consume(&paragraphs, &lines))
    }

    /// Return the shaped batch to a bounded replay consumer.
    pub(crate) fn next_batch_owned(
        &mut self,
        consume: impl FnOnce(Vec<Paragraph>, Vec<Line>) -> Result<()>,
    ) -> Result<bool> {
        if self.failed {
            return Err("The long chapter layout stream is unavailable".into());
        }
        let result = self.produce_batch(consume);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn produce_batch(
        &mut self,
        consume: impl FnOnce(Vec<Paragraph>, Vec<Line>) -> Result<()>,
    ) -> Result<bool> {
        cancelled(self.cancel)?;
        let Some(batch) = self.plan.batches().get(self.next_batch) else {
            return Ok(false);
        };
        if batch.blocks.len() > MAX_BATCH_BLOCKS {
            return Err("The long chapter batch has too many paragraphs".into());
        }
        let mut paragraphs = Vec::new();
        let mut lines = Vec::new();
        let width = self.settings.text_width();
        for index in batch.blocks.clone() {
            cancelled(self.cancel)?;
            let block = self
                .source
                .blocks
                .get(index)
                .ok_or("The long chapter changed")?;
            let projection = self
                .flow
                .view(self.source, block)
                .map_err(|error| error.to_string())?;
            let mut input = self.flow.layout_block(self.source, block)?;
            input.first_line = true;
            let style = self
                .styles
                .get(input.element)
                .ok_or("The long chapter styles changed")?;

            // These are the same operations and order as the no-table,
            // no-margin, no-float branch of ParleyChapter::layout_flow.
            self.trailing_gap = self.normal_gap;
            self.y = self.row_flow.enter(None, self.y);
            self.y += 0.0;
            let indent =
                LineIndent::with_grid(style, width, width, self.settings.font_size, 0, None);
            let (layout, ruby) = layout_with_images(
                &mut self.context,
                &mut self.fonts,
                &input,
                width - indent.common,
                self.settings,
                self.styles,
                &[],
                indent,
                self.rules,
                None,
            );
            cancelled(self.cancel)?;
            if !ruby.is_empty() || self.line_count.saturating_add(layout.len()) > MAX_LINES {
                return Err("The long chapter exceeds the simple layout limit".into());
            }
            let origin = self.y + 0.0
                - layout
                    .lines()
                    .next()
                    .map_or(0.0, |line| line.metrics().content_block_min_coord.min(0.0));
            let rtl = layout.is_rtl();
            let x = 0.0 + if rtl { 0.0 } else { indent.common as f32 };
            let mut bottom = origin + layout.height().max(0.0);
            for line in layout.lines() {
                let metrics = line.metrics();
                let top = origin + metrics.content_block_min_coord;
                let line_bottom = origin + metrics.content_block_max_coord;
                bottom = bottom.max(line_bottom);
                if !top.is_finite()
                    || !line_bottom.is_finite()
                    || top > line_bottom
                    || self
                        .previous_top
                        .is_some_and(|previous| top.total_cmp(&previous).is_lt())
                {
                    return Err("The long chapter lines cannot be streamed safely".into());
                }
                self.previous_top = Some(top);
                let mut source_line = Line {
                    block: block.key,
                    bytes: line.text_range(),
                    spans: None,
                    additional_sources: None,
                    raw_whitespace: None,
                    top,
                    bottom: line_bottom,
                    page_extension: [0.0; 2],
                    missing_glyphs: line
                        .items()
                        .filter_map(|item| match item {
                            PositionedLayoutItem::GlyphRun(run) => Some(
                                run.positioned_glyphs()
                                    .filter(|glyph| glyph.id == 0)
                                    .count(),
                            ),
                            _ => None,
                        })
                        .sum(),
                };
                source_line.project(&projection, line.text_range())?;
                lines.push(source_line);
            }
            self.line_count += layout.len();
            for line in layout.lines().rev() {
                input
                    .text
                    .get(line.text_range())
                    .ok_or("The long chapter line is unavailable")?;
                let has_painted_content = line.items().any(|item| match item {
                    PositionedLayoutItem::GlyphRun(run) if run.style().brush.painted => {
                        input
                            .text
                            .get(run.run().text_range())
                            .is_some_and(|text| text.chars().any(|ch| !ch.is_whitespace()))
                            || run.advance() > 0.0
                                && run.style().brush.decoration != TextDecoration::default()
                    }
                    _ => false,
                });
                if has_painted_content {
                    self.painted_bottom =
                        0.0_f32.max(origin + line.metrics().content_block_max_coord);
                    break;
                }
            }
            let text_bottom = bottom;
            self.y = text_bottom + self.normal_gap;
            if !self.y.is_finite() || self.y > self.settings.chapter_height_limit() {
                return Err("The chapter exceeds the Parley height limit".into());
            }
            self.maximum = self.maximum.max(bottom);
            paragraphs.push(Paragraph {
                key: block.key,
                column: 0.0..width as f32,
                signed_margin: false,
                max_bottom: self.maximum,
                x,
                y: origin,
                text_bottom,
                bottom,
                background: None,
                fitted: None,
                layout,
                ruby,
                flow: None,
                inline_text: None,
                raw_binding: None,
                raw_pre_line: None,
                joined_note: None,
            });
            self.y += 0.0;
        }
        cancelled(self.cancel)?;
        consume(paragraphs, lines)?;
        cancelled(self.cancel)?;
        self.next_batch += 1;
        Ok(true)
    }

    /// Only the complete, successful stream has the final original-layout
    /// extent. No proof of font/source freshness or reading integration is
    /// implied by this value.
    pub(crate) fn finish(self) -> Result<(f32, f32)> {
        if self.failed || self.next_batch != self.plan.batches().len() {
            return Err("The long chapter layout stream is incomplete".into());
        }
        cancelled(self.cancel)?;
        let extent = (self.row_flow.end(self.y) - self.trailing_gap)
            .max(0.0)
            .max(0.0);
        if !extent.is_finite() || extent > self.settings.chapter_height_limit() {
            return Err("The chapter exceeds the Parley height limit".into());
        }
        Ok((extent, self.painted_bottom))
    }
}
