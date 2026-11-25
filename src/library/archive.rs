//! Stage a local archive copy and commit the edition's saved path before removing the source.
use super::*;
use std::fs::OpenOptions;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub(crate) const DEFAULT_FOLDER: &str = "Archive";

pub(crate) struct MovePlan {
    pub(crate) source: LibraryBook,
    pub(crate) id: u64,
    pub(crate) destination: PathBuf,
    source_stamp: FileStamp,
    destination_folder: Option<Node>,
}

pub(crate) struct Archived {
    pub(crate) source_kept: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Node {
    device: u64,
    inode: u64,
}

impl Node {
    fn from(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    node: Node,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl FileStamp {
    pub(super) fn from(metadata: &fs::Metadata) -> io::Result<Self> {
        if !metadata.is_file() {
            return Err(changed_source());
        }
        Ok(Self {
            node: Node::from(metadata),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }

    fn at(path: &Path) -> io::Result<Self> {
        Self::from(&fs::symlink_metadata(path)?)
    }
}

struct ExclusiveCopy {
    path: PathBuf,
    file: File,
    node: Node,
    cleanup_stamp: Option<FileStamp>,
    committed: bool,
}

impl ExclusiveCopy {
    fn create(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    destination_exists()
                } else {
                    error
                }
            })?;
        let node = Node::from(&file.metadata()?);
        Ok(Self {
            path,
            file,
            node,
            cleanup_stamp: None,
            committed: false,
        })
    }
}

impl Drop for ExclusiveCopy {
    fn drop(&mut self) {
        if !self.committed
            && FileStamp::at(&self.path).is_ok_and(|stamp| {
                stamp.node == self.node
                    && self.cleanup_stamp.is_none_or(|expected| stamp == expected)
            })
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn changed_source() -> io::Error {
    io::Error::other("The book changed. Reopen it before moving it.")
}

fn destination_exists() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "A file with this name already exists in the archive folder.",
    )
}

fn require_absent(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(destination_exists()),
    }
}

pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    match File::open(path)?.sync_all() {
        Err(error) if matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOTSUP)) => Ok(()),
        result => result,
    }
}

impl Library {
    pub(crate) fn check_archive_folder(&self, folder: &Path) -> io::Result<()> {
        self.browse_folder(folder).map(|_| ())
    }

