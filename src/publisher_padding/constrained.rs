//! Width, min/max constraints and physical margins on ordinary text boxes.
use super::{BoxArea, horizontal_margins::positive_pixels};
use crate::{
    Settings,
    book_style::{BoxMargin, ImageLength, Style},
};

pub(super) fn candidate(style: &Style) -> bool {
    style.min_width.is_some()
        || [style.image_width, style.image_max_width]
            .into_iter()
            .flatten()
            .any(|length| !matches!(length, ImageLength::Auto))
}

pub(super) fn inset(
    area: BoxArea,
    style: &Style,
    settings: Settings,
    rtl: bool,
) -> Option<BoxArea> {
    let font = settings.styled_font(style.size_scale);
    if style.borders.is_some()
        || style.clear != Default::default()
        || style
            .padding
            .iter()
            .chain(&style.vertical_padding)
            .any(|edge| edge.box_pixels(area.width, settings.font_size) != 0.0)
    {
        return None;
    }
    let [left, right] = style
        .horizontal_margin
        .map(|edge| positive_pixels(edge, area.width, settings));
    let mut left = left?;
    let mut right = right?;
    let width = |length: Option<ImageLength>| {
        length
            .and_then(ImageLength::css_length)
            .map(|value| value.box_pixels(area.width, font))
    };
    let requested = width(style.image_width);
    let automatic = area.width as f32 - left as f32 - right as f32;
    let original = requested.unwrap_or(automatic);
    // The minimum wins if it exceeds the maximum. A constraint that leaves
    // the automatic width alone does not trigger fixed-box alignment.
    let used = original
        .min(width(style.image_max_width).unwrap_or(f32::INFINITY))
        .max(style.min_width.map_or(0.0, |length| {
            length.box_pixels(area.width, settings.font_size)
        }));
    if !(font.ceil()..=area.width as f32).contains(&used) {
        return None;
    }
    let used = used as u32;
    if requested.is_some() || used as f32 != automatic {
        if used + left + right > area.width {
            if rtl {
                left = 0;
            } else {
                right = 0;
            }
        }
        // Placement beyond the containing box follows the existing renderer.
        let remaining = area.width.checked_sub(used + left + right)?;
        left = match style.horizontal_margin {
            [BoxMargin::Auto, BoxMargin::Auto] => remaining / 2,
            [BoxMargin::Auto, _] => remaining,
            [_, BoxMargin::Auto] => left,
            _ if rtl => area.width - used,
            _ => left,
        };
    }
    Some(BoxArea::new(area.x + left, used, area.cell))
}
