//! Measure an existing visual edge without changing shaped text or line breaks.
use core::ops::Range;

use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    raw::TableProvider,
};
use smallvec::SmallVec;

use crate::style::Brush;

use super::{Cluster, Layout, Line};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginEdge {
    Left,
    Right,
}

pub(super) fn measure<B: Brush>(
    layout: &Layout<B>,
    text: &str,
    caps: [f32; 2],
    rule: &impl Fn(Range<usize>, MarginEdge, bool) -> (u8, bool),
) -> alloc::vec::Vec<[f32; 2]> {
    let mut shifts = alloc::vec::Vec::with_capacity(layout.len());
    for line in layout.lines() {
        let mut clusters = line
            .runs()
            .flat_map(|run| run.visual_clusters())
            .filter(|cluster| {
                !cluster.is_space_or_nbsp()
                    && cluster.advance() > 0.0
                    && text.get(cluster.text_range()).is_some()
            });
        let Some(first) = clusters.next() else {
            shifts.push([0.0; 2]);
            continue;
        };
        let last = clusters.last().unwrap_or(first);
        let left = shift(&line, first, MarginEdge::Left, caps[0], rule);
        let right = shift(&line, last, MarginEdge::Right, caps[1], rule);
        shifts.push([left, right]);
    }
    shifts
}

fn shift<B: Brush>(
    line: &Line<'_, B>,
    cluster: Cluster<'_, B>,
    edge: MarginEdge,
    cap: f32,
    rule: &impl Fn(Range<usize>, MarginEdge, bool) -> (u8, bool),
) -> f32 {
    if cap <= 0.0 {
        return 0.0;
    }
    let chosen_hyphen = (edge == MarginEdge::Right && !line.layout.is_rtl())
        .then(|| {
            let index = line.data.soft_hyphen?.get() as usize - 1;
            line.layout.data.soft_hyphens.get(index)?.glyph
        })
        .flatten();
    let (ratio, avoid_existing) = rule(cluster.text_range(), edge, chosen_hyphen.is_some());
    if ratio == 0 {
        return 0.0;
    }
    let run = cluster.run();
    let font = run.font();
    let Ok(face) = FontRef::from_index(font.data.as_ref(), font.index) else {
        return 0.0;
    };
    if face.post().is_ok_and(|table| table.is_fixed_pitch() != 0) {
        return 0.0;
    }
    let Some(glyph) = chosen_hyphen.or_else(|| {
        let mut glyphs = cluster.glyphs();
        let glyph = glyphs.next()?;
        glyphs.next().is_none().then_some(glyph)
    }) else {
        return 0.0;
    };
    if !glyph.advance.is_finite() || glyph.advance <= 0.0 {
        return 0.0;
    }
    let coords: SmallVec<[NormalizedCoord; 8]> = run
        .normalized_coords()
        .iter()
        .map(|coord| NormalizedCoord::from_bits(coord.to_bits()))
        .collect();
    let bearing = face
        .glyph_metrics(Size::new(run.font_size()), LocationRef::new(&coords))
        .bounds(skrifa::GlyphId::new(glyph.id))
        .map_or(0.0, |bounds| match edge {
            MarginEdge::Left => bounds.x_min + glyph.x,
            MarginEdge::Right => glyph.advance - bounds.x_max - glyph.x,
        });
    if (avoid_existing && bearing < 0.0)
        || (glyph.advance > run.font_size() * 0.9 && bearing > glyph.advance * 0.4)
    {
        return 0.0;
    }
    (glyph.advance * f32::from(ratio) / 100.0)
        .floor()
        .max(1.0)
        .min((cap + bearing).max(0.0))
}
