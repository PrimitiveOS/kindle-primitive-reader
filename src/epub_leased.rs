//! Lease-scoped random reads for a background EPUB ZIP reader.
//!
//! This source owns only a path and a logical offset. It never keeps a file
//! descriptor after leaving `with_storage`, so USB export can proceed between
//! chunks. Callers must inspect `Health` even after optional-resource errors:
//! ZIP consumers may intentionally ignore missing navigation or stylesheets.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
};
use std::time::Instant;

use primitive_app::with_storage;

use crate::epub::MAX_ARCHIVE;
use crate::session::screen_pages::FileStamp;

const CHUNK_BYTES: usize = 64 * 1024;
const MAX_READ_BYTES: u64 = 32 * 1024 * 1024;
const MAX_OPERATIONS: usize = 65_536;

#[derive(Clone, Copy)]
struct Limits {
    bytes: u64,
    operations: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: MAX_READ_BYTES,
            operations: MAX_OPERATIONS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Fault {
    Cancelled = 1,
    Deadline = 2,
    Storage = 3,
    Changed = 4,
    Io = 5,
    Budget = 6,
    Seek = 7,
}

impl Fault {
    fn from_byte(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Cancelled,
            2 => Self::Deadline,
            3 => Self::Storage,
            4 => Self::Changed,
            5 => Self::Io,
            6 => Self::Budget,
            7 => Self::Seek,
            _ => return None,
        })
    }

    fn error(self) -> io::Error {
        let (kind, message) = match self {
            Self::Cancelled => (io::ErrorKind::Other, "EPUB reading was cancelled"),
            Self::Deadline => (io::ErrorKind::Other, "EPUB reading exceeded its deadline"),
            Self::Storage => (io::ErrorKind::WouldBlock, "EPUB storage is unavailable"),
            Self::Changed => (io::ErrorKind::Other, "The EPUB changed during reading"),
            Self::Io => (
                io::ErrorKind::Other,
                "The EPUB could not be read completely",
            ),
            Self::Budget => (io::ErrorKind::Other, "The EPUB reading budget was exceeded"),
            Self::Seek => (
                io::ErrorKind::InvalidInput,
                "The EPUB seek position is invalid",
            ),
        };
        // In particular cancellation is not ErrorKind::Interrupted: standard
        // read_exact/read_to_end can retry Interrupted indefinitely.
        io::Error::new(kind, message)
    }
}

struct Shared {
    path: PathBuf,
    stamp: FileStamp,
    len: u64,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    limits: Limits,
    bytes: AtomicU64,
    operations: AtomicUsize,
    fault: AtomicU8,
}

/// Remains available after the ZIP parser takes ownership of the reader.
#[derive(Clone)]
pub(crate) struct Health(Arc<Shared>);

impl Health {
    fn fail(&self, fault: Fault) -> io::Error {
        let _ = self
            .0
            .fault
            .compare_exchange(0, fault as u8, Ordering::AcqRel, Ordering::Acquire);
        Fault::from_byte(self.0.fault.load(Ordering::Acquire))
            .unwrap_or(fault)
            .error()
    }

    /// Check after ZIP opening and every optional-resource or chapter pass.
    pub(crate) fn check(&self) -> io::Result<()> {
        if let Some(fault) = Fault::from_byte(self.0.fault.load(Ordering::Acquire)) {
            return Err(fault.error());
        }
        if self.0.cancelled.load(Ordering::Relaxed) {
            return Err(self.fail(Fault::Cancelled));
        }
        if Instant::now() >= self.0.deadline {
            return Err(self.fail(Fault::Deadline));
        }
        Ok(())
    }

    fn operation(&self) -> io::Result<()> {
        self.check()?;
        self.0
            .operations
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count
                    .checked_add(1)
                    .filter(|&next| next <= self.0.limits.operations)
            })
            .map(|_| ())
            .map_err(|_| self.fail(Fault::Budget))
    }

    fn io_failure(&self, error: &io::Error) -> io::Error {
        self.fail(if error.kind() == io::ErrorKind::WouldBlock {
            Fault::Storage
        } else {
            Fault::Io
        })
    }

    /// Verify the path under a lease immediately before publishing a result.
    pub(crate) fn verify(&self) -> io::Result<()> {
        self.operation()?;
        let result = with_storage(|| {
            self.check()?;
            let metadata = fs::metadata(&self.0.path)?;
            if FileStamp::from_metadata(&metadata).as_ref() != Some(&self.0.stamp) {
                return Err(self.fail(Fault::Changed));
            }
            self.check()
        });
        result.map_err(|error| self.io_failure(&error))
    }
}

