//! Background snapshots for exact screen pagination of packaged reflowable EPUBs.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use primitive_app::with_storage;

use super::{
    FileStamp, Generation, HeadingPages, PageSample, ScreenPageSources, ScreenPages,
    checked_prefix, heading_pages, same_spine,
};
use crate::epub::{Book, Chapter};
use crate::session::Reading;
use crate::{Engine, Probe, Result, Settings, user_hyphenation, user_styles};

const FIRST_FRAME_DELAY: Duration = Duration::from_millis(380);
const TYPOGRAPHY_DELAY: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(150);
const CHAPTER_YIELD: Duration = Duration::from_millis(20);
const STORAGE_CHUNK: usize = 128 * 1024;
pub(super) const ARCHIVE_LIMIT: u64 = 8 * 1024 * 1024;
pub(super) const FONT_LIMIT: u64 = 16 * 1024 * 1024;
pub(super) const FONT_COUNT_LIMIT: usize = 64;
const SPINE_LIMIT: usize = 192;
const CHAPTER_BYTES_LIMIT: u64 = 256 * 1024;
pub(super) const FIRMWARE_FONTS: &str = "/usr/java/lib/fonts";

#[derive(Default)]
pub(crate) struct Worker {
    waiting: Option<Waiting>,
    pending: Option<Pending>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    Visible,
    Typography,
}

struct Waiting {
    layout: ScreenPages,
    intent: Intent,
    after: Instant,
}

impl Waiting {
    fn matches(&self, reading: &Reading, intent: Intent) -> bool {
        Rc::ptr_eq(&self.layout.0, &reading.config.screen_pages.0) && self.intent == intent
    }
}

struct ChapterSample {
    index: usize,
    pagination: PageSample,
    headings: HeadingPages,
}

struct Pending {
    cancelled: Arc<AtomicBool>,
    generation: Rc<()>,
    poll_at: Instant,
    thread: JoinHandle<Result<Vec<ChapterSample>>>,
}

