//! One bounded, versioned archive of per-edition reading records.
use super::archive::{FileStamp, sync_directory};
use super::*;
use ring::rand::{SecureRandom, SystemRandom};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

const NAME: &str = "primitive-reader-book-metadata.json";
const LOCK: &str = ".primitive-reader-book-metadata.lock";
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) enum SavedOn {
    Close,
    Delete,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Snapshot {
    pub(crate) revision: String,
    pub(crate) saved_at: Option<i64>,
    pub(crate) saved_on: SavedOn,
    pub(crate) book: BookState,
}

#[derive(Deserialize, Serialize)]
struct Archive {
    version: u32,
    books: Vec<Snapshot>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MoveOutcome {
    Moved,
    Empty,
    OldCopyKept,
    DateConflict,
}

impl Archive {
    fn open(path: &Path) -> io::Result<Self> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: VERSION,
                    books: Vec::new(),
                });
            }
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err(invalid_state(
                "The book metadata archive is unavailable or too large",
            ));
        }
        let mut data = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut data)?;
        if data.len() as u64 > MAX_BYTES {
            return Err(invalid_state(
                "The book metadata archive exceeds its size limit",
            ));
        }
        let mut archive: Self = serde_json::from_slice(&data)
            .map_err(|_| invalid_state("The book metadata archive cannot be read"))?;
        let mut ids = HashSet::new();
        if archive.version != VERSION
            || archive.books.len() > MAX_BACKUP_BOOKS
            || !archive.books.iter().all(|record| {
                safe_path(&record.book.path)
                    && DocumentFormat::for_path(&record.book.path)
                        .is_some_and(|format| record.book.size <= format.max_bytes())
                    && record.revision.len() == 32
                    && record.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && ids.insert(record.book.id)
                    && validate_state(std::slice::from_ref(&record.book)).is_ok()
            })
        {
            return Err(invalid_state(
                "The book metadata archive contains invalid records",
            ));
        }
        for record in &mut archive.books {
            record.book.custom_contents.sanitize();
            record.book.hidden_flows.sanitize();
        }
        Ok(archive)
    }

    fn save(&self, path: &Path) -> io::Result<FileStamp> {
        let bytes = state_json(self, MAX_BYTES)?;
        let temporary =
            path.with_file_name(format!(".reader-metadata-{}.tmp", archive_revision()?));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        let node = file.metadata()?;
        let owns_path = || {
            fs::symlink_metadata(&temporary).is_ok_and(|info| {
                info.is_file() && info.dev() == node.dev() && info.ino() == node.ino()
            })
        };
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            if !owns_path() {
                return Err(io::Error::other("The temporary metadata archive changed"));
            }
            fs::rename(&temporary, path)?;
            FileStamp::from(&file.metadata()?)
        })();
        if result.is_err() && owns_path() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn merge(&mut self, source: Self) -> io::Result<bool> {
        let mut date_conflict = false;
        let mut indices: HashMap<_, _> = self
            .books
            .iter()
            .enumerate()
            .map(|(index, record)| (record.book.id, index))
            .collect();
        for record in source.books {
            if let Some(&index) = indices.get(&record.book.id) {
                let destination = &mut self.books[index];
                if destination.revision == record.revision {
                    continue;
                }
                match (record.saved_at, destination.saved_at) {
                    (Some(source), Some(target)) if source > target => *destination = record,
                    (Some(source), Some(target)) if source < target => {}
                    _ => date_conflict = true,
                }
            } else {
                if self.books.len() == MAX_BACKUP_BOOKS {
                    return Err(invalid_state(
                        "The combined book metadata archive exceeds its book limit",
                    ));
                }
                indices.insert(record.book.id, self.books.len());
                self.books.push(record);
            }
        }
        Ok(date_conflict)
    }
}

fn archive_stamp(path: &Path) -> io::Result<Option<FileStamp>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Ok(info) if !info.is_file() => Err(invalid_state(
            "The book metadata archive is not a regular file",
        )),
        result => result.and_then(|info| FileStamp::from(&info)).map(Some),
    }
}

fn ensure_stamps(
    source: (&Path, Option<FileStamp>),
    destination: (&Path, Option<FileStamp>),
) -> io::Result<()> {
    if archive_stamp(source.0)? == source.1 && archive_stamp(destination.0)? == destination.1 {
        Ok(())
    } else {
        Err(io::Error::other(
            "An archive changed. Choose the folder again.",
        ))
    }
}

fn archive_revision() -> io::Result<String> {
    let mut nonce = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| io::Error::other("Could not reserve a metadata archive revision"))?;
    Ok(format!("{:032x}", u128::from_ne_bytes(nonce)))
}

fn lock_archive(folder: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(folder.join(LOCK))?;
    let info = lock.metadata()?;
    if !info.is_file() || info.nlink() != 1 {
        return Err(invalid_state("Cannot reserve the metadata archive file"));
    }
    lock.lock()?;
    Ok(lock)
}

