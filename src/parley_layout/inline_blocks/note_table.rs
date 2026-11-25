//! A compact single-row inline box for supported tables inside joined notes.
use super::{InlineText, Segment, TextBox, slice_runs};
use crate::{
    Settings,
    book_style::Style,
    flow_view::LayoutBlock,
    parley_layout::{LineIndent, PaintBrush, make_paragraph, note_inline::Table},
    table_borders::{Frame, Stroke},
    vertical_padding::Fill,
};
use parley::{
    FontContext, InlineBox, InlineBoxKind, LayoutContext, PositionedInlineBox,
    PositionedLayoutItem, VerticalAlign,
};
use std::ops::Range;

pub(in crate::parley_layout) struct Plan {
    pub(in crate::parley_layout) text: InlineText,
    pub(in crate::parley_layout) runs: Vec<crate::source::StyledText>,
    pub(in crate::parley_layout) item: InlineBox,
    element: usize,
    bounds: Vec<Range<f32>>,
    edges: Vec<Stroke>,
    border: [Stroke; 4],
    background: Option<u8>,
}

pub(in crate::parley_layout) fn plan(
    context: &mut LayoutContext<PaintBrush>,
    fonts: &mut FontContext,
    block: &LayoutBlock<'_>,
    table: &Table,
    width: u32,
    settings: Settings,
    styles: &[Style],
) -> Option<Plan> {
    let root = &styles[table.element];
    if !root.border_collapse
        || table.cells.is_empty()
        || width == 0
        || root
            .padding
            .iter()
            .chain(&root.vertical_padding)
            .any(|pad| pad.box_pixels(width, settings.font_size) > 0.0)
    {
        return None;
    }
    let read = |style: &Style| {
        style
            .borders
            .as_ref()
            .map_or([Stroke::default(); 4], |edges| {
                std::array::from_fn(|index| {
                    let mut stroke = Stroke::read(edges[index], settings.font_size);
                    if !style.flow.painted {
                        stroke.gray = None;
                    }
                    stroke
                })
            })
    };
    let mut border = read(root);
    let mut rules = Vec::<[Stroke; 4]>::new();
    let mut cells = Vec::new();
    let mut baseline = 0.0_f32;
    let mut below = 0.0_f32;
    for &(element, ref range) in &table.cells {
        let style = &styles[element];
        let child = block.text_fragment(
            element,
            &block.text[range.clone()],
            slice_runs(block, range, 0),
        );
        let layout = make_paragraph(
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
            &Default::default(),
            None,
        );
        let content_width = layout.calculate_content_widths().max.ceil();
        if content_width < 1.0 || content_width > width as f32 || child.text.contains('\n') {
            return None;
        }
        let mut layout = layout;
        layout.break_all_lines(Some(content_width));
        if layout.len() != 1 {
            return None;
        }
        let height = layout.height();
        let base = layout.first_baseline()?;
        let padding = [
            style.vertical_padding[0],
            style.padding[1],
            style.vertical_padding[1],
            style.padding[0],
        ]
        .map(|edge| edge.box_pixels(width, settings.font_size));
        baseline = baseline.max(base + padding[0]);
        below = below.max(height - base + padding[2]);
        rules.push(read(style));
        cells.push((
            TextBox {
                source: range.clone(),
                layout,
                x: 0.0,
                y: 0.0,
                size: [content_width, height],
                padding,
                margin: [0.0; 4],
                gray: style.background.filter(|_| style.flow.painted),
            },
            base,
        ));
    }
    let stronger = |a: Stroke, b: Stroke| if a.width >= b.width { a } else { b };
    for rule in &rules {
        border[0] = stronger(border[0], rule[0]);
        border[2] = stronger(border[2], rule[2]);
    }
    border[3] = stronger(border[3], rules.first()?[3]);
    border[1] = stronger(border[1], rules.last()?[1]);
    let mut left = border[3].width as f32;
    let top = border[0].width as f32;
    let row_height = baseline + below;
    let total_height = row_height + top + border[2].width as f32;
    let mut bounds = Vec::new();
    let mut edges = Vec::new();
    let mut boxes = Vec::new();
    for (index, (mut cell, base)) in cells.into_iter().enumerate() {
        cell.padding[0] = baseline - base;
        cell.padding[2] = below - (cell.size[1] - base);
        cell.x = left + cell.padding[3];
        cell.y = top + cell.padding[0];
        let right = cell.x + cell.size[0] + cell.padding[1];
        let edge = rules
            .get(index + 1)
            .map_or_else(Stroke::default, |next| stronger(rules[index][1], next[3]));
        bounds.push(left..right + edge.width as f32);
        edges.push(edge);
        boxes.push(cell);
        left = right + edge.width as f32;
    }
    let total_width = left + border[1].width as f32;
    if total_width > width as f32 || total_height >= settings.text_height() as f32 {
        return None;
    }
    let first = table.cells.first()?.1.start;
    let last = table.cells.last()?.1.end;
    let mut text = InlineText {
        text: String::new(),
        segments: Vec::new(),
        boxes,
        generated: false,
        detached_initial: false,
    };
    let mut runs = Vec::new();
    for source in [0..first, last..block.text.len()] {
        if !source.is_empty() {
            let offset = text.text.len();
            runs.extend(slice_runs(block, &source, offset));
            text.text.push_str(&block.text[source.clone()]);
            text.segments.push(Segment { source, offset });
        }
    }
    if text.text.trim().is_empty() {
        return None;
    }
    Some(Plan {
        text,
        runs,
        item: InlineBox {
            id: 0,
            index: first,
            kind: InlineBoxKind::InFlow,
            width: total_width,
            height: total_height,
            baseline: Some(top + baseline),
            vertical_align: VerticalAlign::BASELINE,
        },
        element: table.element,
        bounds,
        edges,
        border,
        background: root.background.filter(|_| root.flow.painted),
    })
}

