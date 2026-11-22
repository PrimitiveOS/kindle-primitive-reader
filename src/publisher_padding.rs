//! Horizontal publisher block boxes, measured against their parent content area.
use crate::{
    book_style::{CssLength, Style},
    source::{Source, is_block_tag},
    table_layout::Cell,
};

#[derive(Clone, Copy)]
pub(crate) struct BoxArea {
    pub x: u32,
    pub width: u32,
    // The background of a paragraph also paints its own padding.
    pub outer_x: u32,
    pub outer_width: u32,
    border: [u32; 2],
    pub cell: Option<Cell>,
}

impl BoxArea {
    pub(crate) fn new(x: u32, width: u32, cell: Option<Cell>) -> Self {
        Self {
            x,
            width,
            outer_x: x,
            outer_width: width,
            border: [0, 0],
            cell,
        }
    }

    /// CREngine lets a negative indent use the declaring box's leading padding.
    /// Clipping the outdent to that padding leaves nearby cells and the page edge intact.
    pub fn paragraph_bounds(
        self,
        block: &crate::source::Block,
        style: &Style,
        font: f32,
    ) -> (u32, u32) {
        self.paragraph_bounds_for_text(block.direction, &block.text, style, font)
    }

    /// Compute one generated native empty-box level without changing the
    /// ordinary/nonempty publisher box plan. Reserve strokes before padding,
    /// so descendants and percentage padding see the remaining content width.
    pub(crate) fn generated_child(parent: Self, style: &Style, tag: &str, font: f32) -> Self {
        let mut area = Self::new(parent.x, parent.width, None);
        if is_block_tag(tag) {
            area.add_border(crate::table_borders::generated_box_edges(style, font));
            area.pad(&style.padding, font);
        }
        area
    }

    pub(crate) fn generated_root(self, style: &Style, tag: &str, font: f32) -> Self {
        Self::generated_child(
            Self::new(self.outer_x, self.outer_width, None),
            style,
            tag,
            font,
        )
    }

    /// The auto direction must use the same text that is shaped. Removed inline
    /// content must not choose the side from which an outdent borrows padding.
    pub(crate) fn paragraph_bounds_for_text(
        self,
        direction: parley::BaseDirection,
        text: &str,
        style: &Style,
        font: f32,
    ) -> (u32, u32) {
        let indent = style.text_indent.box_pixels(self.outer_width, font);
        if indent >= 0.0 || self.width == self.outer_width {
            return (self.x, self.width);
        }
        let rtl = match direction {
            parley::BaseDirection::Ltr => false,
            parley::BaseDirection::Rtl => true,
            parley::BaseDirection::Auto => first_strong_rtl(text),
        };
        let inset = self.x - self.outer_x;
        let before = inset.saturating_sub(self.border[0]);
        let after = (self.outer_width - self.width - inset).saturating_sub(self.border[1]);
        let outdent = ((-indent).round() as u32).min(if rtl { after } else { before });
        (
            if rtl { self.x } else { self.x - outdent },
            self.width + outdent,
        )
    }

    fn add_border(&mut self, strokes: [crate::table_borders::Stroke; 4]) {
        let left = strokes[3].width.min(self.width.saturating_sub(1));
        let right = strokes[1].width.min(self.width - left - 1);
        self.x += left;
        self.width -= left + right;
        self.border = [left, right];
    }
    fn pad(&mut self, sides: &[CssLength; 2], font: f32) {
        let left = sides[0].box_pixels(self.width, font);
        let right = sides[1].box_pixels(self.width, font);
        let sum = left + right;
        if sum <= 0. {
            return;
        }
        // Extreme or deeply nested padding must leave a usable column. Scale
        // both sides together when the publisher requests more than fits.
        let available = self.width.saturating_sub((font.ceil() as u32).max(1)) as f32;
        let remaining = sum.min(available) as u32;
        let inset = if sum <= available {
            left as u32
        } else {
            (remaining as f64 * f64::from(left) / f64::from(sum)).round() as u32
        };
        self.x += inset.min(remaining);
        self.width -= remaining;
    }
}

// Match the paragraph direction used for shaping when the document specifies auto.
// Unicode bidi isolates do not contribute to the enclosing paragraph's first strong letter.
fn first_strong_rtl(text: &str) -> bool {
    use icu_properties::{CodePointMapData, props::BidiClass};
    let classes = CodePointMapData::<BidiClass>::new();
    let mut isolates = 0_usize;
    for ch in text.chars() {
        match classes.get(ch) {
            BidiClass::LeftToRightIsolate
            | BidiClass::RightToLeftIsolate
            | BidiClass::FirstStrongIsolate => isolates += 1,
            BidiClass::PopDirectionalIsolate => isolates = isolates.saturating_sub(1),
            BidiClass::LeftToRight if isolates == 0 => return false,
            BidiClass::RightToLeft | BidiClass::ArabicLetter if isolates == 0 => return true,
            _ => {}
        }
    }
    false
}

