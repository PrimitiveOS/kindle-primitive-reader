//! Per-book page dwell samples used by the reading-time footer.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MIN_SECONDS: u64 = 5;
const MAX_SECONDS: u64 = 120;
const DEFAULT_SECONDS: f64 = 60.0;
const MAX_PAGES: usize = 16_384;
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Deserialize, Serialize)]
struct Stored {
    version: u8,
    pages: BTreeMap<u64, u8>,
}

pub(crate) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
    let stored: Stored = serde_json::from_slice(bytes)?;
    if bytes.len() as u64 > MAX_BYTES || stored.version != 1
        || stored.pages.len() > MAX_PAGES
        || stored.pages.values().any(|&n| !(MIN_SECONDS..=MAX_SECONDS).contains(&u64::from(n))) {
        return Err(io::Error::other("The reading-pace backup is invalid"));
    }
    serde_json::to_value(stored).map_err(io::Error::other)
}

#[derive(Default)]
struct Samples {
    pages: BTreeMap<u64, u8>,
    total: u32,
    dirty: bool,
    read_only: bool,
}

impl Samples {
    fn load(path: &Path) -> Self {
        let read = primitive_app::with_storage(|| {
            let meta = fs::symlink_metadata(path)?;
            if !meta.is_file() || meta.len() > MAX_BYTES {
                return Err(io::Error::other("Invalid reading-pace file"));
            }
            let mut bytes = Vec::with_capacity(meta.len() as usize);
            fs::File::open(path)?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(io::Error::other("Reading-pace file is too large"));
            }
            let stored: Stored = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if stored.version != 1
                || stored.pages.len() > MAX_PAGES
                || stored
                    .pages
                    .values()
                    .any(|&n| !(MIN_SECONDS..=MAX_SECONDS).contains(&u64::from(n)))
            {
                return Err(io::Error::other("Unrecognized reading-pace data"));
            }
            Ok(stored.pages)
        });
        match read {
            Ok(pages) => Self {
                total: pages.values().map(|&n| u32::from(n)).sum(),
                pages,
                ..Self::default()
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self {
                read_only: true,
                ..Self::default()
            },
        }
    }

    fn record(&mut self, visit: &Visit, now: Instant) {
        let elapsed = visit.elapsed(now).as_secs() / u64::from(visit.visible_pages);
        if elapsed < MIN_SECONDS
            || (!self.pages.contains_key(&visit.page) && self.pages.len() >= MAX_PAGES)
        {
            return;
        }
        let total = (u64::from(visit.baseline) + elapsed.min(MAX_SECONDS)).min(MAX_SECONDS) as u8;
        let previous = self.pages.get(&visit.page).copied().unwrap_or_default();
        if total > previous {
            self.pages.insert(visit.page, total);
            self.total += u32::from(total - previous);
            self.dirty = !self.read_only;
        }
    }

    fn save(&mut self, path: &Path) -> io::Result<()> {
        if self.dirty {
            #[derive(Serialize)]
            struct Record<'a> {
                version: u8,
                pages: &'a BTreeMap<u64, u8>,
            }
            let bytes = serde_json::to_vec(&Record {
                version: 1,
                pages: &self.pages,
            })
            .map_err(io::Error::other)?;
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("Missing reading-pace directory"))?;
            primitive_app::with_storage(|| fs::create_dir_all(parent))?;
            primitive_app::save(path, &bytes)?;
            self.dirty = false;
        }
        Ok(())
    }
}

struct Visit {
    book: u64,
    page: u64,
    baseline: u8,
    visible_pages: u8,
    accumulated: Duration,
    since: Option<Instant>,
}

impl Visit {
    fn elapsed(&self, now: Instant) -> Duration {
        self.accumulated.saturating_add(
            self.since
                .map_or(Duration::ZERO, |start| now.saturating_duration_since(start)),
        )
    }
}

pub(crate) struct ReadingPace {
    directory: PathBuf,
    books: BTreeMap<u64, Samples>,
    visit: Option<Visit>,
}

