//! HTML table presentation declared separately from publisher CSS.
use super::{
    Alignment, BoxMargin, CascadeOrigin, CssLength, Declaration, ImageLength, Parser, Property,
    Winners, background, borders, choose,
};
use roxmltree::Node;

pub(crate) struct TableHints(Box<[Declaration]>);

fn length(value: &str, percent: bool) -> Option<ImageLength> {
    let value = value.trim();
    let (number, unit) = if let Some(number) = value.strip_suffix('%') {
        (number, "%")
    } else {
        (value.strip_suffix("px").unwrap_or(value), "px")
    };
    let number = number.trim().parse::<f32>().ok()?;
    if !number.is_finite() || !(0.0..=4096.0).contains(&number) {
        return None;
    }
    match unit {
        "%" if percent => Some(ImageLength::Fraction(number / 100.0)),
        "%" => None,
        _ => Some(ImageLength::Pixels(number)),
    }
}

fn pixels(value: &str) -> Option<CssLength> {
    length(value, false)?.css_length()
}

fn own_color(node: Node<'_, '_>, name: &str) -> Option<u8> {
    let value = node.attribute(name).filter(|v| v.len() <= 128)?;
    Parser::new(value).parse_entirely(background).ok().flatten()
}

fn word(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .filter(|value| value.len() <= 32)
        .map(|value| value.trim().to_ascii_lowercase())
}

