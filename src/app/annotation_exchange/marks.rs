//! Map stock XPointers and physical page rectangles to saved Reader marks.

use super::*;
use crate::Anchor;
use crate::fixed_text::PdfTextRect;
use exchange::{Mark, PageBox, Place, Range};
use std::num::NonZeroU32;

impl Reader {
    pub(super) fn export_transfer_marks(
        &self,
        source: &Path,
        state: &BookState,
    ) -> Result<(Vec<Mark>, usize)> {
        with_storage(|| {
            let result = if let Some(fixed) = &self.fixed {
                let points = state
                    .bookmarks
                    .iter()
                    .filter_map(|mark| export_fixed_bookmark(mark, fixed.reading.count()));
                let ranges = state
                    .pdf_highlights
                    .iter()
                    .filter_map(|mark| export_fixed_highlight(mark, &fixed.reading));
                points.chain(ranges).collect::<Vec<_>>()
            } else {
                let mut book = Book::open(source).map_err(io::Error::other)?;
                let mut marks = Vec::new();
                for mark in &state.bookmarks {
                    if let Some(anchor) = &mark.position.anchor
                        && let Ok(place) = crate::cre_pointer::encode(&mut book, anchor, false)
                    {
                        marks.push(Mark {
                            place: Place::Pointer(place),
                            created: mark.created_at,
                            updated: None,
                            chapter: mark.chapter_title.clone(),
                            text: mark.label.clone(),
                            highlight: None,
                        });
                    }
                }
                for mark in &state.highlights {
                    if let Ok(start) =
                        crate::cre_pointer::encode(&mut book, &mark.selection.start, false)
                        && let Ok(end) =
                            crate::cre_pointer::encode(&mut book, &mark.selection.end, true)
                    {
                        marks.push(Mark {
                            place: Place::Pointer(start.clone()),
                            created: mark.created_at,
                            updated: mark.updated_at,
                            chapter: mark.chapter_title.clone(),
                            text: mark.selection.text.clone(),
                            highlight: Some(exchange::Highlight {
                                note: mark.note.clone(),
                                style: mark.style,
                                color: mark.color,
                                range: Range::Text { start, end },
                            }),
                        });
                    }
                }
                marks
            };
            let unavailable =
                state.bookmarks.len() + state.highlights.len() + state.pdf_highlights.len()
                    - result.len();
            Ok((result, unavailable))
        })
        .map_err(Into::into)
    }

