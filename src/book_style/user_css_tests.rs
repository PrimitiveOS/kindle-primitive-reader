use super::*;
fn styles(markup: &str, css: &str) -> (Source, Vec<Style>) {
    let source = Source::parse(markup).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let author = BookStyle::load(&mut book, &href, &source, &mut fonts);
    let snapshot = crate::user_styles::Snapshot::new(css.into()).unwrap();
    let result = author
        .with_user_css(&snapshot)
        .unwrap()
        .compute(&source.elements);
    (source, result)
}
fn at<'a>(source: &Source, styles: &'a [Style], id: &str) -> &'a Style {
    &styles[source
        .elements
        .iter()
        .position(|e| e.id.as_deref() == Some(id))
        .unwrap()]
}
#[test]
fn user_origin_ranks_before_normal_author_and_above_inline_author_important() {
    let markup = r#"<html><head><style>p {font-weight:300;font-style:italic !important}</style></head><body><p id="target" style="font-weight:900 !important;font-style:italic !important;padding-left:24px !important">Text</p></body></html>"#;
    let (source, s) = styles(
        markup,
        "* {font-weight:400 !important;font-style:normal !important;padding-left:3px !important;}",
    );
    let value = at(&source, &s, "target");
    assert_eq!(value.font_weight.value(), 400.);
    assert_eq!(value.font_style, FontStyle::Normal);
    assert_eq!(value.padding[0].pixels(1000, 42.), 3.);
    let (source, s) = styles(
        "<html><head><style>p{font-weight:300}</style></head><body><p id='target'>Text</p></body></html>",
        "#target {font-weight:900}",
    );
    assert_eq!(
        at(&source, &s, "target").font_weight.value(),
        300.,
        "author normal beats more-specific main/user normal"
    );
}

#[test]
fn list_style_position_inherits_and_the_shorthand_sets_both_values() {
    use crate::list_marker::{MarkerKind, MarkerPosition};

    let markup = r#"<html><body>
        <ol id="outer" style="list-style-position:inside">
          <li id="inherited">One</li>
          <li id="overridden" style="list-style-position:outside">Two</li>
        </ol>
        <ul id="shorthand" style="list-style:circle inside"><li id="circle">Three</li></ul>
        <ul id="position-only" style="list-style:inside"><li id="disc">Four</li></ul>
      </body></html>"#;
    let (source, computed) = styles(markup, "");

    assert_eq!(at(&source, &computed, "outer").marker_position, MarkerPosition::Inside);
    assert_eq!(at(&source, &computed, "inherited").marker_position, MarkerPosition::Inside);
    assert_eq!(at(&source, &computed, "overridden").marker_position, MarkerPosition::Outside);
    assert_eq!(at(&source, &computed, "shorthand").marker, MarkerKind::Circle);
    assert_eq!(at(&source, &computed, "circle").marker, MarkerKind::Circle);
    assert_eq!(at(&source, &computed, "circle").marker_position, MarkerPosition::Inside);
    assert_eq!(at(&source, &computed, "disc").marker, MarkerKind::Disc);
    assert_eq!(at(&source, &computed, "disc").marker_position, MarkerPosition::Inside);
}
#[test]
fn important_parent_does_not_protect_child_and_user_ties_keep_source_order() {
    let (source, s) = styles(
        "<html><body><p id='parent' style='font-weight:900!important'>Parent<span id='child' style='font-weight:300'>Child</span></p></body></html>",
        "#parent{font-size:2em!important}#child{font-size:75%}span{font-style:italic!important}span{font-style:normal!important}",
    );
    assert_eq!(at(&source, &s, "parent").font_weight.value(), 900.);
    assert_eq!(at(&source, &s, "child").font_weight.value(), 300.);
    assert_eq!(at(&source, &s, "child").size_scale, 1.5);
    assert_eq!(at(&source, &s, "child").font_style, FontStyle::Normal);
}
#[test]
fn css_wide_values_resolve_inheritance_and_initial_values_before_computed_padding() {
    let (source, s) = styles(
        "<html><body style='font-family:monospace;font-size:150%;font-weight:700;line-height:1.8;padding-left:2em'><p id='target' style='font-size:200%;font-weight:900;padding-left:12px'>Text</p><strong id='initial'>Strong</strong></body></html>",
        "p{font-family:inherit!important;font-size:inherit!important;font-weight:unset!important;line-height:unset!important;padding-left:unset!important}strong{font-weight:initial!important;font-size:initial!important}",
    );
    let p = at(&source, &s, "target");
    assert_eq!(p.font_family, Family::Generic(GenericFamily::Monospace));
    assert_eq!(p.size_scale, 1.5);
    assert_eq!(p.font_weight.value(), 700.);
    assert_eq!(p.line_height, Some(1.8));
    assert_eq!(p.padding[0].pixels(1000, 42.), 0.);
    assert_eq!(at(&source, &s, "initial").font_weight.value(), 400.);
    assert_eq!(at(&source, &s, "initial").size_scale, 1.);
}
#[test]
fn strict_css_diagnostics_reject_unimplemented_features_and_bound_work() {
    for (css, detail) in [
        ("p {display:block}", "Only display:none"),
        ("p {margin-top:1em}", "margin-top"),
        ("img {float:left}", "float"),
        ("p:hover {font-weight:700}", "selector"),
        ("@import 'https://example.test/a.css';", "at-rule"),
        ("@font-face{font-family:x;src:url(x)}", "at-rule"),
        ("p{font-size:revert}", "font-size"),
        ("p{line-height:99}", "line-height"),
    ] {
        let error = validate_user_css(css).unwrap_err().to_string();
        assert!(error.contains("Line 1"), "{error}");
        assert!(error.contains(detail), "{error}");
    }
    let oversized = " ".repeat(crate::user_styles::MAX_CSS + 1);
    assert!(validate_user_css(&oversized).is_err());
    let too_many = "p{font-weight:700}".repeat(crate::user_styles::MAX_RULES + 1);
    assert!(
        validate_user_css(&too_many)
            .unwrap_err()
            .to_string()
            .contains("128")
    );
    let too_many = format!(
        "p{{{}}}",
        "font-weight:700;".repeat(crate::user_styles::MAX_DECLARATIONS + 1)
    );
    assert!(validate_user_css(&too_many).is_err());
    assert!(validate_user_css("/* empty and commented sheets are harmless */").is_ok());
    for p in crate::user_styles::PRESETS {
        if let crate::user_styles::Effect::Css(css) = p.effect {
            validate_user_css(css).unwrap_or_else(|e| panic!("{}: {e}", p.id));
        }
    }
}