impl Library {
    pub(crate) fn move_metadata_archive(
        &self,
        from: &Path,
        to: &Path,
        preference_path: &Path,
        commit_folder: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<MoveOutcome> {
        let destination_folder = self.browse_folder(to)?;
        let source_folder = match self.browse_folder(from) {
            Ok(folder) => folder,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                commit_folder()?;
                return Ok(MoveOutcome::Empty);
            }
            Err(error) => return Err(error),
        };
        if source_folder == destination_folder {
            commit_folder()?;
            return Ok(MoveOutcome::Empty);
        }
        let (first, second) = if source_folder < destination_folder {
            (&source_folder, &destination_folder)
        } else {
            (&destination_folder, &source_folder)
        };
        let _first = lock_archive(first)?;
        let _second = lock_archive(second)?;
        let source = source_folder.join(NAME);
        let destination = destination_folder.join(NAME);
        let source_stamp = archive_stamp(&source)?;
        if source_stamp.is_none() {
            commit_folder()?;
            return Ok(MoveOutcome::Empty);
        }
        let destination_stamp = archive_stamp(&destination)?;
        let from_archive = Archive::open(&source)?;
        let mut merged = Archive::open(&destination)?;
        let date_conflict = merged.merge(from_archive)?;
        ensure_stamps((&source, source_stamp), (&destination, destination_stamp))?;
        let copied = merged.save(&destination)?;
        sync_directory(&destination_folder)?;
        ensure_stamps((&source, source_stamp), (&destination, Some(copied)))?;

        // The destination is recoverable even if writing the preference is
        // interrupted. Keep the old file until the new setting is durable.
        commit_folder()?;
        if date_conflict {
            return Ok(MoveOutcome::DateConflict);
        }
        let preference_folder = preference_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let source_removed = sync_directory(preference_folder).is_ok()
            && ensure_stamps((&source, source_stamp), (&destination, Some(copied))).is_ok()
            && fs::remove_file(&source).is_ok();
        Ok(if source_removed {
            MoveOutcome::Moved
        } else {
            MoveOutcome::OldCopyKept
        })
    }

    pub(crate) fn archived_books(&self, folder: &Path) -> io::Result<Vec<Snapshot>> {
        let path = self.browse_folder(folder)?.join(NAME);
        let mut records = Archive::open(&path)?.books;
        records.sort_by(|a, b| {
            natural_cmp(&a.book.title, &b.book.title).then_with(|| a.book.id.cmp(&b.book.id))
        });
        Ok(records)
    }

    pub(crate) fn archived_book(&self, folder: &Path, id: u64) -> io::Result<Option<Snapshot>> {
        let path = self.browse_folder(folder)?.join(NAME);
        Ok(Archive::open(&path)?
            .books
            .into_iter()
            .find(|record| record.book.id == id))
    }

    pub(crate) fn archive_metadata(
        &self,
        folder: &Path,
        id: u64,
        saved_on: SavedOn,
        saved_at: Option<i64>,
    ) -> io::Result<()> {
        let Some(book) = self.state(id) else {
            return Ok(());
        };
        validate_state(std::slice::from_ref(book))?;
        let folder = self.browse_folder(folder)?;
        let _lock = lock_archive(&folder)?;
        let path = folder.join(NAME);
        let mut archive = Archive::open(&path)?;
        let new = Snapshot {
            revision: archive_revision()?,
            saved_on,
            saved_at,
            book: book.clone(),
        };
        if let Some(prior) = archive.books.iter_mut().find(|record| record.book.id == id) {
            *prior = new;
        } else {
            if archive.books.len() == MAX_BACKUP_BOOKS {
                return Err(invalid_state(
                    "The book metadata archive reached its book limit",
                ));
            }
            archive.books.push(new);
        }
        archive.save(&path).map(|_| ())
    }

    pub(crate) fn remove_archived_book(
        &self,
        folder: &Path,
        id: u64,
        expected_revision: &str,
    ) -> io::Result<()> {
        let folder = self.browse_folder(folder)?;
        let _lock = lock_archive(&folder)?;
        let path = folder.join(NAME);
        let mut archive = Archive::open(&path)?;
        if let Some(index) = archive.books.iter().position(|record| record.book.id == id) {
            if archive.books[index].revision != expected_revision {
                return Err(io::Error::other("A newer archived copy is available"));
            }
            archive.books.remove(index);
            archive.save(&path)?;
        }
        Ok(())
    }

    pub(crate) fn restore_archived_book(
        &mut self,
        entry: &LibraryBook,
        mut archive: Snapshot,
    ) -> io::Result<()> {
        let book = &mut archive.book;
        if self.state(book.id).is_some()
            || self.identify(entry)? != book.id
            || entry.size != book.size
            || DocumentFormat::for_path(&book.path) != Some(entry.format)
        {
            return Err(invalid_state(
                "The book or its live reading data changed. Open it again.",
            ));
        }
        book.path.clone_from(&entry.path);
        book.file_deleted = false;
        self.recent.insert(0, archive.book);
        if let Err(error) = self.persist() {
            self.recent.remove(0);
            return Err(error);
        }
        Ok(())
    }
}
