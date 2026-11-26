//! Cancellable, source-verified page-bookmark navigation.
use super::{
    HistoryLocation, Reader, ReadingPosition, Screen, chapter_navigation::Direction, with_storage,
};
use crate::library::{BookState, Bookmark, Highlight, PdfHighlight};
use crate::session::screen_pages::FileStamp;
use crate::session::screen_pages::folder_sources::FolderProof;
use crate::session::{Reading, SearchLayoutRecipe};
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const SLICE: usize = 128;
const SNAPSHOT_BYTES: usize = crate::library::MAX_STATE_BYTES as usize;
const TICK: Duration = Duration::from_millis(60);
const PAINT: Duration = Duration::from_secs(2);

#[derive(Clone)]
enum SavedMark {
    Bookmark(Bookmark),
    Reflow(Highlight),
    Fixed(PdfHighlight),
}

#[derive(Clone)]
struct Entry {
    saved: SavedMark,
    position: ReadingPosition,
    chapter: Option<usize>,
}

impl Entry {
    fn created_at(&self) -> Option<i64> {
        match &self.saved {
            SavedMark::Bookmark(mark) => mark.created_at,
            SavedMark::Reflow(mark) => mark.created_at,
            SavedMark::Fixed(mark) => mark.created_at,
        }
    }

    fn matches(&self, book: &BookState, index: usize, bookmarks: usize) -> bool {
        match &self.saved {
            SavedMark::Bookmark(mark) => book.bookmarks.get(index) == Some(mark),
            SavedMark::Reflow(mark) => {
                index
                    .checked_sub(bookmarks)
                    .and_then(|i| book.highlights.get(i))
                    == Some(mark)
            }
            SavedMark::Fixed(mark) => {
                index
                    .checked_sub(bookmarks)
                    .and_then(|i| book.pdf_highlights.get(i))
                    == Some(mark)
            }
        }
    }
}
type Snapshot = Arc<Vec<Entry>>;
type Candidate = (usize, ReadingPosition);
struct FolderResult {
    proof: Arc<FolderProof>,
    prepared: Option<crate::session::PreparedFolderChapter>,
}
enum Message {
    Progress(usize),
    Done(
        Option<Candidate>,
        Option<FolderResult>,
        Option<crate::PreparedParleyChapter>,
    ),
    FolderVerified(bool),
    FolderUnavailable,
    QuoteMismatch,
    QuoteLimit,
    Failed,
}
struct WorkerGuard(Arc<AtomicBool>);
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
#[derive(Clone, Copy)]
enum Phase {
    Chapters,
    Snapshot,
    Worker,
    Validate,
    StartFolderFinal,
    FolderFinal,
}

pub(super) struct Search {
    book: u64,
    screen: Screen,
    original: HistoryLocation,
    original_last_visible: usize,
    direction: Direction,
    boundary: bool,
    latest: bool,
    publication: Option<u64>,
    fixed_pages: Option<usize>,
    fixed_stamp: Option<FileStamp>,
    recipe: Option<Arc<SearchLayoutRecipe>>,
    folder_marker: Option<PathBuf>,
    folder_proof: Option<Arc<FolderProof>>,
    folder_prepared: Option<crate::session::PreparedFolderChapter>,
    archive_prepared: Option<crate::PreparedParleyChapter>,
    staged: Option<Reading>,
    total: usize,
    bookmarks_total: usize,
    marks_total: usize,
    include_highlights: bool,
    preview: bool,
    chapter_total: usize,
    chapter_cursor: usize,
    chapter_paths: HashMap<String, usize>,
    entries: Vec<Entry>,
    snapshot: Option<Snapshot>,
    bytes: usize,
    phase: Phase,
    cursor: usize,
    progress: usize,
    best: Option<Candidate>,
    receiver: Option<mpsc::Receiver<Message>>,
    cancelled: Arc<AtomicBool>,
    invalidated: bool,
    deadline: Instant,
    next_paint: Instant,
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Search {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn invalidate(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.invalidated = true;
        self.deadline = Instant::now();
    }
}

/// Verify the winning highlight in the worker before its location is used.
/// Bookmarks and fixed-document marks keep their existing navigation behavior.
fn verify_winning_quote(
    recipe: &SearchLayoutRecipe,
    probe: &crate::Probe,
    mark: &Highlight,
    folder: Option<&FolderProof>,
    cancel: &AtomicBool,
) -> Result<bool, super::bookmark_browser::focus::Failure> {
    use super::bookmark_browser::focus::{Failure, verify_cross_chapter_with_snapshot};

    let selection = &mark.selection;
    if selection.start.book != probe.book || selection.end.book != probe.book {
        return Ok(false);
    }
    let chapters = probe.chapter_entries();
    let unique = |path: &str| {
        let mut matches = chapters
            .iter()
            .enumerate()
            .filter(|(_, chapter)| chapter.href == path);
        let index = matches.next()?.0;
        matches.next().is_none().then_some(index)
    };
    let (Some(first), Some(last)) = (
        unique(&selection.start.chapter),
        unique(&selection.end.chapter),
    ) else {
        return Ok(false);
    };
    if first > last
        || probe.chapter_index(&probe.chapter) != Some(first)
        || chapters[first..=last]
            .iter()
            .any(|chapter| chapter.unsupported)
        || probe.page_for(&selection.start).is_none()
    {
        return Ok(false);
    }
    if first != last {
        let mut work = 0;
        return verify_cross_chapter_with_snapshot(
            recipe, cancel, probe, first, last, mark, &mut work, folder,
        );
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Failure::Changed);
    }
    if probe.page_for(&selection.end).is_none()
        || !probe
            .compare_text_anchors(&selection.start, &selection.end)
            .is_some_and(|order| order.is_lt())
    {
        return Ok(false);
    }
    let verified = if selection.start.raw_caret.is_some() || selection.end.raw_caret.is_some() {
        probe.select_raw_anchors(&selection.start, &selection.end)
    } else {
        probe.select_source_range(&selection.start, &selection.end)
    };
    Ok(verified.is_some_and(|quote| quote.text == selection.text))
}