impl TableHints {
    pub(crate) fn read(node: Node<'_, '_>) -> Option<Self> {
        use borders::{Pattern, Value};
        use parley::VerticalAlign;

        let tag = node.tag_name().name();
        let cell = matches!(tag, "td" | "th");
        let group = matches!(tag, "thead" | "tbody" | "tfoot");
        let row = tag == "tr";
        if !(matches!(tag, "table" | "caption" | "col" | "colgroup") || cell || group || row) {
            return None;
        }
        let table = node.ancestors().find(|node| node.has_tag_name("table"))?;
        let root = node == table;
        let mut declarations = Vec::new();
        let mut set = |property| {
            declarations.push(Declaration {
                property,
                important: false,
            })
        };

        if (root || cell || tag == "col")
            && let Some(value) = node.attribute("width").and_then(|v| length(v, true))
            && (tag == "col" || value.pixels(100, 16.0) != Some(0.0))
        {
            set(Property::Width(value));
        }
        if (root || cell || group || row)
            && let Some(value) = node.attribute("height").and_then(|v| length(v, true))
            && (!cell || value.pixels(100, 16.0) != Some(0.0))
        {
            set(Property::Height(value));
        }
        if (root || cell || group || row)
            && let Some(gray) = own_color(node, "bgcolor")
        {
            set(Property::Background(Some(gray)));
        }
        if cell || group || row {
            let alignment = match word(node, "align").as_deref() {
                Some("left") => Some(Alignment::Left),
                Some("right") => Some(Alignment::Right),
                Some("center" | "middle" | "absmiddle") => Some(Alignment::Center),
                Some("justify") => Some(Alignment::Justify),
                _ => None,
            };
            if let Some(alignment) = alignment {
                set(Property::Alignment(alignment));
            }
            let vertical = match word(node, "valign").as_deref() {
                Some("top") => Some(VerticalAlign::TOP),
                Some("middle") => Some(VerticalAlign::MIDDLE),
                Some("bottom") => Some(VerticalAlign::BOTTOM),
                Some("baseline") => Some(VerticalAlign::BASELINE),
                _ => None,
            };
            if let Some(vertical) = vertical {
                set(Property::VerticalAlign(vertical));
            }
        }
        if root && word(node, "align").as_deref() == Some("center") {
            for side in 0..2 {
                set(Property::HorizontalMargin(side, Some(BoxMargin::Auto)));
            }
        }
        if tag == "caption"
            && node
                .attribute("align")
                .is_some_and(|value| value.eq_ignore_ascii_case("bottom"))
        {
            set(Property::CaptionBottom(true));
        }
        if root && let Some(space) = table.attribute("cellspacing").and_then(pixels) {
            set(Property::BorderSpacing(Some([space; 2])));
        }
        if cell && let Some(padding) = table.attribute("cellpadding").and_then(pixels) {
            for side in 0..4 {
                set(Property::Padding(side, Some(padding)));
            }
        }
        if cell && node.has_attribute("nowrap") {
            set(Property::NoWrap(true));
        }

        // Width, pattern, and color participate in the same per-edge cascade as CSS.
        let rules = word(table, "rules");
        let grid = rules
            .as_deref()
            .filter(|v| matches!(*v, "none" | "groups" | "rows" | "cols" | "all"));
        let outer = table
            .attribute("border")
            .map(|v| pixels(v).unwrap_or(CssLength::Pixels(1.)));
        let mut width = None;
        let mut patterns = None;
        if root {
            if grid.is_some() {
                set(Property::BorderCollapse(Some(true)));
                patterns = Some([Pattern::Hidden; 4]);
            }
            if let Some(size) = outer {
                width = Some(size);
                if size.pixels(100, 16.0) > 0.0 {
                    patterns = Some([Pattern::Solid; 4]);
                }
            }
            use Pattern::{Hidden, Solid};
            patterns = match word(table, "frame").as_deref() {
                Some("void") => Some([Hidden; 4]),
                Some("above") => Some([Solid, Hidden, Hidden, Hidden]),
                Some("below") => Some([Hidden, Hidden, Solid, Hidden]),
                Some("hsides") => Some([Solid, Hidden, Solid, Hidden]),
                Some("lhs") => Some([Hidden, Hidden, Hidden, Solid]),
                Some("rhs") => Some([Hidden, Solid, Hidden, Hidden]),
                Some("vsides") => Some([Hidden, Solid, Hidden, Solid]),
                Some("box" | "border") => Some([Solid; 4]),
                _ => patterns,
            };
        } else if cell {
            if outer.is_some_and(|edge| edge.pixels(100, 16.0) > 0.0) {
                width = Some(CssLength::Pixels(1.));
                patterns = Some([Pattern::Solid; 4]);
            }
            if let Some(grid) = grid {
                width = Some(CssLength::Pixels(1.));
                patterns = Some(match grid {
                    "all" => [Pattern::Solid; 4],
                    "cols" => [Pattern::None, Pattern::Solid, Pattern::None, Pattern::Solid],
                    _ => [Pattern::None; 4],
                });
            }
        } else if row && grid == Some("rows") || group && grid == Some("groups") {
            width = Some(CssLength::Pixels(1.));
            patterns = Some([Pattern::Solid, Pattern::None, Pattern::Solid, Pattern::None]);
        }
        let gray = patterns
            .as_ref()
            .filter(|patterns| patterns.contains(&Pattern::Solid))
            .and_then(|_| own_color(table, "bordercolor"));
        for side in 0..4 {
            if let Some(width) = width {
                set(Property::Border(side, Value::Width(Some(width))));
            }
            if let Some(patterns) = patterns {
                set(Property::Border(side, Value::Pattern(Some(patterns[side]))));
            }
            if let Some(gray) = gray {
                set(Property::Border(side, Value::Gray(Some(Some((gray, 255))))));
            }
        }

        (!declarations.is_empty()).then(|| Self(declarations.into_boxed_slice()))
    }

    pub(super) fn cascade<'a>(&'a self, winners: &mut Winners<'a>) {
        choose(
            winners,
            &self.0,
            false,
            (0, 1, 0),
            CascadeOrigin::UserAgent,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Settings, book_style::BookStyle, source::Source, table_layout::TableFlow};

