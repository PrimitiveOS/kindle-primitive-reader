//! Conservative additive v2 restore: never discard conflicting source data.
use super::{SavedLibrary, invalid_state};
use crate::library::{self, BackupSummary, BookState};
use std::{collections::HashMap, io};

pub(super) fn merge_libraries(
    current: &SavedLibrary,
    incoming: &SavedLibrary,
) -> io::Result<(SavedLibrary, BackupSummary)> {
    let mut books = current.books.clone();
    let mut indexes: HashMap<u64, usize> = books
        .iter()
        .enumerate()
        .map(|(i, book)| (book.id, i))
        .collect();
    let mut summary = BackupSummary::default();
    for source in &incoming.books {
        let Some(&index) = indexes.get(&source.id) else {
            if books.len() >= library::MAX_BACKUP_BOOKS {
                return Err(invalid_state("The restored library has too many books"));
            }
            indexes.insert(source.id, books.len());
            books.push(source.clone());
            summary.new_books += 1;
            continue;
        };
        let target = &mut books[index];
        if metadata(target)? != metadata(source)? {
            return Err(invalid_state(
                "A book has conflicting metadata; no backup was restored",
            ));
        }
        if let Some(position) = &source.position {
            match &target.position {
                None => target.position = Some(position.clone()),
                Some(existing) if existing == position => {}
                Some(_) => {
                    return Err(invalid_state(
                        "A book has conflicting reading positions; no backup was restored",
                    ));
                }
            }
        }
        if target.custom_contents.enabled != source.custom_contents.enabled
            || target.hidden_flows.enabled != source.hidden_flows.enabled
            || target.hidden_flows.editing != source.hidden_flows.editing
        {
            return Err(invalid_state(
                "A book has conflicting contents or flow settings",
            ));
        }
        for mark in source.custom_contents.entries() {
            if target.custom_contents.merge_exact_v2(mark)? {
                summary.custom_chapters_added += 1;
            } else {
                summary.duplicates += 1;
            }
        }
        for point in source.hidden_flows.points() {
            if target.hidden_flows.merge_exact_v2(point)? {
                summary.custom_flow_points_added += 1;
            } else {
                summary.duplicates += 1;
            }
        }
        for mark in &source.bookmarks {
            if let Some(existing) = target
                .bookmarks
                .iter()
                .find(|m| library::same_position(&m.position, &mark.position))
            {
                if existing != mark {
                    return Err(invalid_state("A bookmark has conflicting metadata"));
                }
                summary.duplicates += 1;
            } else {
                if target.bookmarks.len() >= library::MAX_IMPORTED_MARKS {
                    return Err(invalid_state("A book has too many bookmarks"));
                }
                target.bookmarks.push(mark.clone());
                summary.bookmarks_added += 1;
            }
        }
        for mark in &source.highlights {
            if let Some(existing) = target.highlights.iter().find(|m| {
                super::super::raw_positions::same_selection(&m.selection, &mark.selection)
            }) {
                if existing != mark {
                    return Err(invalid_state("A highlight has conflicting metadata"));
                }
                summary.duplicates += 1;
            } else {
                if target.highlights.len() >= library::MAX_IMPORTED_MARKS {
                    return Err(invalid_state("A book has too many highlights"));
                }
                target.highlights.push(mark.clone());
                summary.highlights_added += 1;
            }
        }
        for mark in &source.pdf_highlights {
            if let Some(existing) = target.pdf_highlights.iter().find(|m| {
                m.page == mark.page
                    && m.bounds == mark.bounds
                    && m.continuation == mark.continuation
            }) {
                if existing != mark {
                    return Err(invalid_state("A PDF highlight has conflicting metadata"));
                }
                summary.duplicates += 1;
            } else {
                if target.pdf_highlights.len() >= library::MAX_PDF_HIGHLIGHTS {
                    return Err(invalid_state("A book has too many PDF highlights"));
                }
                target.pdf_highlights.push(mark.clone());
                summary.pdf_highlights_added += 1;
            }
        }
        summary.existing_books += 1;
    }
    for collection in &incoming.collections {
        if let Some(index) = library::collection_index(&current.collections, &collection.name)
            && current.collections[index].name != collection.name
        {
            return Err(invalid_state("Collections have conflicting names"));
        }
    }
    let collections = library::merge_collections(
        &current.collections,
        &incoming.collections,
        &books,
        &mut summary,
    );
    if summary.skipped != 0 {
        return Err(invalid_state(
            "The restored collections exceed their limits",
        ));
    }
    let grid = match (current.page_browser_grid, incoming.page_browser_grid) {
        (Some(a), Some(b)) if a != b => {
            return Err(invalid_state("The page-browser defaults conflict"));
        }
        (Some(a), _) | (_, Some(a)) => Some(a),
        _ => None,
    };
    let merged = SavedLibrary {
        version: 2,
        books,
        collections,
        page_browser_grid: grid,
    };
    super::encode(&merged)?;
    Ok((merged, summary))
}

/// Compare every field that this merge does not explicitly combine.
fn metadata(state: &BookState) -> io::Result<serde_json::Value> {
    let mut state = state.clone();
    state.position = None;
    state.bookmarks.clear();
    state.highlights.clear();
    state.pdf_highlights.clear();
    state.custom_contents = Default::default();
    state.hidden_flows = Default::default();
    serde_json::to_value(state).map_err(io::Error::other)
}
