//! Place direct sibling empty boxes in the horizontal flow. Source text and
//! physical page cuts have separate coordinates and separate ownership.
use super::{Chain, Edge, Leaf, Rules};
use crate::{
    EmptyBreakMode, Settings,
    book_style::{ImageLength, Style, WritingMode, borders::Pattern, flow::Display},
    source::Source,
    vertical_padding::Fill,
};
use std::ops::Range;

#[derive(Clone, Copy)]
struct Band {
    owner: usize,
    start: f32,
    end: f32,
    gray: u8,
}

pub(crate) struct PositionedRun {
    pub target: usize,
    pub cuts: Box<[f32]>,
}

#[derive(Default)]
pub(crate) struct Run {
    extent: f32,
    cuts: Vec<f32>,
    bands: Vec<Band>,
}

struct BoxGeometry {
    owner: usize,
    leaf: Leaf,
    margins: [f32; 2],
    padding: [f32; 2],
    borders: [(f32, Option<u8>); 2],
    body: f32,
    background: Option<u8>,
}

pub(crate) fn plan(
    source: &Source,
    styles: &[Style],
    settings: Settings,
) -> Option<Vec<Option<Run>>> {
    let mode = settings.empty_break_mode;
    if !settings.empty_break_geometry
        || mode == EmptyBreakMode::Current
        || !styles.iter().any(|style| forced(style.page_breaks))
    {
        return None;
    }

    let mut occupied = vec![false; styles.len()];
    let mut structured = vec![false; styles.len()];
    let mut blocked = vec![false; styles.len()];
    for (index, node) in source.elements.iter().enumerate() {
        blocked[index] = node.parent.is_some_and(|parent| blocked[parent])
            || styles[index].float_side.is_some()
            || styles[index].writing_mode != WritingMode::Horizontal
            || matches!(node.tag.as_str(), "table" | "ul" | "ol" | "dl" | "ruby");
        if let Some(parent) = node.parent
            && !styles[index].flow.suppressed
            && !matches!(Display::native(&node.tag), Display::Inline)
        {
            structured[parent] = true;
        }
    }
    for block in &source.blocks {
        occupied[block.element] |= !styles[block.element].flow.suppressed;
    }
    for (index, node) in source.elements.iter().enumerate().rev() {
        if let Some(parent) = node.parent {
            occupied[parent] |= occupied[index];
            structured[parent] |= structured[index];
        }
    }

    let mut runs: Vec<_> = (0..source.blocks.len()).map(|_| None).collect();
    let mut previous = None;
    for (index, block) in source.blocks.iter().enumerate() {
        let owner = block.element;
        if styles[owner].flow.suppressed {
            continue;
        }
        let Some(prior) = previous.replace(owner) else {
            continue;
        };
        let node = &source.elements[owner];
        if blocked[owner]
            || block.table_cell.is_some()
            || block.list_item.is_some()
            || block.text.is_empty()
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || node.anonymous
            || source.elements[prior].anonymous
            || styles[owner].display != Display::Block
            || styles[prior].display != Display::Block
            || node.parent != source.elements[prior].parent
            || !simple_neighbor(&styles[owner], settings)
            || !simple_neighbor(&styles[prior], settings)
        {
            continue;
        }

        let mut cursor = node.previous_sibling;
        let mut siblings = Vec::new();
        while let Some(sibling) = cursor.filter(|&sibling| sibling != prior) {
            let element = &source.elements[sibling];
            if !styles[sibling].flow.suppressed {
                if blocked[sibling]
                    || occupied[sibling]
                    || structured[sibling]
                    || element.selector_only
                    || element.anonymous
                    || Display::native(&element.tag) != Display::Block
                    || matches!(element.tag.as_str(), "hr" | "html" | "body")
                    || siblings.len() >= 128
                {
                    break;
                }
                let Some(geometry) = measure(sibling, &styles[sibling], settings) else {
                    break;
                };
                siblings.push(geometry);
            }
            cursor = element.previous_sibling;
        }
        if cursor != Some(prior)
            || !siblings
                .iter()
                .any(|shape| shape.leaf.before || shape.leaf.after)
        {
            continue;
        }
        siblings.reverse();
        let run = Run::new(siblings, mode);
        if run.extent > 0.0 && run.extent <= settings.text_height() as f32 * 0.5 {
            runs[index] = Some(run);
        }
    }
    runs.iter().any(Option::is_some).then_some(runs)
}

fn forced(rules: Rules) -> bool {
    rules.before == Edge::Force || rules.after == Edge::Force
}

fn simple_neighbor(style: &Style, settings: Settings) -> bool {
    style.page_breaks == Rules::default()
        && style
            .vertical_margin
            .iter()
            .chain(&style.vertical_padding)
            .all(|edge| edge.box_pixels(settings.text_width(), settings.font_size) == 0.0)
}

