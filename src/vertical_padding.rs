//! Vertical space around a reflow block is added once, at its first and last
//! source block. A wrapper around a table starts/ends outside the whole grid.
use crate::{
    Settings,
    book_style::Style,
    publisher_padding::BoxArea,
    source::{Source, is_block_tag},
};
#[derive(Clone, Copy, Default)]
pub(crate) struct Space {
    pub before: u32,
    pub after: u32,
    pub before_grid: u32,
    pub after_grid: u32,
}
pub(crate) fn layout(
    source: &Source,
    styles: &[Style],
    boxes: Option<&[BoxArea]>,
    cfg: Settings,
) -> Option<Vec<Space>> {
    let width = cfg.text_width();
    let font = cfg.font_size;
    let max = (cfg.text_height() / 2).max(1);
    if !styles.iter().any(|s| {
        s.vertical_padding
            .iter()
            .any(|p| p.pixels(width, font) > 0.)
    }) {
        return None;
    }
    let mut ranges: Vec<Option<(usize, usize)>> = vec![None; source.elements.len()];
    for (n, b) in source.blocks.iter().enumerate() {
        match &mut ranges[b.element] {
            Some((_, end)) => *end = n,
            span @ None => *span = Some((n, n)),
        }
    }
    for idx in (0..ranges.len()).rev() {
        if let (Some((first, last)), Some(parent)) = (ranges[idx], source.elements[idx].parent) {
            match &mut ranges[parent] {
                Some((lo, hi)) => {
                    *lo = (*lo).min(first);
                    *hi = (*hi).max(last);
                }
                span @ None => *span = Some((first, last)),
            }
        }
    }
    let mut in_cell = Vec::with_capacity(styles.len());
    let mut spaces = vec![Space::default(); source.blocks.len()];
    for (idx, node) in source.elements.iter().enumerate() {
        let inside =
            matches!(node.tag.as_str(), "td" | "th") || node.parent.is_some_and(|p| in_cell[p]);
        in_cell.push(inside);
        if node.selector_only
            || !(is_block_tag(&node.tag)
                || matches!(node.tag.as_str(), "html" | "ol" | "ul" | "dl"))
        {
            continue;
        }
        let Some((first, last)) = ranges[idx] else {
            continue;
        };
        let base = boxes.map(|b| b[idx].outer_width).unwrap_or_else(|| {
            if inside
                && let Some(cell) = source.blocks[first].table_cell
                && width as f32 / f32::from(cell.columns) >= 2.5 * font
            {
                cell.bounds(width, (font * 0.22).round() as u32).1
            } else {
                width
            }
        });
        let [top, bottom] = styles[idx]
            .vertical_padding
            .map(|p| (p.box_pixels(base, font) as u32).min(max));
        let lead = &mut spaces[first];
        let before = if !inside && source.blocks[first].table_cell.is_some() {
            &mut lead.before_grid
        } else {
            &mut lead.before
        };
        *before = before.saturating_add(top).min(max);
        let trail = &mut spaces[last];
        let after = if !inside && source.blocks[last].table_cell.is_some() {
            &mut trail.after_grid
        } else {
            &mut trail.after
        };
        *after = after.saturating_add(bottom).min(max);
    }
    Some(spaces)
}

#[derive(Clone, Copy)]
pub(crate) struct Fill {
    pub element: usize,
    top: f32,
    bottom: f32,
    left: u32,
    right: u32,
    gray: u8,
}
impl Fill {
    pub fn cell(element: usize, area: BoxArea, top: f32, bottom: f32, gray: u8) -> Self {
        Self {
            element,
            top,
            bottom,
            left: area.outer_x,
            right: area.outer_x + area.outer_width,
            gray,
        }
    }
    pub fn painted_bottom(&self) -> f32 {
        if self.gray < 255 { self.bottom } else { 0. }
    }
    pub fn paint(
        &self,
        page: &mut [u8],
        stride: usize,
        margin: usize,
        range: &std::ops::Range<f32>,
    ) {
        let start = ((self.top - range.start).floor().max(0.) as usize).min(page.len() / stride);
        let end = ((self.bottom - range.start).ceil().max(0.) as usize).min(page.len() / stride);
        for y in start..end {
            let offset = y * stride + margin;
            page[offset + self.left as usize..offset + self.right as usize].fill(self.gray);
        }
    }
}
/// Backgrounds extend through padding, including a wrapper's child paragraphs.
/// Layout coordinates stay separate from text hit bounds and source offsets.
pub(crate) fn fills(
    source: &Source,
    styles: &[Style],
    boxes: Option<&[BoxArea]>,
    positions: &[(f32, f32)],
    cfg: Settings,
) -> Vec<Fill> {
    if positions.is_empty() {
        return Vec::new();
    }
    let mut spans = vec![(f32::INFINITY, f32::NEG_INFINITY); source.elements.len()];
    for (b, &(top, bottom)) in source.blocks.iter().zip(positions) {
        let r = &mut spans[b.element];
        r.0 = r.0.min(top);
        r.1 = r.1.max(bottom);
    }
    let mut out = Vec::new();
    for idx in (0..spans.len()).rev() {
        let (mut top, mut bottom) = spans[idx];
        if !top.is_finite() {
            continue;
        }
        let el = &source.elements[idx];
        let style = &styles[idx];
        if !el.selector_only
            && (is_block_tag(&el.tag) || matches!(el.tag.as_str(), "html" | "ol" | "ul" | "dl"))
        {
            let (left, width) = boxes.map_or((0, cfg.text_width()), |b| {
                (b[idx].outer_x, b[idx].outer_width)
            });
            let [before, after] = style
                .vertical_padding
                .map(|v| (v.box_pixels(width, cfg.font_size) as u32).min(cfg.text_height() / 2));
            top -= before as f32;
            bottom += after as f32;
            if (before > 0 || after > 0)
                && let Some(gray) = style.background
            {
                out.push(Fill {
                    element: idx,
                    left,
                    right: left + width,
                    top,
                    bottom,
                    gray,
                });
            }
        }
        if let Some(parent) = el.parent {
            let p = &mut spans[parent];
            p.0 = p.0.min(top);
            p.1 = p.1.max(bottom);
        }
    }
    out.reverse();
    out
}
