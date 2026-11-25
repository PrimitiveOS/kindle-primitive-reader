//! Per-book title and author edits used by the optional status header.
use super::super::{ActiveBook, Editor, Item, LIST_Y, Reader, Result, Screen};
use super::{Action, Rect};
use crate::library::{BOOK_IDENTITY_FIELD_LIMIT, BookIdentity, book_identity_field};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum IdentityField {
    Title,
    Author,
}

impl IdentityField {
    pub(in crate::app) fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Author => "Author",
        }
    }

    pub(in crate::app) fn editor_title(self) -> &'static str {
        match self {
            Self::Title => "Edit book title",
            Self::Author => "Edit book author",
        }
    }

    fn value(self, identity: &BookIdentity) -> Option<&str> {
        match self {
            Self::Title => identity.title.as_deref(),
            Self::Author => identity.author.as_deref(),
        }
    }
}

impl Reader {
    pub(in crate::app) fn editable_book_identity(&self) -> Option<&ActiveBook> {
        let details = self.book_details.as_ref()?;
        let book = self.active.as_ref()?;
        (book.id == details.id && !details.close_document).then_some(book)
    }

    pub(in crate::app) fn reading_identity<'a>(
        &'a self,
        book: &'a ActiveBook,
    ) -> (Option<&'a str>, Option<&'a str>) {
        let overrides = self
            .library
            .state(book.id)
            .map(|book| &book.reading_identity);
        let imported = book.header_metadata.as_ref();
        let title = overrides
            .and_then(|entry| entry.title.as_deref())
            .or_else(|| imported.and_then(|entry| entry.title.as_deref()));
        let author = overrides
            .and_then(|entry| entry.author.as_deref())
            .or_else(|| imported.and_then(|entry| entry.author.as_deref()));
        (title, author)
    }

    fn displayed_book_identity(&self, field: IdentityField) -> Option<&str> {
        let book = self.editable_book_identity()?;
        let (title, author) = self.reading_identity(book);
        match field {
            IdentityField::Title => title.or(Some(&book.reading.probe.title)),
            IdentityField::Author => author.or(book.reading.probe.author.as_deref()),
        }
    }

    pub(in crate::app) fn reload_active_header_metadata(&mut self, id: u64) {
        if let Some(book) = self.active.as_ref().filter(|book| book.id == id) {
            let metadata = self.load_header_metadata(book.entry.library_path());
            self.active.as_mut().unwrap().header_metadata = Some(metadata);
        }
    }

    pub(in crate::app) fn book_identity_items(&self) -> [Item; 2] {
        [IdentityField::Title, IdentityField::Author].map(|field| {
            Item::new(field.label(), Action::Show(Screen::BookIdentityEdit(field)))
                .detail(self.displayed_book_identity(field).unwrap_or("Unknown"))
        })
    }

    pub(in crate::app) fn prepare_book_identity_edit(&mut self, field: IdentityField) {
        let mut editor = Editor::new(BOOK_IDENTITY_FIELD_LIMIT);
        if let Some(value) = self
            .displayed_book_identity(field)
            .and_then(book_identity_field)
        {
            editor.text = value.chars().collect();
            editor.caret = editor.text.len();
        }
        self.editor = editor;
    }

    pub(in crate::app) fn book_identity_reset(&self, field: IdentityField) -> Option<Rect> {
        let book = self.editable_book_identity()?;
        field.value(&self.library.state(book.id)?.reading_identity)?;
        Some(self.navigation_layout().body(LIST_Y + 158, 90))
    }

    pub(in crate::app) fn save_book_identity(
        &mut self,
        field: IdentityField,
        input: &str,
    ) -> Result<()> {
        let id = self.editable_book_identity().ok_or("Open a book first")?.id;
        let next = if input.trim().is_empty() {
            None
        } else {
            Some(
                book_identity_field(input)
                    .ok_or("Enter a single line of up to 160 characters")?
                    .to_owned(),
            )
        };
        let state = self
            .library
            .state_mut(id)
            .ok_or("Reading state unavailable")?;
        let value = match field {
            IdentityField::Title => &mut state.reading_identity.title,
            IdentityField::Author => &mut state.reading_identity.author,
        };
        if *value != next {
            *value = next;
            self.mark_dirty();
        }
        self.show(Screen::BookDetails);
        Ok(())
    }
}
