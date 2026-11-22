use super::*;
use crate::{
    book_style::BookStyle,
    epub::Book,
    footnotes::{self, Reference, ReferenceContext},
};
use std::cmp::Ordering;

fn context() -> ReferenceContext {
    ReferenceContext {
        kind: Reference::Explicit,
        label: "1".into(),
        in_prose: true,
        source_block: 0,
    }
}
fn computed(xml: &str, publisher: bool) -> (Source, Vec<Style>) {
    let source = Source::parse(xml).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let styles =
        BookStyle::load_for_flow(&mut book, &href, &source, publisher, &Default::default())
            .unwrap()
            .compute(&source.elements);
    (source, styles)
}
fn excerpt(xml: &str, source: &Source, styles: &[Style], fragment: &str) -> Option<String> {
    let roots = LayoutRoots::new(source, styles)?;
    footnotes::extract_with_layout(
        xml,
        fragment,
        &context(),
        Ordering::Less,
        false,
        false,
        &roots,
    )
}

#[test]
fn eight_actual_stock_oracle_roots_preserve_containers_css_and_canonical_positions() {
    let path = crate::test_fixture("footnote-export-roots.epub");
    let bytes = std::fs::read(&path).unwrap();
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    // The byte-identical authored EPUB was laid out and tapped in pinned native
    // KOReader SDL run footnote-oracle-20261122T095415Z, with all eight decisions.
    assert_eq!(
        digest,
        "84fff6bd39157018dd1c3a2495996bf6ee95a3cd6edcc1b955612f9874a9b981"
    );
    let mut book = Book::open(&path).unwrap();
    let href = book.chapters[0].href.clone();
    let xml = book.chapter(0).unwrap();
    let source = book.chapter_source(&href, &xml).unwrap();
    let styles = BookStyle::load_for_flow(&mut book, &href, &source, true, &Default::default())
        .unwrap()
        .compute(&source.elements);
    let revision = source.revision;
    let positions = source.fragments.clone();
    let canonical: Vec<_> = source
        .blocks
        .iter()
        .map(|b| (b.key, b.text.clone()))
        .collect();
    assert!(
        styles[source.fragment_owner("noteC").unwrap()]
            .flow
            .suppressed,
        "actual linked stylesheet must be loaded"
    );
    for (id, tag, inline, with_context) in [
        ("noteA", "p", false, false),
        ("noteB", "aside", true, false),
        ("noteC", "p", false, false),
        ("noteD", "aside", false, false),
        ("noteE", "p", false, true),
        ("noteF", "span", false, false),
        ("noteG", "p", true, true),
        ("noteH", "div", false, true),
    ] {
        let html = excerpt(&xml, &source, &styles, id).unwrap();
        let doc = roxmltree::Document::parse(&html).unwrap();
        let body = doc.descendants().find(|n| n.has_tag_name("body")).unwrap();
        let root = body.children().find(Node::is_element).unwrap();
        assert_eq!(root.tag_name().name(), tag, "{id}");
        assert_eq!(html.contains("display:none"), inline, "{id}");
        assert_eq!(html.contains("Visible before"), with_context, "{id}");
        assert_eq!(html.contains("visible after"), with_context, "{id}");
        assert!(
            !html.contains("Control "),
            "a different source paragraph must not be added: {id}"
        );
        assert!(
            !html.contains("Hidden before") && !html.contains("hidden after"),
            "{id}"
        );
        assert!(
            !html.contains("notes.css") && !html.contains("<link"),
            "external styles must stay outside the popup: {id}"
        );
        assert!(
            doc.descendants().any(|n| n.attribute("id") == Some(id)),
            "target identity: {id}"
        );
    }
    assert_eq!(source.revision, revision);
    assert_eq!(source.fragments, positions);
    assert_eq!(
        source
            .blocks
            .iter()
            .map(|b| (b.key, b.text.clone()))
            .collect::<Vec<_>>(),
        canonical
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn visibility_hidden_keeps_final_ownership_while_display_none_removes_it() {
    for (property, tag) in [("visibility:hidden", "p"), ("display:none", "span")] {
        let xml = format!(
            "<html><body><p style='{property}'>Before <span id='n'>Target</span> after.</p></body></html>"
        );
        let (source, styles) = computed(&xml, true);
        let html = excerpt(&xml, &source, &styles, "n").unwrap();
        let doc = roxmltree::Document::parse(&html).unwrap();
        let root = doc
            .descendants()
            .find(|n| n.has_tag_name("body"))
            .unwrap()
            .children()
            .find(Node::is_element)
            .unwrap();
        assert_eq!(root.tag_name().name(), tag);
        assert_eq!(html.contains("Before"), tag == "p");
    }
}

#[test]
fn publisher_policy_uses_the_shared_flow_cascade_and_never_strips_exported_inline_css() {
    let xml = "<html><head><style>.note{display:none}</style></head><body><div class='note'><p>Before <span id='n'>Target</span> after.</p></div></body></html>";
    for (publisher, expected) in [(true, "span"), (false, "p")] {
        let (source, styles) = computed(xml, publisher);
        let html = excerpt(xml, &source, &styles, "n").unwrap();
        let doc = roxmltree::Document::parse(&html).unwrap();
        let body = doc.descendants().find(|n| n.has_tag_name("body")).unwrap();
        assert_eq!(
            body.children()
                .find(Node::is_element)
                .unwrap()
                .tag_name()
                .name(),
            expected
        );
    }
}

#[test]
fn incompatible_box_reclassification_refuses_the_popup_instead_of_selecting_a_false_root() {
    for inner in [
        "<p id='n' style='display:inline'>Target</p>",
        "<p><span id='n'>Target</span><span style='display:block'>Sibling block</span></p>",
        "<p><span id='n' style='display:inline-block'>Target</span></p>",
    ] {
        let xml = format!("<html><body>{inner}</body></html>");
        let (source, styles) = computed(&xml, true);
        assert!(excerpt(&xml, &source, &styles, "n").is_none(), "{inner}");
    }
}

#[test]
fn stale_layout_wrong_duplicate_owner_and_incomplete_styles_cannot_redirect_the_raw_target() {
    let xml = "<html><body><p id='n'>First target</p><p>After</p></body></html>";
    let (source, styles) = computed(xml, true);
    assert!(LayoutRoots::new(&source, &styles[..styles.len() - 1]).is_none());
    assert!(excerpt(&xml.replace("First", "Other"), &source, &styles, "n").is_none());
    let duplicate = "<html><body><p id='n' hidden='hidden'>First hidden target</p><p id='n'>Second target</p></body></html>";
    let (source, styles) = computed(duplicate, true);
    assert!(excerpt(duplicate, &source, &styles, "n").is_none());
}

#[test]
fn numeric_reference_ruby_and_empty_inline_targets_keep_true_parent_identity() {
    let xml = "<html><body><p>Before &#146; <ruby>Base<rt>reading</rt></ruby> <span id='n'></span> actual note after.</p><p>Other paragraph</p></body></html>";
    let (source, styles) = computed(xml, true);
    assert_ne!(source.revision, crate::revision(xml));
    assert!(source.matches_input(xml));
    let original = source.fragments.clone();
    let html = excerpt(xml, &source, &styles, "n").unwrap();
    assert!(
        html.contains("Before &#146;")
            && html.contains("<ruby>")
            && html.contains("actual note after")
    );
    assert!(!html.contains("Other paragraph"));
    assert_eq!(source.fragments, original);
}

#[test]
fn exact_raw_source_binding_survives_ruby_numeric_and_anonymous_parse_stages() {
    for xml in [
        "<html><body><p id='n'>Target</p></body></html>",
        "<html><body><p id='n'><ruby>Target<rt>reading</rt></ruby></p></body></html>",
        "<html><body><p id='n'>Target &#146;</p></body></html>",
        "<html><body><p id='n'><ruby>Target<rt>reading</rt></ruby> &#146;</p></body></html>",
        "<html><body>Loose <span id='n'>Target</span><p>Following block</p></body></html>",
    ] {
        let source = Source::parse(xml).unwrap();
        assert!(source.matches_input(xml));
        assert!(!source.matches_input(&xml.replace("Target", "Other")));
    }
}

#[test]
fn anonymous_layout_boxes_and_between_block_extension_remain_explicitly_separate() {
    let loose =
        "<html><body>Loose <span id='n'>inline note</span><p>Following block</p></body></html>";
    let (source, styles) = computed(loose, true);
    assert!(source.elements.iter().any(|e| e.anonymous));
    assert!(excerpt(loose, &source, &styles, "n").is_none());
    let empty = "<html><body><div><a id='n'/><p>Following note paragraph</p></div></body></html>";
    let (source, styles) = computed(empty, true);
    assert!(excerpt(empty, &source, &styles, "n").is_none());
    assert!(
        footnotes::extract(empty, "n", &context(), Ordering::Less, false, false)
            .unwrap()
            .contains("Following note paragraph")
    );
}
