use super::*;
use crate::FontKerningMode;

const SAMPLE: &str = "AVATAR office affinity efficient — To WA";

fn shaped(mode: FontKerningMode) -> Layout<()> {
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut font_context = crate::font_context(&fonts).unwrap();
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut font_context, SAMPLE, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(GenericFamily::SansSerif.into()));
    builder.push_default(StyleProperty::FontSize(48.0));
    if let Some(features) = kerning_features(mode) {
        builder.push_default(StyleProperty::FontFeatures(features.into()));
    }
    let mut layout = builder.build(SAMPLE);
    layout.break_all_lines(None);
    layout
}

fn glyph_count(layout: &Layout<()>) -> usize {
    layout
        .lines()
        .flat_map(|line| line.items())
        .map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) => run.positioned_glyphs().count(),
            PositionedLayoutItem::InlineBox(_) => 0,
        })
        .sum()
}

#[test]
fn font_kerning_modes_select_distinct_reference_feature_sets() {
    assert_eq!(kerning_features(FontKerningMode::Off).unwrap().len(), 2);
    assert_eq!(kerning_features(FontKerningMode::Fast).unwrap().len(), 23);
    assert_eq!(kerning_features(FontKerningMode::Good).unwrap().len(), 22);
    assert!(kerning_features(FontKerningMode::Best).is_none());
}

#[test]
fn basic_modes_preserve_full_ligatures_only_in_best_and_off() {
    let best = shaped(FontKerningMode::Best);
    let off = shaped(FontKerningMode::Off);
    let fast = shaped(FontKerningMode::Fast);
    let good = shaped(FontKerningMode::Good);

    assert_eq!(glyph_count(&best), glyph_count(&off));
    assert!(glyph_count(&fast) > glyph_count(&best));
    assert_eq!(glyph_count(&fast), glyph_count(&good));
    assert_ne!(
        best.get(0).unwrap().metrics().advance,
        off.get(0).unwrap().metrics().advance
    );
}
