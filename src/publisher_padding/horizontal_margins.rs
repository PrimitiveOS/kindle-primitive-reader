//! Physical side margins on ordinary auto-width paragraphs.
use super::BoxArea;
use crate::{
    Settings,
    book_style::{BoxMargin, CssLength, ImageLength, Style},
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

fn positive_pixels(margin: BoxMargin, width: u32, settings: Settings) -> Option<u32> {
    let (negative, pixels) = resolved_edge(margin, width, settings)?;
    (!negative && (0.0..=width as f32).contains(&pixels)).then_some(pixels as u32)
}

pub(crate) fn signed_candidate(style: &Style, settings: Settings, parent_width: u32) -> bool {
    ordinary_box(style, settings, parent_width)
        && style.horizontal_margin.iter().any(|margin| {
            matches!(margin, BoxMargin::Length(length) if length.pixels(parent_width, settings.font_size) < 0.0)
        })
}

/// Text boxes may cross the user margins, within the physical framebuffer.
/// The x origin remains relative to the usual text column for source mapping.
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
    let (left_negative, left) = left?;
    let (right_negative, right) = right?;
    if left_negative == right_negative || (left >= 0.0 && right >= 0.0) {
        return None;
    }
    let box_width = width as f32 - left - right;
    let start = x as f32 + left;
    let physical_left = settings.left_margin() as f32 + start;
    if box_width < settings.styled_font(style.size_scale).ceil()
        || physical_left < 0.0
        || physical_left + box_width > settings.width as f32
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
