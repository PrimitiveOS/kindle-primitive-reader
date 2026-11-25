//! Encode a detached v2 candidate without exposing raw fields to v1 serde.

use super::{
    Entry, MAX_BACKUP_BOOKS, MAX_CARETS, MAX_STATE_BYTES, Place, SavedLibrary, SavedLibraryRef,
    check, decode, invalid_state,
};
use crate::Anchor;
use serde_json::Value;
use std::io;

pub(super) fn encode(library: &SavedLibrary) -> io::Result<Vec<u8>> {
    if library.version != 2 || library.books.len() > MAX_BACKUP_BOOKS {
        return Err(invalid_state(
            "Reader state version or book count is unsupported",
        ));
    }
    let mut books = library.books.clone();
    let mut entries = Vec::new();
    for (book, state) in books.iter_mut().enumerate() {
        detach(
            state.position.as_mut().and_then(|p| p.anchor.as_mut()),
            book,
            Place::Resume,
            &mut entries,
        )?;
        for (i, mark) in state.bookmarks.iter_mut().enumerate() {
            detach(
                mark.position.anchor.as_mut(),
                book,
                Place::Bookmark(i),
                &mut entries,
            )?;
        }
        for (i, mark) in state
            .custom_contents
            .entries_mut_for_raw_v2()
            .iter_mut()
            .enumerate()
        {
            detach(
                mark.position.anchor.as_mut(),
                book,
                Place::CustomContents(i),
                &mut entries,
            )?;
        }
        for (i, point) in state
            .hidden_flows
            .points_mut_for_raw_v2()
            .iter_mut()
            .enumerate()
        {
            detach(
                point.position.anchor.as_mut(),
                book,
                Place::HiddenFlow(i),
                &mut entries,
            )?;
        }
        for (i, mark) in state.highlights.iter_mut().enumerate() {
            detach(
                Some(&mut mark.selection.start),
                book,
                Place::HighlightStart(i),
                &mut entries,
            )?;
            detach(
                Some(&mut mark.selection.end),
                book,
                Place::HighlightEnd(i),
                &mut entries,
            )?;
        }
    }
    super::super::validate_state(&books)?;
    let mut value = serde_json::to_value(SavedLibraryRef {
        version: 2,
        books: &books,
        collections: &library.collections,
        page_browser_grid: library.page_browser_grid,
    })
    .map_err(io::Error::other)?;
    for entry in entries {
        let book = value
            .get_mut("books")
            .and_then(Value::as_array_mut)
            .and_then(|books| books.get_mut(entry.book))
            .ok_or_else(|| invalid_state("The raw caret's book is unavailable"))?;
        let anchor = anchor_at(book, entry.place)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| invalid_state("The raw caret's anchor is unavailable"))?;
        if anchor
            .insert(
                "raw_caret".into(),
                serde_json::to_value(entry.caret).map_err(io::Error::other)?,
            )
            .is_some()
        {
            return Err(invalid_state("The raw caret's anchor is repeated"));
        }
    }
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(invalid_state("Reader state exceeds its size limit"));
    }
    decode(&bytes)?;
    Ok(bytes)
}

fn detach(
    anchor: Option<&mut Anchor>,
    book: usize,
    place: Place,
    entries: &mut Vec<Entry>,
) -> io::Result<()> {
    let Some(anchor) = anchor else {
        return Ok(());
    };
    let Some(caret) = anchor.raw_caret.as_ref().cloned() else {
        return Ok(());
    };
    if entries.len() >= MAX_CARETS {
        return Err(invalid_state(
            "Reader state has too many raw whitespace carets",
        ));
    }
    check(anchor, &caret)?;
    anchor.raw_caret = None;
    entries.push(Entry { book, place, caret });
    Ok(())
}

fn anchor_at(book: &mut Value, place: Place) -> Option<&mut Value> {
    match place {
        Place::Resume => book.get_mut("position")?.get_mut("anchor"),
        Place::Bookmark(i) => book
            .get_mut("bookmarks")?
            .as_array_mut()?
            .get_mut(i)?
            .get_mut("position")?
            .get_mut("anchor"),
        Place::CustomContents(i) => book
            .get_mut("custom_contents")?
            .get_mut("entries")?
            .as_array_mut()?
            .get_mut(i)?
            .get_mut("position")?
            .get_mut("anchor"),
        Place::HiddenFlow(i) => book
            .get_mut("hidden_flows")?
            .get_mut("points")?
            .as_array_mut()?
            .get_mut(i)?
            .get_mut("position")?
            .get_mut("anchor"),
        Place::HighlightStart(i) => book
            .get_mut("highlights")?
            .as_array_mut()?
            .get_mut(i)?
            .get_mut("selection")?
            .get_mut("start"),
        Place::HighlightEnd(i) => book
            .get_mut("highlights")?
            .as_array_mut()?
            .get_mut(i)?
            .get_mut("selection")?
            .get_mut("end"),
    }
}
