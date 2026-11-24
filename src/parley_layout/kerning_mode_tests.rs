use super::*;
use crate::FontKerningMode;

const SAMPLE: &str = "AVATAR office affinity efficient — To WA";

fn shaped(mode: FontKerningMode) -> Layout<()> {
    sample(SAMPLE, "fonts/NotoSans-Regular.ttf", mode, 48.0, None)
}

fn sample(
    text: &str,
    font: &str,
    mode: FontKerningMode,
    size: f32,
    width: Option<f32>,
) -> Layout<()> {
    let fonts = [crate::test_fixture(font)];
    let mut font_context = crate::font_context(&fonts).unwrap();
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut font_context, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(GenericFamily::SansSerif.into()));
    builder.push_default(StyleProperty::FontSize(size));
    if width.is_some() {
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    }
    apply_kerning(&mut builder, mode);
    let mut layout = builder.build(text);
    layout.break_all_lines(width);
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

const PAIR_FONT: &str = "fonts/HarborPairTest-Regular.ttf";

fn advance(layout: &Layout<()>, line: usize) -> f32 {
    layout.get(line).unwrap().metrics().advance
}

fn glyph_starts(layout: &Layout<()>, line: usize) -> Vec<f32> {
    let mut x = Vec::new();
    for item in layout.get(line).unwrap().items() {
        if let PositionedLayoutItem::GlyphRun(run) = item {
            x.extend(run.positioned_glyphs().map(|glyph| glyph.x));
        }
    }
    x
}

fn close(left: f32, right: f32) {
    assert!((left - right).abs() < 0.001, "{left} != {right}");
}

#[test]
fn fast_pair_kerning_matches_freetype_pixel_fitting_and_visible_caret_geometry() {
    let sizes = [12.0, 16.0, 24.0, 25.0, 42.0, 48.0];
    for (pair, expected) in [
        ("AV", [-1., -2., -6., -6., -10., -12.]),
        ("VA", [1., 1., 3., 3., 5., 6.]),
        ("To", [0., -1., -2., -2., -3., -3.]),
    ] {
        for (size, delta) in sizes.into_iter().zip(expected) {
            let plain = sample(pair, PAIR_FONT, FontKerningMode::Off, size, None);
            let fast = sample(pair, PAIR_FONT, FontKerningMode::Fast, size, None);
            close(advance(&fast, 0) - advance(&plain, 0), delta);
            let plain_glyphs = glyph_starts(&plain, 0);
            let fast_glyphs = glyph_starts(&fast, 0);
            close(fast_glyphs[0], plain_glyphs[0]);
            close(fast_glyphs[1] - plain_glyphs[1], delta);
            let cursor_width = 1.0;
            let end = |layout| {
                Cursor::from_byte_index(layout, pair.len(), Affinity::Upstream)
                    .geometry(layout, cursor_width)
                    .x0
            };
            close((end(&fast) - end(&plain)) as f32, delta);
            assert_eq!(fast.get(0).unwrap().text_range(), 0..pair.len());
            let measured = fast.calculate_content_widths();
            close(measured.max, advance(&fast, 0));
            close(measured.min, advance(&fast, 0));
        }
    }
}

#[test]
fn fast_pair_kerning_excludes_gpos_and_kerning_across_a_wrap() {
    let font = "fonts/NotoSans-Regular.ttf";
    let off = sample("AV", font, FontKerningMode::Off, 42., None);
    let fast = sample("AV", font, FontKerningMode::Fast, 42., None);
    let good = sample("AV", font, FontKerningMode::Good, 42., None);
    close(advance(&fast, 0), advance(&off, 0));
    assert!(advance(&good, 0) < advance(&fast, 0));

    let width = ["A", "V"]
        .into_iter()
        .map(|letter| {
            advance(
                &sample(letter, PAIR_FONT, FontKerningMode::Fast, 42., None),
                0,
            )
        })
        .fold(0.0, f32::max);
    for pair in ["AV", "VA", "A\u{ad}V"] {
        let wrapped = sample(
            pair,
            PAIR_FONT,
            FontKerningMode::Fast,
            42.,
            Some(width + 0.1),
        );
        assert_eq!(wrapped.len(), 2, "{pair}");
        let ending = sample(
            &pair[pair.len() - 1..],
            PAIR_FONT,
            FontKerningMode::Fast,
            42.,
            None,
        );
        close(advance(&wrapped, 1), advance(&ending, 0));
        close(glyph_starts(&wrapped, 1)[0], glyph_starts(&ending, 0)[0]);
        close(wrapped.calculate_content_widths().min, width);
    }
}
