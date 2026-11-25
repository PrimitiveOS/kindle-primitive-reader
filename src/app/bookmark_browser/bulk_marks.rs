//! Explicitly reviewed, current-book removal of filtered saved marks.
use super::{
    BookState, Bookmark, Highlight, Locator, Page, PdfHighlight, Reader, Result, ReviewLocation,
    Screen,
};
use std::collections::BTreeSet;

pub(super) enum Snapshot {
    Bookmark(usize, Bookmark),
    Reflow(usize, Highlight),
    Fixed(usize, PdfHighlight),
}

impl Snapshot {
    fn capture(book: &BookState, locator: Locator) -> Result<Self> {
        Ok(match locator {
            Locator::Bookmark(index) => {
                let mark = book
                    .bookmarks
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                if mark
                    .position
                    .anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.raw_caret.is_some())
                {
                    return Err("A matching mark uses an unsupported raw location".into());
                }
                Self::Bookmark(index, mark.clone())
            }
            Locator::Reflow(index) => {
                let mark = book
                    .highlights
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                if mark.selection.start.raw_caret.is_some()
                    || mark.selection.end.raw_caret.is_some()
                {
                    return Err("A matching mark uses an unsupported raw location".into());
                }
                Self::Reflow(index, mark.clone())
            }
            Locator::Fixed(index) => Self::Fixed(
                index,
                book.pdf_highlights
                    .get(index)
                    .ok_or("A matching mark is unavailable")?
                    .clone(),
            ),
        })
    }

    fn matches(&self, book: &BookState) -> bool {
        match self {
            Self::Bookmark(index, mark) => book.bookmarks.get(*index) == Some(mark),
            Self::Reflow(index, mark) => book.highlights.get(*index) == Some(mark),
            Self::Fixed(index, mark) => book.pdf_highlights.get(*index) == Some(mark),
        }
    }

    /// Only saved content is returned here; the caller localizes the kind label.
    pub(super) fn preview(
        &self,
    ) -> (
        &'static str,
        &str,
        Option<&str>,
        Option<&str>,
        ReviewLocation<'_>,
    ) {
        match self {
            Self::Bookmark(_, mark) => (
                "Page bookmark",
                &mark.label,
                mark.note.as_deref(),
                mark.chapter_title.as_deref(),
                ReviewLocation::Saved(&mark.position),
            ),
            Self::Reflow(_, mark) => (
                "Highlight",
                &mark.selection.text,
                mark.note.as_deref(),
                mark.chapter_title.as_deref(),
                ReviewLocation::Reflow(&mark.selection.start),
            ),
            Self::Fixed(_, mark) => (
                "Highlight",
                &mark.text,
                mark.note.as_deref(),
                mark.chapter_title.as_deref(),
                ReviewLocation::Fixed(mark.page),
            ),
        }
    }
}

pub(super) struct ReviewedMark {
    pub(super) target: Snapshot,
    pub(super) selected: bool,
}

pub(super) struct Pending {
    pub(super) book: u64,
    pub(super) title: String,
    pub(super) targets: Vec<ReviewedMark>,
}

impl Pending {
    pub(super) fn selected_count(&self) -> usize {
        self.targets.iter().filter(|item| item.selected).count()
    }
}

pub(super) fn preview(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::Filters)
        || !matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
    {
        return Err("Open a book before removing its marks".into());
    }
    let id = reader.open_book_id().ok_or("Open a book first")?;
    let book = reader.library.state(id).ok_or("That book is unavailable")?;
    let mut targets = Vec::new();
    let mut seen = BTreeSet::new();
    for key in reader
        .bookmark_browser
        .marks
        .iter()
        .filter(|key| key.book == id)
    {
        let identity = match key.locator {
            Locator::Bookmark(index) => (0, index),
            Locator::Reflow(index) => (1, index),
            Locator::Fixed(index) => (2, index),
        };
        if !seen.insert(identity) {
            continue;
        }
        if targets.len() == 512 {
            return Err("Too many matching marks; narrow the filters first".into());
        }
        targets.push(ReviewedMark {
            target: Snapshot::capture(book, key.locator)?,
            selected: false,
        });
    }
    if targets.is_empty() {
        return Err("No matching marks in the current book".into());
    }
    reader.bookmark_browser.pending_mark_removal = Some(Pending {
        book: id,
        title: book.title.clone(),
        targets,
    });
    reader.notice = None;
    reader.show(Screen::BookmarkBrowser(Page::RemoveMarksConfirm));
    Ok(())
}

pub(super) fn confirm(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::RemoveMarksConfirm) {
        return Err("Review the marks before removing them".into());
    }
    let pending = reader
        .bookmark_browser
        .pending_mark_removal
        .as_ref()
        .ok_or("Review the marks before removing them")?;
    if reader.open_book_id() != Some(pending.book)
        || !matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
    {
        return Err("The open book changed; review the marks again".into());
    }
    let id = pending.book;
    let state = reader.library.state(id).ok_or("That book is unavailable")?;
    if state.title != pending.title
        || pending.selected_count() == 0
        || !pending
            .targets
            .iter()
            .filter(|item| item.selected)
            .all(|item| item.target.matches(state))
    {
        return Err("A selected mark changed; review the marks again".into());
    }
    let mut bookmarks = Vec::new();
    let mut highlights = Vec::new();
    let mut fixed = Vec::new();
    for item in pending.targets.iter().filter(|item| item.selected) {
        match &item.target {
            Snapshot::Bookmark(index, _) => bookmarks.push(*index),
            Snapshot::Reflow(index, _) => highlights.push(*index),
            Snapshot::Fixed(index, _) => fixed.push(*index),
        }
    }
    for indexes in [&mut bookmarks, &mut highlights, &mut fixed] {
        indexes.sort_unstable_by(|left, right| right.cmp(left));
        indexes.dedup();
    }
    let before = state.clone();
    let state = reader
        .library
        .state_mut(id)
        .ok_or("That book is unavailable")?;
    for index in bookmarks {
        state.bookmarks.remove(index);
    }
    for index in highlights {
        state.highlights.remove(index);
    }
    for index in fixed {
        state.pdf_highlights.remove(index);
    }
    if let Err(error) = reader.library.persist() {
        if let Some(state) = reader.library.state_mut(id) {
            *state = before;
        }
        return Err(error.into());
    }
    reader.bookmark_browser.pending_mark_removal = None;
    reader.mark_book_changed();
    reader.refresh_bookmark_browser();
    reader.show(Screen::BookmarkBrowser(Page::Results));
    reader.notice = Some(
        reader
            .controls
            .interface_language
            .bookmark_text("Marks removed")
            .into(),
    );
    Ok(())
}
