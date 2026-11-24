//! Opening-edge priority for ordinary paragraphs and their transparent wrappers.
use super::Edge;
use crate::{
    FloatMode, Settings,
    book_style::{ImageLength, Style, WritingMode, flow::Display},
    publisher_padding::{TextBoxCandidate, text_box_candidates_with_parent},
    source::Source,
};

pub(super) fn explicit_before(source: &Source, styles: &[Style], settings: Settings) -> Vec<bool> {
    if !settings.prefer_publisher_opening_breaks
        || settings.float_mode != FloatMode::Ignore
            && styles.iter().any(|style| style.float_side.is_some())
        || !styles
            .iter()
            .any(|style| style.page_breaks.before == Edge::Force)
    {
        return Vec::new();
    }
    let width = settings.text_width();
    let font = settings.font_size;
    let ordinary = |style: &Style| {
        style.display == Display::Block
            && style.writing_mode == WritingMode::Horizontal
            && style.flow.painted
            && style.float_side.is_none()
            && style.clear == Default::default()
            && style.borders.is_none()
            && matches!(style.image_height, None | Some(ImageLength::Auto))
            && matches!(style.image_max_height, None | Some(ImageLength::Auto))
            && style
                .vertical_padding
                .iter()
                .chain(&style.vertical_margin)
                .all(|edge| edge.box_pixels(width, font) == 0.0)
    };

    let mut transparent = vec![false; styles.len()];
    for (index, element) in source.elements.iter().enumerate() {
        let style = &styles[index];
        let within_body = element.parent.is_some_and(|parent| transparent[parent]);
        transparent[index] = ordinary(style)
            && (element.tag == "body"
                || within_body
                    && crate::block_margins::collapsible_wrapper(&element.tag)
                    && style.background.is_none());
    }
    let paragraphs =
        text_box_candidates_with_parent(source, styles, ordinary, |parent| transparent[parent]);

    let mut preferred = vec![false; styles.len()];
    let mut previous_paragraph = false;
    for block in &source.blocks {
        let element = block.element;
        let style = &styles[element];
        if style.flow.suppressed {
            continue;
        }
        let paragraph = paragraphs[element] == TextBoxCandidate::Simple;
        preferred[element] =
            paragraph && previous_paragraph && style.page_breaks.before == Edge::Force;
        previous_paragraph = paragraph;
    }
    preferred
}

#[cfg(test)]
mod tests;
