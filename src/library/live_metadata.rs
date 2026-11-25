//! Per-edition Reader sidecars. The central library also keeps its local index and live state.
use super::nearby_custom_metadata::{descend, open_at, open_directory, read_stable, stat_at};
use super::*;
use ring::rand::{SecureRandom, SystemRandom};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;
use std::time::SystemTime;

const ROOT: &str = ".book-metadata.sdr";
const HASH_ROOT: &str = "hashdocsettings";
const PREFERENCE: &str = "book-metadata-location.json";
const VERSION: u8 = 1;
const MAX_BYTES: u64 = MAX_STATE_BYTES;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Location {
    BookFolder,
    #[default]
    Profile,
    Hash,
}

impl Location {
    pub(crate) const ALL: [Self; 3] = [Self::BookFolder, Self::Profile, Self::Hash];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::BookFolder => "Book folder",
            Self::Profile => "Reader profile",
            Self::Hash => "By edition fingerprint",
        }
    }

    fn order(self) -> [Self; 3] {
        match self {
            Self::BookFolder => [Self::BookFolder, Self::Profile, Self::Hash],
            Self::Profile => [Self::Profile, Self::BookFolder, Self::Hash],
            Self::Hash => [Self::Hash, Self::BookFolder, Self::Profile],
        }
    }
}

pub(super) struct Store {
    profile: PathBuf,
    namespace: PathBuf,
    selected: Location,
    tracked: RefCell<HashMap<u64, [u8; 32]>>,
    requested: RefCell<HashSet<u64>>,
    warning: RefCell<Option<String>>,
}

#[derive(Deserialize)]
struct Snapshot {
    version: u8,
    book: BookState,
}

#[derive(Serialize)]
struct SnapshotRef<'a> {
    version: u8,
    book: &'a BookState,
}

pub(crate) struct HashBook {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) author: Option<String>,
    pub(crate) path: PathBuf,
}

impl Store {
    pub(super) fn open(state: &Path, books: &[BookState]) -> Self {
        let profile = state
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let index_name = state
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("reader.json"));
        let namespace = Path::new(ROOT).join(index_name);
        let selected = open_directory(profile)
            .ok()
            .and_then(|folder| {
                read_stable(&folder, &namespace.join(PREFERENCE), 256, |file, len| {
                    serde_json::from_reader(file.take(len + 1)).map_err(io::Error::other)
                })
                .ok()
                .flatten()
            })
            .unwrap_or_default();
        let tracked = books
            .iter()
            .filter_map(|book| encode(book).ok().map(|bytes| (book.id, digest(&bytes))))
            .collect();
        Self {
            profile: profile.to_owned(),
            namespace,
            selected,
            tracked: RefCell::new(tracked),
            requested: RefCell::new(HashSet::new()),
            warning: RefCell::new(None),
        }
    }

    pub(super) fn checkpoint(&self, library: &Library) {
        let mut tracked = self.tracked.borrow_mut();
        let mut requested = self.requested.borrow_mut();
        let mut warning = self.warning.borrow_mut();
        *warning = None;
        for book in &library.recent {
            if super::raw_v2::book_has_raw(book) {
                requested.insert(book.id);
                *warning = Some(
                    "Raw whitespace positions cannot yet be saved to a metadata sidecar.".into(),
                );
                continue;
            }
            if book.file_deleted {
                requested.remove(&book.id);
                continue;
            }
            let Ok(bytes) = encode(book) else {
                *warning = Some("A book's metadata is too large for its sidecar.".into());
                continue;
            };
            let checksum = digest(&bytes);
            if tracked.get(&book.id) == Some(&checksum) && !requested.contains(&book.id) {
                continue;
            }
            match self.save(library, book, &bytes) {
                Ok(()) => {
                    tracked.insert(book.id, checksum);
                    requested.remove(&book.id);
                }
                Err(error) => {
                    requested.insert(book.id);
                    *warning = Some(format!(
                        "Book metadata location could not be saved: {error}"
                    ));
                }
            }
        }
    }

    fn root(&self, library: &Library, location: Location) -> io::Result<File> {
        open_directory(if location == Location::BookFolder {
            &library.root
        } else {
            &self.profile
        })
    }

    fn save(&self, library: &Library, book: &BookState, bytes: &[u8]) -> io::Result<()> {
        let write = |location| {
            let root = self.root(library, location)?;
            write_atomic(&root, &self.sidecar(location, &book.path, book.id)?, bytes)
        };
        let (destination, checkpoint) = match write(self.selected) {
            Ok(written) => (self.selected, written),
            Err(error)
                if self.selected == Location::BookFolder
                    && matches!(
                        error.raw_os_error(),
                        Some(libc::EROFS | libc::EACCES | libc::EPERM)
                    ) =>
            {
                (Location::Profile, write(Location::Profile)?)
            }
            Err(error) => return Err(error),
        };
        for old in Location::ALL {
            if old == destination {
                continue;
            }
            let result = self.root(library, old).and_then(|root| {
                remove_snapshot(
                    &root,
                    &self.sidecar(old, &book.path, book.id)?,
                    book.id,
                    checkpoint,
                )
            });
            if let Err(error) = result {
                // The durable destination is still usable if a prior folder is read-only.
                eprintln!("reader metadata_sidecar_cleanup_failed={error}");
            }
        }
        Ok(())
    }

    fn candidate(
        &self,
        library: &Library,
        location: Location,
        entry: &LibraryBook,
        id: u64,
    ) -> io::Result<Option<(SystemTime, BookState)>> {
        let root = self.root(library, location)?;
        let path = self.sidecar(location, &entry.path, id)?;
        Ok(read_snapshot(&root, &path, id)?.filter(|(_, book)| {
            book.size == entry.size && DocumentFormat::for_path(&book.path) == Some(entry.format)
        }))
    }

    fn sidecar(&self, location: Location, path: &Path, id: u64) -> io::Result<PathBuf> {
        if location == Location::Hash {
            let key = format!("{id:016x}");
            return Ok(self
                .namespace
                .join(HASH_ROOT)
                .join(&key[..2])
                .join(format!("{key}.sdr"))
                .join("primitive-reader.json"));
        }
        if !safe_path(path) || DocumentFormat::for_path(path).is_none() {
            return Err(denied());
        }
        let beside = path
            .with_extension("sdr")
            .join(format!("primitive-reader-{id:016x}.json"));
        Ok(if location == Location::BookFolder {
            beside
        } else {
            self.namespace.join("docsettings").join(beside)
        })
    }
}

