//! Bounded asynchronous preparation of a single EPUB quote prefix.
//!
//! The UI owns scheduling, epoch changes, and repaint. This worker never
//! holds an open device-storage handle across a lease or shares the UI's Rc.

use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use primitive_app::with_storage;

use super::ForegroundQuotes;
use crate::book_style::pseudo::quotes::{self, Checkpoints, ExactPrefix};
use crate::epub::{Book, Chapter, MAX_ARCHIVE};
use crate::epub_leased::LeasedReader;
use crate::epub_members::Members;
use crate::session::screen_pages::{
    FileStamp,
    background::{ARCHIVE_LIMIT, check_cancelled, read_snapshot},
};
use crate::{Result, Settings, user_styles};

#[cfg(test)]
mod tests;

const LEASED_DEADLINE: Duration = Duration::from_secs(5);

/// The caller must pass the effective profile used by the active layout,
/// including note-layout and fallback settings, rather than generic defaults.
#[derive(Clone)]
pub(crate) struct Request {
    pub(crate) epoch: u64,
    pub(crate) path: PathBuf,
    pub(crate) stamp: FileStamp,
    pub(crate) edition: u64,
    pub(crate) spine: Vec<Chapter>,
    pub(crate) chapter: usize,
    pub(crate) settings: Settings,
    pub(crate) styles: user_styles::Snapshot,
    pub(crate) shaping_language: Option<parley::Language>,
    pub(crate) main_language: String,
}

impl Request {
    /// Capture the marker owned by the committed foreground layout. The
    /// caller's epoch must change whenever the layout or its owner changes.
    pub(crate) fn from_need(
        epoch: u64,
        path: &Path,
        cache: &ForegroundQuotes,
        need: &crate::book_style::pseudo::quotes::QuoteNeed,
        edition: u64,
        spine: &[Chapter],
        chapter: usize,
        settings: Settings,
    ) -> Option<Self> {
        let slot = cache.0.borrow();
        let saved = slot.as_ref()?;
        if saved.edition != edition || need.chapter != chapter || need.settings != settings {
            return None;
        }
        Some(Self {
            epoch,
            path: path.to_owned(),
            stamp: saved.stamp.clone(),
            edition,
            spine: spine.to_vec(),
            chapter,
            settings,
            styles: need.styles.clone(),
            shaping_language: need.shaping_language,
            main_language: need.main_language.clone(),
        })
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.path == other.path
            && self.stamp == other.stamp
            && self.edition == other.edition
            && same_spine(&self.spine, &other.spine)
            && self.chapter == other.chapter
            && self.settings == other.settings
            && self.styles == other.styles
            && self.shaping_language == other.shaping_language
            && self.main_language == other.main_language
    }

    fn verify(&self) -> Result<u64> {
        Ok(with_storage(|| {
            let metadata = std::fs::metadata(&self.path)?;
            if metadata.len() > MAX_ARCHIVE {
                return Err(std::io::Error::other(
                    "This EPUB exceeds the quote preparation archive limit",
                ));
            }
            if FileStamp::from_metadata(&metadata).as_ref() != Some(&self.stamp) {
                return Err(std::io::Error::other(
                    "The EPUB changed during quote preparation",
                ));
            }
            Ok(metadata.len())
        })?)
    }

    fn run(self, cancelled: &Arc<AtomicBool>) -> Result<Prepared> {
        check_cancelled(cancelled)?;
        let deadline = Instant::now() + LEASED_DEADLINE;
        let len = self.verify()?;
        let (mut book, health) = if len <= ARCHIVE_LIMIT {
            let bytes: Arc<[u8]> = read_snapshot(&self.path, &self.stamp, cancelled)?.into();
            check_cancelled(cancelled)?;
            (Book::open_snapshot(bytes)?, None)
        } else {
            let (reader, health) =
                LeasedReader::open(&self.path, self.stamp.clone(), cancelled.clone(), deadline)?;
            let archive = crate::epub_zip::open_leased(reader)?;
            health.check()?;
            let mut book = Book::from_archive(Members::Zip(archive))?;
            health.check()?;
            book.quote_prefixes = Some(Box::new(Checkpoints::snapshot()));
            (book, Some(health))
        };
        if book.revision() != self.edition || !same_spine(&self.spine, &book.chapters) {
            return Err("The EPUB edition changed during quote preparation".into());
        }
        let exact = quotes::prepare_exact(
            &mut book,
            self.chapter,
            self.settings,
            &self.styles,
            self.shaping_language,
            &self.main_language,
            &|| {
                check_cancelled(cancelled)?;
                if let Some(health) = &health {
                    health.check()?;
                }
                Ok(())
            },
        )?;
        check_cancelled(cancelled)?;
        if let Some(health) = &health {
            health.verify()?;
        }
        self.verify()?;
        Ok(Prepared {
            request: self,
            exact,
        })
    }
}

fn same_spine(left: &[Chapter], right: &[Chapter]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.href == b.href && a.nonlinear == b.nonlinear && a.is_svg == b.is_svg)
}

pub(crate) struct Job {
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Prepared>>>,
}

impl Job {
    pub(crate) fn start(request: Request) -> Result<Self> {
        // Reject unsupported or replaced sources before creating a worker.
        request.verify()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let running = cancelled.clone();
        let thread = thread::Builder::new()
            .name("reader-quote-prefix".into())
            .spawn(move || request.run(&running))?;
        Ok(Self {
            cancelled,
            thread: Some(thread),
        })
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub(crate) fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Never blocks the event loop. Poll `finished` before collecting.
    pub(crate) fn collect(mut self) -> Result<Prepared> {
        if !self.finished() {
            return Err("Quote preparation is still running".into());
        }
        let thread = self
            .thread
            .take()
            .ok_or("Quote preparation was already collected")?;
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("Quote preparation was cancelled".into());
        }
        thread
            .join()
            .map_err(|_| "Quote preparation worker stopped")?
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) struct Prepared {
    request: Request,
    exact: ExactPrefix,
}

impl Prepared {
    /// Install only after the UI supplies a newly captured current request.
    /// A false result is stale and must not trigger a repaint.
    pub(crate) fn install(self, current: &Request, cache: &ForegroundQuotes) -> Result<bool> {
        if !self.request.matches(current) {
            return Ok(false);
        }
        self.request.verify()?;
        let mut slot = cache.0.borrow_mut();
        let Some(saved) = slot.as_mut() else {
            return Ok(false);
        };
        if saved.stamp != self.request.stamp || saved.edition != self.request.edition {
            return Ok(false);
        }
        saved.checkpoints.install_exact(self.exact);
        Ok(true)
    }
}

/// Make the stamp used by the active foreground layout available to callers
/// constructing a request; the worker validates it again under storage lease.
pub(crate) fn current_stamp(cache: &ForegroundQuotes, path: &Path) -> Option<FileStamp> {
    let stamp = cache.stamp()?;
    with_storage(|| Ok((FileStamp::read(path).as_ref() == Some(&stamp)).then_some(stamp)))
        .ok()
        .flatten()
}
