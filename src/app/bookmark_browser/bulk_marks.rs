//! Explicitly reviewed, current-book removal of filtered saved marks.
use super::{
    BookState, Bookmark, Highlight, Locator, MarkKey, Page, PdfHighlight, Reader, Result,
    ReviewLocation, Screen,
};
use std::collections::BTreeSet;

#[derive(Clone, PartialEq)]
pub(super) enum Snapshot {
    Bookmark(usize, Bookmark),
    Reflow(usize, Highlight),
    Fixed(usize, PdfHighlight),
}

impl Snapshot {
    pub(super) fn capture(book: &BookState, locator: Locator) -> Result<Self> {
        Ok(match locator {
            Locator::Bookmark(index) => {
                let mark = book
                    .bookmarks
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
                Self::Bookmark(index, mark.clone())
            }
            Locator::Reflow(index) => {
                let mark = book
                    .highlights
                    .get(index)
                    .ok_or("A matching mark is unavailable")?;
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

    fn has_raw(&self) -> bool {
        match self {
            Self::Bookmark(_, mark) => mark
                .position
                .anchor
                .as_ref()
                .is_some_and(|a| a.raw_caret.is_some()),
            Self::Reflow(_, mark) => {
                mark.selection.start.raw_caret.is_some() || mark.selection.end.raw_caret.is_some()
            }
            Self::Fixed(_, _) => false,
        }
    }

    pub(super) fn matches(&self, book: &BookState) -> bool {
        match self {
            Self::Bookmark(index, mark) => book.bookmarks.get(*index) == Some(mark),
            Self::Reflow(index, mark) => book.highlights.get(*index) == Some(mark),
            Self::Fixed(index, mark) => book.pdf_highlights.get(*index) == Some(mark),
        }
    }

    pub(super) fn note(&self) -> Option<&str> {
        match self {
            Self::Bookmark(_, mark) => mark.note.as_deref(),
            Self::Reflow(_, mark) => mark.note.as_deref(),
            Self::Fixed(_, mark) => mark.note.as_deref(),
        }
    }

    pub(super) fn set_note(&self, book: &mut BookState, note: Option<String>, now: Option<i64>) {
        match self {
            Self::Bookmark(index, _) => {
                book.bookmarks[*index].note = note;
                book.bookmarks[*index].updated_at = now;
            }
            Self::Reflow(index, _) => {
                book.highlights[*index].note = note;
                book.highlights[*index].updated_at = now;
            }
            Self::Fixed(index, _) => {
                book.pdf_highlights[*index].note = note;
                book.pdf_highlights[*index].updated_at = now;
            }
        }
    }

    /// Verify against the open fixed document or a freshly opened reflow
    /// chapter before an editor is exposed or a reviewed change is saved.
    pub(super) fn verify_source(&self, reader: &Reader) -> Result<()> {
        if let Some(fixed) = &reader.fixed {
            match self {
                Self::Bookmark(_, mark)
                    if mark.position.chapter == 0
                        && mark.position.anchor.is_none()
                        && mark.position.page < fixed.reading.count() =>
                {
                    return Ok(());
                }
                Self::Fixed(_, mark)
                    if mark.is_valid()
                        && mark.page < fixed.reading.count()
                        && mark
                            .continuation
                            .iter()
                            .all(|part| part.page < fixed.reading.count()) =>
                {
                    return Ok(());
                }
                _ => return Err("This mark is unavailable in the open document".into()),
            }
        }
        let active = reader.active.as_ref().ok_or("Open a book first")?;
        match self {
            Self::Bookmark(_, mark) => {
                let (chapter, anchor) = match &mark.position.anchor {
                    Some(anchor) if anchor.book == active.reading.probe.book => (
                        active
                            .reading
                            .probe
                            .chapter_index(&anchor.chapter)
                            .ok_or("That chapter is unavailable")?,
                        Some(anchor),
                    ),
                    Some(_) => return Err("This mark is unavailable in the open document".into()),
                    None => (mark.position.chapter, None),
                };
                if chapter >= active.reading.probe.chapter_count() {
                    return Err("That chapter is unavailable".into());
                }
                let probe = active.reading.config.chapter(chapter)?;
                if anchor.map_or(mark.position.page < probe.page_count(), |anchor| {
                    probe.page_for(anchor).is_some()
                }) {
                    Ok(())
                } else {
                    Err("This mark is unavailable in the open document".into())
                }
            }
            Self::Reflow(_, mark) => {
                let start = &mark.selection.start;
                let end = &mark.selection.end;
                if start.book != active.reading.probe.book || end.book != active.reading.probe.book
                {
                    return Err("This mark is unavailable in the open document".into());
                }
                if start.raw_caret.is_some() || end.raw_caret.is_some() {
                    let verified = active.reading.anchored_quote(start, end)?;
                    return if verified == mark.selection {
                        Ok(())
                    } else {
                        Err("This saved passage no longer matches the open document".into())
                    };
                }
                if start.chapter != end.chapter {
                    return Err("This mark is unavailable in the open document".into());
                }
                let chapter = active
                    .reading
                    .probe
                    .chapter_index(&start.chapter)
                    .ok_or("That chapter is unavailable")?;
                let probe = active.reading.config.chapter(chapter)?;
                if probe.page_for(start).is_some()
                    && probe.page_for(end).is_some()
                    && probe.select_source_range(start, end).is_some()
                {
                    Ok(())
                } else {
                    Err("This mark is unavailable in the open document".into())
                }
            }
            Self::Fixed(_, _) => Err("This mark is unavailable in the open document".into()),
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
    detail: Option<MarkKey>,
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
        let target = Snapshot::capture(book, key.locator)?;
        if target.has_raw() {
            target.verify_source(reader)?;
        }
        targets.push(ReviewedMark {
            target,
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
        detail: None,
    });
    reader.notice = None;
    reader.show(Screen::BookmarkBrowser(Page::RemoveMarksConfirm));
    Ok(())
}

/// Review exactly the visible mark before opening its floating confirmation.
pub(super) fn ask_detail(reader: &mut Reader) -> Result<()> {
    if reader.screen != Screen::BookmarkBrowser(Page::Detail)
        || !matches!(reader.bookmark_browser.origin, Screen::Book | Screen::Fixed)
    {
        return Err("Open this book before removing its mark".into());
    }
    let key = reader
        .bookmark_browser
        .focus
        .ok_or("That mark is unavailable")?;
    if reader.open_book_id() != Some(key.book) {
        return Err("Open this book before removing its mark".into());
    }
    let book = reader
        .library
        .state(key.book)
        .ok_or("That book is unavailable")?;
    let target = Snapshot::capture(book, key.locator)?;
    target.verify_source(reader)?;
    reader.bookmark_browser.pending_mark_removal = Some(Pending {
        book: key.book,
        title: book.title.clone(),
        targets: vec![ReviewedMark {
            target,
            selected: true,
        }],
        detail: Some(key),
    });
    reader.bookmark_browser.confirmation = Some(super::confirmation::Confirmation::SingleMark);
    reader.notice = None;
    Ok(())
}

pub(super) fn confirm(reader: &mut Reader) -> Result<()> {
    let pending = reader
        .bookmark_browser
        .pending_mark_removal
        .as_ref()
        .ok_or("Review the marks before removing them")?;
    let valid_screen = match pending.detail {
        Some(key) => {
            reader.screen == Screen::BookmarkBrowser(Page::Detail)
                && reader.bookmark_browser.confirmation
                    == Some(super::confirmation::Confirmation::SingleMark)
                && reader.bookmark_browser.focus == Some(key)
                && pending.targets.len() == 1
        }
        None => {
            reader.screen == Screen::BookmarkBrowser(Page::RemoveMarksConfirm)
                && reader.bookmark_browser.confirmation
                    == Some(super::confirmation::Confirmation::Marks)
        }
    };
    if !valid_screen {
        return Err("Review the marks before removing them".into());
    }
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
    for item in pending
        .targets
        .iter()
        .filter(|item| item.selected && (pending.detail.is_some() || item.target.has_raw()))
    {
        item.target.verify_source(reader)?;
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