impl Library {
    pub(crate) fn live_metadata_location(&self) -> Location {
        self.live_metadata.selected
    }

    pub(crate) fn set_live_metadata_location(&mut self, location: Location) -> io::Result<()> {
        if self.live_metadata.selected != location {
            let bytes = serde_json::to_vec(&location).map_err(io::Error::other)?;
            let root = open_directory(&self.live_metadata.profile)?;
            write_atomic(
                &root,
                &self.live_metadata.namespace.join(PREFERENCE),
                &bytes,
            )?;
            self.live_metadata.selected = location;
        }
        Ok(())
    }

    pub(crate) fn request_live_metadata(&self, id: u64) {
        self.live_metadata.requested.borrow_mut().insert(id);
    }

    pub(crate) fn live_metadata_pending(&self) -> bool {
        let pending = self.live_metadata.requested.borrow();
        self.recent.iter().any(|book| pending.contains(&book.id))
    }

    pub(crate) fn take_live_metadata_warning(&self) -> Option<String> {
        self.live_metadata.warning.borrow_mut().take()
    }

    /// A local live library record takes precedence. With a fresh library, recover the
    /// newest usable sidecar for this edition before offering an archived snapshot.
    pub(crate) fn recover_live_metadata(&mut self, entry: &LibraryBook, id: u64) {
        if self.state(id).is_some() {
            return;
        }
        let mut newest = None;
        for location in self.live_metadata.selected.order() {
            if let Ok(Some((modified, book))) =
                self.live_metadata.candidate(self, location, entry, id)
                && newest.as_ref().is_none_or(|(prior, _)| modified > *prior)
            {
                newest = Some((modified, book));
            }
        }
        if let Some((_, mut book)) = newest {
            book.path.clone_from(&entry.path);
            self.recent.push(book);
        }
    }