#[test]
fn user_display_and_visibility_use_the_shared_rendered_flow_projection() {
    let (source, computed) = styles(
        "<html><body><p id='body'>Before <span id='cut'>SECRET</span><span id='veil'>hidden <b id='restore'>visible</b></span> after.</p></body></html>",
        "#cut{display:none}#veil{visibility:hidden}#restore{visibility:visible}",
    );
    assert!(at(&source, &computed, "cut").flow.suppressed);
    assert!(!at(&source, &computed, "veil").flow.suppressed);
    assert!(!at(&source, &computed, "veil").flow.painted);
    assert!(at(&source, &computed, "restore").flow.painted);

    let flow = crate::flow_view::ChapterFlow::from_styles(&source, &computed).unwrap();
    let body = source
        .elements
        .iter()
        .position(|element| element.id.as_deref() == Some("body"))
        .unwrap();
    let block = source
        .blocks
        .iter()
        .find(|block| block.element == body)
        .unwrap();
    let view = flow.view(&source, block).unwrap();
    assert!(!view.text().contains("SECRET"));
    assert!(view.text().contains("hidden"));
    assert!(view.text().contains("visible"));
    let hidden_start = view.text().find("hidden").unwrap();
    let visible_start = view.text().find("visible").unwrap();
    assert!(view.runs().unwrap().iter().any(|run| {
        run.layout.start <= hidden_start
            && hidden_start < run.layout.end
            && !run.painted
    }));
    assert!(view.runs().unwrap().iter().any(|run| {
        run.layout.start <= visible_start && visible_start < run.layout.end && run.painted
    }));
}

