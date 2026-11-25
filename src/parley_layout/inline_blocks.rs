//! Text-only CSS inline blocks. Canonical bytes always remain in the chapter
//! flow. Parley shapes the host and each atomic box independently.
use super::{LineIndent, PaintBrush, layout_with_images, make_paragraph};
use crate::{
    InlineBlockMode, Line, Result, Settings,
    book_style::{BoxMargin, BoxSizing, ImageLength, Style, flow::Display},
    flow_projection::{Affinity, Projection},
    flow_view::LayoutBlock,
    source::{Source, StyledText},
    user_hyphenation::{Opportunities, Rules},
};
use parley::{
    BaseDirection, FontContext, InlineBox, InlineBoxKind, Layout, LayoutContext,
    PositionedLayoutItem,
};
use std::ops::Range;

pub(super) mod generated;
pub(super) mod initial;
pub(super) mod note_table;

pub(super) const MAX_CHAPTER_BOXES: usize = 256;
const MAX_PARAGRAPH_BOXES: usize = 16;
const MAX_BOX_BYTES: usize = 1024;

pub(crate) struct InlineText {
    pub(super) text: String,
    segments: Vec<Segment>,
    pub(crate) boxes: Vec<TextBox>,
    generated: bool,
    detached_initial: bool,
}

struct Segment {
    /// Byte range in the visible text before boxing.
    source: Range<usize>,
    /// Start in the host shaped text, after removing the boxed bytes.
    offset: usize,
}

pub(crate) struct TextBox {
    pub(crate) source: Range<usize>,
    pub(crate) layout: Layout<PaintBrush>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    size: [f32; 2],
    padding: [f32; 4],
    margin: [f32; 4],
    gray: Option<u8>,
}

pub(super) struct Plan {
    pub(super) text: InlineText,
    pub(super) runs: Vec<StyledText>,
    pub(super) items: Vec<InlineBox>,
}

fn slice_runs(block: &LayoutBlock<'_>, range: &Range<usize>, offset: usize) -> Vec<StyledText> {
    let first = block
        .styled_text
        .partition_point(|run| run.bytes.end <= range.start);
    block.styled_text[first..]
        .iter()
        .take_while(|run| run.bytes.start < range.end)
        .map(|run| StyledText {
            bytes: offset + run.bytes.start.max(range.start) - range.start
                ..offset + run.bytes.end.min(range.end) - range.start,
            element: run.element,
            language: run.language,
            preserve_whitespace: run.preserve_whitespace,
        })
        .collect()
}

fn intersection(a: &Range<usize>, b: &Range<usize>) -> Option<Range<usize>> {
    let first = a.start.max(b.start);
    let last = a.end.min(b.end);
    (first < last).then_some(first..last)
}

impl Segment {
    fn layout(&self) -> Range<usize> {
        self.offset..self.offset + self.source.len()
    }
}

impl InlineText {
    pub(crate) fn layout_spans(
        &self,
        source: &Range<usize>,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        let source = source.clone();
        self.segments.iter().filter_map(move |segment| {
            let clipped = intersection(&segment.source, &source)?;
            let left = segment.offset + clipped.start - segment.source.start;
            Some(left..left + clipped.len())
        })
    }

    pub(crate) fn visible_offset(&self, offset: usize, affinity: Affinity) -> Option<usize> {
        if self.generated
            && offset < self.text.len()
            && !self
                .segments
                .iter()
                .any(|part| part.layout().contains(&offset))
        {
            return None;
        }
        let mut matches = self.segments.iter().filter_map(|part| {
            let range = part.layout();
            (range.start <= offset && offset <= range.end)
                .then(|| part.source.start + offset - range.start)
        });
        match affinity {
            Affinity::Before => matches.next(),
            Affinity::After => matches.next_back(),
        }
    }

