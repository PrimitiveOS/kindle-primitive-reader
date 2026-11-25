//! Transactional translation of saved positions from unsplit ODT documents.
//!
//! The book supplies a source-verified mapping. This module only edits a clone
//! of the saved state; it never persists a partial translation.

use super::{BookState, ReadingPosition, valid_highlight};
use crate::epub::Book;

fn position(position: &mut ReadingPosition, book: &Book) -> crate::Result<bool> {
    let anchor = position
        .anchor
        .as_ref()
        .ok_or("An ODT position has no source anchor and cannot be safely migrated")?;
    let Some((chapter, anchor)) = book.migrate_old_odt_anchor(anchor, false)? else {
        return Ok(false);
    };
    position.chapter = chapter;
    position.anchor = Some(anchor);
    Ok(true)
}

/// Produce a complete candidate without changing the original library state.
pub(super) fn translate(state: &BookState, book: &Book) -> crate::Result<Option<BookState>> {
    let mut candidate = state.clone();
    let mut changed = false;

    if let Some(saved) = &mut candidate.position {
        changed |= position(saved, book)?;
    }
    for bookmark in &mut candidate.bookmarks {
        changed |= position(&mut bookmark.position, book)?;
    }
    for point in candidate.hidden_flows.points_mut_for_odt_migration() {
        changed |= position(&mut point.position, book)?;
    }
    for bookmark in candidate.custom_contents.entries_mut_for_odt_migration() {
        changed |= position(&mut bookmark.position, book)?;
    }
    for highlight in &mut candidate.highlights {
        let start = book.migrate_old_odt_anchor(&highlight.selection.start, false)?;
        let end = book.migrate_old_odt_anchor(&highlight.selection.end, true)?;
        match (start, end) {
            (None, None) => {}
            (Some((start_chapter, start)), Some((end_chapter, end))) => {
                if start_chapter > end_chapter {
                    return Err("An ODT highlight has an invalid migrated range".into());
                }
                highlight.selection.start = start;
                highlight.selection.end = end;
                if !valid_highlight(highlight) {
                    return Err("An ODT highlight has an invalid migrated range".into());
                }
                changed = true;
            }
            _ => return Err("An ODT highlight can only be migrated as a complete range".into()),
        }
    }
    if !changed {
        return Ok(None);
    }
    if !candidate.hidden_flows.sort_after_odt_migration()
        || !candidate.custom_contents.sort_after_odt_migration()
    {
        return Err("ODT migration would duplicate a saved flow or custom chapter".into());
    }
    Ok(Some(candidate))
}