fn worker(
    recipe: Option<Arc<SearchLayoutRecipe>>,
    folder_marker: Option<PathBuf>,
    book: u64,
    publication: Option<u64>,
    fixed_pages: Option<usize>,
    direction: Direction,
    boundary: bool,
    latest: bool,
    current: ReadingPosition,
    current_last_visible: usize,
    snapshot: Snapshot,
    cancelled: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Message>,
) {
    let folder = if let Some(marker) = &folder_marker {
        let proof = with_storage(|| FolderProof::capture(marker, &cancelled));
        match proof {
            Ok(proof) if !proof.contains_hidden_members() && proof.legacy_identity() == book => {
                Some(Arc::new(proof))
            }
            _ => {
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(Message::FolderUnavailable);
                }
                return;
            }
        }
    } else {
        None
    };
    let origin_probe = if let Some(proof) = &folder {
        let Some(recipe) = &recipe else {
            let _ = tx.send(Message::FolderUnavailable);
            return;
        };
        let Some(anchor) = &current.anchor else {
            let _ = tx.send(Message::FolderUnavailable);
            return;
        };
        let Ok(probe) = recipe.folder_chapter(proof, current.chapter, &cancelled) else {
            if !cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(Message::FolderUnavailable);
            }
            return;
        };
        if probe
            .page_for(anchor)
            .is_none_or(|page| page < current.page || page > current_last_visible)
        {
            let _ = tx.send(Message::FolderUnavailable);
            return;
        }
        Some(probe)
    } else {
        None
    };
    let mut order: Vec<usize> = if latest {
        // Preserve the existing latest selector: missing timestamps sort
        // first, and the last entry wins a tie. An unresolved latest mark
        // must not silently turn into a different (older) saved mark.
        snapshot
            .iter()
            .enumerate()
            .max_by_key(|(index, entry)| (entry.created_at(), *index))
            .and_then(|(index, entry)| entry.chapter.map(|_| index))
            .into_iter()
            .collect()
    } else {
        snapshot
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let chapter = entry.chapter?;
                let relevant = match direction {
                    Direction::Previous => chapter <= current.chapter,
                    Direction::Next => chapter >= current.chapter,
                };
                relevant.then_some(index)
            })
            .collect()
    };
    // Search chapters in the order that can produce the requested extreme.
    // All candidates within the winning chapter still need checking for page
    // and anchor order; only more distant chapters can then be skipped.
    let prefer_later = matches!(
        (direction, boundary),
        (Direction::Previous, false) | (Direction::Next, true)
    );
    order.sort_unstable_by(|&left, &right| {
        let left_chapter = snapshot[left].chapter.unwrap_or(usize::MAX);
        let right_chapter = snapshot[right].chapter.unwrap_or(usize::MAX);
        let chapter_order = if prefer_later {
            right_chapter.cmp(&left_chapter)
        } else {
            left_chapter.cmp(&right_chapter)
        };
        chapter_order.then_with(|| left.cmp(&right))
    });
    let mut cached_chapter = origin_probe.as_ref().map(|_| current.chapter);
    let mut cached_probe = origin_probe;
    let mut cached_exact = false;
    let mut best: Option<((usize, usize, usize, usize), Candidate)> = None;
    let mut previous_chapter = None;
    for (processed, index) in order.into_iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let entry = &snapshot[index];
        let chapter = entry.chapter.unwrap_or(0);
        if previous_chapter.is_some_and(|previous| previous != chapter) && best.is_some() {
            break;
        }
        previous_chapter = Some(chapter);
        if processed % 256 == 255 && tx.send(Message::Progress(processed + 1)).is_err() {
            return;
        }
        let mut target = entry.position.clone();
        let key = if let Some(recipe) = &recipe {
            if cached_chapter != Some(chapter) {
                let result = if let Some(proof) = &folder {
                    recipe
                        .folder_chapter(proof, chapter, &cancelled)
                        .map(|probe| (probe, false))
                } else {
                    recipe.chapter_with_quote_status(chapter, &cancelled)
                };
                match result {
                    Ok((probe, exact)) => {
                        cached_probe = Some(probe);
                        cached_chapter = Some(chapter);
                        cached_exact = exact;
                    }
                    Err(_) => {
                        if !cancelled.load(Ordering::Relaxed) {
                            let _ = tx.send(Message::Failed);
                        }
                        return;
                    }
                }
            }
            let Some(probe) = cached_probe.as_ref() else {
                continue;
            };
            let page = match &target.anchor {
                Some(anchor) if Some(anchor.book) == publication => match probe.page_for(anchor) {
                    Some(page) => page,
                    None => continue,
                },
                Some(_) => continue,
                None if target.page < probe.page_count() => target.page,
                None => continue,
            };
            target.chapter = chapter;
            target.page = page;
            (
                chapter,
                page,
                target.anchor.as_ref().map_or(0, |a| a.block),
                target.anchor.as_ref().map_or(0, |a| a.offset),
            )
        } else {
            if target.chapter != 0
                || target.anchor.is_some()
                || target.page >= fixed_pages.unwrap_or(0)
            {
                continue;
            }
            (0, target.page, 0, 0)
        };
        let page = (key.0, key.1);
        let current_page = (current.chapter, current.page);
        let beyond = latest
            || match direction {
                Direction::Previous => page < current_page,
                Direction::Next => page > current_page,
            };
        if beyond
            && best
                .as_ref()
                .is_none_or(|(prior, _)| match (direction, boundary) {
                    (Direction::Previous, false) | (Direction::Next, true) => key > *prior,
                    (Direction::Next, false) | (Direction::Previous, true) => key < *prior,
                })
        {
            best = Some((key, (index, target)));
        }
    }
    if !cancelled.load(Ordering::Relaxed) {
        if let (Some((_, (index, _))), Some(recipe), Some(probe)) =
            (&best, recipe.as_deref(), cached_probe.as_ref())
            && let SavedMark::Reflow(mark) = &snapshot[*index].saved
        {
            use super::bookmark_browser::focus::Failure;
            let verification =
                verify_winning_quote(recipe, probe, mark, folder.as_deref(), &cancelled);
            let failure = match verification {
                Ok(true) => None,
                Ok(false) => Some(Message::QuoteMismatch),
                Err(Failure::Limit) => Some(Message::QuoteLimit),
                Err(Failure::Changed) => Some(Message::Failed),
            };
            if let Some(failure) = failure {
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(failure);
                }
                return;
            }
        }
        let best = best.map(|(_, value)| value);
        // Move the existing winning layout rather than cloning or reshaping it.
        // Provisional quote prefixes and other renderers retain the old path.
        let archive_prepared = if folder.is_none()
            && cached_exact
            && recipe
                .as_ref()
                .is_some_and(|recipe| recipe.allows_prepared_navigation())
            && best
                .as_ref()
                .is_some_and(|(_, target)| cached_chapter == Some(target.chapter))
        {
            cached_probe
                .take()
                .and_then(|probe| probe.into_prepared_parley())
        } else {
            None
        };
        let folder = if let Some(proof) = folder {
            let prepared = if let Some((_, target)) = &best {
                drop(cached_probe.take());
                let prepared = recipe.as_ref().and_then(|recipe| {
                    recipe
                        .prepare_folder_chapter(&proof, target.chapter, &cancelled)
                        .ok()
                });
                let Some(prepared) = prepared else {
                    if !cancelled.load(Ordering::Relaxed) {
                        let _ = tx.send(Message::Failed);
                    }
                    return;
                };
                Some(prepared)
            } else {
                None
            };
            Some(FolderResult { proof, prepared })
        } else {
            None
        };
        let _ = tx.send(Message::Done(best, folder, archive_prepared));
    }
}

