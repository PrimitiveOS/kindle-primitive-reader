use super::*;
use crate::flow_projection::ElementFlow;
use flow::{Display, ListChild, Visibility, list_ordinals};
fn computed(markup: &str, user: &str) -> (Source, Vec<Style>) {
    let source = Source::parse(markup).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let mut sheet = BookStyle::load_for_flow(
        &mut book,
        &href,
        &source,
        true,
        &crate::user_styles::Snapshot::default(),
    )
    .unwrap();
    // Trusted test ASTs exercise origin priority without enabling public user input.
    let mut parser = Parser::new(user);
    for rule in StyleSheetParser::new(&mut parser, &mut SheetParser).flatten() {
        if let SheetItem::Rule(rule) = rule {
            sheet.user_rules.push(rule);
        }
    }
    let styles = sheet.compute(&source.elements);
    (source, styles)
}
fn at<'a>(source: &Source, styles: &'a [Style], id: &str) -> &'a Style {
    &styles[source
        .elements
        .iter()
        .position(|e| e.id.as_deref() == Some(id))
        .unwrap()]
}
#[test]
fn inherited_visibility_can_restore_paint_but_never_ancestor_display_suppression() {
    let (source, styles) = computed(
        r#"<html><body><section style="visibility:hidden"><p id="hidden">Hidden</p><p id="restored" style="visibility:visible">Visible</p><p id="collapse" style="visibility:collapse"><span id="inside">Hidden</span><span id="visible" style="visibility:visible">Visible</span></p></section><section style="display:none"><p id="suppressed" style="display:block;visibility:visible">Gone</p></section></body></html>"#,
        "",
    );
    assert_eq!(
        at(&source, &styles, "hidden").flow,
        ElementFlow {
            suppressed: false,
            painted: false
        }
    );
    assert_eq!(
        at(&source, &styles, "restored").flow,
        ElementFlow::default()
    );
    assert_eq!(
        at(&source, &styles, "inside").visibility,
        Visibility::Collapse
    );
    assert!(!at(&source, &styles, "inside").flow.painted);
    assert!(at(&source, &styles, "visible").flow.painted);
    assert_eq!(at(&source, &styles, "suppressed").display, Display::Block);
    assert_eq!(
        at(&source, &styles, "suppressed").flow,
        ElementFlow {
            suppressed: true,
            painted: false
        }
    );
}
#[test]
fn display_is_local_and_reset_keywords_match_the_pinned_inline_inherit_limitation() {
    let (source, styles) = computed(
        r#"<html><body><section style="display:block;visibility:hidden"><span id="native">Inline</span><p id="inherit" style="display:inherit">Inline reset</p><p id="initial" style="display:initial;visibility:initial">Visible</p><li id="unset" style="display:unset;visibility:unset">Hidden</li></section><div style="display:none"><p id="none-inherit" style="display:inherit;visibility:visible">Still gone</p></div></body></html>"#,
        "",
    );
    for id in ["native", "inherit", "initial", "unset", "none-inherit"] {
        assert_eq!(at(&source, &styles, id).display, Display::Inline, "{id}");
    }
    assert_eq!(
        at(&source, &styles, "inherit").visibility,
        Visibility::Hidden
    );
    assert_eq!(
        at(&source, &styles, "initial").visibility,
        Visibility::Visible
    );
    assert_eq!(at(&source, &styles, "unset").visibility, Visibility::Hidden);
    assert!(at(&source, &styles, "none-inherit").flow.suppressed);
}
#[test]
fn display_and_visibility_use_the_existing_user_author_importance_and_inline_tiers() {
    let markup = r#"<html><head><style>#target{display:none!important;visibility:hidden!important}</style></head><body><p id="target" style="display:none!important;visibility:collapse!important">Text</p></body></html>"#;
    let (source, styles) = computed(
        markup,
        "*{display:block!important;visibility:visible!important}",
    );
    assert_eq!(at(&source, &styles, "target").flow, ElementFlow::default());
    let (source, styles) = computed(markup, "#target{display:block;visibility:visible}");
    assert!(at(&source, &styles, "target").flow.suppressed);
    assert_eq!(
        at(&source, &styles, "target").visibility,
        Visibility::Collapse
    );
    let (source, styles) = computed(
        "<html><body><p id='target' style='display:block;visibility:visible'>Text</p></body></html>",
        "#target{display:none;visibility:hidden}",
    );
    assert_eq!(at(&source, &styles, "target").flow, ElementFlow::default());
}
#[test]
fn strict_user_flow_properties_stay_closed_until_all_consumers_are_migrated() {
    for value in [
        "none",
        "inline",
        "block",
        "list-item",
        "inherit",
        "initial",
        "unset",
        "table",
        "flex",
        "grid",
    ] {
        let text = format!("p{{display:{value}}}");
        let message = validate_user_css(&text).unwrap_err().to_string();
        assert!(
            message.contains("display") && message.contains("migrated"),
            "{message}"
        );
    }
    for value in [
        "hidden", "collapse", "visible", "inherit", "initial", "unset",
    ] {
        assert!(
            validate_user_css(&format!("p{{visibility:{value}}}"))
                .unwrap_err()
                .to_string()
                .contains("visibility")
        );
    }
    for value in [
        "none",
        "ruby",
        "run-in",
        "inline",
        "block",
        "-cr-list-item-final",
        "list-item",
        "inline-block",
        "inline-table",
        "table",
        "table-row-group",
        "table-header-group",
        "table-footer-group",
        "table-row",
        "table-column-group",
        "table-column",
        "table-cell",
        "table-caption",
    ] {
        assert!(Display::parse(value).is_some(), "{value}");
    }
    for value in [
        "flex",
        "grid",
        "contents",
        "flow-root",
        "inline flex",
        "normal",
    ] {
        assert!(Display::parse(value).is_none(), "{value}");
    }
    assert!(Display::Block.requires_reclassification("span"));
    assert!(!Display::Block.requires_reclassification("p"));
    assert!(!Display::None.requires_reclassification("p"));
}
#[test]
fn font_free_and_renderer_loaders_share_sheet_inline_and_rule_limits() {
    for fixture in ["field-notes-epub3.epub", "publisher-faces.epub"] {
        let mut book = Book::open(&crate::test_fixture(fixture)).unwrap();
        let href = book.chapters[0].href.clone();
        let source = Source::parse(&book.chapter(0).unwrap()).unwrap();
        let snapshot =
            crate::user_styles::Snapshot::new("p{font-weight:700!important}".into()).unwrap();
        let flow = BookStyle::load_for_flow(&mut book, &href, &source, true, &snapshot).unwrap();
        assert!(flow.families.is_empty());
        assert!(flow.font_weights.is_empty());
        let masks: Vec<_> = flow
            .compute(&source.elements)
            .iter()
            .map(|s| s.flow)
            .collect();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let renderer = BookStyle::load(&mut book, &href, &source, &mut fonts)
            .with_user_css(&snapshot)
            .unwrap();
        assert_eq!(
            masks,
            renderer
                .compute(&source.elements)
                .iter()
                .map(|s| s.flow)
                .collect::<Vec<_>>()
        );
    }
    let mut markup = "<html><head>".to_owned();
    for _ in 0..8 {
        markup.push_str("<style>p{visibility:hidden}</style>");
    }
    markup.push_str("<style>p{visibility:visible!important}</style></head><body><p id='target'>Text</p></body></html>");
    let (source, styles) = computed(&markup, "");
    assert!(
        !at(&source, &styles, "target").flow.painted,
        "ninth inline sheet is outside the shared budget"
    );
}
#[test]
fn publisher_switch_removes_flow_declarations_and_preserves_canonical_text() {
    let markup = "<html><head><style>p{display:none}</style></head><body><p id='target' style='visibility:hidden'>Canonical text</p></body></html>";
    let source = Source::parse(markup).unwrap();
    let before: Vec<_> = source
        .blocks
        .iter()
        .map(|b| (b.key, b.text.clone()))
        .collect();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let off = BookStyle::load_for_flow(
        &mut book,
        &href,
        &source,
        false,
        &crate::user_styles::Snapshot::default(),
    )
    .unwrap()
    .compute(&source.elements);
    assert_eq!(at(&source, &off, "target").flow, ElementFlow::default());
    let on = BookStyle::load_for_flow(
        &mut book,
        &href,
        &source,
        true,
        &crate::user_styles::Snapshot::default(),
    )
    .unwrap()
    .compute(&source.elements);
    assert!(at(&source, &on, "target").flow.suppressed);
    assert_eq!(
        before,
        source
            .blocks
            .iter()
            .map(|b| (b.key, b.text.clone()))
            .collect::<Vec<_>>()
    );
}
fn items() -> Vec<Style> {
    (0..7)
        .map(|_| Style {
            display: Display::ListItem,
            marker: crate::list_marker::MarkerKind::Decimal,
            ..Style::default()
        })
        .collect()
}
#[test]
fn list_counter_skips_suppression_and_hidden_resets_but_keeps_invisible_and_markerless_items() {
    let mut styles = items();
    styles[1].flow = ElementFlow {
        suppressed: true,
        painted: false,
    };
    styles[3].visibility = Visibility::Hidden;
    styles[3].flow.painted = false;
    styles[4].marker = crate::list_marker::MarkerKind::None;
    styles[5].display = Display::Block;
    let children: Vec<_> = (0..7)
        .map(|element| ListChild {
            element,
            value: match element {
                1 => Some(100),
                3 => Some(9),
                5 => Some(50),
                _ => None,
            },
        })
        .collect();
    assert_eq!(
        list_ordinals(&styles, &children, Some(3), false).unwrap(),
        [Some(3), None, Some(4), Some(9), Some(10), None, Some(11)]
    );
    assert_eq!(
        list_ordinals(&styles, &children, None, true).unwrap(),
        [Some(5), None, Some(4), Some(9), Some(8), None, Some(7)]
    );
}
#[test]
fn list_explicit_values_equal_to_old_ordinals_are_not_lost_after_suppression() {
    let mut styles = items();
    styles[1].flow.suppressed = true;
    let forward = [
        ListChild {
            element: 0,
            value: None,
        },
        ListChild {
            element: 1,
            value: None,
        },
        ListChild {
            element: 2,
            value: Some(3),
        },
        ListChild {
            element: 3,
            value: None,
        },
    ];
    assert_eq!(
        list_ordinals(&styles, &forward, None, false).unwrap(),
        [Some(1), None, Some(3), Some(4)]
    );
    let reverse = [
        ListChild {
            element: 0,
            value: None,
        },
        ListChild {
            element: 1,
            value: Some(90),
        },
        ListChild {
            element: 2,
            value: Some(1),
        },
    ];
    assert_eq!(
        list_ordinals(&styles, &reverse, Some(3), true).unwrap(),
        [Some(3), None, Some(1)]
    );
    assert_eq!(
        list_ordinals(&styles, &forward, Some(0), false).unwrap(),
        [Some(0), None, Some(3), Some(4)]
    );
}
#[test]
fn list_native_display_legacy_and_bounds_are_explicit_and_do_not_mutate_canonical_ordinals() {
    let mut styles = items();
    styles[0].display = Display::LegacyListItem;
    styles[1].display = Display::Inline;
    let children = [
        ListChild {
            element: 0,
            value: Some(i32::MAX),
        },
        ListChild {
            element: 1,
            value: None,
        },
        ListChild {
            element: 2,
            value: None,
        },
    ];
    assert_eq!(
        list_ordinals(&styles, &children, None, false).unwrap(),
        [Some(i32::MAX), None, Some(i32::MAX)]
    );
    assert!(
        list_ordinals(
            &styles,
            &[ListChild {
                element: 99,
                value: None
            }],
            None,
            false
        )
        .is_err()
    );
    assert!(list_ordinals(&styles, &[children[0], children[0]], None, false).is_err());
    assert!(list_ordinals(&styles, &[], None, false).unwrap().is_empty());
    for style in &mut styles {
        style.flow.suppressed = true;
    }
    assert_eq!(
        list_ordinals(&styles, &children, None, true).unwrap(),
        [None, None, None]
    );
}