    pub(super) fn project_line(
        &mut self,
        output: &mut Line,
        line: &parley::Line<'_, PaintBrush>,
        view: &Projection<'_>,
        first_line: bool,
    ) -> Result<()> {
        let range = line.text_range();
        let mut spans: Vec<_> = self
            .segments
            .iter()
            .filter_map(|part| {
                let clipped = intersection(&part.layout(), &range)?;
                let start = part.source.start + clipped.start - part.offset;
                Some(start..start + clipped.len())
            })
            .collect();
        if self.detached_initial && first_line {
            let initial = &self.boxes[0];
            spans.push(initial.source.clone());
            output.missing_glyphs += initial.missing_glyphs();
        }
        for item in line.items() {
            if let PositionedLayoutItem::InlineBox(item) = item {
                let child = &mut self.boxes[item.id as usize];
                child.x = item.x + child.margin[3] + child.padding[3];
                child.y = item.y + child.margin[0] + child.padding[0];
                spans.push(child.source.clone());
                output.missing_glyphs += child.missing_glyphs();
            }
        }
        spans.sort_unstable_by_key(|span| span.start);
        let canonical = spans
            .into_iter()
            .map(|span| {
                view.source_spans(span)
                    .ok_or("Invalid inline box source boundaries")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        output.set_source_spans(canonical.into_iter().flatten(), || {
            self.visible_offset(range.start, Affinity::After)
                .or_else(|| {
                    self.generated.then(|| {
                        self.segments
                            .iter()
                            .find(|part| part.offset >= range.end)
                            .map_or(view.text().len(), |part| part.source.start)
                    })
                })
                .and_then(|offset| view.source_offset(offset, Affinity::After))
                .unwrap_or(0)
        });
        Ok(())
    }

    pub(super) fn initial_bounds(&self) -> Option<Range<f32>> {
        self.detached_initial
            .then(|| self.boxes[0].painted_bounds().1)
    }
}

impl TextBox {
    fn missing_glyphs(&self) -> usize {
        self.layout
            .lines()
            .flat_map(|line| line.items())
            .filter_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => Some(
                    run.positioned_glyphs()
                        .filter(|glyph| glyph.id == 0)
                        .count(),
                ),
                _ => None,
            })
            .sum()
    }

    pub(crate) fn hit(&self, x: f32, y: f32) -> bool {
        (self.x - self.padding[3]..self.x + self.size[0] + self.padding[1]).contains(&x)
            && (self.y - self.padding[0]..self.y + self.size[1] + self.padding[2]).contains(&y)
    }

    pub(crate) fn local_span(&self, visible: &Range<usize>) -> Option<Range<usize>> {
        let hit = intersection(&self.source, visible)?;
        Some(hit.start - self.source.start..hit.end - self.source.start)
    }

    pub(super) fn painted_bounds(&self) -> (Range<f32>, Range<f32>) {
        (
            self.x - self.padding[3]..self.x + self.size[0] + self.padding[1],
            self.y - self.padding[0]..self.y + self.size[1] + self.padding[2],
        )
    }

    pub(super) fn background(&self) -> Option<u8> {
        self.gray
    }
}

struct Candidate {
    element: usize,
    authored: Range<usize>,
    text: Range<usize>,
}