    pub(crate) fn hashed_metadata_books(&self) -> io::Result<Vec<HashBook>> {
        let root = open_directory(&self.live_metadata.profile)?;
        let mut rows = Vec::new();
        let hash_folder = self.live_metadata.namespace.join(HASH_ROOT);
        match descend(&root, &hash_folder) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(rows),
            Err(error) => return Err(error),
        };
        let buckets = fs::read_dir(self.live_metadata.profile.join(&hash_folder))?;
        let mut examined = 0;
        'buckets: for bucket in buckets.take(256).flatten().filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.file_name().to_str().is_some_and(|name| hex(name, 2))
        }) {
            let bucket_name = bucket.file_name();
            let Ok(editions) = fs::read_dir(bucket.path()) else {
                continue;
            };
            for edition in editions.flatten() {
                examined += 1;
                if examined > MAX_SCAN_ENTRIES || rows.len() == MAX_BOOKS {
                    break 'buckets;
                }
                let filename = edition.file_name();
                let Some(key) = filename
                    .to_str()
                    .and_then(|name| name.strip_suffix(".sdr"))
                    .filter(|key| hex(key, 16) && &key[..2] == bucket_name.to_string_lossy())
                else {
                    continue;
                };
                let Ok(id) = u64::from_str_radix(key, 16) else {
                    continue;
                };
                let path = hash_folder
                    .join(&bucket_name)
                    .join(filename)
                    .join("primitive-reader.json");
                let Ok(Some((_, book))) = read_snapshot(&root, &path, id) else {
                    continue;
                };
                rows.push(HashBook {
                    id,
                    title: book.title,
                    author: book.author,
                    path: book.path,
                });
            }
        }
        rows.sort_by(|a, b| natural_cmp(&a.title, &b.title).then_with(|| a.path.cmp(&b.path)));
        Ok(rows)
    }

    pub(crate) fn locate_hashed_metadata_book(&self, id: u64) -> io::Result<LibraryBook> {
        let root = open_directory(&self.live_metadata.profile)?;
        let path = self
            .live_metadata
            .sidecar(Location::Hash, Path::new(""), id)?;
        let (_, book) = read_snapshot(&root, &path, id)?
            .ok_or_else(|| invalid_state("The fingerprint metadata is no longer available"))?;
        self.locate(&book)
    }
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_snapshot(root: &File, path: &Path, id: u64) -> io::Result<Option<(SystemTime, BookState)>> {
    read_stable(root, path, MAX_BYTES, |file, len| {
        let modified = file.metadata()?.modified()?;
        let mut snapshot: Snapshot =
            serde_json::from_reader(file.take(len + 1)).map_err(io::Error::other)?;
        let book = &mut snapshot.book;
        if snapshot.version != VERSION
            || book.id != id
            || book.file_deleted
            || !safe_path(&book.path)
            || DocumentFormat::for_path(&book.path)
                .is_none_or(|format| book.size > format.max_bytes())
            || book
                .position
                .as_ref()
                .is_some_and(|position| !valid_position(position))
            || book.bookmarks.len() > MAX_IMPORTED_MARKS
            || !book
                .bookmarks
                .iter()
                .all(|mark| valid_position(&mark.position))
            || book.highlights.len() > MAX_IMPORTED_MARKS
            || !book.highlights.iter().all(valid_highlight)
        {
            return Err(invalid_state(
                "Reader book sidecar does not match a supported edition",
            ));
        }
        validate_state(std::slice::from_ref(book))?;
        book.custom_contents.sanitize();
        book.hidden_flows.sanitize();
        Ok((modified, snapshot.book))
    })
}

fn encode(book: &BookState) -> io::Result<Vec<u8>> {
    state_json(
        &SnapshotRef {
            version: VERSION,
            book,
        },
        MAX_BYTES,
    )
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .try_into()
        .unwrap()
}

fn c_name(path: &Path) -> io::Result<CString> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(denied)?;
    if name.chars().any(char::is_control) {
        return Err(denied());
    }
    Ok(CString::new(name)?)
}

fn ensure_directory(root: &File, path: &Path) -> io::Result<File> {
    let device = root.metadata()?.dev();
    let mut folder = root.try_clone()?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            return Err(denied());
        };
        let name = c_name(Path::new(name))?;
        // Names are single components held alive across the syscalls.
        if unsafe { libc::mkdirat(folder.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        } else {
            folder.sync_all()?;
        }
        folder = open_at(&folder, &name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        if folder.metadata()?.dev() != device {
            return Err(denied());
        }
    }
    Ok(folder)
}

fn unlink(folder: &File, name: &CStr) -> io::Result<()> {
    if unsafe { libc::unlinkat(folder.as_raw_fd(), name.as_ptr(), 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn write_atomic(root: &File, path: &Path, bytes: &[u8]) -> io::Result<SystemTime> {
    let parent = path.parent().ok_or_else(denied)?;
    let folder = ensure_directory(root, parent)?;
    folder.lock()?;
    let name = c_name(path)?;
    match stat_at(&folder, &name) {
        Ok(leaf) if leaf.st_mode & libc::S_IFMT == libc::S_IFREG && leaf.st_nlink == 1 => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => return Err(denied()),
        Err(error) => return Err(error),
    }
    let mut nonce = [0; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| io::Error::other("Cannot reserve a book metadata file"))?;
    let temporary = CString::new(format!(".reader-{:032x}.tmp", u128::from_ne_bytes(nonce)))?;
    // The file is created under the held directory descriptor without following links.
    let fd = unsafe {
        libc::openat(
            folder.as_raw_fd(),
            temporary.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // The new descriptor is owned by this writer.
    let mut file = unsafe { File::from_raw_fd(fd) };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        let current = descend(root, parent)?.metadata()?;
        let held = folder.metadata()?;
        if current.ino() != held.ino() || current.dev() != held.dev() {
            return Err(denied());
        }
        if unsafe {
            libc::renameat(
                folder.as_raw_fd(),
                temporary.as_ptr(),
                folder.as_raw_fd(),
                name.as_ptr(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        folder.sync_all()?;
        file.metadata()?.modified()
    })();
    if result.is_err() {
        let _ = unlink(&folder, &temporary);
    }
    result
}

fn remove_snapshot(root: &File, path: &Path, id: u64, checkpoint: SystemTime) -> io::Result<()> {
    let folder = match descend(root, path.parent().ok_or_else(denied)?) {
        Ok(folder) => folder,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    folder.lock()?;
    let name = c_name(path)?;
    if read_snapshot(&folder, Path::new(path.file_name().ok_or_else(denied)?), id)?
        .is_some_and(|(modified, _)| modified <= checkpoint)
    {
        unlink(&folder, &name)?;
        folder.sync_all()?;
    }
    Ok(())
}