impl ReadingPace {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            books: BTreeMap::new(),
            visit: None,
        }
    }

    fn file(directory: &Path, book: u64) -> PathBuf {
        directory.join(format!("{book:016x}.json"))
    }

    pub fn rate(&self, book: u64) -> f64 {
        self.books
            .get(&book)
            .filter(|s| !s.pages.is_empty())
            .map_or(DEFAULT_SECONDS, |s| {
                f64::from(s.total) / s.pages.len() as f64
            })
    }

    pub fn set_page(&mut self, page: Option<(u64, u64, u8)>, now: Instant) {
        if let Some(visit) = self.visit.as_mut() {
            if page == Some((visit.book, visit.page, visit.visible_pages)) {
                visit.since.get_or_insert(now);
                return;
            }
            if visit.since.is_some() {
                visit.accumulated = visit.elapsed(now);
                visit.since = None;
                if let Some(samples) = self.books.get_mut(&visit.book) {
                    samples.record(visit, now);
                }
            }
        }
        let Some((book, page, visible_pages)) = page else {
            return;
        };
        if !self.books.contains_key(&book) {
            // A failed save stays in memory for a future checkpoint. Completed
            // clean books need no memory while another title is open.
            for (&id, samples) in &mut self.books {
                let _ = samples.save(&Self::file(&self.directory, id));
            }
            self.books.retain(|_, samples| samples.dirty);
            if self.books.len() >= 4 {
                // Keep unwritten samples for retry when storage is unavailable.
                self.visit = None;
                return;
            }
            let file = Self::file(&self.directory, book);
            self.books.insert(book, Samples::load(&file));
        }
        let baseline = self.books[&book].pages.get(&page).copied().unwrap_or(0);
        self.visit = Some(Visit {
            book,
            page,
            baseline,
            visible_pages: visible_pages.max(1),
            accumulated: Duration::ZERO,
            since: Some(now),
        });
    }

    pub fn checkpoint(&mut self, now: Instant) -> io::Result<()> {
        if let Some(visit) = &self.visit
            && let Some(samples) = self.books.get_mut(&visit.book)
        {
            samples.record(visit, now);
        }
        for (&id, samples) in &mut self.books {
            samples.save(&Self::file(&self.directory, id))?;
        }
        let current = self.visit.as_ref().map(|v| v.book);
        self.books.retain(|&book, _| Some(book) == current);
        Ok(())
    }

    pub fn storage_returned(&mut self) {
        self.visit = None;
        self.books.clear();
    }
}

impl crate::Probe {
    /// A layout owns its cache. Changing a font, chapter or viewport
    /// creates a fresh layout, while panning on a page reuses its source position.
    pub(crate) fn pace_key(&self, chapter: usize, page: usize) -> u64 {
        if let Some((current, key)) = self.pace_page.get()
            && current == page
        {
            return key;
        }
        let anchor = self.page_anchor(page);
        let (block, offset, illustration) =
            anchor
                .as_ref()
                .map_or((u64::MAX, page as u64, u64::MAX), |a| {
                    (
                        a.block as u64,
                        a.offset as u64,
                        a.illustration.map_or(u64::MAX, |index| index as u64),
                    )
                });
        let mut bytes = [0u8; 32];
        for (part, value) in
            bytes
                .chunks_exact_mut(8)
                .zip([chapter as u64, block, offset, illustration])
        {
            part.copy_from_slice(&value.to_le_bytes());
        }
        let key = crate::library::fingerprint_bytes(&bytes, b"layout-page");
        self.pace_page.set(Some((page, key)));
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "reader-pace-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn replays_quick_turns_pause_resume_rereads_limits_and_distinct_books() {
        let dir = temp("replay");
        let start = Instant::now();
        let at = |s| start + Duration::from_secs(s);
        let mut p = ReadingPace::new(dir.clone());
        assert_eq!(p.rate(1), 60.);
        p.set_page(Some((1, 10, 1)), at(0));
        p.set_page(None, at(3));
        p.set_page(Some((1, 10, 1)), at(603));
        p.set_page(Some((1, 11, 1)), at(620)); // 20 seconds of reading and 600 paused.
        assert_eq!(p.rate(1), 20.);
        p.set_page(Some((1, 12, 1)), at(623)); // Three seconds is a skim.
        p.checkpoint(at(683)).unwrap();
        p.checkpoint(at(698)).unwrap(); // Successive checkpoints update the original visit once.
        p.set_page(Some((1, 10, 1)), at(873)); // A long uninterrupted visit counts 120.
        p.set_page(Some((1, 13, 1)), at(888)); // Re-reading adds 15 to the first page.
        p.set_page(Some((1, 14, 1)), at(893));
        assert!((p.rate(1) - 160. / 3.).abs() < 1e-10);
        p.set_page(Some((2, 0, 2)), at(893));
        assert_eq!(p.rate(2), 60.);
        p.set_page(None, at(953));
        assert_eq!(p.rate(2), 30.); // A minute on a two-page spread.
        p.checkpoint(at(953)).unwrap();
        let mut restored = ReadingPace::new(dir.clone());
        restored.set_page(Some((1, 10, 1)), at(1000));
        assert!((restored.rate(1) - 160. / 3.).abs() < 1e-10);
        restored.set_page(None, at(1400)); // Revisits cannot exceed 120 seconds for one page.
        assert!((restored.rate(1) - 245. / 3.).abs() < 1e-10);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retries_atomic_pace_saves_and_preserves_unrecognized_files() {
        let dir = temp("retry");
        let start = Instant::now();
        let mut p = ReadingPace::new(dir.clone());
        p.set_page(Some((3, 1, 1)), start);
        fs::write(&dir, b"occupied").unwrap();
        let later = start + Duration::from_secs(19);
        assert!(p.checkpoint(later).is_err());
        fs::remove_file(&dir).unwrap();
        p.checkpoint(later).unwrap();
        let file = ReadingPace::file(&dir, 3);
        assert_eq!(Samples::load(&file).total, 19);
        let future = br#"{"version":4,"pages":{}}"#;
        fs::write(&file, future).unwrap();
        p.storage_returned();
        p.set_page(Some((3, 2, 1)), later);
        p.checkpoint(later + Duration::from_secs(40)).unwrap();
        assert_eq!(fs::read(file).unwrap(), future);
        fs::remove_dir_all(dir).unwrap();
    }
}
