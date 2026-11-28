use super::*;

fn styled(markup: &str, selection: crate::BaseStylesheet) -> (Source, Vec<Style>) {
    let source = Source::parse(markup).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let chapter = book.chapters[0].href.clone();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let styles = BookStyle::load_with_base(
        &mut book,
        &chapter,
        &source,
        &mut fonts,
        crate::Settings {
            base_stylesheet: selection,
            ..Default::default()
        },
    )
    .compute(&source.elements);
    (source, styles)
}

fn element<'a>(source: &Source, styles: &'a [Style], id: &str) -> &'a Style {
    &styles[source
        .elements
        .iter()
        .position(|element| element.id.as_deref() == Some(id))
        .unwrap()]
}

#[test]
fn auto_uses_epub_traditional_style_and_none_adds_no_user_agent_rules() {
    let markup = "<html><body><p id='prose'>A paragraph.</p></body></html>";
    let (source, auto) = styled(markup, crate::BaseStylesheet::Auto);
    let (none_source, none) = styled(markup, crate::BaseStylesheet::None);
    assert_eq!(
        element(&source, &auto, "prose").alignment,
        Some(Alignment::Justify)
    );
    assert_eq!(
        element(&source, &auto, "prose")
            .text_indent
            .pixels(500, 20.0),
        24.0
    );
    assert_eq!(element(&none_source, &none, "prose").alignment, None);
    assert_eq!(
        element(&none_source, &none, "prose")
            .text_indent
            .pixels(500, 20.0),
        0.0
    );
}

#[test]
fn html_standard_uses_browser_paragraph_spacing_without_book_indentation() {
    let (source, styles) = styled(
        "<html><body><p id='prose'>A paragraph.</p></body></html>",
        crate::BaseStylesheet::HtmlStandard,
    );
    let paragraph = element(&source, &styles, "prose");
    assert_eq!(paragraph.alignment, None);
    assert_eq!(paragraph.text_indent.pixels(500, 20.0), 0.0);
    assert_eq!(paragraph.vertical_padding[1].pixels(500, 20.0), 20.0);
    assert_eq!(paragraph.size_scale, 1.0);
}

#[test]
fn publisher_and_inline_styles_override_the_base_stylesheet() {
    let (source, styles) = styled(
        r#"<html><head><style>p { text-align: right; text-indent: 2em; }</style></head>
        <body><p id="css">Publisher stylesheet.</p>
        <p id="inline" style="text-align: left; text-indent: 0">Inline style.</p></body></html>"#,
        crate::BaseStylesheet::Traditional,
    );
    assert_eq!(
        element(&source, &styles, "css").alignment,
        Some(Alignment::Right)
    );
    assert_eq!(
        element(&source, &styles, "css")
            .text_indent
            .pixels(500, 20.0),
        40.0
    );
    assert_eq!(
        element(&source, &styles, "inline").alignment,
        Some(Alignment::Left)
    );
    assert_eq!(
        element(&source, &styles, "inline")
            .text_indent
            .pixels(500, 20.0),
        0.0
    );
}

#[test]
fn fictionbook_style_restores_semantic_heading_and_verse_layout() {
    let (source, styles) = styled(
        r#"<html><body><h1 id="heading">A section</h1>
        <p id="verse" class="fb2-verse">A line of verse.</p></body></html>"#,
        crate::BaseStylesheet::FictionBook,
    );
    let heading = element(&source, &styles, "heading");
    let verse = element(&source, &styles, "verse");
    assert_eq!(heading.alignment, Some(Alignment::Center));
    assert!(matches!(
        heading.page_breaks.before,
        crate::publisher_breaks::Edge::Force
    ));
    assert!(verse.text_indent.hanging);
    assert_eq!(verse.alignment, Some(Alignment::Left));
}

#[test]
fn publisher_important_rule_overrides_base_heading_break() {
    let (source, styles) = styled(
        r#"<html><head><style>h1 { page-break-before: auto !important; }</style></head>
        <body><h1 id="heading">A section</h1></body></html>"#,
        crate::BaseStylesheet::FictionBook,
    );
    assert!(matches!(
        element(&source, &styles, "heading").page_breaks.before,
        crate::publisher_breaks::Edge::Auto
    ));
}

#[test]
fn a_book_base_sheet_replaces_the_builtin_base_sheet_and_clears_back() {
    let markup = r#"<html><body><p id="p">Text</p></body></html>"#;
    let (builtin_source, builtin) = styled(markup, crate::BaseStylesheet::Traditional);
    let builtin_indent = element(&builtin_source, &builtin, "p")
        .text_indent
        .pixels(1000, 42.0);

    let mut data = crate::user_styles::Data::default();
    data.choose_base_sheet(7, Some("p { text-indent: 3em; }".into()))
        .unwrap();
    let snapshot = data.snapshot(7).unwrap();
    assert_eq!(snapshot.base_css(), Some("p { text-indent: 3em; }"));
    assert!(data.snapshot(8).unwrap().base_css().is_none());

    let source = Source::parse(markup).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let chapter = book.chapters[0].href.clone();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let custom = BookStyle::load_with_base(
        &mut book,
        &chapter,
        &source,
        &mut fonts,
        crate::Settings {
            base_stylesheet: crate::BaseStylesheet::None,
            ..Default::default()
        },
    )
    .with_user_css(&snapshot)
    .unwrap()
    .compute(&source.elements);
    let custom_indent = element(&source, &custom, "p").text_indent.pixels(1000, 42.0);
    assert!(custom_indent > 0.0);
    assert_ne!(custom_indent, builtin_indent);

    data.choose_base_sheet(7, None).unwrap();
    assert!(data.snapshot(7).unwrap().base_css().is_none());
}

#[test]
fn a_book_base_sheet_refuses_oversized_text_and_keeps_the_previous_choice() {
    let mut data = crate::user_styles::Data::default();
    data.choose_base_sheet(1, Some("p { text-indent: 1em; }".into()))
        .unwrap();
    let huge = "p { text-indent: 1em; }\n".repeat(10_000);
    assert!(data.choose_base_sheet(1, Some(huge)).is_err());
    assert_eq!(
        data.snapshot(1).unwrap().base_css(),
        Some("p { text-indent: 1em; }")
    );
}
