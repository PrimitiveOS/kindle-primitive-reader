//! Bounded real geometry for empty retained block boxes. Canonical extraction
//! never creates fake text/keys: each target carries its immutable DOM origin.
use std::collections::BTreeMap;

use super::*;
use crate::{publisher_padding::BoxArea, source::is_block_tag};

const MAX_BOXES: usize = 512;

#[derive(Clone, Copy, Debug)]
pub(crate) struct BoundaryGeometry {
    pub origin: usize,
    pub block: usize,
    pub offset: usize,
    pub top: f32,
    pub bottom: f32,
    pub generated_only: bool,
}

impl BoundaryGeometry {
    pub(crate) fn position(&self, relative: u16) -> f32 {
        let fraction = f32::from(relative) / 10_000.;
        (self.top + (self.bottom - self.top) * fraction).min((self.bottom - 0.25).max(self.top))
    }
}

struct EmptyBox {
    owner: usize,
    area: BoxArea,
    before: f32,
    after: f32,
    children: Vec<usize>,
    point: bool,
}

pub(super) struct GeneratedBoxes {
    boxes: Vec<EmptyBox>,
    slots: BTreeMap<usize, Vec<usize>>,
    pub excluded: Vec<bool>,
    pub limitations: Vec<String>,
}

impl GeneratedBoxes {
    pub fn new(
        source: &Source,
        styles: &[Style],
        flow: &ChapterFlow,
        areas: Option<&[BoxArea]>,
        cfg: Settings,
    ) -> Result<Self> {
        let mut spans = vec![None::<(usize, usize)>; source.elements.len()];
        let mut content = vec![false; source.elements.len()];
        for (index, block) in source.blocks.iter().enumerate() {
            match &mut spans[block.element] {
                Some((_, last)) => *last = index,
                span @ None => *span = Some((index, index)),
            }
            if !styles[block.element].flow.suppressed {
                let view = flow
                    .view(source, block)
                    .map_err(|error| error.to_string())?;
                content[block.element] |= !view.text().is_empty()
                    || view
                        .images()
                        .map_or(!block.images.is_empty(), |images| !images.is_empty());
            }
            // A retained marker is real layout content even if all LI children
            // disappear. Its established marker path owns that geometry.
            if let Some(item) = block.list_item {
                if !styles[item.element].flow.suppressed
                    && flow
                        .ordinal(item)
                        .and_then(|ordinal| styles[item.element].marker.label(ordinal))
                        .is_some()
                {
                    content[item.element] = true;
                }
            }
        }
        for index in (0..source.elements.len()).rev() {
            if let Some(parent) = source.elements[index].parent {
                content[parent] |= content[index];
                if let Some((first, last)) = spans[index] {
                    match &mut spans[parent] {
                        Some((a, b)) => {
                            *a = (*a).min(first);
                            *b = (*b).max(last);
                        }
                        span @ None => *span = Some((first, last)),
                    }
                }
            }
        }
        let mut points = HashSet::new();
        let mut point_cap = false;
        for owner in source.fragment_owner_indices() {
            if !content[owner] && !styles[owner].flow.suppressed {
                if points.len() < 4096 || points.contains(&owner) {
                    points.insert(owner);
                } else {
                    point_cap = true;
                }
            }
        }
        let mut result = Self {
            boxes: Vec::new(),
            slots: BTreeMap::new(),
            excluded: vec![false; source.blocks.len()],
            limitations: Vec::new(),
        };
        let mut owners = BTreeMap::<usize, usize>::new();
        let mut excluded_context = Vec::with_capacity(source.elements.len());
        let mut unsupported = point_cap;
        let mut box_count = 0;
        for (owner, element) in source.elements.iter().enumerate() {
            let inside = matches!(
                element.tag.as_str(),
                "table"
                    | "tr"
                    | "td"
                    | "th"
                    | "tbody"
                    | "thead"
                    | "tfoot"
                    | "ul"
                    | "ol"
                    | "dl"
                    | "li"
            ) || element
                .parent
                .is_some_and(|parent| excluded_context[parent]);
            excluded_context.push(inside);
            let style = &styles[owner];
            if element.selector_only || element.anonymous || style.flow.suppressed || content[owner]
            {
                continue;
            }
            let mut parent = element.parent;
            let mut ancestor = None;
            let mut path = vec![owner];
            while let Some(current) = parent {
                if let Some(&index) = owners.get(&current) {
                    ancestor = Some(index);
                    break;
                }
                path.push(current);
                parent = source.elements[current].parent;
            }
            let area = if let Some(parent) = ancestor {
                let mut area = result.boxes[parent].area;
                for &element in path.iter().rev() {
                    area = BoxArea::generated_child(
                        area,
                        &styles[element],
                        &source.elements[element].tag,
                        cfg.font_size,
                    );
                }
                area
            } else {
                areas
                    .map_or_else(
                        || BoxArea::new(0, cfg.text_width(), None),
                        |areas| areas[owner],
                    )
                    .generated_root(style, &element.tag, cfg.font_size)
            };
            let padding = style.vertical_padding.map(|length| {
                length
                    .box_pixels(area.outer_width, cfg.font_size)
                    .min((cfg.text_height() / 2) as f32)
            });
            let edges = crate::table_borders::generated_box_edges(style, cfg.font_size);
            let before = padding[0] + edges[0].width as f32;
            let after = padding[1] + edges[2].width as f32;
            let real_box = is_block_tag(&element.tag)
                && (before + after > 0.
                    || edges.iter().any(|edge| edge.width > 0)
                    || style.background.is_some()
                    || style
                        .padding
                        .iter()
                        .any(|length| length.box_pixels(area.outer_width, cfg.font_size) > 0.));
            let point = !real_box;
            if point && !points.contains(&owner) {
                continue;
            }
            if !point && !is_block_tag(&element.tag) {
                continue;
            }
            // An inline empty ID inside a canonical paragraph uses its real
            // retained caret. Do not replace it with an unrelated block slot.
            if point {
                let n = source.blocks.partition_point(|block| {
                    source.elements[block.element].source_bytes.start <= element.source_bytes.start
                });
                if let Some(block) = n.checked_sub(1).and_then(|n| source.blocks.get(n)) {
                    if block.element != owner
                        && source.elements[block.element].source_bytes.end
                            > element.source_bytes.start
                    {
                        continue;
                    }
                }
            }
            // Empty native cells and lists need their existing row/marker plan;
            // do not invent a second independent box coordinate system there.
            if inside || style.display.requires_reclassification(&element.tag) {
                unsupported = true;
                continue;
            }
            if source.boundary_position(owner).is_none() {
                unsupported = true;
                continue;
            }
            if !point {
                if box_count == MAX_BOXES {
                    return Err("The chapter exceeds the 512 generated empty-box limit".into());
                }
                box_count += 1;
            }
            let index = result.boxes.len();
            result.boxes.push(EmptyBox {
                owner,
                area,
                before: if point { 0. } else { before },
                after: if point { 0. } else { after },
                children: Vec::new(),
                point,
            });
            owners.insert(owner, index);
            if let Some(parent) = ancestor {
                result.boxes[parent].children.push(index);
            } else {
                let slot = spans[owner].map_or_else(
                    || {
                        source.blocks.partition_point(|block| {
                            source.elements[block.element].source_bytes.start
                                < element.source_bytes.start
                        })
                    },
                    |(first, _)| first,
                );
                result.slots.entry(slot).or_default().push(index);
                if let Some((first, last)) = spans[owner] {
                    result.excluded[first..=last].fill(true);
                }
            }
        }
        if unsupported {
            result.limitations.push("Some empty generated boxes require unsupported table/list/reclassified or source-empty geometry; their DOM targets remain eligible but unresolved".into());
        }
        Ok(result)
    }

