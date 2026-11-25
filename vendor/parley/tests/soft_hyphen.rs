use parley::{
    Alignment, AlignmentOptions, BaseDirection, Cluster, FontContext, FontFamily, Layout,
    LayoutContext, LineBreak, OverflowWrap, PositionedLayoutItem, SoftHyphenAction, StyleProperty,
    WordBreak, fontique::Blob,
};
use skrifa::{FontRef, MetadataProvider};
use std::sync::Arc;

mod first_break;

fn measure(
    text: &str,
    width: f32,
    direction: BaseDirection,
    align: Alignment,
    fonts: &mut FontContext,
    family: &str,
) -> Layout<()> {
    measure_with_policy(
        text,
        width,
        direction,
        align,
        fonts,
        family,
        Some(&|_| SoftHyphenAction::Draw),
    )
}

fn measure_with_policy(
    text: &str,
    width: f32,
    direction: BaseDirection,
    align: Alignment,
    fonts: &mut FontContext,
    family: &str,
    policy: Option<&(dyn Fn(usize) -> SoftHyphenAction + Send + Sync)>,
) -> Layout<()> {
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(24.0));
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    builder.set_base_direction(direction);
    builder.set_soft_hyphen_policy(policy);
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(width));
    layout.align(align, AlignmentOptions::default());
    layout
}

fn font(data: Vec<u8>) -> (FontContext, u32) {
    let face = FontRef::from_index(&data, 0).unwrap();
    let cmap = face.charmap();
    let id = cmap
        .map('\u{2010}')
        .or_else(|| cmap.map('-'))
        .unwrap()
        .to_u32();
    let mut context = FontContext::new();
    assert!(
        !context
            .collection
            .register_fonts(Blob::new(Arc::new(data)), None)
            .is_empty()
    );
    (context, id)
}

fn glyphs(layout: &Layout<()>, line: usize) -> Vec<(u32, f32, f32, bool)> {
    layout
        .get(line)
        .unwrap()
        .items()
        .flat_map(|item| {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                return Vec::new();
            };
            let rtl = run.run().is_rtl();
            run.positioned_glyphs()
                .map(|g| (g.id, g.x, g.advance, rtl))
                .collect()
        })
        .collect()
}

fn exact_prefix<T>(
    whole: &str,
    shy: usize,
    id: u32,
    mut layout: impl FnMut(f32) -> T,
    view: impl Fn(&T) -> &Layout<()>,
) -> (f32, T) {
    let after = shy + '\u{ad}'.len_utf8();
    (1..1000)
        .map(|w| w as f32)
        .find_map(|w| {
            let current = layout(w);
            let view = view(&current);
            let line = view.get(0).unwrap();
            if line.text_range() == (0..after) && glyphs(view, 0).iter().any(|g| g.0 == id) {
                assert!(line.metrics().advance <= w + 0.001);
                assert_eq!(
                    view.lines()
                        .map(|line| &whole[line.text_range()])
                        .collect::<String>(),
                    whole
                );
                Some((w, current))
            } else {
                None
            }
        })
        .expect("An authored soft hyphen must paint on a chosen width-safe break")
}

#[test]
fn discretionary_width_and_utf8_selection() {
    let (mut fonts, id) =
        font(include_bytes!("../../../tests/fixtures/fonts/Silkscreen-Regular.ttf").to_vec());
    let text = "LEAD AB\u{ad}CDEFGHIJK DELTA";
    let shy = text.find('\u{ad}').unwrap();
    let after = shy + '\u{ad}'.len_utf8();
    let mut layout = |width, align| {
        measure(
            text,
            width,
            BaseDirection::Ltr,
            align,
            &mut fonts,
            "Silkscreen",
        )
    };
    let (width, narrow) = exact_prefix(
        text,
        shy,
        id,
        |w| layout(w, Alignment::Justify),
        |value| value,
    );
    let line = narrow.get(0).unwrap();
    let all = glyphs(&narrow, 0);
    let mark = all.iter().find(|glyph| glyph.0 == id).unwrap();
    assert_eq!(all.last().unwrap().0, id);
    let midpoint = (line.metrics().block_min_coord + line.metrics().block_max_coord) / 2.;
    let (cluster, _) = Cluster::from_point_exact(&narrow, mark.1 + mark.2 * 0.5, midpoint).unwrap();
    assert_eq!(cluster.text_range(), shy..after);
    assert!((cluster.advance() - mark.2).abs() < 0.01);
    let narrower = layout(width - mark.2 / 2., Alignment::Start);
    assert_ne!(narrower.get(0).unwrap().text_range().end, after);
    assert!(!glyphs(&narrower, 0).iter().any(|glyph| glyph.0 == id));
    let wide = layout(1000.0, Alignment::Center);
    assert_eq!(wide.get(0).unwrap().text_range(), 0..text.len());
    assert!(!glyphs(&wide, 0).iter().any(|glyph| glyph.0 == id));
}

#[cfg(target_os = "macos")]
#[test]
fn rtl_hyphen_uses_visual_left_and_retains_logical_range() {
    let path = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
    let (mut fonts, id) = font(std::fs::read(path).unwrap());
    let text = "אב גד\u{ad}הוזחטיכל מנ";
    let shy = text.find('\u{ad}').unwrap();
    let after = shy + '\u{ad}'.len_utf8();
    let (_, narrow) = exact_prefix(
        text,
        shy,
        id,
        |w| {
            measure(
                text,
                w,
                BaseDirection::Rtl,
                Alignment::Start,
                &mut fonts,
                "Arial Unicode MS",
            )
        },
        |value| value,
    );
    let line = narrow.get(0).unwrap();
    let all = glyphs(&narrow, 0);
    let mark = all.iter().find(|glyph| glyph.0 == id).unwrap();
    assert!(mark.3);
    assert_eq!(all.first().unwrap().0, id);
    let midpoint = (line.metrics().block_min_coord + line.metrics().block_max_coord) / 2.;
    let (cluster, _) = Cluster::from_point_exact(&narrow, mark.1 + mark.2 * 0.5, midpoint).unwrap();
    assert_eq!(cluster.text_range(), shy..after);
}