fn boxes(
    block: &LayoutBlock<'_>,
    source: &Source,
    styles: &[Style],
    limit: usize,
) -> Option<Vec<Candidate>> {
    let mut boxes: Vec<(usize, Range<usize>)> = Vec::new();
    for run in block.styled_text.iter() {
        let mut owner = run.element;
        let mut found = None;
        while owner != block.element {
            let style = &styles[owner];
            match style.display {
                Display::Inline => {}
                Display::InlineBlock if found.is_none() => found = Some(owner),
                _ => return None,
            }
            owner = source.elements[owner].parent?;
        }
        let Some(owner) = found else { continue };
        if let Some((last, bytes)) = boxes.last_mut()
            && *last == owner
            && bytes.end == run.bytes.start
        {
            bytes.end = run.bytes.end;
        } else {
            if boxes.len() == limit || boxes.iter().any(|(previous, _)| *previous == owner) {
                return None;
            }
            boxes.push((owner, run.bytes.clone()));
        }
    }
    let mut candidates = Vec::with_capacity(boxes.len());
    for (element, authored) in boxes {
        let text = block.text.get(authored.clone())?;
        let trimmed = text.trim_matches(|ch: char| ch.is_ascii_whitespace());
        if trimmed.len() > MAX_BOX_BYTES || trimmed.contains('\n') {
            return None;
        }
        if !trimmed.is_empty() {
            let start = authored.start + text.len()
                - text
                    .trim_start_matches(|ch: char| ch.is_ascii_whitespace())
                    .len();
            candidates.push(Candidate {
                element,
                authored,
                text: start..start + trimmed.len(),
            });
        }
    }
    Some(candidates)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &LayoutBlock<'_>,
    source: &Source,
    styles: &[Style],
    width: u32,
    settings: Settings,
    rules: &Rules,
    remaining: usize,
) -> Option<Plan> {
    if remaining == 0
        || matches!(block.direction, BaseDirection::Rtl)
        || !block.images.is_empty()
        || !block.ruby.is_empty()
    {
        return None;
    }
    let candidates = boxes(block, source, styles, remaining.min(MAX_PARAGRAPH_BOXES))?;
    if candidates.is_empty() {
        return None;
    }

    let mut plan = Plan {
        text: InlineText {
            generated: false,
            detached_initial: false,
            text: String::new(),
            segments: Vec::new(),
            boxes: Vec::new(),
        },
        runs: Vec::new(),
        items: Vec::new(),
    };
    let mut start = 0;
    for candidate in &candidates {
        let element = candidate.element;
        let bytes = candidate.text.clone();
        plan.append(block, start..bytes.start, &candidates);
        let style = &styles[element];
        if style.borders.is_some()
            || style.float_side.is_some()
            || style.preformatted_spaces
            || matches!(style.image_height, Some(height) if !matches!(height, ImageLength::Auto))
            || style.min_width.is_some()
            || style.image_max_width.is_some()
            || style.min_height.is_some()
            || style.image_max_height.is_some()
            || matches!(style.image_width, Some(ImageLength::FitContent))
        {
            return None;
        }
        let edge =
            |length: crate::book_style::CssLength| length.box_pixels(width, settings.font_size);
        let padding = [
            style.vertical_padding[0],
            style.padding[1],
            style.vertical_padding[1],
            style.padding[0],
        ]
        .map(edge);
        let horizontal = style.horizontal_margin.map(|margin| match margin {
            BoxMargin::Length(length) => edge(length),
            BoxMargin::Auto => 0.0,
        });
        let vertical = style.vertical_margin.map(edge);
        let margin = [vertical[0], horizontal[1], vertical[1], horizontal[0]];
        if margin.iter().any(|&value| value < 0.0) {
            return None;
        }
        let horizontal = padding[1] + padding[3];
        let available = width as f32 - horizontal - margin[1] - margin[3];
        if available < 1.0 {
            return None;
        }

        let child = block.text_fragment(
            element,
            &block.text[bytes.clone()],
            slice_runs(block, &bytes, 0),
        );
        let declared = style
            .image_width
            .and_then(ImageLength::css_length)
            .map(edge);
        let content = if let Some(declared) = declared {
            let pad_in_width = style.box_sizing == BoxSizing::BorderBox
                || settings.inline_block_mode == InlineBlockMode::Flat;
            declared - if pad_in_width { horizontal } else { 0.0 }
        } else {
            make_paragraph(
                context,
                fonts,
                &child,
                None,
                settings,
                styles,
                &[],
                &[],
                false,
                LineIndent::default(),
                &Opportunities::default(),
                None,
            )
            .calculate_content_widths()
            .max
            .ceil()
            .min(available)
        };
        if content < 1.0 || content > available {
            return None;
        }
        let (layout, _) = layout_with_images(
            context,
            fonts,
            &child,
            content as u32,
            settings,
            styles,
            &[],
            LineIndent::default(),
            rules,
            None,
        );
        let height = layout.height();
        let baseline = layout.lines().last()?.metrics().baseline;
        let outer_height = height + padding[0] + padding[2] + margin[0] + margin[2];
        if outer_height >= settings.text_height() as f32 {
            return None;
        }

        plan.items.push(InlineBox {
            id: plan.text.boxes.len() as u64,
            index: plan.text.text.len(),
            kind: InlineBoxKind::InFlow,
            width: content + horizontal + margin[1] + margin[3],
            height: outer_height,
            baseline: Some(baseline + padding[0] + margin[0]),
            vertical_align: style.image_align,
        });
        start = bytes.end;
        plan.text.boxes.push(TextBox {
            source: bytes,
            layout,
            x: 0.0,
            y: 0.0,
            size: [content, height],
            padding,
            margin,
            gray: style.background.filter(|_| style.flow.painted),
        });
    }
    plan.append(block, start..block.text.len(), &candidates);
    Some(plan)
}

impl Plan {
    fn append(&mut self, block: &LayoutBlock<'_>, source: Range<usize>, boxes: &[Candidate]) {
        if source.is_empty() {
            return;
        }
        let offset = self.text.text.len();
        self.runs.extend(
            slice_runs(block, &source, offset)
                .into_iter()
                .filter(|run| {
                    let start = source.start + run.bytes.start - offset;
                    let end = source.start + run.bytes.end - offset;
                    // Extraction may assign collapsed boundary whitespace to the next
                    // node. Use the host style on that whitespace outside the box.
                    !boxes
                        .iter()
                        .any(|child| child.authored.start <= start && end <= child.authored.end)
                }),
        );
        self.text.text.push_str(&block.text[source.clone()]);
        self.text.segments.push(Segment { source, offset });
    }
}
