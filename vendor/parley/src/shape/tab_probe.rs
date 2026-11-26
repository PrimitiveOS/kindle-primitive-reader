// Required Notice: Copyright (C) 2026 Doan Nguyen (https://github.com/DoanNguyenTrong/kindle-primitive)
// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0

//! Isolated glyphs used for source-preserving CSS tab stops.
use parley_engine::{
    Analysis, AnalysisOptions, Analyzer, FontInstance, FontSelector,
    ShapeOptions, ShapedText, Shaper, itemize::Item,
};
use skrifa::{FontRef, MetadataProvider};

pub(super) fn standalone_glyph(
    shaper: &mut Shaper,
    expected: &FontInstance,
    select_font: impl FontSelector,
    options: ShapeOptions<'_>,
    style_index: u16,
    ch: char,
) -> Option<f32> {
    // Do not silently measure a missing-glyph box or a different fallback face.
    let face = FontRef::from_index(expected.font.data.as_ref(), expected.font.index).ok()?;
    face.charmap().map(ch)?;
    let text = match ch {
        '0' => "0",
        ' ' => " ",
        _ => return None,
    };
    if !options.font_size.is_finite() || options.font_size <= 0.0 {
        return None;
    }
    let mut analysis = Analysis::new();
    Analyzer::new().analyze(text, &AnalysisOptions::default(), &mut analysis);
    let mut shaped = ShapedText::new();
    shaper.shape_text(
        text, &analysis, &[style_index], [Item { char_end: 1, options }], select_font, &mut shaped,
    );
    if shaped.runs().len() != 1 || shaped.characters().len() != 1
        || shaped.fonts() != core::slice::from_ref(expected)
    {
        return None;
    }
    let [cluster] = shaped.shaped_clusters() else { return None; };
    if cluster.glyph_len() != 1 || cluster.chars_range() != (0..1) {
        return None;
    }
    let glyph_id = if cluster.has_inline_glyph() {
        cluster.glyph_offset
    } else {
        shaped.glyphs().get(cluster.glyph_offset as usize)?.id
    };
    if glyph_id == 0 || !cluster.advance.is_finite() || cluster.advance <= 0.0 {
        return None;
    }
    Some(cluster.advance)
}
