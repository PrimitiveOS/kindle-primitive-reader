// Required Notice: Copyright (C) 2026 Doan Nguyen (https://github.com/DoanNguyenTrong/kindle-primitive)
// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0

//! Isolated zero glyph used for a source-preserving CSS tab-stop threshold.
use parley_engine::{
    Analysis, AnalysisOptions, Analyzer, FontInstance, FontInstanceRef, FontSelector,
    ShapeOptions, ShapedText, Shaper, itemize::{Item, Segment}, shape::CharCluster,
};
use skrifa::{FontRef, MetadataProvider};

struct SelectedFont(FontInstance);

impl FontSelector for SelectedFont {
    fn select_font(
        &mut self,
        _: &Segment,
        _: &ShapeOptions<'_>,
        _: &mut CharCluster,
    ) -> Option<FontInstanceRef<'_>> {
        Some(self.0.as_ref())
    }
}

pub(super) fn standalone_zero(
    shaper: &mut Shaper,
    font: FontInstance,
    options: ShapeOptions<'_>,
) -> Option<f32> {
    // Do not silently measure a missing-glyph box or a different fallback face.
    let face = FontRef::from_index(font.font.data.as_ref(), font.font.index).ok()?;
    face.charmap().map('0')?;
    if !options.font_size.is_finite() || options.font_size <= 0.0 {
        return None;
    }
    let mut analysis = Analysis::new();
    Analyzer::new().analyze("0", &AnalysisOptions::default(), &mut analysis);
    let mut shaped = ShapedText::new();
    shaper.shape_text(
        "0", &analysis, &[0], [Item { char_end: 1, options }], SelectedFont(font), &mut shaped,
    );
    if shaped.runs().len() != 1 || shaped.characters().len() != 1 {
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