impl Worker {
    pub(crate) fn cancel(&mut self) {
        self.waiting = None;
        if let Some(pending) = &self.pending {
            pending.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn deadline(&self, request: Option<(&Reading, Intent)>) -> Option<Instant> {
        if let Some(pending) = &self.pending {
            return Some(pending.poll_at);
        }
        let (reading, intent) = request?;
        wants_background(reading).then(|| {
            self.waiting
                .as_ref()
                .filter(|waiting| waiting.matches(reading, intent))
                .map_or_else(Instant::now, |waiting| waiting.after)
        })
    }

    /// True once page numbers available on the current screen have changed.
    pub(crate) fn tick(&mut self, now: Instant, request: Option<(&Reading, Intent)>) -> bool {
        let reading = request.map(|(reading, _)| reading);
        if let Some(pending) = &mut self.pending {
            let current =
                reading.is_some_and(|reading| current_generation(reading, &pending.generation));
            if !current {
                pending.cancelled.store(true, Ordering::Relaxed);
            }
            if !pending.thread.is_finished() {
                if now >= pending.poll_at {
                    pending.poll_at = now + POLL_INTERVAL;
                }
                return false;
            }
            let pending = self.pending.take().unwrap();
            let result = pending
                .thread
                .join()
                .ok()
                .and_then(|received| received.ok());
            if pending.cancelled.load(Ordering::Relaxed) || !current {
                return false;
            }
            return reading
                .is_some_and(|reading| accept_result(reading, &pending.generation, result));
        }

        let Some((reading, intent)) = request.filter(|(reading, _)| wants_background(reading))
        else {
            self.waiting = None;
            return false;
        };
        if self
            .waiting
            .as_ref()
            .is_none_or(|waiting| !waiting.matches(reading, intent))
        {
            self.waiting = Some(Waiting {
                layout: reading.config.screen_pages.clone(),
                intent,
                after: now
                    + match intent {
                        Intent::Visible => FIRST_FRAME_DELAY,
                        Intent::Typography => TYPOGRAPHY_DELAY,
                    },
            });
        }
        if now < self.waiting.as_ref().unwrap().after {
            return false;
        }
        self.waiting = None;

        let mut slot = reading.config.screen_pages.0.borrow_mut();
        let probe = &reading.probe;
        let config = &reading.config;
        let Some(sources) = &probe.screen_page_sources else {
            return false;
        };
        if slot
            .as_ref()
            .is_none_or(|generation| !generation.matches(config, probe))
        {
            *slot = Some(Generation::new(config, probe, sources));
        }
        let generation = slot.as_mut().unwrap();
        let request = Request::new(reading, generation);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let thread = thread::Builder::new()
            .name("reader-screen-pages".into())
            .spawn(move || request.run(&worker_cancelled));
        match thread {
            Ok(thread) => {
                self.pending = Some(Pending {
                    generation: generation.id.clone(),
                    cancelled,
                    poll_at: now + POLL_INTERVAL,
                    thread,
                });
            }
            Err(error) => {
                generation.background_rejected = true;
                eprintln!("reader screen-page worker: {error}");
            }
        }
        false
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) fn wants_background(reading: &Reading) -> bool {
    let config = &reading.config;
    let probe = &reading.probe;
    let Some(sources) = &probe.screen_page_sources else {
        return false;
    };
    if probe.is_vertical()
        || config.scroll
        || config.facing_pages
        || config.hidden_flows.enabled
        || reading.hides_nonlinear()
        || config.settings != probe.settings
        || config.engine != Engine::Parley
        || probe.chapter_count() > SPINE_LIMIT
        || sources.book.len > ARCHIVE_LIMIT
        || sources.fonts.len() != config.fonts.len()
        || sources.fonts.len() > FONT_COUNT_LIMIT
        || config
            .fonts
            .iter()
            .zip(&sources.fonts)
            .filter(|(path, _)| path.parent() != Some(Path::new(FIRMWARE_FONTS)))
            .try_fold(0_u64, |size, (_, stamp)| size.checked_add(stamp.len))
            .is_none_or(|size| size > FONT_LIMIT)
    {
        return false;
    }
    config
        .screen_pages
        .0
        .borrow()
        .as_ref()
        .is_none_or(|generation| {
            !generation.matches(config, probe)
                || (!generation.rejected
                    && !generation.background_rejected
                    && generation.missing > 0)
        })
}

/// Check the layout and owner before considering a worker result. The result
/// stays out of the cache while the background task is in progress.
fn current_generation(reading: &Reading, id: &Rc<()>) -> bool {
    let slot = reading.config.screen_pages.0.borrow();
    slot.as_ref().is_some_and(|generation| {
        Rc::ptr_eq(&generation.id, id)
            && generation.matches(&reading.config, &reading.probe)
            && generation.missing > 0
            && !generation.rejected
    })
}

fn accept_result(reading: &Reading, id: &Rc<()>, result: Option<Vec<ChapterSample>>) -> bool {
    if !current_generation(reading, id) {
        return false;
    }
    let mut slot = reading.config.screen_pages.0.borrow_mut();
    let generation = slot.as_mut().unwrap();
    let Some(result) = result else {
        generation.background_rejected = true;
        return false;
    };
    if with_storage(|| Ok(ScreenPageSources::read(&reading.config)))
        .ok()
        .flatten()
        .as_ref()
        != Some(&generation.sources)
    {
        generation.background_rejected = true;
        return false;
    }
    let active = PageSample::new(&reading.probe);
    if result.iter().any(|part| {
        let ChapterSample {
            index,
            pagination: sample,
            ..
        } = part;
        sample.count == 0
            || generation
                .samples
                .get(*index)
                .is_none_or(|prior| prior.is_some_and(|prior| prior != *sample))
            || (*index == reading.chapter && *sample != active)
    }) {
        generation.background_rejected = true;
        return false;
    }
    let mut completed = generation.samples.clone();
    for part in &result {
        completed[part.index] = Some(part.pagination);
    }
    let Some(prefix) = checked_prefix(&completed) else {
        generation.background_rejected = true;
        return false;
    };
    generation.samples = completed;
    for part in result {
        generation.heading_pages[part.index] = Some(part.headings);
    }
    generation.missing = 0;
    generation.prefix = Some(prefix);
    reading.config.screen_pages.complete_typography();
    true
}

struct Request {
    book: PathBuf,
    publication: u64,
    spine: Vec<Chapter>,
    settings: Settings,
    fonts: Vec<PathBuf>,
    sources: ScreenPageSources,
    styles: user_styles::Snapshot,
    hyphenation: user_hyphenation::Rules,
    chapters: Vec<usize>,
    witness: (usize, PageSample),
}

impl Request {
    fn new(reading: &Reading, generation: &Generation) -> Self {
        let witness = (reading.chapter, PageSample::new(&reading.probe));
        Self {
            book: generation.book.clone(),
            publication: generation.publication,
            spine: generation.spine.clone(),
            settings: reading.config.settings,
            fonts: generation.fonts.clone(),
            sources: generation.sources.clone(),
            styles: generation.styles.clone(),
            hyphenation: (*generation.hyphenation).clone(),
            chapters: generation
                .samples
                .iter()
                .enumerate()
                .filter_map(|(index, sample)| {
                    (sample.is_none() || index == witness.0).then_some(index)
                })
                .collect(),
            witness,
        }
    }

    fn run(self, cancelled: &AtomicBool) -> Result<Vec<ChapterSample>> {
        let archive: Arc<[u8]> = read_snapshot(&self.book, &self.sources.book, cancelled)?.into();
        let fonts =
            crate::font_context_with_font_data(&self.fonts, Path::new(FIRMWARE_FONTS), |path| {
                let index = self
                    .fonts
                    .iter()
                    .position(|font| font == path)
                    .ok_or("The reading font changed during indexing")?;
                let bytes = read_snapshot(path, &self.sources.fonts[index], cancelled)?;
                Ok(parley::fontique::Blob::new(Arc::new(bytes)))
            })?;
        self.verify_sources(cancelled)?;
        let mut result = Vec::with_capacity(self.chapters.len());
        let mut book = Book::open_snapshot(archive)?;
        if book.revision() != self.publication
            || !book.supports_screen_page_count()
            || !same_spine(&self.spine, &book.chapters)
        {
            return Err("The EPUB edition changed during indexing".into());
        }
        if book
            .chapter_offsets()?
            .windows(2)
            .any(|ends| ends[1] - ends[0] > CHAPTER_BYTES_LIMIT)
        {
            return Err("An EPUB chapter exceeds the background pagination limit".into());
        }
        let mut opened = Some(book);
        let check = || Ok(check_cancelled(cancelled)?);
        // Check the visible chapter first. This also guards against an ostensibly
        // equivalent worker font context making different page breaks.
        let indices = std::iter::once(self.witness.0).chain(
            self.chapters
                .iter()
                .copied()
                .filter(|index| *index != self.witness.0),
        );
        for index in indices {
            check()?;
            let probe = Probe::layout_opened_book_cancellable(
                &mut opened,
                Engine::Parley,
                self.settings,
                index,
                None,
                &self.hyphenation,
                &self.styles,
                || Ok(fonts.clone()),
                &check,
            )?;
            if probe.is_vertical() {
                return Err("This EPUB uses vertical page layouts".into());
            }
            let sample = PageSample::new(&probe);
            if index == self.witness.0 && sample != self.witness.1 {
                return Err("The visible chapter's page count changed during indexing".into());
            }
            result.push(ChapterSample {
                index,
                pagination: sample,
                headings: heading_pages(&probe),
            });
            drop(probe);
            thread::park_timeout(CHAPTER_YIELD);
        }
        self.verify_sources(cancelled)?;
        Ok(result)
    }

    fn verify_sources(&self, cancelled: &AtomicBool) -> io::Result<()> {
        check_cancelled(cancelled)?;
        with_storage(|| {
            check_cancelled(cancelled)?;
            (ScreenPageSources::read_paths(&self.book, &self.fonts).as_ref() == Some(&self.sources))
                .then_some(())
                .ok_or_else(changed_file)
        })
    }
}

fn changed_file() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "An EPUB or reading font changed during indexing",
    )
}

pub(super) fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Screen-page counting cancelled",
        ))
    } else {
        Ok(())
    }
}

