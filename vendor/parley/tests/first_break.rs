use super::{font, glyphs};
use parley::{
    Alignment, AlignmentOptions, BaseDirection, BreakReason, FontContext, FontFamily, Layout,
    LayoutContext, OverflowWrap, SoftHyphenAction, StyleProperty, YieldData,
};

fn shape(
    text: &str,
    fonts: &mut FontContext,
    family: &str,
    direction: BaseDirection,
    styled_end: usize,
    automatic: &[usize],
) -> Layout<()> {
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(24.0));
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    builder.push(StyleProperty::FontSize(18.0), 0..styled_end);
    builder.set_base_direction(direction);
    builder.set_automatic_hyphens(automatic);
    builder.set_soft_hyphen_policy(Some(&|_| SoftHyphenAction::Draw));
    builder.build(text)
}

fn capped(layout: &mut Layout<()>, width: f32, request: (usize, BreakReason, bool)) {
    let mut breaker = layout.break_lines();
    assert!(breaker.require_first_break(request.0, request.1, request.2));
    breaker.state_mut().set_layout_max_advance(width);
    breaker.state_mut().set_line_max_advance(width);
    while breaker.break_next().is_some() {}
    breaker.finish();
}

fn silkscreen() -> (FontContext, u32) {
    font(include_bytes!("../../../tests/fixtures/fonts/Silkscreen-Regular.ttf").to_vec())
}

fn check_text(layout: &Layout<()>, text: &str) {
    assert_eq!(
        layout
            .lines()
            .map(|line| &text[line.text_range()])
            .collect::<String>(),
        text,
    );
}

#[test]
fn first_break_crosses_runs_without_replacing_contour_or_alignment() {
    let (mut fonts, _) = silkscreen();
    let text = "OMEGA ALPHA GOES ON";
    let end = "OMEGA ".len();
    let mut layout = shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, end, &[]);
    layout.break_all_lines(Some(410.0));
    assert_eq!(layout.get(0).unwrap().text_range(), 0..text.len());
    let mut breaker = layout.break_lines();
    assert!(breaker.require_first_break(end, BreakReason::Regular, false));
    breaker.state_mut().set_layout_max_advance(500.0);
    breaker.state_mut().set_line_max_advance(410.0);
    breaker.state_mut().set_line_x(22.0);
    breaker.state_mut().set_line_y(11.0);
    let first = |breaker: &mut parley::BreakLines<'_, ()>| {
        let YieldData::LineBreak(line) = breaker.break_next().unwrap() else {
            panic!("Expected a regular line");
        };
        assert_eq!(line.reason, BreakReason::Regular);
        assert_eq!(line.line_y_start, 11.0);
        assert_eq!(line.metrics.inline_min_coord, 22.0);
        assert_eq!(line.metrics.inline_max_coord, 432.0);
    };
    first(&mut breaker);
    assert!(breaker.revert());
    first(&mut breaker);
    breaker.state_mut().set_line_x(0.0);
    breaker.state_mut().set_line_max_advance(500.0);
    while breaker.break_next().is_some() {}
    breaker.finish();
    layout.align(Alignment::Center, AlignmentOptions::default());
    let first = layout.get(0).unwrap();
    assert_eq!(first.text_range(), 0..end);
    assert_eq!(first.break_reason(), BreakReason::Regular);
    assert!(!first.has_discretionary_hyphen());
    assert!(first.metrics().offset > 0.0);
    assert!(first.metrics().hanging_advance > 0.0);
    assert_eq!(layout.get(1).unwrap().text_range(), end..text.len());
    assert_eq!(layout.get(1).unwrap().break_reason(), BreakReason::None);
    check_text(&layout, text);
}

#[test]
fn first_break_rejects_partial_atoms_and_retains_necessary_earlier_breaks() {
    let (mut fonts, _) = silkscreen();
    let text = "ÉÉABCDEF";
    let end = "ÉÉ".len();
    let mut make = || shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    let mut layout = make();
    capped(&mut layout, 500.0, (end, BreakReason::Emergency, false));
    assert_eq!(layout.get(0).unwrap().text_range(), 0..end);
    assert_eq!(
        layout.get(0).unwrap().break_reason(),
        BreakReason::Emergency
    );
    check_text(&layout, text);

    let mut layout = make();
    capped(&mut layout, 12.0, (end, BreakReason::Emergency, false));
    assert!(layout.get(0).unwrap().text_range().end < end);
    check_text(&layout, text);

    let mut layout = make();
    capped(&mut layout, 500.0, (1, BreakReason::Emergency, false));
    assert_eq!(layout.get(0).unwrap().text_range(), 0..text.len());
    assert_eq!(layout.get(0).unwrap().break_reason(), BreakReason::None);

    let text = "E\u{301}GARDEN";
    let mut layout = shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    capped(&mut layout, 500.0, (1, BreakReason::Emergency, false));
    assert_eq!(layout.get(0).unwrap().text_range(), 0..text.len());
}