#[test]
fn forced_and_end_breaks_do_not_draw_an_unused_mark() {
    let (mut fonts, id) =
        font(include_bytes!("../../../tests/fixtures/fonts/Silkscreen-Regular.ttf").to_vec());
    let text = "AB\u{ad}\nCDE\u{ad}";
    let wide = measure(
        text,
        500.0,
        BaseDirection::Ltr,
        Alignment::Start,
        &mut fonts,
        "Silkscreen",
    );
    assert_eq!(wide.len(), 2);
    for index in 0..wide.len() {
        assert!(!glyphs(&wide, index).iter().any(|glyph| glyph.0 == id));
    }
    assert_eq!(wide.get(0).unwrap().text_range(), 0.."AB\u{ad}\n".len());
}

#[cfg(target_os = "macos")]
#[test]
fn mixed_bidi_places_the_mark_inside_the_ltr_run() {
    let path = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
    let (mut fonts, id) = font(std::fs::read(path).unwrap());
    let text = "אב AB\u{ad}CDEFGHIJK";
    let shy = text.find('\u{ad}').unwrap();
    let (_, narrow) = exact_prefix(
        text,
        shy,
        id,
        |w| {
            measure(
                text,
                w,
                BaseDirection::Rtl,
                Alignment::Start,
                &mut fonts,
                "Arial Unicode MS",
            )
        },
        |value| value,
    );
    let all = glyphs(&narrow, 0);
    let (_, mark_x, _, rtl) = all.iter().find(|glyph| glyph.0 == id).unwrap();
    assert!(!rtl, "The authored SHY belongs to the LTR Latin run");
    assert!(all.iter().any(|glyph| glyph.1 < *mark_x && !glyph.3));
    assert!(all.iter().any(|glyph| glyph.1 > *mark_x && glyph.3));
}

#[test]
fn callback_controls_byte_offsets_without_affecting_the_disabled_builder() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let (mut fonts, id) =
        font(include_bytes!("../../../tests/fixtures/fonts/Silkscreen-Regular.ttf").to_vec());
    let text = "LEAD AB\u{ad}CDEFGHIJK";
    let shy = text.find('\u{ad}').unwrap();
    let (width, _) = exact_prefix(
        text,
        shy,
        id,
        |width| {
            measure(
                text,
                width,
                BaseDirection::Ltr,
                Alignment::Start,
                &mut fonts,
                "Silkscreen",
            )
        },
        |layout| layout,
    );
    let seen = AtomicUsize::new(usize::MAX);
    let decline = |offset| {
        seen.store(offset, Ordering::Relaxed);
        SoftHyphenAction::Suppress
    };
    let forbidden = measure_with_policy(
        text,
        width,
        BaseDirection::Ltr,
        Alignment::Start,
        &mut fonts,
        "Silkscreen",
        Some(&decline),
    );
    assert_eq!(seen.load(Ordering::Relaxed), shy);
    assert_eq!(forbidden.get(0).unwrap().text_range(), 0.."LEAD ".len());
    assert!(!glyphs(&forbidden, 0).iter().any(|glyph| glyph.0 == id));

    let upstream = measure_with_policy(
        text,
        width,
        BaseDirection::Ltr,
        Alignment::Start,
        &mut fonts,
        "Silkscreen",
        None,
    );
    assert_eq!(
        upstream.get(0).unwrap().text_range(),
        0..shy + '\u{ad}'.len_utf8()
    );
    assert!(!glyphs(&upstream, 0).iter().any(|glyph| glyph.0 == id));
}

#[test]
fn preserves_ordinary_arbitrary_wraps_without_drawing_hyphens() {
    let (mut fonts, id) =
        font(include_bytes!("../../../tests/fixtures/fonts/Silkscreen-Regular.ttf").to_vec());
    let text = "AB\u{ad}CDEFG";
    let after = "AB\u{ad}".len();
    for rule in [
        StyleProperty::LineBreak(LineBreak::Anywhere),
        StyleProperty::WordBreak(WordBreak::BreakAll),
    ] {
        let mut layout = |width, action: Option<SoftHyphenAction>| {
            let mut context = LayoutContext::new();
            let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily(FontFamily::named("Silkscreen")));
            builder.push_default(StyleProperty::FontSize(24.0));
            builder.push_default(rule.clone());
            let choose = |_| action.unwrap_or(SoftHyphenAction::LeaveBoundary);
            if action.is_some() {
                builder.set_soft_hyphen_policy(Some(&choose));
            }
            let mut layout = builder.build(text);
            layout.break_all_lines(Some(width));
            layout
        };
        let width = (20..130)
            .map(|width| width as f32)
            .find(|&width| layout(width, None).get(0).unwrap().text_range() == (0..after))
            .expect("An ordinary wrapping rule should permit this boundary");
        let preserved = layout(width, Some(SoftHyphenAction::LeaveBoundary));
        assert_eq!(preserved.get(0).unwrap().text_range(), 0..after);
        assert!(!glyphs(&preserved, 0).iter().any(|glyph| glyph.0 == id));
    }
}