pub(crate) fn layout(
    source: &Source,
    styles: &[Style],
    width: u32,
    font: f32,
    borders: Option<&crate::table_borders::Borders>,
    measure: impl FnMut(&crate::source::Block) -> (u32, u32),
) -> Option<Vec<BoxArea>> {
    layout_with_flow(
        source,
        styles,
        width,
        font,
        borders,
        &crate::table_layout::TableFlow::identity(source),
        measure,
    )
}

pub(crate) fn layout_with_flow(
    source: &Source,
    styles: &[Style],
    width: u32,
    font: f32,
    borders: Option<&crate::table_borders::Borders>,
    flow: &crate::table_layout::TableFlow<'_>,
    measure: impl FnMut(&crate::source::Block) -> (u32, u32),
) -> Option<Vec<BoxArea>> {
    if flow.is_identity()
        && borders.is_none()
        && !styles.iter().zip(&source.elements).any(|(style, element)| {
            !style.flow.suppressed
                && ((element.tag == "table" && style.border_collapse)
                    || (matches!(element.tag.as_str(), "th" | "td") && style.image_width.is_some())
                    || (style.background.is_some()
                        && style
                            .vertical_padding
                            .iter()
                            .any(|edge| edge.pixels(width, font) > 0.))
                    || style
                        .padding
                        .iter()
                        .any(|edge| edge.pixels(width, font) > 0.0))
        })
    {
        return None;
    }
    // The source attaches its row/column metadata to text blocks. Identify
    // each cell's element once so child paragraphs share the correct box.
    let mut owners = Vec::with_capacity(source.elements.len());
    let mut tables = Vec::with_capacity(source.elements.len());
    for (idx, element) in source.elements.iter().enumerate() {
        let owner = if matches!(element.tag.as_str(), "td" | "th") {
            Some(idx)
        } else {
            element.parent.and_then(|p| owners[p])
        };
        owners.push(owner);
        tables.push(if element.tag == "table" {
            Some(idx)
        } else {
            element.parent.and_then(|p| tables[p])
        });
    }
    let mut cells = vec![None; owners.len()];
    for &(element, cell) in flow.cells() {
        cells[element] = Some(cell);
    }
    let mut widths = crate::table_widths::TableWidths::collect(
        source, styles, &tables, &owners, &cells, flow, font, width, borders, measure,
    );
    let page = BoxArea::new(0, width, None);
    let mut boxes: Vec<BoxArea> = Vec::with_capacity(styles.len());
    for (idx, element) in source.elements.iter().enumerate() {
        let table = tables[idx];
        let parent = element.parent.map_or(page, |i| boxes[i]);
        if styles[idx].flow.suppressed {
            boxes.push(BoxArea::new(parent.x, parent.width, None));
            continue;
        }
        let mut area = BoxArea::new(parent.x, parent.width, parent.cell);
        if let Some(cell) = cells[idx]
            && area.width as f32 / f32::from(cell.columns) >= 2.5 * font
        {
            let decorated = borders.filter(|b| b.cells.contains_key(&cell));
            let (offset, available) =
                decorated.map_or((0, area.width), |b| b.horizontal(cell, area.width));
            let inset = if decorated.is_some() || table.is_some_and(|id| styles[id].border_collapse)
            {
                0
            } else {
                (font * 0.22).round() as u32
            };
            let (start, width) = table
                .and_then(|id| widths.get(&id))
                .and_then(|s| s.bounds(cell, available, inset))
                .unwrap_or_else(|| cell.bounds(available, inset));
            area = BoxArea::new(area.x + offset + start, width, Some(cell));
            if let Some(b) = decorated {
                area.add_border(b.cells[&cell].edges);
            }
        }
        if let Some(edges) = borders.and_then(|b| b.outer_edges(idx)) {
            area.add_border(edges);
        }
        if !element.selector_only
            && !(element.tag == "table" && styles[idx].border_collapse)
            && (is_block_tag(&element.tag)
                || matches!(element.tag.as_str(), "html" | "dl" | "ul" | "ol" | "table"))
        {
            area.pad(&styles[idx].padding, font);
        }
        if let Some(tracks) = widths.get_mut(&idx)
            && area.width >= 32
        {
            let (space, n) = borders.map_or((0, 0), |b| b.spacing_for_table(idx));
            let available = area.width
                - space.min(area.width.saturating_sub(n.saturating_sub(1)) / n.max(1)) * n;
            tracks.resolve(available);
        }
        boxes.push(area);
    }
    Some(boxes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Book, book_style::BookStyle};

    #[test]
    fn projected_auto_direction_chooses_the_remaining_leading_padding() {
        let source =
            Source::parse("<html><body><p style='text-indent:-100px'>Latin אבג</p></body></html>")
                .unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles = BookStyle::load_for_flow(&mut book, &href, &source, true, &Default::default())
            .unwrap()
            .compute(&source.elements);
        let style = &styles[source.blocks[0].element];
        let area = BoxArea {
            x: 20,
            width: 150,
            outer_x: 0,
            outer_width: 200,
            border: [0, 0],
            cell: None,
        };
        assert_eq!(
            area.paragraph_bounds_for_text(parley::BaseDirection::Auto, "Latin אבג", style, 40.),
            (0, 170)
        );
        assert_eq!(
            area.paragraph_bounds_for_text(parley::BaseDirection::Auto, "אבג", style, 40.),
            (20, 180)
        );
        assert_eq!(
            area.paragraph_bounds_for_text(parley::BaseDirection::Ltr, "אבג", style, 40.),
            (0, 170)
        );
    }

    #[test]
    fn hidden_cell_cannot_contribute_measurement_or_padding_and_plain_grids_still_repack() {
        let source=Source::parse("<html><body><table><tr><td style='display:none;padding:0 200px'>Hidden huge width</td><td id='kept' style='width:100px'>Kept</td></tr></table></body></html>").unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let styles = BookStyle::load_for_flow(&mut book, &href, &source, true, &Default::default())
            .unwrap()
            .compute(&source.elements);
        let flow = crate::table_layout::TableFlow::new(&source, &styles).unwrap();
        let mut measured = 0;
        let boxes = layout_with_flow(&source, &styles, 600, 40., None, &flow, |block| {
            assert!(!styles[block.element].flow.suppressed);
            measured += 1;
            (40, 40)
        })
        .unwrap();
        assert_eq!(measured, 1);
        let kept = source
            .elements
            .iter()
            .position(|element| element.id.as_deref() == Some("kept"))
            .unwrap();
        assert_eq!(boxes[kept].cell.unwrap().columns, 1);
        // The unchanged separate-cell gutter is retained; removed padding is not.
        assert_eq!((boxes[kept].x, boxes[kept].width), (9, 582));
        assert!(
            source
                .blocks
                .iter()
                .filter(|b| styles[b.element].flow.suppressed)
                .all(|b| boxes[b.element].cell.is_none())
        );
    }

    #[test]
    fn nested_boxes_resolve_percentage_edges_inside_their_table_cells() {
        let text = r#"<html><body style="padding:0 10%"><table><tr>
        <td style="padding-left:10%"><p id="first" style="padding:0 10%">Inside first</p></td>
        <td><p id="second" dir="rtl" style="padding-right:30px">Second</p></td>
        </tr></table><aside style="padding-left:20%"><p id="child" style="padding:0 1000%">Tight</p></aside>
        <p id="sibling">Outside the aside</p></body></html>"#;
        let source = Source::parse(text).unwrap();
        let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
        let href = book.chapters[0].href.clone();
        let mut fonts =
            crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
        let styles =
            BookStyle::load(&mut book, &href, &source, &mut fonts).compute(&source.elements);
        let boxes = layout(&source, &styles, 1000, 40., None, |_| (0, 0)).unwrap();
        let r = |id| {
            boxes[source
                .elements
                .iter()
                .position(|el| el.id.as_deref() == Some(id))
                .unwrap()]
        };
        let first = r("first");
        let second = r("second");
        let tight = r("child");
        let sibling = r("sibling");
        let (first_x, cell_width) = first.cell.unwrap().bounds(800, 9);
        let (second_x, second_width) = second.cell.unwrap().bounds(800, 9);
        let ten_percent = (cell_width as f32 * 0.1).floor() as u32;
        let nested = ((cell_width - ten_percent) as f32 * 0.1).floor() as u32;
        assert_eq!(
            (first.x, first.width),
            (
                100 + first_x + ten_percent + nested,
                cell_width - ten_percent - 2 * nested
            )
        );
        assert_eq!(
            (second.x, second.width),
            (100 + second_x, second_width - 30)
        );
        assert_eq!(tight.width, 40);
        assert_eq!((sibling.x, sibling.width), (100, 800));
    }
}
