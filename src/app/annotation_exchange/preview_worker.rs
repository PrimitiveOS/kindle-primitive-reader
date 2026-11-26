//! Bounded, cancellable packaged-EPUB transfer preview. No saved state is mutated.
use super::*;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const MAX_MARKS: usize = 64;
const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(100);
const FALLBACK: &str = "This transfer needs the standard preview, which may take time";

struct DigestWriter {
    hash: ring::digest::Context,
    bytes: usize,
}
impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("Reading data is too large"))?;
        if self.bytes > MAX_STATE_BYTES {
            return Err(io::Error::other("Reading data exceeds the preview limit"));
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn state_digest(state: &BookState) -> Result<[u8; 32]> {
    let mut writer = DigestWriter {
        hash: ring::digest::Context::new(&ring::digest::SHA256),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, state)?;
    Ok(writer
        .hash
        .finish()
        .as_ref()
        .try_into()
        .map_err(|_| "Reading data cannot be checked")?)
}

#[derive(Clone)]
pub(super) struct Reviewed {
    state: [u8; 32],
    settings: Preferences,
    verified: Option<Verified>,
}
#[derive(Clone)]
struct Verified {
    book: u64,
    path: PathBuf,
    recipe: crate::session::SearchLayoutRecipe,
    source: super::super::bookmark_browser::other_epub::Proof,
    transfer: discovery::TransferProof,
    replacement: [u8; 32],
}
impl Reviewed {
    pub(super) fn matches(&self, reader: &Reader, book: u64) -> bool {
        reader.annotation_exchange.settings == self.settings
            && reader
                .library
                .state(book)
                .and_then(|state| state_digest(state).ok())
                == Some(self.state)
            && self.verified.as_ref().is_none_or(|verified| {
                verified.book == book
                    && reader
                        .current_document()
                        .is_some_and(|(entry, id)| id == book && entry.path == verified.path)
                    && reader.active.as_ref().is_some_and(|active| {
                        active
                            .reading
                            .config
                            .search_layout_recipe(&active.reading.probe)
                            == verified.recipe
                    })
                    && verified.source.revalidate().is_ok()
                    && verified
                        .transfer
                        .revalidate(&AtomicBool::new(false))
                        .is_ok()
            })
    }

    pub(super) fn matches_replacement(&self, state: &BookState) -> bool {
        self.verified
            .as_ref()
            .is_some_and(|verified| state_digest(state).ok() == Some(verified.replacement))
    }
}

struct Outcome {
    transfer: discovery::TransferProof,
    source: super::super::bookmark_browser::other_epub::Proof,
    digest: String,
    summary: Summary,
    imported: bool,
    replacement: [u8; 32],
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct Job {
    book: u64,
    path: PathBuf,
    origin: Option<super::super::HistoryLocation>,
    reviewed: Reviewed,
    recipe: crate::session::SearchLayoutRecipe,
    cancel: Arc<AtomicBool>,
    receiver: mpsc::Receiver<std::result::Result<Outcome, &'static str>>,
    deadline: Instant,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Job {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl Reader {
    pub(super) fn start_annotation_preview_background(&mut self) -> Result<()> {
        self.annotation_exchange.preview_job = None;
        self.show(Screen::AnnotationExchangePreview);
        self.annotation_exchange.preview = None;
        self.annotation_exchange.preview_issue = None;
        let prepare = (|| -> Result<_> {
            let active = self.active.as_ref().ok_or(FALLBACK)?;
            if active.entry.format != DocumentFormat::Epub
                || crate::epub_members::is_folder_book(&active.entry.path)
                || active.reading.probe.has_raw_layout()
                || active.reading.probe.in_page_notes.is_some()
                || !active.reading.probe.foreign_note_bands.is_empty()
            {
                return Err(FALLBACK.into());
            }
            let recipe = active
                .reading
                .config
                .search_layout_recipe(&active.reading.probe);
            if !recipe.has_verified_sources() || !recipe.allows_prepared_navigation() {
                return Err(FALLBACK.into());
            }
            let state = self.library.state(active.id).ok_or(FALLBACK)?;
            if state.bookmarks.len().saturating_add(state.highlights.len()) > MAX_MARKS {
                return Err(FALLBACK.into());
            }
            let reviewed = Reviewed {
                state: state_digest(state).map_err(|_| FALLBACK)?,
                settings: self.annotation_exchange.settings.clone(),
                verified: None,
            };
            Ok((
                active.id,
                active.entry.path.clone(),
                self.library.verification_root().to_owned(),
                self.current_history_location(),
                reviewed,
                recipe,
                state.clone(),
            ))
        })();
        let (book, path, root, origin, reviewed, recipe, state) = match prepare {
            Ok(value) => value,
            Err(_) => {
                self.annotation_exchange.preview_issue = Some(FALLBACK);
                return Ok(());
            }
        };
        if self
            .annotation_exchange
            .preview_busy
            .swap(true, Ordering::AcqRel)
        {
            self.annotation_exchange.preview_issue = Some("The previous preview is still stopping");
            return Ok(());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let busy = Arc::clone(&self.annotation_exchange.preview_busy);
        let worker_path = path.clone();
        let worker_reviewed = reviewed.clone();
        let worker_recipe = recipe.clone();
        let (tx, receiver) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("reader-transfer-preview".into())
            .spawn(move || {
                let _guard = Guard(busy);
                let result = run(
                    root,
                    worker_path,
                    book,
                    state,
                    worker_reviewed,
                    worker_recipe,
                    &worker_cancel,
                );
                if !worker_cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(result);
                }
            })
            .is_err()
        {
            self.annotation_exchange
                .preview_busy
                .store(false, Ordering::Release);
            self.annotation_exchange.preview_issue = Some("Unable to start the background preview");
            return Ok(());
        }
        self.annotation_exchange.preview_job = Some(Job {
            book,
            path,
            origin,
            reviewed,
            recipe,
            cancel,
            receiver,
            deadline: Instant::now() + POLL,
        });
        Ok(())
    }

    pub(in crate::app) fn annotation_preview_deadline(&self) -> Option<Instant> {
        self.annotation_exchange
            .preview_job
            .as_ref()
            .map(Job::deadline)
    }
    pub(in crate::app) fn cancel_annotation_preview(&mut self) {
        if self.annotation_exchange.preview_job.take().is_some()
            && self.screen == Screen::AnnotationExchangePreview
        {
            self.annotation_exchange.preview_issue = Some("Background preview cancelled");
        }
    }
    pub(in crate::app) fn poll_annotation_preview(&mut self, now: Instant) -> Option<Change> {
        let mut job = self.annotation_exchange.preview_job.take()?;
        if now < job.deadline {
            self.annotation_exchange.preview_job = Some(job);
            return None;
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                job.deadline = now + POLL;
                self.annotation_exchange.preview_job = Some(job);
                return None;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Background preview stopped"),
        };
        let current = self.screen == Screen::AnnotationExchangePreview
            && self.open_book_id() == Some(job.book)
            && self.current_history_location() == job.origin
            && self
                .current_document()
                .is_some_and(|(entry, id)| id == job.book && entry.path == job.path)
            && job.reviewed.matches(self, job.book)
            && self.active.as_ref().is_some_and(|active| {
                active
                    .reading
                    .config
                    .search_layout_recipe(&active.reading.probe)
                    == job.recipe
            });
        if !current {
            self.annotation_exchange.preview_issue =
                Some("The book or reading data changed; preview again");
            return Some(Change::Partial);
        }
        match result {
            Ok(output)
                if output.source.revalidate().is_ok()
                    && output.transfer.revalidate(&job.cancel).is_ok() =>
            {
                self.annotation_exchange.preview_issue = None;
                let mut reviewed = job.reviewed.clone();
                reviewed.verified = Some(Verified {
                    book: job.book,
                    path: job.path.clone(),
                    recipe: job.recipe.clone(),
                    source: output.source,
                    transfer: output.transfer,
                    replacement: output.replacement,
                });
                self.annotation_exchange.preview = Some(Preview {
                    id: job.book,
                    digest: output.digest,
                    summary: output.summary,
                    imported: output.imported,
                    confirming: false,
                    background: Some(reviewed),
                });
            }
            Ok(_) => {
                self.annotation_exchange.preview_issue =
                    Some("The book or transfer changed; preview again")
            }
            Err(message) => self.annotation_exchange.preview_issue = Some(message),
        }
        Some(Change::Partial)
    }
}

fn run(
    root: PathBuf,
    path: PathBuf,
    id: u64,
    mut state: BookState,
    reviewed: Reviewed,
    recipe: crate::session::SearchLayoutRecipe,
    cancel: &AtomicBool,
) -> std::result::Result<Outcome, &'static str> {
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err("Background preview cancelled")
        } else {
            Ok(())
        }
    };
    check()?;
    let (transfer, bytes, digest, checksum) = discovery::TransferProof::capture(
        &root,
        &path,
        reviewed.settings.folder.as_deref(),
        cancel,
    )
    .map_err(|_| "The transfer could not be verified")?;
    let (source, epub) =
        super::super::bookmark_browser::other_epub::Proof::capture(root, path, id, cancel)
            .map_err(|_| "The EPUB could not be verified within the background limit")?;
    check()?;
    let snapshot =
        exchange::decode(&bytes).map_err(|_| "The annotation transfer could not be read")?;
    if snapshot.fixed
        || snapshot
            .fingerprint
            .as_deref()
            .is_some_and(|value| !value.eq_ignore_ascii_case(&checksum))
    {
        return Err("The transfer belongs to a different document edition or format");
    }
    let imported = reviewed.settings.device.as_ref() == Some(&snapshot.device)
        || state.annotation_exchange_digest.as_ref() == Some(&digest);
    let mut summary = Summary::default();
    if !imported {
        if snapshot.marks.len() > MAX_MARKS {
            return Err(FALLBACK);
        }
        let mut book = Book::open_snapshot(epub).map_err(|_| "The EPUB could not be read")?;
        let mut points = Vec::new();
        let mut highlights = Vec::new();
        let mut unavailable = snapshot.unavailable
            + state
                .bookmarks
                .iter()
                .filter(|mark| marks::raw_bookmark(mark))
                .count()
            + state
                .highlights
                .iter()
                .filter(|mark| marks::raw_highlight(mark))
                .count();
        let fixed = snapshot.fixed;
        let incomplete = snapshot.incomplete;
        let datetime = snapshot.datetime;
        for mark in snapshot.marks {
            check()?;
            match marks::load_reflow_mark(mark, &mut book) {
                Some(marks::TextMark::Point(point)) => points.push(point),
                Some(marks::TextMark::Range(start, end, mark, details)) => {
                    if start.chapter != end.chapter
                        || start.raw_caret.is_some()
                        || end.raw_caret.is_some()
                    {
                        return Err(FALLBACK);
                    }
                    let index = book.chapter_index(&start.chapter).ok_or(FALLBACK)?;
                    let (probe, exact) = recipe
                        .chapter_with_quote_status(index, cancel)
                        .map_err(|_| FALLBACK)?;
                    if !exact {
                        return Err(FALLBACK);
                    }
                    if probe.has_raw_layout()
                        || probe.in_page_notes.is_some()
                        || !probe.foreign_note_bands.is_empty()
                    {
                        return Err(FALLBACK);
                    }
                    if probe.compare_text_anchors(&start, &end) != Some(std::cmp::Ordering::Less) {
                        return Err(FALLBACK);
                    }
                    let selection = probe.select_source_range(&start, &end).filter(|selection| {
                        selection.text.len() <= crate::library::MAX_HIGHLIGHT_TEXT_BYTES
                            && !selection.text.trim().is_empty()
                            && marks::same_words(&selection.text, &mark.text)
                    });
                    if let Some(selection) = selection {
                        highlights.push(Highlight {
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
                None => unavailable += 1,
            }
        }
        let keep_all = reviewed.settings.keep_all
            || (snapshot.fingerprint.is_none() && reviewed.settings.folder.is_some());
        summary = marks::merge_transfer(
            &mut state,
            fixed,
            incomplete,
            datetime,
            keep_all,
            points,
            highlights,
            Vec::new(),
            unavailable,
        );
        if summary.unavailable == 0 {
            state.annotation_exchange_digest = Some(digest.clone());
        }
    }
    let replacement = state_digest(&state).map_err(|_| FALLBACK)?;
    check()?;
    source
        .revalidate()
        .map_err(|_| "The EPUB changed; preview again")?;
    transfer
        .revalidate(cancel)
        .map_err(|_| "The transfer changed; preview again")?;
    check()?;
    Ok(Outcome {
        transfer,
        source,
        digest,
        summary,
        imported,
        replacement,
    })
}