fn measure(owner: usize, style: &Style, settings: Settings) -> Option<BoxGeometry> {
    if style.display != Display::Block
        || style.page_breaks.keep_together
        || style.page_breaks.before == Edge::Avoid
        || style.page_breaks.after == Edge::Avoid
        || style
            .image_width
            .is_some_and(|width| !matches!(width, ImageLength::Auto))
        || style.image_max_height.is_some()
        || style.image_max_width.is_some()
    {
        return None;
    }
    let width = settings.text_width();
    let font = settings.font_size;
    let margins = style
        .vertical_margin
        .map(|edge| edge.box_pixels(width, font));
    let padding = style
        .vertical_padding
        .map(|edge| edge.box_pixels(width, font));
    let mut borders = [(0.0, None); 2];
    if let Some(edges) = &style.borders {
        for (side, border) in edges.iter().enumerate() {
            if matches!(border.pattern, Pattern::None | Pattern::Hidden) {
                continue;
            }
            let size = border.width.box_pixels(width, font);
            if size <= 0.0 {
                continue;
            }
            if border.pattern != Pattern::Solid || side % 2 != 0 {
                return None;
            }
            let gray = border
                .gray
                .and_then(|(gray, alpha)| (style.flow.painted && alpha == 255).then_some(gray));
            if border.gray.is_some_and(|(_, alpha)| alpha != 255) {
                return None;
            }
            borders[side / 2] = (size, gray);
        }
    }
    let body = match (settings.empty_break_mode, style.image_height) {
        (EmptyBreakMode::Coalesce, _) | (_, None | Some(ImageLength::Auto)) => 0.0,
        (_, Some(ImageLength::Fraction(_))) => return None,
        (_, Some(height)) => height
            .pixels(0, settings.styled_font(style.size_scale))?
            .floor(),
    };
    if margins
        .iter()
        .chain(&padding)
        .chain(&[body])
        .any(|size| !size.is_finite() || *size < 0.0)
    {
        return None;
    }
    let legacy = settings.empty_break_mode == EmptyBreakMode::PreserveStructure;
    let inner_extent = body > 0.0
        || padding.iter().any(|&p| p > 0.0)
        || borders.iter().any(|&(width, _)| width > 0.0);
    let margin = margins.iter().any(|&p| p > 0.0);
    if !legacy && margin && !inner_extent {
        // The pinned enhanced renderer has a separate path for collapsed
        // margins on zero-height boxes. Keep its existing topology here.
        return None;
    }
    Some(BoxGeometry {
        owner,
        margins,
        padding,
        borders,
        body,
        leaf: Leaf {
            before: style.page_breaks.before == Edge::Force,
            after: style.page_breaks.after == Edge::Force,
            inner_extent,
            margin,
        },
        background: style.flow.painted.then_some(style.background).flatten(),
    })
}

impl Run {
    fn new(boxes: Vec<BoxGeometry>, mode: EmptyBreakMode) -> Self {
        let legacy = mode == EmptyBreakMode::PreserveStructure;
        let coalesce = mode == EmptyBreakMode::Coalesce;
        let mut chain = Chain::default();
        let mut result = Self::default();
        let mut trailing = 0.0_f32;
        for shape in boxes {
            let (before, after) = if coalesce {
                (shape.leaf.before, shape.leaf.after)
            } else {
                chain.append(&shape.leaf, None, legacy)
            };
            if before && (!coalesce || result.cuts.is_empty()) {
                result.cuts.push(result.extent);
            }
            let leading = shape.margins[0];
            result.extent += if legacy {
                leading
            } else {
                trailing.max(leading)
            };
            let top = shape.padding[0] + shape.borders[0].0;
            let bottom = shape.padding[1] + shape.borders[1].0;
            let height = if legacy {
                top.max(shape.body) + bottom
            } else {
                top + shape.body + bottom
            };
            let start = result.extent;
            let end = start + height;
            if let Some(gray) = shape.background {
                result.bands.push(Band {
                    owner: shape.owner,
                    start,
                    end,
                    gray,
                });
            }
            for ((thickness, gray), range) in [
                (shape.borders[0], start..start + shape.borders[0].0),
                (shape.borders[1], end - shape.borders[1].0..end),
            ] {
                if let Some(gray) = gray
                    && thickness > 0.0
                {
                    result.bands.push(Band {
                        owner: shape.owner,
                        start: range.start,
                        end: range.end,
                        gray,
                    });
                }
            }
            result.extent = end;
            trailing = if legacy {
                result.extent += shape.margins[1];
                0.0
            } else if mode == EmptyBreakMode::PreserveSized
                && shape.leaf.after
                && shape.leaf.inner_extent
            {
                0.0
            } else {
                shape.margins[1]
            };
            if after && (!coalesce || result.cuts.is_empty()) {
                result.cuts.push(result.extent);
            }
        }
        result.extent += trailing;
        result
    }

    pub(crate) fn place(
        &self,
        target: usize,
        y: f32,
        width: u32,
        fills: &mut Vec<Fill>,
    ) -> (f32, PositionedRun) {
        // Whole-pixel relative box dimensions are stable when capture clips a
        // partial page. This affects only the diagnostic physical policy.
        let start = y.ceil();
        let columns: Range<u32> = 0..width;
        for band in &self.bands {
            fills.push(Fill::rectangle(
                band.owner,
                columns.clone(),
                start + band.start,
                start + band.end,
                band.gray,
            ));
        }
        (
            start + self.extent,
            PositionedRun {
                target,
                cuts: self.cuts.iter().map(|&cut| start + cut).collect(),
            },
        )
    }
}
