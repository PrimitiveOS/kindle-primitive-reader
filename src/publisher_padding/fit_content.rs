//! Intrinsic sizing for ordinary `width: fit-content` paragraphs.
use super::{TextBoxCandidate, constrained, horizontal_margins, text_box_candidates_with_parent};
use crate::{
    FitContentMode, Settings,
    book_style::{BoxSizing, CssLength, ImageLength, Style, borders::Pattern},
    source::Source,
    table_borders::Stroke,
};
use std::ops::Range;

pub(crate) struct HeightTail {
    pub(crate) element: usize,
    pub(crate) closing: Range<f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct Placed {
    pub(crate) left: i32,
    pub(crate) width: u32,
    pub(crate) text_left: i32,
    pub(crate) text_width: u32,
    pub(crate) vertical_padding: [u32; 2],
    pub(crate) desired_height: Option<f32>,
    // Top/right/bottom/left as in publisher table edges.
    pub(crate) strokes: [Stroke; 4],
}

impl Placed {
    pub(crate) fn vertical_insets(self) -> [u32; 2] {
        let [before, after] = self.vertical_padding;
        [
            before + self.strokes[0].width,
            after + self.strokes[2].width,
        ]
    }
}

fn desired_height(style: &Style, settings: Settings, edges: u32) -> Option<f32> {
    if settings.fit_content != FitContentMode::Web {
        return None;
    }
    let font = settings.styled_font(style.size_scale);
    let offset = if style.box_sizing == BoxSizing::ContentBox {
        edges as f32
    } else {
        0.0
    };
    let limit = |value: Option<ImageLength>| {
        let value = value?;
        let px = match value {
            ImageLength::Auto | ImageLength::FitContent | ImageLength::Fraction(_) => return None,
            ImageLength::Physical(px) => px.floor(),
            ImageLength::RootEm(value) => {
                CssLength::RootEm(value).box_pixels(0, settings.font_size)
            }
            _ => value.css_length()?.box_pixels(0, font),
        };
        let one = settings.render_dpi != 0 && value.pixels(0, font).is_some_and(|raw| raw > 0.0);
        Some(px.max(if one { 1.0 } else { 0.0 }) + offset)
    };
    // The max caps an explicit height only. The min can establish a height,
    // and wins even when it exceeds max. The paint stage keeps natural text.
    let wanted = limit(style.image_height)
        .map(|height| height.min(limit(style.image_max_height).unwrap_or(f32::INFINITY)));
    wanted
        .into_iter()
        .chain(limit(style.min_height))
        .reduce(f32::max)
}

fn supported_edges(style: &Style, settings: Settings) -> bool {
    style.clear == Default::default()
        && style.borders.as_ref().is_none_or(|borders| {
            borders.iter().all(|border| {
                matches!(border.pattern, Pattern::None | Pattern::Hidden)
                    || (border.pattern == Pattern::Solid
                        && border.gray.is_none_or(|(_, alpha)| alpha == 255)
                        && (0.0..=64.0).contains(&border.width.box_pixels(0, settings.font_size)))
            })
        })
}

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
                && supported_edges(style, settings)
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
) -> Option<Placed> {
    let web = settings.fit_content == FitContentMode::Web;
    let [left, right] = style.horizontal_margin.map(|edge| {
        horizontal_margins::resolved_edge(edge, width, settings)
            .map(|(negative, value)| if !web && negative { 0.0 } else { value })
    });
    let sides = [left?, right?];
    let parent = width as f32;
    let font = settings.font_size;
    // Percentages resolve against the containing width, even when the fit
    // creates a much narrower paragraph. Preserve the ordinary path for
    // edge sizes that could fill an appreciable part of a page on their own.
    let vertical_padding = style
        .vertical_padding
        .map(|edge| edge.box_pixels(width, font));
    if vertical_padding
        .iter()
        .any(|edge| !(0.0..=(settings.text_height() / 4) as f32).contains(edge))
    {
        return None;
    }
    let strokes = style
        .borders
        .as_ref()
        .map_or_else(Default::default, |edges| {
            edges.map(|edge| Stroke::read(edge, font))
        });
    let horizontal_border = (strokes[1].width + strokes[3].width) as f32;
    let fixed_inset = horizontal_border
        + style
            .padding
            .iter()
            .map(|edge| edge.box_pixels(0, font))
            .sum::<f32>();
    let [left_pad, right_pad] = style.padding.map(|edge| edge.box_pixels(width, font));
    let inset = horizontal_border + left_pad + right_pad;
    let content_box = style.box_sizing == BoxSizing::ContentBox;
    // CRE's intrinsic measurement includes this element's borders and
    // padding even when ignoring its margins. Content-box limits come before
    // those edges. Border-box limits come before inverse percentage padding.
    let natural = if content_box {
        constrained::limit(content.ceil(), width, style, settings, false) + fixed_inset
    } else {
        constrained::limit(content.ceil() + fixed_inset, width, style, settings, false)
    };
    let natural = add_intrinsic_percentages(natural, style.padding)?;
    // In fit-content placement CRE treats that intrinsic demand as content,
    // clips it to the available column, then adds the resolved edges again.
    let fit = natural.min(parent - sides[0] - sides[1]) + inset;
    let used = if web {
        constrained::limit_with_padding(
            fit,
            width,
            style,
            settings,
            true,
            if content_box { inset } else { 0.0 },
        )
    } else {
        fit
    };
    let (mut offset, mut used) = constrained::align(parent, used, style, rtl, sides, true)?;
    if !web {
        if used > parent {
            offset = 0.0;
            used = parent;
        } else {
            offset = offset.min(parent - used);
        }
    }
    // A narrow word can fit in less than an em. An author constraint that
    // forces a wider run below that floor would leave most glyphs clipped.
    if used - inset
        < content
            .ceil()
            .min(settings.styled_font(style.size_scale).ceil())
    {
        return None;
    }
    let (left, width) = horizontal_margins::visible_box(x, offset, used, settings)?;
    let vertical_padding = vertical_padding.map(|pixels| pixels as u32);
    let height_edges = vertical_padding.iter().sum::<u32>() + strokes[0].width + strokes[2].width;
    Some(Placed {
        left,
        width,
        text_left: left + strokes[3].width as i32 + left_pad as i32,
        text_width: width - inset as u32,
        vertical_padding,
        desired_height: desired_height(style, settings, height_edges),
        strokes,
    })
}

fn add_intrinsic_percentages(base: f32, padding: [CssLength; 2]) -> Option<f32> {
    let mut percentage = 0_i64;
    let mut terms = 0;
    for edge in padding {
        if let CssLength::Fraction(value) = edge {
            percentage = percentage.checked_add((f64::from(value) * 25_600.0).round() as i64)?;
            terms += 1;
        }
    }
    if !(0..25_600).contains(&percentage) {
        return None;
    }
    Some(
        base + (f64::from(base) * percentage as f64 / (25_600 - percentage) as f64).floor() as f32
            + terms as f32,
    )
}

#[cfg(test)]
mod tests;