#[test]
fn pseudo_generated_user_css_accepts_supported_rules_and_rejects_inert_declarations() {
    let markup = r#"<html><head><style>
        #target::before {content:"Publisher"}
        #target::after {content:"Suffix"}
        </style></head><body><p id="target">Original text</p></body></html>"#;
    let (source, computed) = styles(
        markup,
        "#target::before {content:'Reader' !important;font-weight:700}\
         #target::after {display:none !important}",
    );
    let host = at(&source, &computed, "target");
    let generated = host.generated.as_deref().unwrap();
    let before = generated[0].as_ref().unwrap();
    assert_eq!(before.text, "Reader");
    assert_eq!(before.style.font_weight.value(), 700.);
    assert_eq!(host.font_weight.value(), 400.);
    assert!(generated[1].is_none());
    assert_eq!(source.blocks[0].text, "Original text");

    for (css, detail) in [
        ("p{content:'Inert'}", "needs a before/after"),
        ("p, p::before{content:'Mixed'}", "needs a before/after"),
        (
            "p::first-letter{content:'Replacement'}",
            "needs a before/after",
        ),
        ("p{initial-letter:3 2}", "needs a first-letter"),
        ("p::before{initial-letter:3 2}", "needs a first-letter"),
        ("p::before{display:block}", "Unsupported declaration"),
        ("p::before{padding-left:4px}", "Unsupported declaration"),
        ("p::before{margin-right:4px}", "Unsupported declaration"),
        ("p::first-letter{float:right}", "Unsupported declaration"),
    ] {
        let error = validate_user_css(css).unwrap_err().to_string();
        assert!(error.contains(detail), "{error}");
    }
    let (_, with_initial) = styles(
        markup,
        "#target::first-letter {font-weight:700;float:left;initial-letter:3 2;margin-right:4px}",
    );
    assert!(at(&source, &with_initial, "target").first_letter.is_some());
}

#[test]
fn pseudo_first_line_user_css_keeps_paragraph_rules_separate_and_rejects_box_properties() {
    let markup =
        r#"<html><body><p id="target" style="font-weight:400">Harbor lights</p></body></html>"#;
    let (source, computed) = styles(
        markup,
        "#target::first-line {font-weight:700;font-size:150%;line-height:1.2}",
    );
    let host = at(&source, &computed, "target");
    assert!(host.first_line.is_some());
    assert_eq!(host.font_weight.value(), 400.);
    assert_eq!(host.size_scale, 1.);
    assert_eq!(source.blocks[0].text, "Harbor lights");
    for (css, detail) in [
        (
            "p::first-line{content:'Unreachable'}",
            "needs a before/after",
        ),
        ("p::first-line{padding-left:4px}", "Unsupported declaration"),
        ("p::first-line{display:block}", "Unsupported declaration"),
    ] {
        let error = validate_user_css(css).unwrap_err().to_string();
        assert!(error.contains(detail), "{error}");
    }
}

#[test]
fn user_rules_remain_effective_without_author_css_and_never_mutate_source() {
    let markup =
        "<html><body><p id='target' style='padding-left:90px'>Canonical text</p></body></html>";
    let source = Source::parse(markup).unwrap();
    let before = source.blocks[0].text.clone();
    let snapshot =
        crate::user_styles::Snapshot::new("p{padding-left:12px!important}".into()).unwrap();
    let styled = BookStyle::default()
        .with_user_css(&snapshot)
        .unwrap()
        .compute(&source.elements);
    assert_eq!(
        at(&source, &styled, "target").padding[0].pixels(1000, 42.),
        12.
    );
    assert_eq!(source.blocks[0].text, before);
    assert_eq!(
        user_css_matches("#absent{font-weight:700}", &source.elements).unwrap(),
        (1, 0)
    );
    assert_eq!(
        user_css_matches("p{font-weight:700}", &source.elements).unwrap(),
        (1, 1)
    );
}