    pub(super) fn apply_transfer(
        &self,
        source: &Path,
        snapshot: exchange::Snapshot,
        state: &mut BookState,
        keep_all: bool,
    ) -> Result<Summary> {
        let mut points = Vec::new();
        let mut ranges = Vec::new();
        let mut fixed_ranges = Vec::new();
        let mut unavailable = snapshot.unavailable;
        if let Some(fixed) = &self.fixed {
            for mark in snapshot.marks {
                if let Some(location) = load_fixed_mark(mark, &fixed.reading) {
                    match location {
                        FixedMark::Point(point) => points.push(point),
                        FixedMark::Range(range) => fixed_ranges.push(range),
                    }
                } else {
                    unavailable += 1;
                }
            }
        } else if let Some(active) = &self.active {
            let (mapped, missing) = with_storage(|| {
                let mut book = Book::open(source).map_err(io::Error::other)?;
                let mut mapped = Vec::new();
                let mut missing = 0;
                for mark in snapshot.marks {
                    if let Some(mark) = load_reflow_mark(mark, &mut book) {
                        mapped.push(mark);
                    } else {
                        missing += 1;
                    }
                }
                Ok((mapped, missing))
            })?;
            unavailable += missing;
            for mark in mapped {
                match mark {
                    TextMark::Point(point) => points.push(point),
                    TextMark::Range(a, b, mark, details) => {
                        let selection = active
                            .reading
                            .anchored_quote(&a, &b)
                            .ok()
                            .filter(|selected| same_words(&selected.text, &mark.text));
                        if let Some(selection) = selection {
                            ranges.push(Highlight {
                                chapter_title: mark.chapter,
                                created_at: mark.created,
                                updated_at: mark.updated,
                                color: details.color,
                                style: details.style,
                                selection,
                                note: details.note,
                            });
                        } else {
                            unavailable += 1;
                        }
                    }
                }
            }
        }
        let mut summary = Summary {
            unavailable,
            ..Summary::default()
        };
        let cutoff = (!keep_all
            && !snapshot.incomplete
            && unavailable == 0
            && !(points.is_empty() && ranges.is_empty() && fixed_ranges.is_empty()))
        .then_some(snapshot.datetime);
        merge(
            &mut state.bookmarks,
            points,
            cutoff,
            |mark| mark.created_at,
            |a, b| match (&a.position.anchor, &b.position.anchor) {
                (Some(a), Some(b)) => a == b,
                (None, None) => a.position.page == b.position.page,
                _ => false,
            },
            &mut summary,
        );
        if snapshot.fixed {
            merge(
                &mut state.pdf_highlights,
                fixed_ranges,
                cutoff,
                |mark| {
                    if mark.continuation.is_empty() {
                        mark.updated_at.or(mark.created_at)
                    } else {
                        None
                    }
                },
                |a, b| {
                    a.continuation.is_empty()
                        && b.continuation.is_empty()
                        && same_pdf_highlight(a, b)
                },
                &mut summary,
            );
        } else {
            merge(
                &mut state.highlights,
                ranges,
                cutoff,
                |mark| mark.updated_at.or(mark.created_at),
                |a, b| a.selection.start == b.selection.start,
                &mut summary,
            );
        }
        Ok(summary)
    }
}

fn merge<T>(
    saved: &mut Vec<T>,
    incoming: Vec<T>,
    cutoff: Option<i64>,
    touched: impl Fn(&T) -> Option<i64>,
    same_place: impl Fn(&T, &T) -> bool,
    summary: &mut Summary,
) {
    if let Some(cutoff) = cutoff {
        let before = saved.len();
        saved.retain(|local| {
            touched(local).is_none_or(|time| time >= cutoff)
                || incoming.iter().any(|remote| same_place(local, remote))
        });
        summary.removed += before - saved.len();
    }
    for mark in incoming {
        if let Some(local) = saved.iter_mut().find(|local| same_place(local, &mark)) {
            if let (Some(old), Some(new)) = (touched(local), touched(&mark))
                && new > old
            {
                *local = mark;
                summary.updated += 1;
            }
        } else {
            saved.push(mark);
            summary.added += 1;
        }
    }
}

fn same_words(left: &str, right: &str) -> bool {
    left.split_whitespace().eq(right.split_whitespace())
}

fn export_fixed_bookmark(mark: &Bookmark, count: usize) -> Option<Mark> {
    let index = mark.position.page;
    (index < count).then_some(())?;
    Some(Mark {
        place: Place::Page(NonZeroU32::new(u32::try_from(index + 1).ok()?)?),
        created: mark.created_at,
        updated: None,
        chapter: mark.chapter_title.clone(),
        text: mark.label.clone(),
        highlight: None,
    })
}

fn export_fixed_highlight(
    mark: &PdfHighlight,
    reader: &crate::fixed_session::FixedReading,
) -> Option<Mark> {
    if !mark.continuation.is_empty() || !mark.is_valid() {
        return None;
    }
    let (w, h) = reader.annotation_page_size(mark.page).ok()?;
    let boxes = mark
        .bounds
        .iter()
        .map(|bounds| {
            let x = f64::from(bounds.x0) * f64::from(w);
            let y = f64::from(bounds.y0) * f64::from(h);
            PageBox {
                x,
                y,
                w: f64::from(bounds.x1) * f64::from(w) - x,
                h: f64::from(bounds.y1) * f64::from(h) - y,
            }
        })
        .collect();
    Some(Mark {
        place: Place::Page(NonZeroU32::new(u32::try_from(mark.page + 1).ok()?)?),
        created: mark.created_at,
        updated: mark.updated_at,
        chapter: mark.chapter_title.clone(),
        text: mark.text.clone(),
        highlight: Some(exchange::Highlight {
            note: mark.note.clone(),
            style: mark.style,
            color: mark.color,
            range: Range::Boxes(boxes),
        }),
    })
}