/// A read lease never keeps a file handle across chunks, storage export, or layout.
pub(super) fn read_snapshot(
    path: &Path,
    stamp: &FileStamp,
    cancelled: &AtomicBool,
) -> io::Result<Vec<u8>> {
    check_cancelled(cancelled)?;
    let mut bytes = vec![0; stamp.len as usize];
    for (index, chunk) in bytes.chunks_mut(STORAGE_CHUNK).enumerate() {
        check_cancelled(cancelled)?;
        with_storage(|| {
            check_cancelled(cancelled)?;
            let mut file = File::open(path)?;
            if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(stamp) {
                return Err(changed_file());
            }
            file.seek(SeekFrom::Start((index * STORAGE_CHUNK) as u64))?;
            file.read_exact(chunk)?;
            if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(stamp) {
                return Err(changed_file());
            }
            Ok(())
        })?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitive_app::App;

    #[test]
    fn screen_pages_background_snapshot_matches_normal_publisher_layouts() {
        for fixture in ["publisher-faces.epub", "field-notes-epub3.epub"] {
            let reading = Reading::open(super::super::tests::config(fixture)).unwrap();
            let slot = reading.config.screen_pages.0.borrow();
            let request = Request::new(&reading, slot.as_ref().unwrap());
            drop(slot);
            let counts = request.run(&AtomicBool::new(false)).unwrap();
            assert_eq!(counts.len(), reading.probe.chapter_count());
            for ChapterSample {
                index,
                pagination: sample,
                headings,
            } in counts
            {
                let loaded = reading.config.chapter(index).unwrap();
                assert!(
                    sample == PageSample::new(&loaded),
                    "{fixture} chapter {index}"
                );
                assert_eq!(
                    headings,
                    heading_pages(&loaded),
                    "{fixture} outline positions"
                );
            }
        }
    }

    #[test]
    fn screen_pages_background_discards_stale_work_on_visibility_reflow_and_usb() {
        let mut reading = Reading::open(super::super::tests::config("many-chapters.epub")).unwrap();
        let mut worker = Worker::default();
        let now = Instant::now();
        let begin = |worker: &mut Worker, reading: &Reading| {
            assert!(!worker.tick(now, Some((reading, Intent::Visible))));
            assert!(!worker.tick(now + FIRST_FRAME_DELAY, Some((reading, Intent::Visible))));
            assert!(worker.pending.is_some());
        };
        let drain = |worker: &mut Worker, reading: Option<&Reading>| {
            let timeout = Instant::now() + Duration::from_secs(4);
            while worker.pending.is_some() {
                assert!(Instant::now() < timeout);
                assert!(!worker.tick(
                    Instant::now(),
                    reading.map(|reading| (reading, Intent::Visible))
                ));
                thread::sleep(Duration::from_millis(5));
            }
        };

        begin(&mut worker, &reading);
        reading
            .reflow(Settings {
                font_gamma_index: 24,
                ..reading.config.settings
            })
            .unwrap();
        assert!(current_generation(
            &reading,
            &worker.pending.as_ref().unwrap().generation
        ));
        worker.cancel();
        drain(&mut worker, None);
        assert_eq!(reading.whole_book_screen_pages(), None);

        begin(&mut worker, &reading);
        reading
            .reflow(Settings {
                height: reading.config.settings.height - 48,
                ..reading.config.settings
            })
            .unwrap();
        drain(&mut worker, Some(&reading));
        assert_eq!(reading.whole_book_screen_pages(), None);

        begin(&mut worker, &reading);
        reading.storage_returned().unwrap();
        drain(&mut worker, Some(&reading));
        assert_eq!(reading.whole_book_screen_pages(), None);

        let placeholder = Reading::open(super::super::tests::config("many-chapters.epub")).unwrap();
        begin(&mut worker, &placeholder);
        drain(&mut worker, Some(&reading));
        assert_eq!(placeholder.whole_book_screen_pages(), None);
    }

    #[test]
    fn screen_pages_background_rejects_changed_and_cancelled_snapshot_reads() {
        let path = std::env::temp_dir().join(format!(
            "reader-pagination-snapshot-{}.epub",
            std::process::id()
        ));
        let source = vec![b'a'; STORAGE_CHUNK + 1];
        std::fs::write(&path, &source).unwrap();
        let stamp = FileStamp::read(&path).unwrap();
        let cancelled = AtomicBool::new(false);
        assert_eq!(read_snapshot(&path, &stamp, &cancelled).unwrap(), source);
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            read_snapshot(&path, &stamp, &cancelled).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        cancelled.store(false, Ordering::Relaxed);
        std::fs::write(&path, b"Different content").unwrap();
        let result = read_snapshot(&path, &stamp, &cancelled);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
