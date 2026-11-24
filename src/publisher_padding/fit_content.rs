//! Intrinsic sizing for ordinary `width: fit-content` paragraphs.
use super::{TextBoxCandidate, constrained, horizontal_margins, text_box_candidates_with_parent};
use crate::{
    FitContentMode, Settings,
    book_style::{ImageLength, Style},
    source::Source,
};

pub(crate) fn candidates(
    source: &Source,
    styles: &[Style],
    settings: Settings,
    width: u32,
) -> Vec<TextBoxCandidate> {
    if settings.fit_content == FitContentMode::Ignore
        || !styles
            .iter()
            .any(|style| matches!(style.image_width, Some(ImageLength::FitContent)))
    {
        return Vec::new();
    }
    let parents = horizontal_margins::full_width_parents(source, styles, settings, width);
    text_box_candidates_with_parent(
        source,
        styles,
        |style| {
            matches!(style.image_width, Some(ImageLength::FitContent))
                && constrained::plain_edges(style, settings, width)
                && !style.text_indent.hanging
                && style.text_indent.pixels(width, settings.font_size) >= 0.0
        },
        |parent| parents[parent],
    )
}

pub(crate) fn inset(
    x: u32,
    width: u32,
    style: &Style,
    settings: Settings,
    rtl: bool,
    content: f32,
) -> Option<(i32, u32)> {
    let web = settings.fit_content == FitContentMode::Web;
    let [left, right] = style.horizontal_margin.map(|edge| {
        horizontal_margins::resolved_edge(edge, width, settings)
            .map(|(negative, value)| if !web && negative { 0.0 } else { value })
    });
    let sides = [left?, right?];
    let parent = width as f32;
    // Both modes use absolute constraints for intrinsic measurement. Web
    // also enforces the resolved constraints after clipping to available width.
    let natural = constrained::limit(content.ceil(), width, style, settings, false);
    let fit = natural.min(parent - sides[0] - sides[1]);
    let used = if web {
        constrained::limit(fit, width, style, settings, true)
    } else {
        fit
    };
    // A narrow word can fit in less than an em. An author constraint that
    // forces a wider run below that floor would leave most glyphs clipped.
    if used
        < content
            .ceil()
            .min(settings.styled_font(style.size_scale).ceil())
    {
        return None;
    }
    let (offset, used) = constrained::align(parent, used, style, rtl, sides, web)?;
    horizontal_margins::visible_box(x, offset, used, settings)
}

#[cfg(test)]
mod tests;
