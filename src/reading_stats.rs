//! Time spent on a book's reading screen, kept only in Reader's local storage.

pub(crate) mod history;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::{Duration, Instant};

use primitive_app::with_storage;
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;
const MAX_BOOKS: usize = 10_000;
const MAX_STATE_BYTES: u64 = 512 * 1024;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Deserialize, Serialize)]
struct SavedStats {
    version: u32,
    seconds_read: BTreeMap<u64, u64>,
}

pub(crate) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
    if bytes.len() as u64 > MAX_STATE_BYTES { return Err(invalid_state()); }
    let saved: SavedStats = serde_json::from_slice(bytes)?;
    if saved.version != VERSION || saved.seconds_read.len() > MAX_BOOKS {
        return Err(invalid_state());
    }
    serde_json::to_value(saved).map_err(io::Error::other)
}

#[derive(Clone, Copy)]
struct ActiveReading {
    book: u64,
    since: Instant,
    deadline: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct ReadingStats {
    totals: BTreeMap<u64, Duration>,
    active: Option<ActiveReading>,
    dirty: bool,
}

impl ReadingStats {
    pub(crate) fn recorded_totals(&self) -> BTreeMap<u64, u64> {
        self.totals
            .iter()
            .map(|(&id, time)| (id, time.as_secs()))
            .collect()
    }

    pub fn load(path: &Path) -> Self {
        let totals = with_storage(|| {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES {
                return Err(invalid_state());
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(MAX_STATE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_STATE_BYTES {
                return Err(invalid_state());
            }
            let disk: SavedStats = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if disk.version != VERSION || disk.seconds_read.len() > MAX_BOOKS {
                return Err(invalid_state());
            }
            Ok(disk
                .seconds_read
                .into_iter()
                .filter_map(|(book, seconds)| {
                    (seconds > 0).then_some((book, Duration::from_secs(seconds)))
                })
                .collect())
        })
        .unwrap_or_default();
        Self {
            totals,
            ..Self::default()
        }
    }

    /// A repeated notification for the same book leaves the active segment running.
    pub fn set_active(&mut self, book: Option<u64>, now: Instant) {
        if self.active.map(|active| active.book) == book {
            return;
        }
        if let Some((previous, total)) = self.projected_total(now) {
            self.remember(previous, total);
        }
        self.active = book
            .filter(|id| self.totals.len() < MAX_BOOKS || self.totals.contains_key(id))
            .map(|book| ActiveReading {
                book,
                since: now,
                deadline: now.checked_add(CHECKPOINT_INTERVAL),
            });
    }

    /// Drops the uncheckpointed segment when storage returns after USB use.
    pub fn discard_active(&mut self) {
        self.active = None;
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.active.and_then(|active| active.deadline)
    }

    pub fn seconds_read(&self, book: u64, now: Instant) -> u64 {
        self.projected_total(now)
            .filter(|(active_book, _)| *active_book == book)
            .map_or_else(|| self.total(book), |(_, total)| total)
            .as_secs()
    }

    /// Saves accrued whole seconds at the scheduled checkpoint or on a forced transition.
    /// An unsuccessful write leaves the active segment available for retry or USB discard.
    pub fn checkpoint(&mut self, path: &Path, now: Instant, force: bool) -> io::Result<bool> {
        if !force && self.deadline().is_none_or(|deadline| now < deadline) {
            return Ok(false);
        }
        let projected = self.projected_total(now);
        if let Some(active) = &mut self.active {
            active.deadline = now.max(active.since).checked_add(CHECKPOINT_INTERVAL);
        }
        let changed = self.dirty
            || projected.is_some_and(|(book, total)| total.as_secs() != self.total(book).as_secs());
        if changed {
            let mut seconds_read = self
                .totals
                .iter()
                .filter_map(|(&book, total)| {
                    let seconds = total.as_secs();
                    (seconds > 0).then_some((book, seconds))
                })
                .collect::<BTreeMap<_, _>>();
            if let Some((book, total)) = projected.filter(|(_, total)| total.as_secs() > 0) {
                seconds_read.insert(book, total.as_secs());
            }
            let saved = SavedStats {
                version: VERSION,
                seconds_read,
            };
            let bytes = serde_json::to_vec(&saved).map_err(io::Error::other)?;
            if bytes.len() as u64 > MAX_STATE_BYTES {
                return Err(invalid_state());
            }
            primitive_app::save(path, &bytes)?;
        }
        if let Some((book, total)) = projected {
            self.remember(book, total);
        }
        if let Some(active) = &mut self.active {
            active.since = active.since.max(now);
        }
        self.dirty = false;
        Ok(changed)
    }

    fn total(&self, book: u64) -> Duration {
        self.totals.get(&book).copied().unwrap_or_default()
    }

    fn projected_total(&self, now: Instant) -> Option<(u64, Duration)> {
        let active = self.active?;
        let elapsed = now.saturating_duration_since(active.since);
        Some((active.book, self.total(active.book).saturating_add(elapsed)))
    }

    fn remember(&mut self, book: u64, total: Duration) {
        let previous = self.total(book);
        self.dirty |= total.as_secs() != previous.as_secs();
        if total > previous {
            self.totals.insert(book, total);
        }
    }
}

fn invalid_state() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Reading statistics exceed the supported format or limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_only_active_time_and_discards_unwritten_usb_interval() {
        let directory =
            std::env::temp_dir().join(format!("primitiveos-reading-stats-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("reading-stats.json");
        let blocked = directory.join("missing-parent/reading-stats.json");
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut stats = ReadingStats::load(&path);

        stats.set_active(Some(17), at(0));
        stats.set_active(Some(17), at(20));
        assert_eq!(stats.deadline(), Some(at(60)));
        assert_eq!(stats.seconds_read(17, at(59)), 59);
        assert!(!stats.checkpoint(&path, at(59), false).unwrap());
        assert!(stats.checkpoint(&path, at(60), false).unwrap());
        assert_eq!(ReadingStats::load(&path).seconds_read(17, at(96)), 60);

        stats.set_active(None, at(64));
        assert!(stats.checkpoint(&path, at(64), true).unwrap());
        assert_eq!(stats.deadline(), None);
        stats.set_active(Some(23), at(100));
        assert!(stats.checkpoint(&blocked, at(160), false).is_err());
        assert_eq!(stats.deadline(), Some(at(220)));
        stats.discard_active();
        assert_eq!(stats.seconds_read(23, at(180)), 0);
        stats.set_active(Some(23), at(180));
        stats.set_active(None, at(183));
        assert!(stats.checkpoint(&blocked, at(183), true).is_err());
        assert!(stats.checkpoint(&path, at(183), true).unwrap());
        let reloaded = ReadingStats::load(&path);
        assert_eq!(reloaded.seconds_read(17, at(500)), 64);
        assert_eq!(reloaded.seconds_read(23, at(500)), 3);

        fs::write(
            &path,
            br#"{"version":1,"seconds_read":{"17":18446744073709551615}}"#,
        )
        .unwrap();
        let mut saturated = ReadingStats::load(&path);
        saturated.set_active(Some(17), at(0));
        assert_eq!(saturated.seconds_read(17, at(500)), u64::MAX);
        assert!(!saturated.checkpoint(&path, at(60), false).unwrap());
        fs::write(&path, b"corrupt").unwrap();
        assert_eq!(ReadingStats::load(&path).seconds_read(17, at(500)), 0);
        fs::remove_dir_all(directory).unwrap();
    }
}