fn panel(reader: &Reader) -> (Rect, Rect) {
    let layout = reader.navigation_layout();
    let width = (layout.width - 80).clamp(200, 760);
    let height = (layout.height - 80).clamp(260, 380);
    let panel = Rect::new(
        (layout.width - width) / 2,
        (layout.height - height) / 2,
        width,
        height,
    );
    let button = Rect::new(panel.x + (width - 240) / 2, panel.bottom() - 120, 240, 100);
    (panel, button)
}

fn report_status(reader: &mut Reader, text: &str) -> Change {
    let message = reader
        .controls
        .interface_language
        .skim_text(text)
        .to_owned();
    reader.report(message)
}

impl Reader {
    pub(super) fn start_page_bookmark_search(&mut self, direction: Direction) -> crate::Result<()> {
        self.start_adjacent_mark_search(direction, false, false)
    }

    pub(super) fn start_adjacent_mark_search(
        &mut self,
        direction: Direction,
        include_highlights: bool,
        boundary: bool,
    ) -> crate::Result<()> {
        self.start_mark_search(direction, include_highlights, boundary, false, false)
    }

    pub(super) fn start_latest_mark_search(&mut self) -> crate::Result<()> {
        if self.latest_bookmark_action().is_none() {
            return Err("No saved marks".into());
        }
        self.start_mark_search(Direction::Next, true, false, false, true)
    }

    pub(super) fn start_skim_mark_search(
        &mut self,
        direction: Direction,
        boundary: bool,
    ) -> crate::Result<()> {
        if self.screen != Screen::Skim || self.skim.is_none() {
            return Err("Skim is not open".into());
        }
        self.start_mark_search(direction, true, boundary, true, false)
    }