pub(crate) struct LeasedReader {
    health: Health,
    position: u64,
    #[cfg(test)]
    after_read: Option<Box<dyn FnOnce() + Send>>,
}

impl LeasedReader {
    pub(crate) fn len(&self) -> u64 {
        self.health.0.len
    }

    pub(crate) fn stamp(&self) -> &FileStamp {
        &self.health.0.stamp
    }

    pub(crate) fn open(
        path: &Path,
        stamp: FileStamp,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
    ) -> io::Result<(Self, Health)> {
        Self::open_with_limits(path, stamp, cancelled, deadline, Limits::default())
    }

    fn open_with_limits(
        path: &Path,
        stamp: FileStamp,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
        limits: Limits,
    ) -> io::Result<(Self, Health)> {
        if cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
            return Err(io::Error::other("EPUB reading was cancelled or expired"));
        }
        let len = with_storage(|| {
            let metadata = fs::metadata(path)?;
            if FileStamp::from_metadata(&metadata).as_ref() != Some(&stamp)
                || metadata.len() > MAX_ARCHIVE
            {
                return Err(io::Error::other(
                    "The EPUB changed or exceeds the archive limit",
                ));
            }
            Ok(metadata.len())
        })?;
        let health = Health(Arc::new(Shared {
            path: path.to_owned(),
            stamp,
            len,
            cancelled,
            deadline,
            limits,
            bytes: AtomicU64::new(0),
            operations: AtomicUsize::new(0),
            fault: AtomicU8::new(0),
        }));
        health.check()?;
        Ok((
            Self {
                health: health.clone(),
                position: 0,
                #[cfg(test)]
                after_read: None,
            },
            health,
        ))
    }
}

impl Read for LeasedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.health.check()?;
        if output.is_empty() || self.position >= self.health.0.len {
            return Ok(0);
        }
        self.health.operation()?;
        let remaining = self
            .health
            .0
            .limits
            .bytes
            .saturating_sub(self.health.0.bytes.load(Ordering::Acquire));
        if remaining == 0 {
            return Err(self.health.fail(Fault::Budget));
        }
        let count = output
            .len()
            .min(CHUNK_BYTES)
            .min(usize::try_from(remaining).unwrap_or(usize::MAX))
            .min(usize::try_from(self.health.0.len - self.position).unwrap_or(usize::MAX));
        let mut chunk = vec![0; count];
        let position = self.position;
        #[cfg(test)]
        let hook = self.after_read.take();
        let result = with_storage(|| {
            self.health.check()?;
            let mut file = File::open(&self.health.0.path)?;
            if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(&self.health.0.stamp) {
                return Err(self.health.fail(Fault::Changed));
            }
            file.seek(SeekFrom::Start(position))?;
            let read = file.read(&mut chunk)?;
            #[cfg(test)]
            if let Some(hook) = hook {
                hook();
            }
            if read == 0 {
                return Err(self.health.fail(Fault::Io));
            }
            if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(&self.health.0.stamp)
                || FileStamp::read(&self.health.0.path).as_ref() != Some(&self.health.0.stamp)
            {
                return Err(self.health.fail(Fault::Changed));
            }
            self.health.check()?;
            Ok(read)
        });
        let count = result.map_err(|error| self.health.io_failure(&error))?;
        output[..count].copy_from_slice(&chunk[..count]);
        self.position += count as u64;
        self.health
            .0
            .bytes
            .fetch_add(count as u64, Ordering::AcqRel);
        Ok(count)
    }
}

impl Seek for LeasedReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.health.operation()?;
        let position = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.health.0.len.checked_add_signed(delta),
        }
        .ok_or_else(|| self.health.fail(Fault::Seek))?;
        self.position = position;
        Ok(position)
    }
}

#[cfg(test)]
mod tests;
