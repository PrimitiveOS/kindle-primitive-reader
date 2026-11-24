//! Check the book's declared font resources independently of the active chapter and switch.
use super::{
    Book, BookStyle, FontContext, FontFace, Parser, SheetItem, SheetParser, StyleSheetParser,
};
use super::{MAX_CSS_BYTES, MAX_FACE_RULES, MAX_RULES};
use crate::source::{Source, Stylesheet};
use std::{collections::HashSet, sync::Arc};

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

    fn register_faces(&self, book: &mut Book) -> BookStyle {
        let mut style = BookStyle::default();
        if !self.faces.is_empty() {
            style.register_fonts(book, &self.faces, &mut FontContext::default());
        }
        style
    }

    fn contains_valid_font(&self, book: &mut Book) -> bool {
        !self.register_faces(book).registered_faces().is_empty()
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

pub(super) fn chapter_has_fonts(book: &mut Book, chapter: usize) -> bool {
    let mut inventory = Inventory::default();
    book.may_have_embedded_fonts()
        && read_chapter(&mut inventory, book, chapter, MAX_CHAPTER_BYTES) > 0
        && inventory.contains_valid_font(book)
}

pub(super) fn has_fonts(book: &mut Book, active: usize) -> bool {
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
    read_chapters(&mut inventory, book, active);
    inventory.faces.len() != known_faces && inventory.contains_valid_font(book)
}

pub(super) fn catalog(book: &mut Book, active: usize) -> Vec<Arc<str>> {
    if !book.may_have_embedded_fonts() {
        return Vec::new();
    }
    let mut inventory = Inventory::default();
    for path in book.publisher_stylesheets().to_vec() {
        inventory.read_sheet(book, path);
    }
    read_chapters(&mut inventory, book, active);
    inventory
        .register_faces(book)
        .registered_faces
        .into_values()
        .collect()
}

fn read_chapters(inventory: &mut Inventory, book: &mut Book, active: usize) {
    let mut chapter_bytes = 0;
    let first_chapters = 0..book.chapters.len().min(MAX_INVENTORY_CHAPTERS);
    for index in std::iter::once(active).chain(first_chapters.filter(|&index| index != active)) {
        if inventory.full() || chapter_bytes >= MAX_CHAPTER_BYTES {
            break;
        }
        chapter_bytes += read_chapter(inventory, book, index, MAX_CHAPTER_BYTES - chapter_bytes);
    }
}

fn read_chapter(
    inventory: &mut Inventory,
    book: &mut Book,
    index: usize,
    remaining: usize,
) -> usize {
    let Ok(xhtml) = book.chapter(index) else {
        return 0;
    };
    let len = xhtml.len();
    if len > remaining {
        return len;
    }
    let href = book.chapters[index].href.clone();
    let Ok(source) = Source::parse_with_language(&xhtml, book.language) else {
        return len;
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
    len
}
