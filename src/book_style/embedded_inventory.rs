//! Check the book's declared font resources independently of the active chapter and switch.
use super::{
    Book, BookStyle, FontContext, FontFace, Parser, SheetItem, SheetParser, StyleSheetParser,
};
use super::{MAX_CSS_BYTES, MAX_FACE_RULES, MAX_RULES};
use crate::source::{Source, Stylesheet};
use std::collections::HashSet;

pub(crate) const MAX_INVENTORY_SHEETS: usize = 64;
const MAX_INVENTORY_CHAPTERS: usize = 96;
const MAX_CHAPTER_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct Inventory {
    checked_sheets: HashSet<String>,
    faces: Vec<FontFace>,
    css_bytes: usize,
}

impl Inventory {
    fn full(&self) -> bool {
        self.faces.len() >= MAX_FACE_RULES || self.css_bytes >= MAX_CSS_BYTES
    }

    fn read_sheet(&mut self, book: &mut Book, path: String) {
        if self.full()
            || self.checked_sheets.len() >= MAX_INVENTORY_SHEETS
            || !self.checked_sheets.insert(path.clone())
        {
            return;
        }
        let remaining = MAX_CSS_BYTES - self.css_bytes;
        if let Ok(bytes) = book.resource_limited(&path, remaining)
            && let Ok(text) = std::str::from_utf8(&bytes)
        {
            self.read_styles(&path, text);
        }
    }

    fn contains_valid_font(&self, book: &mut Book) -> bool {
        if self.faces.is_empty() {
            return false;
        }
        let mut style = BookStyle::default();
        let mut scratch = FontContext::default();
        style.register_fonts(book, &self.faces, &mut scratch);
        style.has_registered_fonts()
    }

    fn read_styles(&mut self, base: &str, text: &str) {
        if self.full() || text.len() > MAX_CSS_BYTES - self.css_bytes {
            return;
        }
        self.css_bytes += text.len();
        let mut input = Parser::new(text);
        for item in StyleSheetParser::new(&mut input, &mut SheetParser).take(MAX_RULES) {
            if let Ok(SheetItem::Font(declarations)) = item
                && let Some(face) = FontFace::from_rule(base, declarations)
            {
                self.faces.push(face);
                if self.faces.len() == MAX_FACE_RULES {
                    break;
                }
            }
        }
    }
}

pub(super) fn has_fonts(book: &mut Book) -> bool {
    if !book.may_have_embedded_fonts() {
        return false;
    }
    let mut inventory = Inventory::default();
    for path in book.publisher_stylesheets().to_vec() {
        inventory.read_sheet(book, path);
    }
    if inventory.contains_valid_font(book) {
        return true;
    }
    let known_faces = inventory.faces.len();
    let mut chapter_bytes = 0;
    for index in 0..book.chapters.len().min(MAX_INVENTORY_CHAPTERS) {
        if inventory.full() || chapter_bytes >= MAX_CHAPTER_BYTES {
            break;
        }
        let Ok(xhtml) = book.chapter(index) else {
            continue;
        };
        chapter_bytes += xhtml.len();
        if chapter_bytes > MAX_CHAPTER_BYTES {
            break;
        }
        let href = book.chapters[index].href.clone();
        let Ok(source) = Source::parse_with_language(&xhtml, book.language) else {
            continue;
        };
        for sheet in source.stylesheets {
            match sheet {
                Stylesheet::Linked(link) => {
                    if let Ok(path) = Book::resolve(&href, &link) {
                        inventory.read_sheet(book, path);
                    }
                }
                Stylesheet::Inline(text) => inventory.read_styles(&href, &text),
            }
        }
    }
    inventory.faces.len() != known_faces && inventory.contains_valid_font(book)
}