    fn start_mark_search(
        &mut self,
        direction: Direction,
        include_highlights: bool,
        boundary: bool,
        preview: bool,
        latest: bool,
    ) -> crate::Result<()> {
        if self.page_bookmark_worker_busy.load(Ordering::Acquire) {
            return Err(self
                .controls
                .interface_language
                .skim_text("Previous bookmark search is still stopping")
                .into());
        }
        let book = self.open_book_id().ok_or("No open document")?;
        let original = self.current_history_location().ok_or("No open document")?;
        let original_last_visible = self
            .active
            .as_ref()
            .map_or(original.position.page, |active| {
                active.reading.last_visible_page()
            });
        let state = self.library.state(book).ok_or("This book is unavailable")?;
        let bookmarks_total = state.bookmarks.len();
        let marks_total = if !include_highlights {
            0
        } else if self.fixed.is_some() {
            state.pdf_highlights.len()
        } else {
            state.highlights.len()
        };
        let total = bookmarks_total
            .checked_add(marks_total)
            .ok_or("Too many saved marks")?;
        let (publication, chapter_total, recipe, fixed_pages, fixed_stamp, folder_marker) =
            if let Some(fixed) = &self.fixed {
                let stamp = with_storage(|| {
                    let path = self.library.absolute_path(&fixed.entry.path)?;
                    FileStamp::read(&path).ok_or_else(|| {
                        std::io::Error::other(
                            "This book cannot be verified for bookmark navigation",
                        )
                    })
                })
                .map_err(|_| {
                    self.controls
                        .interface_language
                        .skim_text("This book cannot be verified for bookmark navigation")
                        .to_owned()
                })?;
                (
                    None,
                    0,
                    None,
                    Some(fixed.reading.count()),
                    Some(stamp),
                    None,
                )
            } else {
                let reading = &self.active.as_ref().ok_or("No open document")?.reading;
                let recipe = reading.config.search_layout_recipe(&reading.probe);
                let folder_marker = crate::epub_members::is_folder_book(&reading.config.book)
                    .then(|| reading.config.book.clone());
                let verified = if folder_marker.is_some() {
                    recipe.has_verified_fonts()
                } else {
                    recipe.has_verified_sources()
                };
                if !verified {
                    return Err(self
                        .controls
                        .interface_language
                        .skim_text("This book cannot be verified for bookmark navigation")
                        .into());
                }
                (
                    Some(reading.probe.book),
                    reading.probe.chapter_count(),
                    Some(Arc::new(recipe)),
                    None,
                    None,
                    folder_marker,
                )
            };
        let now = Instant::now();
        self.page_bookmark_search = Some(Search {
            book,
            screen: self.screen,
            original,
            original_last_visible,
            direction,
            boundary,
            latest,
            publication,
            fixed_pages,
            fixed_stamp,
            recipe,
            folder_marker,
            folder_proof: None,
            folder_prepared: None,
            archive_prepared: None,
            staged: None,
            total,
            bookmarks_total,
            marks_total,
            include_highlights,
            preview,
            chapter_total,
            chapter_cursor: 0,
            chapter_paths: HashMap::new(),
            entries: Vec::new(),
            snapshot: None,
            bytes: 0,
            phase: Phase::Chapters,
            cursor: 0,
            progress: 0,
            best: None,
            receiver: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            invalidated: false,
            deadline: now,
            next_paint: now + PAINT,
        });
        Ok(())
    }