enum FixedMark {
    Point(Bookmark),
    Range(PdfHighlight),
}

fn load_fixed_mark(mark: Mark, reader: &crate::fixed_session::FixedReading) -> Option<FixedMark> {
    let Place::Page(page) = mark.place else {
        return None;
    };
    let index = page.get() as usize - 1;
    if index >= reader.count() {
        return None;
    }
    let Some(details) = mark.highlight else {
        return Some(FixedMark::Point(Bookmark {
            chapter_title: mark.chapter,
            created_at: mark.updated.or(mark.created),
            label: label(mark.text, || format!("Page {page}")),
            position: ReadingPosition {
                chapter: 0,
                page: index,
                anchor: None,
            },
        }));
    };
    let Range::Boxes(boxes) = details.range else {
        return None;
    };
    let (width, height) = reader.annotation_page_size(index).ok()?;
    let (width, height) = (f64::from(width), f64::from(height));
    let mut bounds = Vec::new();
    for rect in boxes {
        let right = rect.x + rect.w;
        let bottom = rect.y + rect.h;
        if right > width + 1.0 || bottom > height + 1.0 {
            return None;
        }
        bounds.push(PdfTextRect {
            x0: (rect.x / width).clamp(0., 1.) as f32,
            y0: (rect.y / height).clamp(0., 1.) as f32,
            x1: (right / width).clamp(0., 1.) as f32,
            y1: (bottom / height).clamp(0., 1.) as f32,
        });
    }
    let page_text = reader.page_text(index).ok()?;
    let matches = page_text
        .words
        .iter()
        .filter(|word| {
            let x = (word.bounds.x0 + word.bounds.x1) / 2.;
            let y = (word.bounds.y0 + word.bounds.y1) / 2.;
            bounds
                .iter()
                .any(|rect| x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1)
        })
        .filter_map(|word| page_text.text.get(word.bytes.clone()));
    if !matches
        .flat_map(str::split_whitespace)
        .eq(mark.text.split_whitespace())
    {
        return None;
    }
    let mark = PdfHighlight {
        chapter_title: mark.chapter,
        created_at: mark.created,
        updated_at: mark.updated,
        color: details.color,
        style: details.style,
        page: index,
        bounds,
        text: mark.text,
        note: details.note,
        continuation: Vec::new(),
    };
    mark.is_valid().then_some(FixedMark::Range(mark))
}

enum TextMark {
    Point(Bookmark),
    Range(Anchor, Anchor, Mark, exchange::Highlight),
}

fn load_reflow_mark(mut mark: Mark, book: &mut Book) -> Option<TextMark> {
    let Place::Pointer(value) = &mark.place else {
        return None;
    };
    let version = Some(crate::cre_pointer::DOM_VERSION);
    let p = crate::cre_pointer::resolve(book, value, version).ok()?;
    let Some(details) = mark.highlight.take() else {
        let bookmark = Bookmark {
            chapter_title: mark.chapter,
            created_at: mark.updated.or(mark.created),
            label: label(mark.text, || book.chapters[p.chapter].label.clone()),
            position: ReadingPosition {
                chapter: p.chapter,
                page: 0,
                anchor: Some(p.anchor),
            },
        };
        return Some(TextMark::Point(bookmark));
    };
    let Range::Text { start, end } = &details.range else {
        return None;
    };
    if value != start {
        return None;
    }
    let end = crate::cre_pointer::resolve(book, end, version).ok()?;
    Some(TextMark::Range(p.anchor, end.anchor, mark, details))
}

fn label(text: String, default: impl FnOnce() -> String) -> String {
    if text.trim().is_empty() {
        default()
    } else {
        text
    }
}
