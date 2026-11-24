//! Search page marks, unannotated highlights and notes from either reading bar.
use std::collections::HashMap;

use super::{Action, Canvas, Item, LIST_X, LIST_Y, Reader, Rect, Screen};
use crate::Result;
use crate::search::Query;

pub(super) struct AnnotationSearch {
    pub(super) text: String,
    pub(super) origin: Option<Screen>,
    query: Option<Query>,
    case_sensitive: bool,
    bookmarks: bool,
    highlights: bool,
    notes: bool,
}

impl Default for AnnotationSearch {
    fn default() -> Self {
        Self {
            text: String::new(),
            origin: None,
            query: None,
            case_sensitive: false,
            bookmarks: true,
            highlights: true,
            notes: true,
        }
    }
}

#[derive(Clone, Copy)]
enum Filter {
    Case,
    Bookmarks,
    Highlights,
    Notes,
}

impl AnnotationSearch {
    fn controls(&self, width: i32) -> [(Filter, &'static str, bool, Rect); 4] {
        let width = (width - 2 * LIST_X - 36) / 3;
        let category = |col| Rect::new(LIST_X + col * (width + 18), LIST_Y + 232, width, 84);
        [
            (
                Filter::Case,
                "Case sensitive",
                self.case_sensitive,
                Rect::new(LIST_X, LIST_Y + 145, width, 70),
            ),
            (Filter::Bookmarks, "Bookmarks", self.bookmarks, category(0)),
            (
                Filter::Highlights,
                "Highlights",
                self.highlights,
                category(1),
            ),
            (Filter::Notes, "Notes", self.notes, category(2)),
        ]
    }

    pub(super) fn draw_filters(&self, canvas: &mut Canvas, width: i32) {
        for (_, label, selected, rect) in self.controls(width) {
            canvas.button(rect, label, selected, false);
        }
    }

    pub(super) fn toggle_at(&mut self, width: i32, x: f32, y: f32) -> bool {
        let Some((filter, _, _, _)) = self
            .controls(width)
            .into_iter()
            .find(|(_, _, _, rect)| rect.contains(x, y))
        else {
            return false;
        };
        let field = match filter {
            Filter::Case => &mut self.case_sensitive,
            Filter::Bookmarks => &mut self.bookmarks,
            Filter::Highlights => &mut self.highlights,
            Filter::Notes => &mut self.notes,
        };
        *field = !*field;
        true
    }

    pub(super) fn submit(&mut self, text: &str) -> Result<()> {
        let text = text.trim();
        self.query = if text.is_empty() {
            None
        } else {
            Some(Query::with_options(text, self.case_sensitive, false)?)
        };
        self.text = text.to_owned();
        Ok(())
    }

    fn contains(&self, text: &str) -> bool {
        self.query
            .as_ref()
            .is_none_or(|query| !query.matches(text, 1).is_empty())
    }

    fn highlight(&self, text: &str, note: Option<&str>, action: Action) -> Option<Item> {
        let note = note.filter(|s| !s.trim().is_empty());
        if let Some(note) = note {
            if !self.notes {
                return None;
            }
            if self.contains(note) {
                return Some(
                    Item::new(excerpt(note), action).detail(format!("Note · {}", excerpt(text))),
                );
            }
            return self.contains(text).then(|| {
                Item::new(excerpt(text), action).detail(format!("Note · {}", excerpt(note)))
            });
        }
        (self.highlights && self.contains(text))
            .then(|| Item::new(excerpt(text), action).detail("Highlight"))
    }
}

fn excerpt(text: &str) -> String {
    text.split_whitespace()
        .take(22)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Reader {
    pub(super) fn annotation_results(&self) -> Vec<Item> {
        let Some(state) = self.open_book_id().and_then(|id| self.library.state(id)) else {
            return Vec::new();
        };
        let query = &self.annotation_search;
        let mut rows = Vec::new();
        if query.bookmarks {
            for (index, mark) in state
                .bookmarks
                .iter()
                .enumerate()
                .filter(|(_, m)| query.contains(&m.label))
            {
                let pos = &mark.position;
                let key = if self.fixed.is_some() {
                    (pos.page, 0.0, 0.0)
                } else {
                    let legacy = self
                        .active
                        .as_ref()
                        .filter(|active| {
                            active.reading.chapter == pos.chapter && pos.anchor.is_none()
                        })
                        .and_then(|active| active.reading.probe.page_anchor(pos.page));
                    pos.anchor
                        .as_ref()
                        .or(legacy.as_ref())
                        .map_or((pos.chapter, 0.0, 0.0), |a| {
                            (pos.chapter, a.block as f64, a.offset as f64)
                        })
                };
                rows.push((
                    key,
                    Item::new(&mark.label, Action::Bookmark(index)).detail("Bookmark"),
                ));
            }
        }
        if self.fixed.is_some() {
            for (index, mark) in state.pdf_highlights.iter().enumerate() {
                if let Some(item) = query.highlight(
                    &mark.text,
                    mark.note.as_deref(),
                    Action::PdfHighlight(index),
                ) {
                    let (y, x) = mark
                        .bounds
                        .first()
                        .map_or((0.0, 0.0), |rect| (f64::from(rect.y0), f64::from(rect.x0)));
                    rows.push(((mark.page, y, x), item));
                }
            }
        } else if let Some(active) = &self.active {
            let spine: HashMap<_, _> = active
                .reading
                .probe
                .chapter_entries()
                .iter()
                .enumerate()
                .map(|(n, chapter)| (chapter.href.as_str(), n))
                .collect();
            for (index, mark) in state.highlights.iter().enumerate() {
                if let Some(item) = query.highlight(
                    &mark.selection.text,
                    mark.note.as_deref(),
                    Action::Highlight(index),
                ) {
                    let anchor = &mark.selection.start;
                    let chapter = spine
                        .get(anchor.chapter.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);
                    rows.push(((chapter, anchor.block as f64, anchor.offset as f64), item));
                }
            }
        }
        rows.sort_by(|(a, _), (b, _)| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| a.2.total_cmp(&b.2))
        });
        rows.into_iter().map(|(_, item)| item).collect()
    }
}
