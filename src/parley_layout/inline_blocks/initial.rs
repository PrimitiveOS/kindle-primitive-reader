//! A displaced initial keeps a real text range, just like a styled inline box.
use super::{InlineText, Segment, TextBox, slice_runs};
use crate::{
    Settings,
    book_style::{
        Style,
        pseudo::{Placement, Run},
    },
    float_layout::Contour,
    flow_view::LayoutBlock,
    parley_layout::{LineIndent, PaintBrush, make_paragraph},
    source::StyledText,
};
use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
};

pub(crate) struct Plan {
    pub(crate) text: InlineText,
    pub(crate) runs: Vec<StyledText>,
    pub(crate) contour: Contour,
}

impl Plan {
    pub(crate) fn new(
        context: &mut LayoutContext<PaintBrush>,
        fonts: &mut FontContext,
        block: &LayoutBlock<'_>,
        styles: &[Style],
        settings: Settings,
        width: u32,
    ) -> Option<Self> {
        let letter = block.first_letter.as_ref()?;
        let placement = letter.rule.placement?;
        if settings.float_mode == crate::FloatMode::Ignore
            || letter.bytes.start != 0
            || letter.bytes.len() > 64
            || letter.bytes.end >= block.text.len()
            || !block.images.is_empty()
            || !block.ruby.is_empty()
            || styles[block.element].preformatted_spaces
            || crate::publisher_padding::paragraph_is_rtl(block.direction, block.text)
        {
            return None;
        }
        let text = block.text.get(letter.bytes.clone())?;
        let shape =
            |context: &mut LayoutContext<PaintBrush>, fonts: &mut FontContext, style: &Style| {
                let mut fragment = block.text_fragment(block.element, text, Vec::new());
                let typography = [Run {
                    bytes: 0..text.len(),
                    style,
                }];
                fragment.generated = &typography;
                make_paragraph(
                    context,
                    fonts,
                    &fragment,
                    Some(width),
                    settings,
                    styles,
                    &[],
                    &[],
                    false,
                    LineIndent::default(),
                    &Default::default(),
                    None,
                )
            };
        let mut style = letter.rule.apply(&styles[letter.owner]);
        let gap = letter.rule.gap(width, style.font_pixels(settings)).max(0.);
        let (layout, y, height, cutoff) = match placement {
            Placement::Float => {
                let layout = shape(context, fonts, &style);
                let height = layout.height();
                (layout, 0., height, height)
            }
            Placement::Numeric(initial) => {
                let body = shape(context, fonts, &styles[block.element]);
                let current = shape(context, fonts, &style);
                let strut = body.height();
                let target = cap_height(&body)? + (initial.size - 1.) * strut;
                style.size_scale *= target / cap_height(&current)?;
                if !style.size_scale.is_finite() || style.size_scale > 8. {
                    return None;
                }
                let layout = shape(context, fonts, &style);
                let y = body.first_baseline()? + (f32::from(initial.sink) - 1.) * strut
                    - layout.first_baseline()?;
                let ink = ink_bottom(&layout).unwrap_or_else(|| layout.height());
                // The selected font's empty descent and line spacing do not
                // keep text away after the last requested line. Real ink does.
                let cutoff = (f32::from(initial.sink) * strut).max(y + ink);
                (layout, y, ink, cutoff)
            }
        };
        let size = [layout.width(), height];
        let inset = (size[0] + gap).ceil() as u32;
        let extent = cutoff - y.min(0.);
        if layout.len() != 1
            || inset == 0
            || width.saturating_sub(inset) < settings.font_size.ceil() as u32 * 3
            || extent > settings.text_height() as f32
        {
            return None;
        }
        let cap = TextBox {
            source: letter.bytes.clone(),
            layout,
            x: 0.,
            y,
            size,
            padding: [0.; 4],
            margin: [0.; 4],
            gray: None,
        };
        let remaining = letter.bytes.end..block.text.len();
        let contour = Contour::left_initial(width, inset, cutoff);
        Some(Self {
            text: InlineText {
                text: block.text[remaining.clone()].to_owned(),
                segments: vec![Segment {
                    source: remaining.clone(),
                    offset: 0,
                }],
                boxes: vec![cap],
                generated: false,
                detached_initial: true,
            },
            runs: slice_runs(block, &remaining, 0),
            contour,
        })
    }
}

fn cap_height(layout: &Layout<PaintBrush>) -> Option<f32> {
    let line = layout.get(0)?;
    let (font, size) = line.items().find_map(|item| match item {
        PositionedLayoutItem::GlyphRun(glyphs) => {
            Some((glyphs.run().font().clone(), glyphs.run().font_size()))
        }
        _ => None,
    })?;
    let face = FontRef::from_index(font.data.data(), font.index).ok()?;
    let metrics = face.metrics(Size::new(size), LocationRef::default());
    metrics.cap_height.filter(|height| *height > 0.)
}

fn ink_bottom(layout: &Layout<PaintBrush>) -> Option<f32> {
    let mut bottom: Option<f32> = None;
    for item in layout.get(0)?.items() {
        if let PositionedLayoutItem::GlyphRun(glyphs) = item {
            let run = glyphs.run();
            let font = run.font();
            let face = FontRef::from_index(font.data.data(), font.index).ok()?;
            let metrics = face.glyph_metrics(Size::new(run.font_size()), LocationRef::default());
            for glyph in glyphs.positioned_glyphs() {
                let box_ = metrics.bounds(GlyphId::new(glyph.id))?;
                let edge = glyph.y - box_.y_min;
                bottom = Some(bottom.map_or(edge, |current| current.max(edge)));
            }
        }
    }
    bottom.filter(|height| *height > 0.)
}