    #[test]
    fn table_caption_side_resolves_html_hints_inheritance_and_publisher_resets() {
        let chapter = r#"<html><head><style>
            .start { caption-side: top; }
            .end { caption-side: bottom; }
            .reset { caption-side: initial; }
        </style></head><body>
            <table><caption id="default" style="caption-side:left">Default</caption>
                <tr><td>Cell</td></tr></table>
            <table><caption id="html" align="BoTtOm">HTML</caption>
                <tr><td>Cell</td></tr></table>
            <table><caption id="css" class="end">CSS</caption>
                <tr><td>Cell</td></tr></table>
            <table><caption id="priority" class="start" align="bottom">Priority</caption>
                <tr><td>Cell</td></tr></table>
            <section style="caption-side:bottom">
                <table><caption id="inherit" style="caption-side:inherit">Inherit</caption>
                    <tr><td>Cell</td></tr></table>
                <table><caption id="unset" style="caption-side:unset">Unset</caption>
                    <tr><td>Cell</td></tr></table>
                <table><caption id="initial" class="reset">Initial</caption>
                    <tr><td>Cell</td></tr></table>
                <table><caption id="unknown" style="caption-side:right">Unknown</caption>
                    <tr><td>Cell</td></tr></table>
            </section>
        </body></html>"#;
        let source = Source::parse(chapter).unwrap();
        let path = crate::test_fixture("field-notes-epub3.epub");
        let mut book = crate::Book::open(&path).unwrap();
        let href = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles = BookStyle::load(&mut book, &href, &source, &mut fonts);
        let original = styles.compute(&source.elements);
        let mut data = crate::user_styles::Data::default();
        data.choose(1, "table_no_presentational_hints", Some(true))
            .unwrap();
        let filtered = styles
            .with_user_css(&data.snapshot(1).unwrap())
            .unwrap()
            .compute(&source.elements);
        let expected = [
            ("default", false, false),
            ("html", true, false),
            ("css", true, true),
            ("priority", false, false),
            ("inherit", true, true),
            ("unset", true, true),
            ("initial", false, false),
            ("unknown", true, true),
        ];
        for (id, before, after) in expected {
            let index = source
                .elements
                .iter()
                .position(|element| element.id.as_deref() == Some(id))
                .unwrap();
            assert_eq!(original[index].caption_bottom, before, "{id}");
            assert_eq!(filtered[index].caption_bottom, after, "{id}");
        }
    }

