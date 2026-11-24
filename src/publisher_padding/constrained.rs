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
    if !plain_edges(style, settings, area.width) {
        return None;
    }
    let [left, right] = style
        .horizontal_margin
        .map(|edge| positive_pixels(edge, area.width, settings));
    let (left, width) = place(
        area.width,
        style,
        settings,
        rtl,
        [left? as f32, right? as f32],
        false,
    )?;
    Some(BoxArea::new(area.x + left as u32, width as u32, area.cell))
}

pub(super) fn plain_edges(style: &Style, settings: Settings, parent_width: u32) -> bool {
    style.borders.is_none()
        && style.clear == Default::default()
        && style
            .padding
            .iter()
            .chain(&style.vertical_padding)
            .all(|edge| edge.box_pixels(parent_width, settings.font_size) == 0.0)
}

pub(super) fn place(
    parent_width: u32,
    style: &Style,
    settings: Settings,
    rtl: bool,
    [mut left, mut right]: [f32; 2],
    permit_overflow: bool,
) -> Option<(f32, f32)> {
    let font = settings.styled_font(style.size_scale);
    let parent = parent_width as f32;
    let width = |length: Option<ImageLength>| {
        length
            .and_then(ImageLength::css_length)
            .map(|value| value.box_pixels(parent_width, font))
    };
    let requested = width(style.image_width);
    let automatic = parent - left - right;
    let original = requested.unwrap_or(automatic);
    // The minimum wins if it exceeds the maximum. A constraint that leaves
    // the automatic width alone does not trigger fixed-box alignment.
    let used = original
        .min(width(style.image_max_width).unwrap_or(f32::INFINITY))
        .max(style.min_width.map_or(0.0, |length| {
            length.box_pixels(parent_width, settings.font_size)
        }));
    if !used.is_finite() || used < font.ceil() || (!permit_overflow && used > parent) {
        return None;
    }
    if requested.is_some() || used != automatic {
        if used + left + right > parent {
            if rtl {
                left = 0.0;
            } else {
                right = 0.0;
            }
        }
        let remaining = parent - used - left - right;
        // When the width still overflows, CREngine leaves auto sides unresolved.
        if remaining < 0.0 {
            return permit_overflow.then_some((left, used));
        }
        left = match style.horizontal_margin {
            [BoxMargin::Auto, BoxMargin::Auto] => (remaining / 2.0).floor(),
            [BoxMargin::Auto, _] => remaining,
            [_, BoxMargin::Auto] => left,
            _ if rtl => parent - used,
            _ => left,
        };
    }
    Some((left, used))
}