    pub fn has_slot(&self, slot: usize) -> bool {
        self.slots.contains_key(&slot)
    }

    pub fn emit(
        &self,
        slot: usize,
        y: &mut f32,
        gap: f32,
        source: &Source,
        styles: &[Style],
        cfg: Settings,
        boundaries: &mut BTreeMap<usize, BoundaryGeometry>,
        fills: &mut Vec<crate::vertical_padding::Fill>,
        frames: &mut Vec<crate::table_borders::Frame>,
    ) -> Result<()> {
        for &index in self.slots.get(&slot).into_iter().flatten() {
            let top = *y;
            *y = self.emit_box(index, top, source, styles, cfg, boundaries, fills, frames)?;
            if !self.boxes[index].point && *y > top {
                *y += gap;
            }
        }
        Ok(())
    }

    fn emit_box(
        &self,
        index: usize,
        top: f32,
        source: &Source,
        styles: &[Style],
        cfg: Settings,
        boundaries: &mut BTreeMap<usize, BoundaryGeometry>,
        fills: &mut Vec<crate::vertical_padding::Fill>,
        frames: &mut Vec<crate::table_borders::Frame>,
    ) -> Result<f32> {
        let node = &self.boxes[index];
        let mut bottom = top + node.before;
        for &child in &node.children {
            bottom = self.emit_box(
                child, bottom, source, styles, cfg, boundaries, fills, frames,
            )?;
        }
        bottom += node.after;
        if !bottom.is_finite() || bottom > MAX_HEIGHT {
            return Err("The chapter exceeds the generated empty-box height limit".into());
        }
        let (block, offset) = source
            .boundary_position(node.owner)
            .ok_or("The generated box has no canonical fallback")?;
        let origin = source.elements[node.owner].source_bytes.start;
        boundaries.insert(
            origin,
            BoundaryGeometry {
                origin,
                block,
                offset,
                top,
                bottom,
                generated_only: !node.point,
            },
        );
        let style = &styles[node.owner];
        if !node.point
            && style.flow.painted
            && let Some(gray) = style.background
        {
            fills.push(crate::vertical_padding::Fill::cell(
                node.owner, node.area, top, bottom, gray,
            ));
        }
        if !node.point {
            crate::table_borders::Frame::generated_box(
                frames,
                node.area,
                top,
                bottom,
                style,
                cfg.font_size,
            );
        }
        Ok(bottom)
    }
}