    pub(crate) fn archive_plan(
        &self,
        source: &LibraryBook,
        id: u64,
        folder: &Path,
    ) -> io::Result<MovePlan> {
        if !source.can_delete_file() {
            return Err(io::Error::other(
                "Use Files to move extracted publications.",
            ));
        }
        if source.path.parent() == Some(folder) {
            return Err(io::Error::other(
                "This book is already in the archive folder.",
            ));
        }
        let filename = source.path.file_name().ok_or_else(changed_source)?;
        let destination = folder.join(filename);
        if !safe_path(&destination) {
            return Err(denied());
        }
        let absolute = self.absolute_path(&source.path)?;
        let source_stamp = FileStamp::at(&absolute)?;
        let destination_folder = match self.browse_folder(folder) {
            Ok(directory) => {
                require_absent(&directory.join(filename))?;
                Some(Node::from(&fs::symlink_metadata(directory)?))
            }
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && folder == Path::new(DEFAULT_FOLDER) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        Ok(MovePlan {
            source: source.clone(),
            id,
            destination,
            source_stamp,
            destination_folder,
        })
    }

    pub(crate) fn archive_book(&mut self, plan: &MovePlan) -> io::Result<Archived> {
        self.archive_with(plan, primitive_app::save, |path| fs::remove_file(path))
    }

    pub(crate) fn archive_with(
        &mut self,
        plan: &MovePlan,
        save: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
        remove: impl FnOnce(&Path) -> io::Result<()>,
    ) -> io::Result<Archived> {
        let absolute = self.absolute_path(&plan.source.path)?;
        if FileStamp::at(&absolute)? != plan.source_stamp || self.identify(&plan.source)? != plan.id
        {
            return Err(changed_source());
        }
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&absolute)?;
        let source_metadata = source.metadata()?;
        if FileStamp::from(&source_metadata)? != plan.source_stamp {
            return Err(changed_source());
        }
        let folder = plan.destination.parent().ok_or_else(denied)?;
        let filename = plan.destination.file_name().ok_or_else(denied)?;
        if plan.destination_folder.is_none() {
            let root = self.browse_folder(Path::new(""))?;
            fs::create_dir(root.join(DEFAULT_FOLDER))?;
            sync_directory(&root)?;
        }
        let directory = self.browse_folder(folder)?;
        if let Some(expected) = plan.destination_folder
            && Node::from(&fs::symlink_metadata(&directory)?) != expected
        {
            return Err(io::Error::other(
                "The archive folder changed. Choose it again.",
            ));
        }
        let mut output = ExclusiveCopy::create(directory.join(filename))?;
        let copied = io::copy(&mut (&source).take(plan.source.size + 1), &mut output.file)?;
        if copied != plan.source.size || FileStamp::at(&absolute)? != plan.source_stamp {
            return Err(changed_source());
        }
        if let Ok(modified) = source_metadata.modified() {
            output
                .file
                .set_times(fs::FileTimes::new().set_modified(modified))?;
        }
        output.file.sync_all()?;
        sync_directory(&directory)?;
        let moved = LibraryBook {
            path: plan.destination.clone(),
            ..plan.source.clone()
        };
        let copy_stamp = FileStamp::from(&output.file.metadata()?)?;
        output.cleanup_stamp = Some(copy_stamp);
        if FileStamp::at(&output.path)? != copy_stamp || self.identify(&moved)? != plan.id {
            return Err(io::Error::other("The archive copy could not be verified."));
        }

        // A completed archive is recoverable by its saved identity even if removing
        // the old path is interrupted. A failed save still leaves the old path live.
        let state = self.state_mut(plan.id).ok_or_else(changed_source)?;
        let previous_path = std::mem::replace(&mut state.path, plan.destination.clone());
        let saved = if self.state_version.get() == 2 || self.recent.iter().any(raw_v2::book_has_raw)
        {
            self.persist()
        } else {
            self.save_with(save)
        };
        if let Err(error) = saved {
            self.state_mut(plan.id).unwrap().path = previous_path;
            if !matches!(
                crate::reading_backup::transaction_pending(&self.state_path),
                Ok(false)
            ) {
                // The journal may have published the destination before a
                // rollback failed. Keep both copies until startup recovery
                // determines which path the restored state references.
                output.committed = true;
                return Err(io::Error::other(format!(
                    "{error}; both book copies were retained because reader recovery is pending or uncertain"
                )));
            }
            return Err(error);
        }
        output.committed = true;

        let state_directory = self
            .state_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let source_kept = !(sync_directory(state_directory).is_ok()
            && FileStamp::at(&absolute).is_ok_and(|stamp| stamp == plan.source_stamp)
            && self.identify(&plan.source).is_ok_and(|id| id == plan.id)
            && FileStamp::at(&absolute).is_ok_and(|stamp| stamp == plan.source_stamp)
            && FileStamp::at(&output.path).is_ok_and(|stamp| stamp == copy_stamp)
            && remove(&absolute).is_ok());
        if !source_kept {
            self.books.retain(|book| book.path != plan.source.path);
        }
        self.books.retain(|book| book.path != plan.destination);
        if self.books.len() < MAX_BOOKS {
            let indexed = library_book(moved.path, moved.size, moved.format, self.state(plan.id));
            self.books.push(indexed);
            self.books
                .sort_by_cached_key(|book| (book.title.to_lowercase(), book.path.clone()));
        } else {
            self.truncated = true;
        }
        Ok(Archived { source_kept })
    }
}
