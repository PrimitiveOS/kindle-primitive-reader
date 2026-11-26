//! Cancellable, bounded cross-chapter focus for current-book saved marks.
use super::{Locator, Page, Reader, RowKey, Screen};
use crate::library::{Bookmark, Highlight};
use crate::session::SearchLayoutRecipe;
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const MAX_MARKS: usize = 512;
const MAX_ROWS: usize = 4096;
const MAX_SNAPSHOT: usize = 2 * 1024 * 1024;
const MAX_CROSS_WORK: usize = 64;
const TICK: Duration = Duration::from_millis(80);
const PAINT: Duration = Duration::from_secs(2);

#[derive(Clone)]
enum Mark {
    Bookmark(Bookmark),
    Highlight(Highlight),
}
struct Entry {
    ordinal: usize,
    chapter: usize,
    last_chapter: usize,
    mark: Mark,
}
#[derive(Clone, Copy)]
pub(in crate::app) enum Failure {
    Changed,
    Limit,
}

enum Message {
    Progress { checked: usize, total: usize },
    Done(Result<Option<usize>, Failure>),
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(in crate::app) struct Focus {
    book: u64,
    origin: super::super::HistoryLocation,
    recipe: Arc<SearchLayoutRecipe>,
    rows: Vec<RowKey>,
    entries: Arc<Vec<Entry>>,
    rx: mpsc::Receiver<Message>,
    checked: usize,
    total: usize,
    next_paint: Instant,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
}
impl Drop for Focus {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Focus {
    pub(in crate::app) fn deadline(&self) -> Instant {
        self.deadline
    }
}

// Verify the complete saved quotation against the same snapshot used for the
// focus probe. Match Reading::collect_anchored_quote's source and ODT joins.
pub(super) fn verify_cross_chapter(
    recipe: &SearchLayoutRecipe,
    cancel: &AtomicBool,
    first_probe: &crate::Probe,
    first: usize,
    last: usize,
    mark: &Highlight,
    work: &mut usize,
) -> Result<bool, Failure> {
    verify_cross_chapter_with_snapshot(recipe, cancel, first_probe, first, last, mark, work, None)
}

/// A bookmark-navigation worker can use the immutable folder proof it just
/// captured, rather than allowing verification to reopen the live folder.
pub(in crate::app) fn verify_cross_chapter_with_snapshot(
    recipe: &SearchLayoutRecipe,
    cancel: &AtomicBool,
    first_probe: &crate::Probe,
    first: usize,
    last: usize,
    mark: &Highlight,
    work: &mut usize,
    folder: Option<&crate::session::screen_pages::folder_sources::FolderProof>,
) -> Result<bool, Failure> {
    let saved = &mark.selection;
    if first >= last
        || saved.start.book != saved.end.book
        || saved.text.len() > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
        || first_probe.page_for(&saved.start).is_none()
    {
        return Ok(false);
    }
    let span = last
        .checked_sub(first)
        .and_then(|count| count.checked_add(1))
        .ok_or(Failure::Limit)?;
    if span > MAX_CROSS_WORK.saturating_sub(*work) {
        return Err(Failure::Limit);
    }
    let raw = saved.start.raw_caret.is_some() || saved.end.raw_caret.is_some();
    let mut text = String::new();
    for chapter in first..=last {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Changed);
        }
        *work = work.checked_add(1).ok_or(Failure::Limit)?;
        if *work > MAX_CROSS_WORK {
            return Err(Failure::Limit);
        }
        let loaded;
        let probe = if chapter == first {
            first_probe
        } else {
            loaded = if let Some(proof) = folder {
                recipe.folder_chapter(proof, chapter, cancel)
            } else {
                recipe.chapter(chapter, cancel)
            }
            .map_err(|_| Failure::Changed)?;
            &loaded
        };
        if chapter == last && probe.page_for(&saved.end).is_none() {
            return Ok(false);
        }
        let fragment = if raw {
            match probe.raw_chapter_fragment(&saved.start, &saved.end) {
                Some(fragment) => Some(fragment.text),
                None => return Ok(false),
            }
        } else {
            match probe.select_source_range(&saved.start, &saved.end) {
                Some(fragment) => Some(fragment.text),
                None if probe.has_raw_layout() || chapter == first || chapter == last => {
                    return Ok(false);
                }
                None => None,
            }
        };
        if let Some(fragment) = fragment {
            let joins = first_probe
                .odt_continued_before
                .get(chapter)
                .copied()
                .unwrap_or(false);
            let extra = usize::from(!text.is_empty() && !joins);
            if text
                .len()
                .saturating_add(extra)
                .saturating_add(fragment.len())
                > crate::library::MAX_HIGHLIGHT_TEXT_BYTES
            {
                return Ok(false);
            }
            if extra != 0 {
                text.push('\n');
            }
            text.push_str(&fragment);
        }
    }
    Ok(!text.trim().is_empty() && text == saved.text && !cancel.load(Ordering::Relaxed))
}