impl Plan {
    pub(in crate::parley_layout) fn missing_glyphs(&self) -> usize {
        self.text
            .boxes
            .iter()
            .flat_map(|cell| cell.layout.lines())
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

    pub(in crate::parley_layout) fn ranges(
        &self,
        host: Range<usize>,
        contains_table: bool,
    ) -> Vec<Range<usize>> {
        let mut ranges: Vec<_> = self
            .text
            .segments
            .iter()
            .filter_map(|part| {
                let hit = super::intersection(&part.layout(), &host)?;
                let start = part.source.start + hit.start - part.offset;
                Some(start..start + hit.len())
            })
            .collect();
        if contains_table {
            ranges.extend(self.text.boxes.iter().map(|cell| cell.source.clone()));
            ranges.sort_unstable_by_key(|range| range.start);
        }
        ranges
    }

    pub(in crate::parley_layout) fn place(
        &mut self,
        position: &PositionedInlineBox,
        x: f32,
        y: f32,
        frames: &mut Vec<Frame>,
        fills: &mut Vec<Fill>,
    ) {
        let left = x + position.x;
        let top = y + position.y;
        let right = (left + position.width).round() as u32;
        let bottom = top + position.height;
        let left_pixel = left.round() as u32;
        if let Some(gray) = self.background {
            fills.push(Fill::rectangle(
                self.element,
                left_pixel..right,
                top,
                bottom,
                gray,
            ));
        }
        Frame::push(
            frames,
            left_pixel,
            right - left_pixel,
            top,
            bottom,
            self.border,
        );
        for (range, &edge) in self.bounds.iter().zip(&self.edges) {
            let start = (left + range.start).round() as u32;
            let end = (left + range.end).round() as u32;
            Frame::push(
                frames,
                start,
                end - start,
                top,
                bottom,
                [
                    Stroke::default(),
                    edge,
                    Stroke::default(),
                    Stroke::default(),
                ],
            );
        }
        for cell in &mut self.text.boxes {
            cell.x += position.x;
            cell.y += position.y;
        }
    }
}
