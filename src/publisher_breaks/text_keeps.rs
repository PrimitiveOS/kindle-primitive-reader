//! Oversized keep groups made exclusively of ordinary, in-flow paragraphs.
use crate::{
    FloatMode, Settings,
    book_style::{ImageLength, Style, WritingMode, flow::Display},
    source::Source,
};
use std::ops::Range;

pub(super) fn oversized(
    source: &Source,
    styles: &[Style],
    settings: Settings,
    extents: &[Option<Range<f32>>],
) -> Vec<bool> {
    if !settings.relax_oversized_publisher_keeps
        || settings.float_mode != FloatMode::Ignore
            && styles.iter().any(|style| style.float_side.is_some())
    {
        return Vec::new();
    }
    let page_height = settings.text_height() as f32;
    let width = settings.text_width();
    let plain_box = |style: &Style| {
        style.display == Display::Block
            && style.writing_mode == WritingMode::Horizontal
            && style.float_side.is_none()
            && style.clear == Default::default()
            && style.borders.is_none()
            && matches!(style.image_height, None | Some(ImageLength::Auto))
            && matches!(style.image_max_height, None | Some(ImageLength::Auto))
            && style
                .vertical_padding
                .iter()
                .all(|edge| edge.box_pixels(width, settings.font_size) == 0.0)
    };
    let mut oversized = source
        .elements
        .iter()
        .zip(styles)
        .zip(extents)
        .map(|((element, style), extent)| {
            style.page_breaks.keep_together
                && crate::block_margins::collapsible_wrapper(&element.tag)
                && element
                    .parent
                    .is_some_and(|parent| source.elements[parent].tag == "body")
                && !style.flow.suppressed
                && style.background.is_none()
                && plain_box(style)
                && extent
                    .as_ref()
                    .is_some_and(|range| range.end - range.start > page_height)
        })
        .collect::<Vec<_>>();
    if !oversized.contains(&true) {
        return oversized;
    }
    // Keep smaller child keeps and explicit page rules on their existing path.
    // Only relax a container with a fully understood ordinary text footprint.
    for (element, style) in source.elements.iter().zip(styles) {
        if let Some(parent) = element.parent
            && oversized[parent]
            && !style.flow.suppressed
            && !element.selector_only
            && (element.tag != "p"
                || !plain_box(style)
                || style.page_breaks != Default::default()
                || style.orphans > 1
                || style.widows > 1)
        {
            oversized[parent] = false;
        }
    }
    for block in &source.blocks {
        if styles[block.element].flow.suppressed {
            continue;
        }
        // Unwrapped text is owned by the outer block itself.
        oversized[block.element] = false;
        if let Some(parent) = source.elements[block.element].parent
            && (!block.images.is_empty()
                || !block.ruby.is_empty()
                || block.table_cell.is_some()
                || block.list_item.is_some())
        {
            oversized[parent] = false;
        }
    }
    oversized
}

#[cfg(test)]
mod tests;