#[test]
fn first_break_uses_finite_width_word_space_shrinking() {
    let (mut fonts, _) = silkscreen();
    let prefix = "AB CD";
    let text = "AB CD EF";
    let end = prefix.len() + 1;
    let mut measured = shape(prefix, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    measured.set_word_spacing_percent(prefix, 200, 100, &[]);
    measured.break_all_lines(Some(500.0));
    let width = measured.get(0).unwrap().metrics().advance - 2.0;

    let mut control = shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    control.set_word_spacing_percent(text, 200, 100, &[]);
    capped(&mut control, width, (end, BreakReason::Regular, false));
    assert!(control.get(0).unwrap().text_range().end < end);

    let mut layout = shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    layout.set_word_spacing_percent(text, 200, 25, &[]);
    capped(&mut layout, width, (end, BreakReason::Regular, false));
    let line = layout.get(0).unwrap();
    assert_eq!(line.text_range(), 0..end);
    assert_eq!(line.break_reason(), BreakReason::Regular);
    assert!((line.metrics().advance - line.metrics().hanging_advance - width).abs() < 0.001);
    check_text(&layout, text);
}

#[test]
fn first_break_retains_authored_and_automatic_discretionary_hyphens() {
    let (mut fonts, id) = silkscreen();
    let automatic_end = "LEAD AB".len();
    for (text, end, automatic) in [
        ("LEAD AB\u{ad}CDEFGH", "LEAD AB\u{ad}".len(), [].as_slice()),
        ("LEAD ABCDEFGH", automatic_end, [automatic_end].as_slice()),
    ] {
        let mut make = || {
            shape(
                text,
                &mut fonts,
                "Silkscreen",
                BaseDirection::Ltr,
                0,
                automatic,
            )
        };
        let mut layout = make();
        capped(&mut layout, 500.0, (end, BreakReason::Regular, true));
        let line = layout.get(0).unwrap();
        assert_eq!(line.text_range(), 0..end);
        assert_eq!(line.break_reason(), BreakReason::Regular);
        assert!(line.has_discretionary_hyphen());
        assert_eq!(glyphs(&layout, 0).last().unwrap().0, id);
        check_text(&layout, text);

        let mut layout = make();
        capped(&mut layout, 500.0, (end, BreakReason::Regular, false));
        assert_eq!(layout.get(0).unwrap().text_range(), 0..text.len());
        assert!(!layout.get(0).unwrap().has_discretionary_hyphen());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn first_break_retains_rtl_hyphen_and_logical_utf8_end() {
    let font_file = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
    let (mut fonts, id) = font(std::fs::read(font_file).unwrap());
    let text = "אב גד\u{ad}הוזחטיכל מנ";
    let end = "אב גד\u{ad}".len();
    let mut layout = shape(
        text,
        &mut fonts,
        "Arial Unicode MS",
        BaseDirection::Rtl,
        0,
        &[],
    );
    capped(&mut layout, 500.0, (end, BreakReason::Regular, true));
    layout.align(Alignment::Start, AlignmentOptions::default());
    let line = layout.get(0).unwrap();
    assert_eq!(line.text_range(), 0..end);
    assert!(line.has_discretionary_hyphen());
    let placed = glyphs(&layout, 0);
    assert_eq!(placed.first().unwrap().0, id);
    assert!(placed.first().unwrap().3);
    check_text(&layout, text);
}

#[test]
fn first_break_leaves_mandatory_and_end_breaks_to_normal_layout() {
    let (mut fonts, _) = silkscreen();
    let text = "LOOK\nPAST HERE";
    let mut layout = shape(text, &mut fonts, "Silkscreen", BaseDirection::Ltr, 0, &[]);
    let mut breaker = layout.break_lines();
    assert!(!breaker.require_first_break("LOOK\n".len(), BreakReason::Explicit, false));
    assert!(!breaker.require_first_break(text.len(), BreakReason::Regular, false));
    assert!(!breaker.require_first_break(1, BreakReason::None, false));
    assert!(!breaker.require_first_break(1, BreakReason::Emergency, true));
    assert!(breaker.require_first_break("LOOK\nPAST ".len(), BreakReason::Regular, false));
    breaker.state_mut().set_layout_max_advance(500.0);
    breaker.state_mut().set_line_max_advance(500.0);
    while breaker.break_next().is_some() {}
    breaker.finish();
    assert_eq!(layout.get(0).unwrap().break_reason(), BreakReason::Explicit);
    assert_eq!(layout.get(0).unwrap().text_range(), 0.."LOOK\n".len());
    assert_eq!(layout.get(1).unwrap().break_reason(), BreakReason::None);
    check_text(&layout, text);
}
