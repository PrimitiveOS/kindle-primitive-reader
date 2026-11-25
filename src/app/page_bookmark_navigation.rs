//! Cancellable, source-verified page-bookmark navigation.
use super::{
    HistoryLocation, Reader, ReadingPosition, Screen, chapter_navigation::Direction, with_storage,
};
use crate::library::Bookmark;
use crate::session::screen_pages::FileStamp;
use crate::session::{Reading, SearchLayoutRecipe};
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
use std::collections::HashMap;
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
struct Entry {
    bookmark: Bookmark,
    chapter: Option<usize>,
}
type Snapshot = Arc<Vec<Entry>>;
type Candidate = (usize, ReadingPosition);
enum Message {
    Progress(usize),
    Done(Option<Candidate>),
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
}

pub(super) struct Search {
    book: u64,
    screen: Screen,
    original: HistoryLocation,
    direction: Direction,
    publication: Option<u64>,
    fixed_pages: Option<usize>,
    fixed_stamp: Option<FileStamp>,
    recipe: Option<Arc<SearchLayoutRecipe>>,
    total: usize,
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

fn worker(
    recipe: Option<Arc<SearchLayoutRecipe>>,
    publication: Option<u64>,
    fixed_pages: Option<usize>,
    direction: Direction,
    current: ReadingPosition,
    snapshot: Snapshot,
    cancelled: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Message>,
) {
    let mut order: Vec<usize> = snapshot
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
        .collect();
    order.sort_unstable_by_key(|&index| (snapshot[index].chapter.unwrap_or(usize::MAX), index));
    let mut cached_chapter = None;
    let mut cached_probe = None;
    let mut best: Option<((usize, usize, usize, usize), Candidate)> = None;
    for (processed, index) in order.into_iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        if processed % 256 == 255 && tx.send(Message::Progress(processed + 1)).is_err() {
            return;
        }
        let entry = &snapshot[index];
        let chapter = entry.chapter.unwrap_or(0);
        let mut target = entry.bookmark.position.clone();
        let key = if let Some(recipe) = &recipe {
            if cached_chapter != Some(chapter) {
                match recipe.chapter(chapter, &cancelled) {
                    Ok(probe) => {
                        cached_probe = Some(probe);
                        cached_chapter = Some(chapter);
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
        let beyond = match direction {
            Direction::Previous => page < current_page,
            Direction::Next => page > current_page,
        };
        if beyond
            && best.as_ref().is_none_or(|(prior, _)| match direction {
                Direction::Previous => key > *prior,
                Direction::Next => key < *prior,
            })
        {
            best = Some((key, (index, target)));
        }
    }
    if !cancelled.load(Ordering::Relaxed) {
        let _ = tx.send(Message::Done(best.map(|(_, value)| value)));
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
    let button = Rect::new(panel.x + (width - 240) / 2, panel.bottom() - 120, 240, 90);
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
        if self.page_bookmark_worker_busy.load(Ordering::Acquire) {
            return Err(self
                .controls
                .interface_language
                .skim_text("Previous bookmark search is still stopping")
                .into());
        }
        let book = self.open_book_id().ok_or("No open document")?;
        let original = self.current_history_location().ok_or("No open document")?;
        let total = self
            .library
            .state(book)
            .ok_or("This book is unavailable")?
            .bookmarks
            .len();
        let (publication, chapter_total, recipe, fixed_pages, fixed_stamp) = if let Some(fixed) =
            &self.fixed
        {
            let stamp = with_storage(|| {
                let path = self.library.absolute_path(&fixed.entry.path)?;
                FileStamp::read(&path).ok_or_else(|| {
                    std::io::Error::other("This book cannot be verified for bookmark navigation")
                })
            })
            .map_err(|_| {
                self.controls
                    .interface_language
                    .skim_text("This book cannot be verified for bookmark navigation")
                    .to_owned()
            })?;
            (None, 0, None, Some(fixed.reading.count()), Some(stamp))
        } else {
            let reading = &self.active.as_ref().ok_or("No open document")?.reading;
            let recipe = reading.config.search_layout_recipe(&reading.probe);
            if !recipe.has_verified_sources() {
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
            )
        };
        let now = Instant::now();
        self.page_bookmark_search = Some(Search {
            book,
            screen: self.screen,
            original,
            direction,
            publication,
            fixed_pages,
            fixed_stamp,
            recipe,
            total,
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
            language.skim_text("Finding page bookmark"),
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
            || self
                .library
                .state(search.book)
                .is_none_or(|book| book.bookmarks.len() != search.total)
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
                for bookmark in &book.bookmarks[search.cursor..end] {
                    let position = &bookmark.position;
                    let extra = std::mem::size_of::<Entry>()
                        .saturating_add(std::mem::size_of::<usize>() + 32)
                        .saturating_add(
                            position
                                .anchor
                                .as_ref()
                                .map_or(0, |anchor| anchor.chapter.len()),
                        )
                        .saturating_add(bookmark.label.len())
                        .saturating_add(bookmark.note.as_ref().map_or(0, String::len))
                        .saturating_add(bookmark.chapter_title.as_ref().map_or(0, String::len));
                    search.bytes = search.bytes.saturating_add(extra);
                    if search.bytes > SNAPSHOT_BYTES {
                        failure = Some("Bookmarks exceed the search memory limit");
                        break;
                    }
                    let chapter = if search.recipe.is_none() {
                        (position.chapter == 0 && position.anchor.is_none()).then_some(0)
                    } else if let Some(anchor) = &position.anchor {
                        (Some(anchor.book) == search.publication)
                            .then(|| search.chapter_paths.get(&anchor.chapter).copied())
                            .flatten()
                    } else {
                        (position.chapter < search.chapter_total).then_some(position.chapter)
                    };
                    search.entries.push(Entry {
                        bookmark: bookmark.clone(),
                        chapter,
                    });
                }
                search.cursor = end;
                if failure.is_none() && end == search.total {
                    search.chapter_paths.clear();
                    let snapshot = Arc::new(std::mem::take(&mut search.entries));
                    let snapshot_worker = Arc::clone(&snapshot);
                    let recipe = search.recipe.clone();
                    let cancelled = Arc::clone(&search.cancelled);
                    let (tx, rx) = mpsc::sync_channel(2);
                    let publication = search.publication;
                    let fixed_pages = search.fixed_pages;
                    let direction = search.direction;
                    let current = search.original.position.clone();
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
                                    publication,
                                    fixed_pages,
                                    direction,
                                    current,
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
                        Ok(Message::Done(best)) => {
                            search.best = best;
                            search.receiver = None;
                            search.cursor = 0;
                            search.phase = Phase::Validate;
                            break;
                        }
                        Ok(Message::Failed) => {
                            failure = Some("A bookmarked chapter could not be checked");
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
                if book.bookmarks[search.cursor..end]
                    .iter()
                    .zip(&snapshot[search.cursor..end])
                    .any(|(mark, saved)| mark != &saved.bookmark)
                {
                    failure = Some("Bookmarks changed; try again");
                }
                search.cursor = end;
                if end == search.total && failure.is_none() {
                    finished = true;
                }
            }
        }
        if let Some(error) = failure {
            return Some(report_status(self, error));
        }
        if finished {
            let Some((_, target)) = search.best.take() else {
                let message = match search.direction {
                    Direction::Previous => "No earlier page bookmark",
                    Direction::Next => "No later page bookmark",
                };
                return Some(self.report(self.controls.interface_language.skim_text(message)));
            };
            let result = self.commit_verified_page_bookmark(
                &target,
                &search.original,
                search.recipe.as_deref(),
                search.fixed_stamp.as_ref(),
            );
            return Some(match result {
                Ok(()) => Change::Full,
                Err(error) => self.report(error),
            });
        }
        search.deadline = now
            + if matches!(search.phase, Phase::Worker) {
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

    fn commit_verified_page_bookmark(
        &mut self,
        target: &ReadingPosition,
        original: &HistoryLocation,
        expected_recipe: Option<&SearchLayoutRecipe>,
        expected_fixed: Option<&FileStamp>,
    ) -> crate::Result<()> {
        if let Some(fixed) = &self.fixed {
            let current = with_storage(|| {
                let path = self.library.absolute_path(&fixed.entry.path)?;
                Ok(FileStamp::read(&path))
            })?;
            if current.as_ref() != expected_fixed {
                return Err("The bookmarked page changed; try again".into());
            }
            return self.follow_fixed(target.page, None);
        }
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        let restored = Reading::restore_with_scroll_top(
            reading.config.clone(),
            target.chapter,
            target.page,
            target.anchor.as_ref(),
            None,
        )?;
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
        self.show(Screen::Book);
        self.record_position();
        if self.current_history_location().as_ref() != Some(original) {
            self.push_history(original.clone());
        }
        Ok(())
    }
}