    pub(super) fn draw_page_bookmark_search(&self, canvas: &mut Canvas) {
        let Some(search) = &self.page_bookmark_search else {
            return;
        };
        let language = self.controls.interface_language;
        let (panel, cancel) = panel(self);
        canvas.popup(panel);
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 24, panel.w - 72, 64),
            language.skim_text(if search.include_highlights {
                "Finding saved mark"
            } else {
                "Finding page bookmark"
            }),
            BODY.bold(),
        );
        let status = if language.is_english() {
            format!(
                "Checked {} of {}",
                search.progress.min(search.total),
                search.total
            )
        } else {
            format!(
                "Đã kiểm tra {} / {}",
                search.progress.min(search.total),
                search.total
            )
        };
        canvas.text(
            Rect::new(panel.x + 36, panel.y + 108, panel.w - 72, 64),
            &status,
            CAPTION,
        );
        canvas.button(cancel, language.skim_text("Cancel"), false, false);
    }

    pub(super) fn page_bookmark_search_input(&mut self, input: Input) -> Option<Change> {
        self.page_bookmark_search.as_ref()?;
        let (popup, cancel) = panel(self);
        Some(match input {
            Input::PageBack => {
                self.page_bookmark_search = None;
                Change::Partial
            }
            Input::Tap(x, y) if cancel.contains(x, y) || !popup.contains(x, y) => {
                self.page_bookmark_search = None;
                Change::Partial
            }
            _ => Change::None,
        })
    }

    pub(super) fn poll_page_bookmark_search(&mut self, now: Instant) -> Option<Change> {
        let mut search = self.page_bookmark_search.take()?;
        if now < search.deadline {
            self.page_bookmark_search = Some(search);
            return None;
        }
        if search.invalidated {
            return Some(report_status(
                self,
                "The book or bookmarks changed; try again",
            ));
        }
        if self.screen != search.screen
            || self.open_book_id() != Some(search.book)
            || self.current_history_location().as_ref() != Some(&search.original)
            || self.library.state(search.book).is_none_or(|book| {
                book.bookmarks.len() != search.bookmarks_total
                    || (search.include_highlights
                        && if search.fixed_pages.is_some() {
                            book.pdf_highlights.len() != search.marks_total
                        } else {
                            book.highlights.len() != search.marks_total
                        })
            })
        {
            return Some(report_status(
                self,
                "The book or bookmarks changed; try again",
            ));
        }
        let same_provider = if let Some(pages) = search.fixed_pages {
            self.fixed
                .as_ref()
                .is_some_and(|fixed| fixed.reading.count() == pages)
        } else {
            self.fixed.is_none()
                && self.active.as_ref().is_some_and(|active| {
                    Some(active.reading.probe.book) == search.publication
                        && active.reading.probe.chapter_count() == search.chapter_total
                })
        };
        if !same_provider {
            return Some(report_status(self, "The book changed; try again"));
        }
        if matches!(search.phase, Phase::Validate)
            && (search.cursor == 0 || search.cursor.saturating_add(SLICE) >= search.total)
            && let Some(expected) = &search.recipe
        {
            let Some(active) = &self.active else {
                return Some(report_status(self, "The book changed; try again"));
            };
            if active
                .reading
                .config
                .search_layout_recipe(&active.reading.probe)
                != **expected
            {
                return Some(report_status(
                    self,
                    "The book or reading settings changed; try again",
                ));
            }
        }
        if matches!(search.phase, Phase::Validate)
            && (search.cursor == 0 || search.cursor.saturating_add(SLICE) >= search.total)
            && let Some(expected) = &search.fixed_stamp
        {
            let actual = self.fixed.as_ref().and_then(|fixed| {
                with_storage(|| {
                    let path = self.library.absolute_path(&fixed.entry.path)?;
                    Ok(FileStamp::read(&path))
                })
                .ok()
                .flatten()
            });
            if actual.as_ref() != Some(expected) {
                return Some(report_status(self, "The book changed; try again"));
            }
        }
        let mut finished = false;
        let mut failure = None;
        match search.phase {
            Phase::Chapters => {
                if search.recipe.is_some()
                    && let Some(active) = &self.active
                {
                    let chapters = active.reading.probe.chapter_entries();
                    if chapters.len() != search.chapter_total {
                        return Some(report_status(self, "The book changed; try again"));
                    }
                    let end = search
                        .chapter_cursor
                        .saturating_add(SLICE)
                        .min(chapters.len());
                    for (index, chapter) in chapters
                        .iter()
                        .enumerate()
                        .take(end)
                        .skip(search.chapter_cursor)
                    {
                        if !search.chapter_paths.contains_key(&chapter.href) {
                            search.bytes = search
                                .bytes
                                .saturating_add(chapter.href.len())
                                .saturating_add(std::mem::size_of::<(String, usize)>())
                                .saturating_add(64);
                            if search.bytes > SNAPSHOT_BYTES {
                                failure = Some("Bookmarks exceed the search memory limit");
                                break;
                            }
                            search.chapter_paths.insert(chapter.href.clone(), index);
                        }
                    }
                    search.chapter_cursor = end;
                }
                if search.chapter_cursor == search.chapter_total {
                    search.phase = Phase::Snapshot;
                    search.cursor = 0;
                }
            }
            Phase::Snapshot => {
                let Some(book) = self.library.state(search.book) else {
                    return Some(report_status(self, "The book changed; try again"));
                };
                let end = search.cursor.saturating_add(SLICE).min(search.total);
                let overhead =
                    std::mem::size_of::<Entry>().saturating_add(std::mem::size_of::<usize>() + 32);
                for index in search.cursor..end {
                    let (saved, position, extra, valid_fixed) = if index < search.bookmarks_total {
                        let Some(mark) = book.bookmarks.get(index) else {
                            failure = Some("Bookmarks changed; try again");
                            break;
                        };
                        let anchor_bytes =
                            mark.position.anchor.as_ref().map_or(0, |a| a.chapter.len());
                        let bytes = anchor_bytes
                            .saturating_mul(2)
                            .saturating_add(mark.label.len())
                            .saturating_add(mark.note.as_ref().map_or(0, String::len))
                            .saturating_add(mark.chapter_title.as_ref().map_or(0, String::len));
                        if search.bytes.saturating_add(overhead).saturating_add(bytes)
                            > SNAPSHOT_BYTES
                        {
                            failure = Some("Bookmarks exceed the search memory limit");
                            break;
                        }
                        (
                            SavedMark::Bookmark(mark.clone()),
                            mark.position.clone(),
                            bytes,
                            true,
                        )
                    } else if search.fixed_pages.is_some() {
                        let Some(mark) = book.pdf_highlights.get(index - search.bookmarks_total)
                        else {
                            failure = Some("Saved marks changed; try again");
                            break;
                        };
                        let mut bytes = mark
                            .text
                            .len()
                            .saturating_add(mark.note.as_ref().map_or(0, String::len))
                            .saturating_add(mark.chapter_title.as_ref().map_or(0, String::len))
                            .saturating_add(std::mem::size_of_val(mark.bounds.as_slice()))
                            .saturating_add(std::mem::size_of_val(mark.continuation.as_slice()));
                        for part in &mark.continuation {
                            bytes =
                                bytes.saturating_add(std::mem::size_of_val(part.bounds.as_slice()));
                        }
                        if search.bytes.saturating_add(overhead).saturating_add(bytes)
                            > SNAPSHOT_BYTES
                        {
                            failure = Some("Bookmarks exceed the search memory limit");
                            break;
                        }
                        let valid = mark.is_valid()
                            && mark
                                .continuation
                                .iter()
                                .all(|part| part.page < search.fixed_pages.unwrap_or(0));
                        let position = ReadingPosition {
                            chapter: 0,
                            page: mark.page,
                            anchor: None,
                        };
                        (SavedMark::Fixed(mark.clone()), position, bytes, valid)
                    } else {
                        let Some(mark) = book.highlights.get(index - search.bookmarks_total) else {
                            failure = Some("Saved marks changed; try again");
                            break;
                        };
                        let anchor = &mark.selection.start;
                        let bytes = anchor
                            .chapter
                            .len()
                            .saturating_mul(2)
                            .saturating_add(mark.selection.end.chapter.len())
                            .saturating_add(mark.selection.text.len())
                            .saturating_add(mark.note.as_ref().map_or(0, String::len))
                            .saturating_add(mark.chapter_title.as_ref().map_or(0, String::len));
                        if search.bytes.saturating_add(overhead).saturating_add(bytes)
                            > SNAPSHOT_BYTES
                        {
                            failure = Some("Bookmarks exceed the search memory limit");
                            break;
                        }
                        let position = ReadingPosition {
                            chapter: 0,
                            page: 0,
                            anchor: Some(anchor.clone()),
                        };
                        (SavedMark::Reflow(mark.clone()), position, bytes, true)
                    };
                    search.bytes = search
                        .bytes
                        .saturating_add(std::mem::size_of::<Entry>())
                        .saturating_add(std::mem::size_of::<usize>() + 32)
                        .saturating_add(extra);
                    if search.bytes > SNAPSHOT_BYTES {
                        failure = Some("Bookmarks exceed the search memory limit");
                        break;
                    }
                    let chapter = if search.recipe.is_none() {
                        (valid_fixed && position.chapter == 0 && position.anchor.is_none())
                            .then_some(0)
                    } else if let Some(anchor) = &position.anchor {
                        (Some(anchor.book) == search.publication)
                            .then(|| search.chapter_paths.get(&anchor.chapter).copied())
                            .flatten()
                    } else {
                        (position.chapter < search.chapter_total).then_some(position.chapter)
                    };
                    search.entries.push(Entry {
                        saved,
                        position,
                        chapter,
                    });
                }
                search.cursor = end;
                if failure.is_none() && end == search.total {
                    search.chapter_paths.clear();
                    let snapshot = Arc::new(std::mem::take(&mut search.entries));
                    let snapshot_worker = Arc::clone(&snapshot);
                    let recipe = search.recipe.clone();
                    let folder_marker = search.folder_marker.clone();
                    let book = search.book;
                    let cancelled = Arc::clone(&search.cancelled);
                    let (tx, rx) = mpsc::sync_channel(2);
                    let publication = search.publication;
                    let fixed_pages = search.fixed_pages;
                    let direction = search.direction;
                    let boundary = search.boundary;
                    let latest = search.latest;
                    let current = search.original.position.clone();
                    let current_last_visible = search.original_last_visible;
                    if self.page_bookmark_worker_busy.swap(true, Ordering::AcqRel) {
                        failure = Some("Previous bookmark search is still stopping");
                    } else {
                        let busy = Arc::clone(&self.page_bookmark_worker_busy);
                        match std::thread::Builder::new()
                            .name("reader-bookmark-search".into())
                            .spawn(move || {
                                let _guard = WorkerGuard(busy);
                                worker(
                                    recipe,
                                    folder_marker,
                                    book,
                                    publication,
                                    fixed_pages,
                                    direction,
                                    boundary,
                                    latest,
                                    current,
                                    current_last_visible,
                                    snapshot_worker,
                                    cancelled,
                                    tx,
                                );
                            }) {
                            Ok(_) => {
                                search.snapshot = Some(snapshot);
                                search.receiver = Some(rx);
                                search.phase = Phase::Worker;
                                search.progress = 0;
                            }
                            Err(_) => {
                                self.page_bookmark_worker_busy
                                    .store(false, Ordering::Release);
                                failure = Some("Unable to start bookmark search");
                            }
                        }
                    }
                }
            }
            Phase::Worker => {
                if search.receiver.is_none() {
                    return Some(report_status(self, "Bookmark search stopped"));
                }
                for _ in 0..8 {
                    let event = search
                        .receiver
                        .as_ref()
                        .expect("checked receiver")
                        .try_recv();
                    match event {
                        Ok(Message::Progress(value)) => search.progress = value,
                        Ok(Message::Done(best, folder, prepared)) => {
                            search.best = best;
                            search.archive_prepared = prepared;
                            if let Some(folder) = folder {
                                search.folder_proof = Some(folder.proof);
                                search.folder_prepared = folder.prepared;
                            }
                            search.receiver = None;
                            search.cursor = 0;
                            search.phase = Phase::Validate;
                            break;
                        }
                        Ok(Message::Failed) => {
                            failure = Some("A bookmarked chapter could not be checked");
                            break;
                        }
                        Ok(Message::QuoteMismatch) => {
                            failure = Some("The saved highlight no longer matches this document");
                            break;
                        }
                        Ok(Message::QuoteLimit) => {
                            failure = Some("The saved highlight exceeds the verification limit");
                            break;
                        }
                        Ok(Message::FolderVerified(_)) => {
                            failure = Some("Bookmark search stopped");
                            break;
                        }
                        Ok(Message::FolderUnavailable) => {
                            failure = Some("Folder book changed or cannot be safely checked");
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            failure = Some("Bookmark search stopped");
                            break;
                        }
                    }
                }
            }
            Phase::Validate => {
                let Some(snapshot) = &search.snapshot else {
                    return Some(report_status(self, "Bookmark search stopped"));
                };
                let Some(book) = self.library.state(search.book) else {
                    return Some(report_status(self, "The book changed; try again"));
                };
                let end = search.cursor.saturating_add(SLICE).min(search.total);
                if (search.cursor..end)
                    .any(|index| !snapshot[index].matches(book, index, search.bookmarks_total))
                {
                    failure = Some("Saved marks changed; try again");
                }
                search.cursor = end;
                if end == search.total && failure.is_none() {
                    if search.folder_marker.is_some() {
                        if search.folder_proof.is_none() {
                            failure = Some("Folder book changed or cannot be safely checked");
                        } else if let Some((_, target)) = &search.best {
                            let Some(prepared) = search.folder_prepared.take() else {
                                return Some(report_status(
                                    self,
                                    "Folder book changed or cannot be safely checked",
                                ));
                            };
                            match self.stage_folder_page_bookmark(
                                target,
                                search.recipe.as_deref(),
                                prepared,
                            ) {
                                Ok(staged) => search.staged = Some(staged),
                                Err(error) => return Some(self.report(error)),
                            }
                            search.phase = Phase::StartFolderFinal;
                        } else {
                            search.phase = Phase::StartFolderFinal;
                        }
                    } else {
                        finished = true;
                    }
                }
            }
            Phase::StartFolderFinal => {
                if !self.page_bookmark_worker_busy.swap(true, Ordering::AcqRel) {
                    let (Some(recipe), Some(proof)) =
                        (search.recipe.clone(), search.folder_proof.clone())
                    else {
                        self.page_bookmark_worker_busy
                            .store(false, Ordering::Release);
                        return Some(report_status(
                            self,
                            "Folder book changed or cannot be safely checked",
                        ));
                    };
                    let cancelled = Arc::clone(&search.cancelled);
                    let busy = Arc::clone(&self.page_bookmark_worker_busy);
                    let (tx, rx) = mpsc::sync_channel(1);
                    match std::thread::Builder::new()
                        .name("reader-folder-final".into())
                        .spawn(move || {
                            let _guard = WorkerGuard(busy);
                            let valid = recipe.verify_folder_proof(&proof, &cancelled).is_ok();
                            if !cancelled.load(Ordering::Relaxed) {
                                let _ = tx.send(Message::FolderVerified(valid));
                            }
                        }) {
                        Ok(_) => {
                            search.receiver = Some(rx);
                            search.phase = Phase::FolderFinal;
                        }
                        Err(_) => {
                            self.page_bookmark_worker_busy
                                .store(false, Ordering::Release);
                            failure = Some("Unable to start bookmark search");
                        }
                    }
                }
            }
            Phase::FolderFinal => {
                let Some(receiver) = &search.receiver else {
                    return Some(report_status(self, "Bookmark search stopped"));
                };
                match receiver.try_recv() {
                    Ok(Message::FolderVerified(true)) => {
                        let Some(active) = &self.active else {
                            return Some(report_status(self, "The book changed; try again"));
                        };
                        if search.recipe.as_deref()
                            != Some(
                                &active
                                    .reading
                                    .config
                                    .search_layout_recipe(&active.reading.probe),
                            )
                        {
                            failure = Some("The book or reading settings changed; try again");
                        } else {
                            finished = true;
                        }
                    }
                    Ok(Message::FolderVerified(false)) => {
                        failure = Some("Folder book changed or cannot be safely checked")
                    }
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                        failure = Some("Bookmark search stopped")
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        if let Some(error) = failure {
            return Some(report_status(self, error));
        }
        if finished {
            let Some((_, target)) = search.best.take() else {
                if search.latest {
                    return Some(self.report("The latest saved mark cannot be verified"));
                }
                let message = match (search.direction, search.include_highlights) {
                    (Direction::Previous, true) => "No earlier saved mark",
                    (Direction::Next, true) => "No later saved mark",
                    (Direction::Previous, false) => "No earlier page bookmark",
                    (Direction::Next, false) => "No later page bookmark",
                };
                return Some(self.report(self.controls.interface_language.skim_text(message)));
            };
            let result = if search.folder_marker.is_some() {
                match search.staged.take() {
                    Some(staged) => self.commit_staged_folder_bookmark(
                        staged,
                        &target,
                        &search.original,
                        search.recipe.as_deref(),
                        search.preview,
                    ),
                    None => Err("Folder book changed or cannot be safely checked".into()),
                }
            } else {
                self.commit_verified_page_bookmark(
                    &target,
                    &search.original,
                    search.recipe.as_deref(),
                    search.fixed_stamp.as_ref(),
                    search.archive_prepared.take(),
                    search.preview,
                )
            };
            return Some(match result {
                Ok(change) => change,
                Err(error) => self.report(error),
            });
        }
        search.deadline = now
            + if matches!(search.phase, Phase::Worker | Phase::FolderFinal) {
                Duration::from_millis(200)
            } else {
                TICK
            };
        let repaint = now >= search.next_paint;
        if repaint {
            search.next_paint = now + PAINT;
        }
        self.page_bookmark_search = Some(search);
        repaint.then_some(Change::Partial)
    }

    fn stage_folder_page_bookmark(
        &self,
        target: &ReadingPosition,
        expected_recipe: Option<&SearchLayoutRecipe>,
        prepared: crate::session::PreparedFolderChapter,
    ) -> crate::Result<Reading> {
        let reading = &self.active.as_ref().ok_or("No open book")?.reading;
        let staged = Reading::restore_prepared_folder(
            reading.config.clone(),
            target.chapter,
            target.page,
            target.anchor.as_ref(),
            prepared,
        )?;
        let recipe = staged.config.search_layout_recipe(&staged.probe);
        if staged.chapter != target.chapter
            || staged.page > target.page
            || staged.last_visible_page() < target.page
            || target
                .anchor
                .as_ref()
                .is_some_and(|anchor| staged.probe.page_for(anchor) != Some(target.page))
            || expected_recipe != Some(&recipe)
        {
            return Err("The bookmarked page changed; try again".into());
        }
        Ok(staged)
    }

    fn commit_staged_folder_bookmark(
        &mut self,
        staged: Reading,
        target: &ReadingPosition,
        original: &HistoryLocation,
        expected_recipe: Option<&SearchLayoutRecipe>,
        preview: bool,
    ) -> crate::Result<Change> {
        if preview && (self.screen != Screen::Skim || self.skim.is_none()) {
            return Err("Skim is not open".into());
        }
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        if expected_recipe != Some(&reading.config.search_layout_recipe(&reading.probe))
            || staged.chapter != target.chapter
            || staged.page > target.page
            || staged.last_visible_page() < target.page
            || target
                .anchor
                .as_ref()
                .is_some_and(|anchor| staged.probe.page_for(anchor) != Some(target.page))
        {
            return Err("The bookmarked page changed; try again".into());
        }
        *reading = staged;
        self.selection = None;
        self.result_selection = None;
        self.search_result = None;
        if preview {
            return Ok(self.skim_finish_shortcut(Some(original.position.clone())));
        }
        self.show(Screen::Book);
        self.record_position();
        if self.current_history_location().as_ref() != Some(original) {
            self.push_history(original.clone());
        }
        Ok(Change::Full)
    }

    fn commit_verified_page_bookmark(
        &mut self,
        target: &ReadingPosition,
        original: &HistoryLocation,
        expected_recipe: Option<&SearchLayoutRecipe>,
        expected_fixed: Option<&FileStamp>,
        prepared: Option<crate::PreparedParleyChapter>,
        preview: bool,
    ) -> crate::Result<Change> {
        if preview && (self.screen != Screen::Skim || self.skim.is_none()) {
            return Err("Skim is not open".into());
        }
        if let Some(fixed) = &self.fixed {
            let current = with_storage(|| {
                let path = self.library.absolute_path(&fixed.entry.path)?;
                Ok(FileStamp::read(&path))
            })?;
            if current.as_ref() != expected_fixed {
                return Err("The bookmarked page changed; try again".into());
            }
            if preview {
                self.fixed
                    .as_mut()
                    .ok_or("No open book")?
                    .reading
                    .jump_to(target.page, None)?;
                return Ok(self.skim_finish_shortcut(Some(original.position.clone())));
            }
            self.follow_fixed(target.page, None)?;
            return Ok(Change::Full);
        }
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        let config = reading.config.clone();
        let restored = if let Some(prepared) = prepared {
            Reading::restore_prepared(
                config.clone(),
                target.chapter,
                target.page,
                target.anchor.as_ref(),
                prepared,
            )
            .or_else(|_| {
                Reading::restore_with_scroll_top(
                    config,
                    target.chapter,
                    target.page,
                    target.anchor.as_ref(),
                    None,
                )
            })?
        } else {
            Reading::restore_with_scroll_top(
                config,
                target.chapter,
                target.page,
                target.anchor.as_ref(),
                None,
            )?
        };
        let restored_recipe = restored.config.search_layout_recipe(&restored.probe);
        if restored.chapter != target.chapter
            || restored.page > target.page
            || restored.last_visible_page() < target.page
            || target
                .anchor
                .as_ref()
                .is_some_and(|anchor| restored.probe.page_for(anchor) != Some(target.page))
            || !restored_recipe.has_verified_sources()
            || expected_recipe != Some(&restored_recipe)
        {
            return Err(self
                .controls
                .interface_language
                .skim_text("The bookmarked page changed; try again")
                .into());
        }
        *reading = restored;
        self.selection = None;
        self.result_selection = None;
        self.search_result = None;
        if preview {
            return Ok(self.skim_finish_shortcut(Some(original.position.clone())));
        }
        self.show(Screen::Book);
        self.record_position();
        if self.current_history_location().as_ref() != Some(original) {
            self.push_history(original.clone());
        }
        Ok(Change::Full)
    }
}
