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
        if root && let Some(space) = table.attribute("cellspacing").and_then(pixels) {
            set(Property::BorderSpacing(Some([space; 2])));
        }
        if cell && let Some(padding) = table.attribute("cellpadding").and_then(pixels) {
            for side in 0..4 {
                set(Property::Padding(side, Some(padding)));
            }
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