    #[test]
    fn table_defaults_and_cell_alignments_follow_the_selected_book_stylesheet() {
        let text = r#"<html><body><p id="outside">Ordinary prose</p>
            <table id="stock"><tbody valign="bottom"><tr>
                <td id="inherited"><p id="child">Cell paragraph</p></td>
                <th id="override" style="font-size:125%;padding:0;vertical-align:top;text-align:right">Styled heading</th>
                <td id="another" style="vertical-align:inherit">Inherited alignment</td>
            </tr><tr style="vertical-align:top"><td id="row">Row override</td></tr></tbody></table>
            <table><tr><td id="middle">Default middle</td></tr></table></body></html>"#;
        let source = Source::parse(text).unwrap();
        let book_path = crate::test_fixture("field-notes-epub3.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut fonts = crate::font_context(&fonts).unwrap();
        let mut book = crate::Book::open(&book_path).unwrap();
        let chapter = book.chapters[0].href.clone();
        let book_mode = crate::PublisherRenderMode::Book.apply(Settings::default());
        let mut read = |settings: Settings, snapshot: &crate::user_styles::Snapshot| {
            BookStyle::load_with_base(&mut book, &chapter, &source, &mut fonts, settings)
                .with_user_css(snapshot)
                .unwrap()
                .compute_with_footnotes(&source.elements, settings, None)
        };
        let index = |id| {
            source
                .elements
                .iter()
                .position(|e| e.id.as_deref() == Some(id))
                .unwrap()
        };
        let def = crate::user_styles::Snapshot::default();
        let stock = read(book_mode, &def);
        assert_eq!(stock[index("outside")].size_scale, 1.);
        assert_eq!(stock[index("child")].size_scale, 0.8);
        assert_eq!(stock[index("override")].size_scale, 1.);
        assert!(stock[index("stock")].has_table_spacing);
        assert_eq!(
            stock[index("stock")].vertical_margin[0].pixels(100, 32.),
            3.
        );
        assert_eq!(stock[index("inherited")].padding[0].pixels(100, 32.), 3.);
        assert_eq!(
            stock[index("inherited")].vertical_padding[0].pixels(100, 32.),
            3.
        );
        assert_eq!(stock[index("override")].padding[0].pixels(100, 32.), 0.);
        assert_eq!(stock[index("override")].alignment, Some(Alignment::Right));
        for (id, vertical) in [
            ("inherited", parley::VerticalAlign::BOTTOM),
            ("another", parley::VerticalAlign::BOTTOM),
            ("override", parley::VerticalAlign::TOP),
            ("row", parley::VerticalAlign::TOP),
            ("middle", parley::VerticalAlign::MIDDLE),
        ] {
            assert_eq!(stock[index(id)].image_align, vertical, "{id}");
            assert!(stock[index(id)].table_valign_explicit, "{id}");
        }
        assert_eq!(
            stock[index("child")].image_align,
            parley::VerticalAlign::BASELINE
        );
        assert!(!stock[index("child")].table_valign_explicit);

        let mut data = crate::user_styles::Data::default();
        data.choose(1, "td_vertical_align_none", Some(true))
            .unwrap();
        let adjusted = read(book_mode, &data.snapshot(1).unwrap());
        for id in ["inherited", "another", "row", "middle"] {
            assert_eq!(
                adjusted[index(id)].image_align,
                parley::VerticalAlign::BASELINE
            );
        }
        assert_eq!(
            adjusted[index("override")].image_align,
            parley::VerticalAlign::TOP
        );

        let legacy = read(Settings::default(), &def);
        assert_eq!(legacy[index("child")].size_scale, 1.);
        assert!(!legacy[index("middle")].table_valign_explicit);
        let plain = read(
            Settings {
                base_stylesheet: crate::BaseStylesheet::None,
                ..book_mode
            },
            &def,
        );
        assert_eq!(plain[index("child")].size_scale, 1.);
        assert!(!plain[index("middle")].table_valign_explicit);
    }

    #[test]
    fn table_presentational_hints_are_separate_from_css_and_spans() {
        let path = crate::test_fixture("table-presentational-hints.xhtml");
        let markup = std::fs::read_to_string(&path).unwrap();
        let source = Source::parse(&markup).unwrap();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let mut book = crate::Book::open(&path).unwrap();
        let chapter = book.chapters[0].href.clone();
        let mut choices = crate::user_styles::Data::default();
        choices
            .choose(1, "table_no_presentational_hints", Some(true))
            .unwrap();
        let preset = choices.snapshot(1).unwrap();
        let settings = Settings::default();
        let original =
            BookStyle::load_with_base(&mut book, &chapter, &source, &mut fonts, settings)
                .compute_with_footnotes(&source.elements, settings, None);
        let filtered =
            BookStyle::load_with_base(&mut book, &chapter, &source, &mut fonts, settings)
                .with_user_css(&preset)
                .unwrap()
                .compute_with_footnotes(&source.elements, settings, None);
        let id = |key: &str| {
            source
                .elements
                .iter()
                .position(|e| e.id.as_deref() == Some(key))
                .unwrap()
        };
        let table = id("legacy");
        let dock = id("dock");
        let merged = id("merged");
        let css = id("css-cell");
        assert_eq!(
            original[table].image_width.unwrap().pixels(100, 32.),
            Some(84.)
        );
        assert!(filtered[table].image_width.is_none());
        assert_eq!(original[table].border_spacing[0].pixels(100, 32.), 10.);
        assert!(filtered[table].border_spacing[0].pixels(100, 32.) < 10.);
        assert!(original[table].borders.is_some());
        assert!(filtered[table].borders.is_none());
        assert_eq!(original[dock].alignment, Some(Alignment::Right));
        assert_ne!(filtered[dock].alignment, Some(Alignment::Right));
        assert_eq!(original[dock].padding[0].pixels(100, 32.), 12.);
        assert!(filtered[dock].padding[0].pixels(100, 32.) < 12.);
        assert_eq!(original[merged].padding, filtered[merged].padding);
        assert_eq!(original[merged].background, Some(230));
        assert_eq!(original[merged].background, filtered[merged].background);
        assert_eq!(original[css].background, filtered[css].background);
        let rows = TableFlow::new(&source, &original).unwrap();
        let clean_rows = TableFlow::new(&source, &filtered).unwrap();
        let rings = id("rings");
        assert_eq!(rows.cell(rings).unwrap().bounds(1000, 0), (550, 280));
        assert_eq!(clean_rows.cell(rings).unwrap().bounds(1000, 0), (360, 280));
        assert_eq!(rows.cell(merged).unwrap().span, 2);
        assert_eq!(clean_rows.cell(merged).unwrap().span, 2);
        assert_eq!(
            source
                .blocks
                .iter()
                .find(|block| block.element == rings)
                .unwrap()
                .text,
            "Mooring rings"
        );
    }
}
