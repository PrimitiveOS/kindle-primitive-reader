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
        ("p {display:none}", "display"),
        ("p {visibility:hidden}", "visibility"),
        ("p {margin-top:1em}", "margin-top"),
        ("img {float:left}", "float"),
        ("p:not(.x) {font-weight:700}", "selector"),
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
    let markup = r#"<html><body style="font-size:150%;font-family:monospace"><p id="p" style="text-indent:2em!important;padding:8px!important;text-align:right!important;break-before:page!important;break-after:page!important;orphans:1;widows:1">Paragraph <a id="a" href="x" style="font-weight:300!important">link</a></p><h1 id="h" style="text-align:left!important">Heading</h1><img id="i" src="x" style="width:72px!important"/><table id="t"><tr><td>Cell</td></tr></table><code id="code">code</code></body></html>"#;
    for preset in crate::user_styles::PRESETS {
        let crate::user_styles::Effect::Css(css) = preset.effect else {
            continue;
        };
        let markup = if preset.id == "link-underline" {
            markup.replace(
                "font-weight:300!important",
                "font-weight:300!important;text-decoration:none!important",
            )
        } else {
            markup.into()
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
            "paragraph-align-left" | "paragraph-align-justify" => {
                assert_ne!(p.alignment, old.alignment)
            }
            "headings-centered" => assert_ne!(
                at(&source, &after, "h").alignment,
                at(&source, &before, "h").alignment
            ),
            "horizontal-padding-none" => assert_ne!(p.padding, old.padding),
            "vertical-padding-none" => assert_ne!(p.vertical_padding, old.vertical_padding),
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
            "link-bold" => assert_ne!(
                at(&source, &after, "a").font_weight,
                at(&source, &before, "a").font_weight
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
            "table-visible-borders" => {
                assert!(at(&source, &before, "t").borders.is_none());
                assert!(at(&source, &after, "t").borders.is_some());
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
    let error = validate_user_css("p {\n font-weight:700;\n display:none;\n}")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Line 3"), "{error}");
}
