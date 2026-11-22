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
    pub cell: Option<Cell>,
}

impl BoxArea {
    fn new(x: u32, width: u32, cell: Option<Cell>) -> Self {
        Self {
            x,
            width,
            outer_x: x,
            outer_width: width,
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
        let indent = style.text_indent.pixels(self.outer_width, font);
        if indent >= 0.0 || self.width == self.outer_width {
            return (self.x, self.width);
        }
        let rtl = match block.direction {
            parley::BaseDirection::Ltr => false,
            parley::BaseDirection::Rtl => true,
            parley::BaseDirection::Auto => first_strong_rtl(&block.text),
        };
        let before = self.x - self.outer_x;
        let after = self.outer_width - self.width - before;
        let outdent = ((-indent).round() as u32).min(if rtl { after } else { before });
        (
            if rtl { self.x } else { self.x - outdent },
            self.width + outdent,
        )
    }

    fn pad(&mut self, sides: &[CssLength; 2], font: f32) {
        let left = sides[0].pixels(self.width, font).round();
        let right = sides[1].pixels(self.width, font).round();
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
) -> Option<Vec<BoxArea>> {
    if !styles.iter().any(|style| {
        style
            .padding
            .iter()
            .any(|edge| edge.pixels(width, font) > 0.0)
    }) {
        return None;
    }
    // The source attaches its row/column metadata to text blocks. Identify
    // each cell's element once so child paragraphs share the correct box.
    let mut owners = Vec::with_capacity(source.elements.len());
    for (idx, element) in source.elements.iter().enumerate() {
        let owner = if matches!(element.tag.as_str(), "td" | "th") {
            Some(idx)
        } else {
            element.parent.and_then(|p| owners[p])
        };
        owners.push(owner);
    }
    let mut cells = vec![None; owners.len()];
    for block in &source.blocks {
        if let (Some(owner), Some(cell)) = (owners[block.element], block.table_cell) {
            cells[owner] = Some(cell);
        }
    }
    let page = BoxArea::new(0, width, None);
    let mut boxes: Vec<BoxArea> = Vec::with_capacity(styles.len());
    for (idx, element) in source.elements.iter().enumerate() {
        let parent = element.parent.map_or(page, |i| boxes[i]);
        let mut area = BoxArea::new(parent.x, parent.width, parent.cell);
        if let Some(cell) = cells[idx]
            && area.width as f32 / f32::from(cell.columns) >= 2.5 * font
        {
            let (start, width) = cell.bounds(area.width, (font * 0.22).round() as u32);
            area = BoxArea::new(area.x + start, width, Some(cell));
        }
        if !element.selector_only
            && (is_block_tag(&element.tag)
                || matches!(element.tag.as_str(), "html" | "dl" | "ul" | "ol" | "table"))
        {
            area.pad(&styles[idx].padding, font);
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
        let boxes = layout(&source, &styles, 1000, 40.).unwrap();
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
        let ten_percent = (cell_width as f32 * 0.1).round() as u32;
        let nested = ((cell_width - ten_percent) as f32 * 0.1).round() as u32;
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