fn panel(reader: &Reader) -> (Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(200, 760);
    let height = (layout.height - 80).clamp(260, 360);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let cancel = Rect::new(panel.x + (width - 240) / 2, panel.bottom() - 120, 240, 100);
    (panel, cancel)
}

impl Reader {
    /// Returns false when the existing current-chapter fallback should be used.
    pub(in crate::app) fn start_bookmark_focus(&mut self) -> bool {
        if self.bookmark_focus_busy.load(Ordering::Acquire) {
            return false;
        }
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        if self.bookmark_browser.origin != Screen::Book {
            return false;
        }
        let Some(origin) = self.current_history_location() else {
            return false;
        };
        let Some(origin_anchor) = origin.position.anchor.clone() else {
            return false;
        };
        let recipe = Arc::new(
            active
                .reading
                .config
                .search_layout_recipe(&active.reading.probe),
        );
        if !recipe.has_verified_sources() || self.bookmark_browser.rows.len() > MAX_ROWS {
            return false;
        }
        let Some(state) = self.library.state(active.id) else {
            return false;
        };
        let mut entries = Vec::new();
        let mut bytes = 0usize;
        for (ordinal, row) in self.bookmark_browser.rows.iter().enumerate() {
            let RowKey::Mark(key) = row else {
                continue;
            };
            if key.book != active.id {
                continue;
            }
            let (mark, chapter, last_chapter, size) = match key.locator {
                Locator::Bookmark(index) => {
                    let Some(mark) = state.bookmarks.get(index) else {
                        return false;
                    };
                    let chapter = match &mark.position.anchor {
                        Some(anchor) => match active.reading.probe.chapter_index(&anchor.chapter) {
                            Some(c) => c,
                            None => continue,
                        },
                        None => mark.position.chapter,
                    };
                    let Ok(raw) = serde_json::to_vec(mark) else {
                        return false;
                    };
                    (Mark::Bookmark(mark.clone()), chapter, chapter, raw.len())
                }
                Locator::Reflow(index) => {
                    let Some(mark) = state.highlights.get(index) else {
                        return false;
                    };
                    let Some(chapter) = active
                        .reading
                        .probe
                        .chapter_index(&mark.selection.start.chapter)
                    else {
                        continue;
                    };
                    let Some(last_chapter) = active
                        .reading
                        .probe
                        .chapter_index(&mark.selection.end.chapter)
                    else {
                        continue;
                    };
                    if last_chapter < chapter
                        || mark.selection.start.book != mark.selection.end.book
                    {
                        continue;
                    }
                    let Ok(raw) = serde_json::to_vec(mark) else {
                        return false;
                    };
                    (
                        Mark::Highlight(mark.clone()),
                        chapter,
                        last_chapter,
                        raw.len(),
                    )
                }
                Locator::Fixed(_) => continue,
            };
            bytes = match bytes.checked_add(size) {
                Some(size) if size <= MAX_SNAPSHOT => size,
                _ => return false,
            };
            entries.push(Entry {
                ordinal,
                chapter,
                last_chapter,
                mark,
            });
            if entries.len() > MAX_MARKS {
                return false;
            }
        }
        if entries.is_empty() {
            return false;
        }
        let book = active.id;
        let last_visible = active.reading.last_visible_page();
        let rows = self.bookmark_browser.rows.clone();
        let entries = Arc::new(entries);
        let cancelled = Arc::new(AtomicBool::new(false));
        if self.bookmark_focus_busy.swap(true, Ordering::AcqRel) {
            return false;
        }
        let gate = self.bookmark_focus_busy.clone();
        let worker_cancel = cancelled.clone();
        let worker_recipe = recipe.clone();
        let worker_entries = entries.clone();
        let current = origin.position.clone();
        let folder = crate::epub_members::is_folder_book(&active.reading.config.book);
        let chapter_total = active.reading.probe.chapter_count();
        let (tx, rx) = mpsc::sync_channel(1);
        let spawn = std::thread::Builder::new()
            .name("bookmark-focus".into())
            .spawn(move || {
                let _guard = Guard(gate);
                let run = || -> Result<Option<usize>, Failure> {
                    let origin_probe = worker_recipe
                        .chapter(current.chapter, &worker_cancel)
                        .map_err(|_| Failure::Changed)?;
                    let page = origin_probe
                        .page_for(&origin_anchor)
                        .ok_or(Failure::Changed)?;
                    if page < current.page || page > last_visible {
                        return Err(Failure::Changed);
                    }
                    let mut chapters: Vec<_> =
                        worker_entries.iter().map(|entry| entry.chapter).collect();
                    chapters.sort_unstable();
                    chapters.dedup();
                    chapters.retain(|chapter| *chapter < chapter_total);
                    // KOReader focuses near the current page in its filtered list.
                    // Prefer the current and subsequent chapters; only if none
                    // resolves, visit preceding chapters from nearest to farthest.
                    chapters.sort_unstable_by_key(|&chapter| {
                        (chapter < current.chapter, chapter.abs_diff(current.chapter))
                    });
                    let total = chapters.len();
                    let mut best: Option<((bool, usize, usize, usize), usize)> = None;
                    let mut cross_work = 0usize;
                    for (checked, chapter) in chapters.into_iter().enumerate() {
                        // A preceding page in the current chapter beats every
                        // earlier chapter once all later chapters are exhausted.
                        if chapter < current.chapter && best.is_some() {
                            break;
                        }
                        let _ = tx.try_send(Message::Progress { checked, total });
                        if worker_cancel.load(Ordering::Relaxed) {
                            return Err(Failure::Changed);
                        }
                        let loaded;
                        let probe = if chapter == current.chapter {
                            &origin_probe
                        } else {
                            loaded = worker_recipe
                                .chapter(chapter, &worker_cancel)
                                .map_err(|_| Failure::Changed)?;
                            &loaded
                        };
                        for entry in worker_entries
                            .iter()
                            .filter(|entry| entry.chapter == chapter)
                        {
                            if worker_cancel.load(Ordering::Relaxed) {
                                return Err(Failure::Changed);
                            }
                            let page = match &entry.mark {
                                Mark::Bookmark(mark) => match &mark.position.anchor {
                                    Some(anchor) => match probe.page_for(anchor) {
                                        Some(page) => page,
                                        None => continue,
                                    },
                                    None if mark.position.page < probe.page_count() => {
                                        mark.position.page
                                    }
                                    None => continue,
                                },
                                Mark::Highlight(mark) => {
                                    if entry.last_chapter != chapter {
                                        if !verify_cross_chapter(
                                            &worker_recipe,
                                            &worker_cancel,
                                            probe,
                                            chapter,
                                            entry.last_chapter,
                                            mark,
                                            &mut cross_work,
                                        )? {
                                            continue;
                                        }
                                        match probe.page_for(&mark.selection.start) {
                                            Some(page) => page,
                                            None => continue,
                                        }
                                    } else {
                                        if probe.page_for(&mark.selection.end).is_none() {
                                            continue;
                                        }
                                        if (mark.selection.start.raw_caret.is_some()
                                            || mark.selection.end.raw_caret.is_some())
                                            && !probe
                                                .select_raw_anchors(
                                                    &mark.selection.start,
                                                    &mark.selection.end,
                                                )
                                                .is_some_and(|selection| {
                                                    selection.start == mark.selection.start
                                                        && selection.end == mark.selection.end
                                                })
                                        {
                                            continue;
                                        }
                                        match probe.page_for(&mark.selection.start) {
                                            Some(page) => page,
                                            None => continue,
                                        }
                                    }
                                }
                            };
                            let after = (chapter, page) >= (current.chapter, current.page);
                            let key = if after {
                                (false, chapter - current.chapter, page, entry.ordinal)
                            } else {
                                (
                                    true,
                                    current.chapter - chapter,
                                    usize::MAX - page,
                                    entry.ordinal,
                                )
                            };
                            if best.as_ref().is_none_or(|(old, _)| key < *old) {
                                best = Some((key, entry.ordinal));
                            }
                        }
                        // The nearest eligible chapter has been exhausted; any
                        // unvisited chapter is strictly farther in reading order.
                        if best
                            .as_ref()
                            .is_some_and(|(key, _)| !key.0 || chapter < current.chapter)
                        {
                            break;
                        }
                    }
                    if folder {
                        // Recheck the original immutable inventory and fonts in
                        // the worker after the last candidate, never during draw.
                        worker_recipe
                            .verify_bound_folder_snapshot(&worker_cancel)
                            .map_err(|_| Failure::Changed)?;
                    }
                    Ok(best.map(|(_, ordinal)| ordinal))
                };
                let result = run();
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(Message::Done(result));
                }
            });
        if spawn.is_err() {
            self.bookmark_focus_busy.store(false, Ordering::Release);
            return false;
        }
        self.show(Screen::BookmarkBrowser(Page::Results));
        self.bookmark_focus = Some(Focus {
            book,
            origin,
            recipe,
            rows,
            entries,
            rx,
            checked: 0,
            total: 0,
            next_paint: Instant::now() + PAINT,
            cancel: cancelled,
            deadline: Instant::now() + TICK,
        });
        true
    }

    pub(in crate::app) fn draw_bookmark_focus(&self, canvas: &mut Canvas) {
        let Some(focus) = &self.bookmark_focus else {
            return;
        };
        let (panel, cancel) = panel(self);
        canvas.popup(panel);
        let vi = !self.controls.interface_language.is_english();
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 24, panel.w - 72, 64),
            if vi {
                "Đang tìm dấu trang"
            } else {
                "Finding saved marks"
            },
            BODY.bold(),
        );
        let status = if focus.total == 0 {
            if vi {
                "Đang chuẩn bị".to_owned()
            } else {
                "Preparing".to_owned()
            }
        } else if vi {
            format!("Đã kiểm tra {} / {} chương", focus.checked, focus.total)
        } else {
            format!("Checked {} of {} chapters", focus.checked, focus.total)
        };
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 100, panel.w - 72, 64),
            &status,
            CAPTION,
        );
        canvas.button(cancel, if vi { "Hủy" } else { "Cancel" }, false, false);
    }
    pub(in crate::app) fn bookmark_focus_input(&mut self, input: Input) -> Option<Change> {
        self.bookmark_focus.as_ref()?;
        let (popup, cancel) = panel(self);
        Some(match input {
            Input::PageBack => {
                self.bookmark_focus = None;
                Change::Partial
            }
            Input::Tap(x, y) if cancel.contains(x, y) || !popup.contains(x, y) => {
                self.bookmark_focus = None;
                Change::Partial
            }
            _ => Change::None,
        })
    }
    pub(in crate::app) fn poll_bookmark_focus(&mut self, now: Instant) -> Option<Change> {
        let mut focus = self.bookmark_focus.take()?;
        if now < focus.deadline {
            self.bookmark_focus = Some(focus);
            return None;
        }
        let result = match focus.rx.try_recv() {
            Ok(Message::Done(result)) => result,
            Ok(Message::Progress { checked, total }) => {
                focus.checked = checked.min(total);
                focus.total = total;
                focus.deadline = now + TICK;
                let repaint = now >= focus.next_paint;
                if repaint {
                    focus.next_paint = now + PAINT;
                }
                self.bookmark_focus = Some(focus);
                return repaint.then_some(Change::Partial);
            }
            Err(mpsc::TryRecvError::Empty) => {
                focus.deadline = now + TICK;
                self.bookmark_focus = Some(focus);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(Failure::Changed),
        };
        let valid = self.screen == Screen::BookmarkBrowser(Page::Results)
            && self.open_book_id() == Some(focus.book)
            && self.current_history_location().as_ref() == Some(&focus.origin)
            && self.bookmark_browser.rows == focus.rows
            && self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe)
                    == *focus.recipe
            })
            && self.library.state(focus.book).is_some_and(|state| {
                focus.entries.iter().all(|entry| {
                    let Some(RowKey::Mark(key)) = focus.rows.get(entry.ordinal) else {
                        return false;
                    };
                    match (&entry.mark, key.locator) {
                        (Mark::Bookmark(mark), Locator::Bookmark(index)) => {
                            state.bookmarks.get(index) == Some(mark)
                        }
                        (Mark::Highlight(mark), Locator::Reflow(index)) => {
                            state.highlights.get(index) == Some(mark)
                        }
                        _ => false,
                    }
                })
            });
        match (valid, result) {
            (true, Ok(Some(ordinal))) => {
                let page = ordinal / self.list_geometry().rows;
                self.set_bookmark_list_anchor(page, ordinal);
            }
            (true, Ok(None)) => {
                self.notice = Some(
                    self.controls
                        .interface_language
                        .bookmark_text("No matching marks in the current book")
                        .to_owned(),
                )
            }
            (true, Err(Failure::Limit)) => {
                self.notice = Some(
                    if self.controls.interface_language.is_english() {
                        "Highlight verification limit reached; narrow filters or open the mark"
                    } else {
                        "Đạt giới hạn kiểm tra; hãy thu hẹp bộ lọc hoặc mở dấu trang"
                    }
                    .into(),
                );
            }
            _ => {
                self.notice = Some(
                    if self.controls.interface_language.is_english() {
                        "The book or marks changed; try again"
                    } else {
                        "Sách hoặc dấu trang đã thay đổi; hãy thử lại"
                    }
                    .into(),
                )
            }
        }
        Some(Change::Partial)
    }
}