#[test]
fn every_published_preset_changes_its_supported_computed_property_through_user_origin() {
    let markup = r#"<html><body style="font-size:150%;font-family:monospace"><p id="p" style="text-indent:2em!important;padding:8px!important;text-align:right!important;break-before:page!important;break-after:page!important;orphans:1;widows:1">Paragraph <a id="a" href="x" style="font-weight:300!important">link</a></p><p id="following" style="text-indent:2em!important">Following paragraph</p><h1 id="h" style="text-align:left!important">Heading</h1><img id="i" src="x" style="width:72px!important"/><table id="t"><tr id="r"><td>Cell</td></tr></table><code id="code">code</code></body></html>"#;
    for preset in crate::user_styles::PRESETS {
        let css = match preset.effect {
            crate::user_styles::Effect::Css(css)
            | crate::user_styles::Effect::NativeTableWidth(css) => css,
            _ => continue,
        };
        let markup = match preset.id {
            "link-underline" => markup.replace(
                "font-weight:300!important",
                "font-weight:300!important;text-decoration:none!important",
            ),
            "link-no-italic" => markup.replace(
                "font-weight:300!important",
                "font-weight:300!important;font-style:italic!important",
            ),
            "margin_horizontal_all_0" | "paragraph_no_horizontal_margin" => markup.replace(
                "text-indent:2em!important;padding:8px!important",
                "margin-left:14px!important;margin-right:20px!important;text-indent:2em!important;padding:8px!important",
            ),
            "margin_vertical_all_0" | "paragraph_no_vertical_margin" => markup.replace(
                "text-indent:2em!important;padding:8px!important",
                "margin-top:14px!important;margin-bottom:20px!important;text-indent:2em!important;padding:8px!important",
            ),
            "margin_body_0" => markup.replace(
                "<body style=\"",
                "<body id=\"body\" style=\"margin:14px!important;",
            ),
            "text_align_most_right" | "text_align_all_right" => {
                markup.replace("text-align:right!important", "text-align:left!important")
            }
            "hyphenate_all_auto" => markup.replace(
                "text-indent:2em!important;padding:8px!important",
                "hyphens:none!important;text-indent:2em!important;padding:8px!important",
            ),
            "lineheight_all_inherit" => markup.replace(
                "text-indent:2em!important;padding:8px!important",
                "line-height:2.5!important;text-indent:2em!important;padding:8px!important",
            ),
            "h2_page-break-before_always" | "h3_page-break-before_always"
            | "h4_page-break-before_always" | "h5_page-break-before_always"
            | "h6_page-break-before_always" => markup.replace(
                "</body>",
                "<h2 id='h2'>Two</h2><p>Gap</p><h3 id='h3'>Three</h3><p>Gap</p><h4 id='h4'>Four</h4><p>Gap</p><h5 id='h5'>Five</h5><p>Gap</p><h6 id='h6'>Six</h6></body>",
            ),
            "sub_sup_smaller" => markup.replace(
                "</body>",
                "<p><sup id='sup'>1</sup><sub id='sub'>2</sub></p></body>",
            ),
            "ruby_font_sans_serif" | "ruby_font_size_larger" | "ruby_most_line_height_larger" => {
                markup.replace(
                    "</body>",
                    "<p><ruby id='ruby'>\u{6F22}<rt id='rt'>kan</rt></ruby></p></body>",
                )
            }
            "widows_orphans_all_inherit" => markup.replace(
                "orphans:1;widows:1",
                "orphans:3!important;widows:3!important",
            ),
            "td_vertical_align_none" => markup.replace(
                "<td>Cell</td>",
                "<td id='td' style='vertical-align:middle!important'>Cell</td><th id='th' style='vertical-align:bottom!important'>Header</th>",
            ),
            "ul_li_type_disc" | "ol_li_type_decimal" => markup.replace(
                "</body>",
                "<ul><li id='ul' style='list-style-type:circle!important'>Bell</li></ul>\
                 <ol type='I'><li id='ol' style='list-style-type:upper-roman!important'>Light</li></ol></body>",
            ),
            _ => markup.into(),
        };
        let (source, before) = styles(&markup, "");
        let (_, after) = styles(&markup, css);
        let p = at(&source, &after, "p");
        let old = at(&source, &before, "p");
        match preset.id {
            "paragraph-indent-none" | "paragraph-indent-em" | "first-paragraph-flush" => {
                assert_ne!(
                    p.text_indent.pixels(1000, 42.),
                    old.text_indent.pixels(1000, 42.)
                )
            }
            "paragraph_following_no_indent" => {
                let indent = |computed: &[Style], id| {
                    at(&source, computed, id).text_indent.pixels(1000, 42.)
                };
                assert_eq!(indent(&after, "p"), indent(&before, "p"));
                assert!(indent(&before, "following") > 0.0);
                assert_eq!(indent(&after, "following"), 0.0);
            }
            "paragraph-align-left" | "paragraph-align-justify" => {
                assert_ne!(p.alignment, old.alignment)
            }
            "text_align_most_right" | "text_align_all_right" => {
                assert_eq!(p.alignment, Some(Alignment::Right));
                assert_ne!(p.alignment, old.alignment);
                assert_eq!(at(&source, &after, "h").alignment, Some(Alignment::Right));
            }
            "text_align_all_left" => {
                assert_eq!(p.alignment, Some(Alignment::Left));
                assert_ne!(p.alignment, old.alignment);
                assert_eq!(at(&source, &after, "h").alignment, Some(Alignment::Left));
            }
            "text_align_all_justify" => {
                assert_eq!(p.alignment, Some(Alignment::Justify));
                assert_ne!(p.alignment, old.alignment);
                assert_eq!(
                    at(&source, &after, "h").alignment,
                    Some(Alignment::Justify)
                );
            }
            "inherit-font-size" | "font_size_most_reset" => {
                assert_ne!(p.size_scale, old.size_scale)
            }
            "body_direction_ltr" => {
                assert_eq!(p.direction, parley::BaseDirection::Ltr);
                assert_ne!(p.direction, old.direction);
            }
            "body_direction_rtl" => {
                assert_eq!(p.direction, parley::BaseDirection::Rtl);
                assert_ne!(p.direction, old.direction);
            }
            "h1_page-break-before_always" | "h2_page-break-before_always"
            | "h3_page-break-before_always" | "h4_page-break-before_always"
            | "h5_page-break-before_always" | "h6_page-break-before_always" => {
                let id = match preset.id.get(..2) {
                    Some("h1") => "h",
                    Some(tag) => tag,
                    None => unreachable!(),
                };
                assert_eq!(
                    at(&source, &after, id).page_breaks.before,
                    crate::publisher_breaks::Edge::Force
                );
                assert_ne!(
                    at(&source, &before, id).page_breaks.before,
                    crate::publisher_breaks::Edge::Force
                );
            }
            "hyphenate_all_auto" => {
                assert_eq!(p.hyphens, Hyphens::Auto);
                assert_ne!(p.hyphens, old.hyphens);
            }
            "ligature_all_no_common_ligature" => {
                assert!(!p.common_ligatures);
                assert_ne!(p.common_ligatures, old.common_ligatures);
            }
            "line_break_cre_loose" => {
                assert!(p.loose_nonbreaking);
                assert!(!old.loose_nonbreaking);
            }
            "lineheight_all_inherit" | "ruby_most_line_height_larger" => {
                assert_ne!(p.line_height, old.line_height)
            }
            "widows_avoid_orphans_allow" => {
                let following = at(&source, &after, "following");
                assert_eq!(following.widows, 2);
                assert_eq!(following.orphans, 1);
            }
            "widows_orphans_avoid" => {
                let following = at(&source, &after, "following");
                assert_eq!(following.widows, 2);
                assert_eq!(following.orphans, 2);
            }
            "widows_orphans_all_inherit" => {
                assert_ne!(p.orphans, old.orphans);
                assert_ne!(p.widows, old.widows);
            }
            "sub_sup_smaller" => {
                for id in ["sup", "sub"] {
                    assert!(at(&source, &after, id).size_scale < at(&source, &before, id).size_scale);
                }
            }
            "ruby_font_sans_serif" => {
                assert_ne!(
                    at(&source, &after, "rt").font_family,
                    at(&source, &before, "rt").font_family
                );
            }
            "ruby_font_size_larger" => {
                // KOReader uses the same `rt {font-size:50%}` rule. Its engine
                // default ruby size is not verified here, so assert the rule's
                // effect: ruby text is half of its parent's computed size.
                let rt = at(&source, &after, "rt").size_scale;
                let parent = at(&source, &after, "ruby").size_scale;
                assert!((rt - parent * 0.5).abs() < 1e-4);
                assert_ne!(rt, at(&source, &before, "rt").size_scale);
            }
            "headings-centered" => assert_ne!(
                at(&source, &after, "h").alignment,
                at(&source, &before, "h").alignment
            ),
            "horizontal-padding-none" | "paragraph_no_horizontal_padding" => {
                assert_ne!(p.padding, old.padding)
            }
            "vertical-padding-none" | "paragraph_no_vertical_padding" => {
                assert_ne!(p.vertical_padding, old.vertical_padding)
            }
            "margin_vertical_all_0" | "paragraph_no_vertical_margin" => {
                assert_eq!(
                    old.vertical_margin.map(|x| x.pixels(1000, 42.0)),
                    [14.0, 20.0]
                );
                assert_eq!(p.vertical_margin.map(|x| x.pixels(1000, 42.0)), [0.0, 0.0]);
            }
            "margin_body_0" => {
                let body = at(&source, &before, "body");
                let changed = at(&source, &after, "body");
                let pixels = |margin: BoxMargin| match margin {
                    BoxMargin::Length(value) => value.pixels(1000, 42.0),
                    BoxMargin::Auto => f32::NAN,
                };
                assert_ne!(
                    body.horizontal_margin.map(pixels),
                    changed.horizontal_margin.map(pixels)
                );
                assert_ne!(body.vertical_margin, changed.vertical_margin);
            }
            "publisher-breaks-none" => assert!(p.page_breaks.before != old.page_breaks.before),
            "headings-new-page" => assert!(
                at(&source, &after, "h").page_breaks.before
                    != at(&source, &before, "h").page_breaks.before
            ),
            "protect-lines" => assert_ne!((p.orphans, p.widows), (old.orphans, old.widows)),
            "link-underline" | "link-no-underline" => assert_ne!(
                at(&source, &after, "a").decoration.underline,
                at(&source, &before, "a").decoration.underline
            ),
            "link-bold" | "link-no-bold" => assert_ne!(
                at(&source, &after, "a").font_weight,
                at(&source, &before, "a").font_weight
            ),
            "link-italic" | "link-no-italic" => assert_ne!(
                at(&source, &after, "a").font_style,
                at(&source, &before, "a").font_style
            ),
            "image-text-width" => assert_ne!(
                at(&source, &after, "i")
                    .image_width
                    .unwrap()
                    .pixels(1000, 42.),
                at(&source, &before, "i")
                    .image_width
                    .unwrap()
                    .pixels(1000, 42.)
            ),
            "td_vertical_align_none" => {
                assert_eq!(
                    at(&source, &before, "td").image_align,
                    parley::VerticalAlign::MIDDLE
                );
                assert_eq!(
                    at(&source, &after, "td").image_align,
                    parley::VerticalAlign::BASELINE
                );
                assert_eq!(
                    at(&source, &after, "th").image_align,
                    parley::VerticalAlign::BOTTOM
                );
            }
            "ul_li_type_disc" | "ol_li_type_decimal" => {
                use crate::list_marker::MarkerKind::{Circle, Decimal, Disc, UpperRoman};
                let (id, initial, changed, other) = if preset.id == "ul_li_type_disc" {
                    ("ul", Circle, Disc, "ol")
                } else {
                    ("ol", UpperRoman, Decimal, "ul")
                };
                assert_eq!(at(&source, &before, id).marker, initial);
                assert_eq!(at(&source, &after, id).marker, changed);
                assert_eq!(
                    at(&source, &after, other).marker,
                    at(&source, &before, other).marker
                );
            }
            "table_force_border" | "table-visible-borders" => {
                assert!(at(&source, &before, "t").borders.is_none());
                assert!(at(&source, &after, "t").borders.is_some());
                assert!(at(&source, &after, "t").border_collapse);
            }
            "table_row_odd_even" => {
                assert_eq!(at(&source, &before, "r").background, None);
                assert_eq!(at(&source, &after, "r").background, Some(238));
            }
            "table_full_width" => {
                assert!(at(&source, &before, "t").image_width.is_none());
                assert!(matches!(
                    at(&source, &after, "t").image_width,
                    Some(ImageLength::Fraction(1.0))
                ));
            }
            "table_td_width_auto" => {
                assert!(at(&source, &before, "t").image_width.is_none());
                assert!(matches!(
                    at(&source, &after, "t").image_width,
                    Some(ImageLength::Auto)
                ));
            }
            "margin_horizontal_all_0" | "paragraph_no_horizontal_margin" => {
                let pixels = |margin: BoxMargin| match margin {
                    BoxMargin::Length(value) => value.pixels(1000, 42.0),
                    BoxMargin::Auto => f32::NAN,
                };
                assert_eq!(old.horizontal_margin.map(pixels), [14.0, 20.0]);
                assert_eq!(p.horizontal_margin.map(pixels), [0.0, 0.0]);
            }
            "table_margin_left_right_auto" => {
                assert!(matches!(
                    at(&source, &before, "t").horizontal_margin,
                    [BoxMargin::Length(_), BoxMargin::Length(_)]
                ));
                assert!(matches!(
                    at(&source, &after, "t").horizontal_margin,
                    [BoxMargin::Auto, BoxMargin::Auto]
                ));
            }
            "image_valign_middle" => {
                assert_eq!(
                    at(&source, &before, "i").image_align,
                    parley::VerticalAlign::BASELINE
                );
                assert_eq!(
                    at(&source, &after, "i").image_align,
                    parley::VerticalAlign::MIDDLE
                );
            }
            "inherit-font-size" => assert_ne!(p.size_scale, old.size_scale),
            "inherit-font-family" => {
                assert_ne!(p.font_family, old.font_family);
                assert_eq!(
                    at(&source, &after, "code").font_family,
                    Family::Generic(GenericFamily::Monospace)
                );
            }
            _ => panic!("Add a functional assertion for {}", preset.id),
        }
    }
    let error = validate_user_css("p {\n font-weight:700;\n display:block;\n}")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Line 3"), "{error}");
}
