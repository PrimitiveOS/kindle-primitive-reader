//! Physical side margins on ordinary auto-width paragraphs.
use super::{BoxArea, TextBoxCandidate, text_box_candidates_with_parent};
use crate::{
    Settings,
    book_style::{BoxMargin, CssLength, ImageLength, Style, WritingMode, flow::Display},
    source::Source,
};

pub(super) fn candidate(style: &Style, settings: Settings, parent_width: u32) -> bool {
    ordinary_box(style, settings, parent_width)
        && style.horizontal_margin.iter().any(|margin| {
            matches!(margin, BoxMargin::Length(length) if length.pixels(parent_width, settings.font_size) > 0.0)
        })
}

fn ordinary_box(style: &Style, settings: Settings, parent_width: u32) -> bool {
    matches!(style.image_width, None | Some(ImageLength::Auto))
        && matches!(style.image_max_width, None | Some(ImageLength::Auto))
        && style.min_width.is_none()
        && style.borders.is_none()
        && style.clear == Default::default()
        && style
            .padding
            .iter()
            .chain(&style.vertical_padding)
            .all(|edge| edge.box_pixels(parent_width, settings.font_size) == 0.0)
}

pub(super) fn inset(area: BoxArea, style: &Style, settings: Settings) -> Option<BoxArea> {
    let [left, right] = style
        .horizontal_margin
        .map(|margin| positive_pixels(margin, area.width, settings));
    let left = left?;
    let width = area.width.checked_sub(left)?.checked_sub(right?)?;
    if width < settings.styled_font(style.size_scale).ceil() as u32 || width == area.width {
        return None;
    }
    Some(BoxArea::new(area.x + left, width, area.cell))
}

pub(super) fn positive_pixels(margin: BoxMargin, width: u32, settings: Settings) -> Option<u32> {
    let (negative, pixels) = resolved_edge(margin, width, settings)?;
    (!negative && (0.0..=width as f32).contains(&pixels)).then_some(pixels as u32)
}

pub(crate) fn signed_candidates(
    source: &Source,
    styles: &[Style],
    settings: Settings,
    parent_width: u32,
) -> Vec<TextBoxCandidate> {
    let mut full_width = Vec::with_capacity(styles.len());
    for (element, style) in source.elements.iter().zip(styles) {
        let in_full_width = element.parent.is_some_and(|parent| full_width[parent]);
        full_width.push(
            element.tag == "body"
                || in_full_width
                    && crate::block_margins::collapsible_wrapper(&element.tag)
                    && style.display == Display::Block
                    && style.writing_mode == WritingMode::Horizontal
                    && !style.flow.suppressed
                    && style.float_side.is_none()
                    && style.background.is_none()
                    && ordinary_box(style, settings, parent_width)
                    && style.horizontal_margin.iter().all(|margin| {
                        resolved_edge(*margin, parent_width, settings) == Some((false, 0.0))
                    }),
        );
    }
    let has_signed_margin = |style: &Style| {
        ordinary_box(style, settings, parent_width)
            && style.horizontal_margin.iter().any(|margin| {
                resolved_edge(*margin, parent_width, settings).is_some_and(|(_, px)| px < 0.0)
            })
    };
    text_box_candidates_with_parent(source, styles, has_signed_margin, |parent| {
        full_width[parent]
    })
}

/// Keep the full publisher box for line wrapping and source mapping. The
/// painter clips the portion drawn outside the physical viewport.
pub(crate) fn signed_inset(
    x: u32,
    width: u32,
    style: &Style,
    settings: Settings,
) -> Option<(i32, u32)> {
    if !ordinary_box(style, settings, width) {
        return None;
    }
    let [left, right] = style
        .horizontal_margin
        .map(|margin| resolved_edge(margin, width, settings));
    let (_, left) = left?;
    let (_, right) = right?;
    if left >= 0.0 && right >= 0.0 {
        return None;
    }
    let box_width = width as f32 - left - right;
    let start = x as f32 + left;
    let physical_left = settings.left_margin() as f32 + start;
    let physical_right = physical_left + box_width;
    let screen = settings.width as f32;
    // Limit extreme publisher geometry to one extra viewport on each side.
    if box_width < settings.styled_font(style.size_scale).ceil()
        || !(-screen..screen).contains(&physical_left)
        || !(physical_right > 0.0 && physical_right <= 2.0 * screen)
    {
        return None;
    }
    Some((start as i32, box_width as u32))
}

fn resolved_edge(margin: BoxMargin, width: u32, settings: Settings) -> Option<(bool, f32)> {
    let BoxMargin::Length(length) = margin else {
        return Some((false, 0.0));
    };
    let stored = match length {
        CssLength::Em(value)
        | CssLength::RootEm(value)
        | CssLength::ComputedEm { value, .. }
        | CssLength::Pixels(value)
        | CssLength::Physical(value) => f64::from(value),
        CssLength::Fraction(ratio) => f64::from(ratio) * 100.0,
    };
    if !stored.is_finite() {
        return None;
    }
    let scaled = length.for_dpi(settings.render_dpi);
    let mut pixels = scaled.box_pixels(width, settings.font_size);
    // With a nonzero content DPI, CREngine uses one pixel for a declared
    // length with a nonzero fixed-point value that rounds to zero.
    if pixels == 0.0 && settings.render_dpi != 0 && (stored * 256.0).round() != 0.0 {
        pixels = 1.0;
    }
    pixels.is_finite().then_some((stored < 0.0, pixels))
}

#[cfg(test)]
mod signed_tests;
#[cfg(test)]
mod tests;
