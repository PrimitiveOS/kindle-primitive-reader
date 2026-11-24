//! Publisher widths on direct body wrappers made of ordinary paragraphs.
use super::{Source, Style, TextBoxCandidate, text_box_candidates_with_parent};
use crate::{
    block_margins::collapsible_wrapper,
    book_style::{BoxMargin, ImageLength, WritingMode, flow::Display},
};

pub(super) fn candidates(source: &Source, styles: &[Style], width: u32, font: f32) -> Vec<bool> {
    let ordinary_geometry = |style: &Style| {
        matches!(style.image_height, None | Some(ImageLength::Auto))
            && style.min_width.is_none()
            && matches!(style.image_max_height, None | Some(ImageLength::Auto))
            && style.clear == Default::default()
            && style.borders.is_none()
            && style.page_breaks == Default::default()
            && style
                .padding
                .iter()
                .chain(&style.vertical_padding)
                .chain(&style.vertical_margin)
                .all(|edge| edge.box_pixels(width, font) == 0.0)
    };
    // An eligible wrapper needs at least one retained paragraph.
    let mut wrappers = source
        .elements
        .iter()
        .zip(styles)
        .map(|(element, style)| {
            (collapsible_wrapper(&element.tag)
                && element
                    .parent
                    .is_some_and(|parent| source.elements[parent].tag == "body")
                && !style.flow.suppressed
                && style.float_side.is_none()
                && style.display == Display::Block
                && style.writing_mode == WritingMode::Horizontal
                && matches!(
                    style.image_width,
                    Some(ImageLength::Pixels(_) | ImageLength::Fraction(_))
                )
                && ordinary_geometry(style))
            .then_some(false)
        })
        .collect::<Vec<_>>();
    if !wrappers.contains(&Some(false)) {
        return Vec::new();
    }
    let paragraphs = text_box_candidates_with_parent(
        source,
        styles,
        |style| {
            ordinary_geometry(style)
                && matches!(style.image_width, None | Some(ImageLength::Auto))
                && matches!(style.image_max_width, None | Some(ImageLength::Auto))
                && style.horizontal_margin.iter().all(|margin| match margin {
                    BoxMargin::Auto => true,
                    BoxMargin::Length(length) => length.box_pixels(width, font) == 0.0,
                })
        },
        |parent| wrappers[parent].is_some(),
    );

    for (index, element) in source.elements.iter().enumerate() {
        if let Some(parent) = element.parent
            && wrappers[parent].is_some()
            && !element.selector_only
            && !styles[index].flow.suppressed
        {
            wrappers[parent] = (paragraphs[index] == TextBoxCandidate::Simple).then_some(true);
        }
    }
    for block in &source.blocks {
        // Text owned by the wrapper itself has a different formatting path.
        wrappers[block.element] = None;
    }
    wrappers
        .into_iter()
        .map(|item| item == Some(true))
        .collect()
}
