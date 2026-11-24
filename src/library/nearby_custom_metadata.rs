//! Locate only the selected publication's stock, beside-book metadata table.
use super::{DocumentFormat, Library, denied, safe_path};
use crate::koreader_sidecar::custom_metadata;
use std::ffi::{CStr, CString};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

impl Library {
    pub(crate) fn nearby_koreader_custom_metadata(
        &self,
        book: &Path,
    ) -> io::Result<Option<Vec<u8>>> {
        let relative = if book.is_absolute() {
            book.strip_prefix(&self.root)
                .or_else(|_| book.strip_prefix(&self.original_root))
                .map_err(|_| denied())?
        } else {
            book
        };
        if !safe_path(relative) || DocumentFormat::for_path(relative).is_none() {
            return Err(denied());
        }
        if crate::epub_members::is_folder_book(relative) || relative.extension().is_none() {
            return Ok(None);
        }
        let parent = relative.parent().ok_or_else(denied)?;
        self.browse_folder(parent)?;
        let stem = relative
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(denied)?;
        if stem.chars().any(char::is_control) {
            return Err(denied());
        }
        let dirname = CString::new(format!("{stem}.sdr"))?;
        let filename = c"custom_metadata.lua";
        let Some(folder) = absent(self.open_custom_sidecar(parent, &dirname))? else {
            return Ok(None);
        };
        let Some(named) = absent(stat_at(&folder, filename))? else {
            return Ok(None);
        };
        if !ordinary(&named) {
            return Err(denied());
        }
        let mut source = open_at(&folder, filename, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let before = source.metadata()?;
        if !same_entry(&before, &named) {
            return Err(denied());
        }
        let mut bytes = Vec::with_capacity(before.len() as usize);
        (&mut source)
            .take(custom_metadata::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let current = self.open_custom_sidecar(parent, &dirname)?;
        let named = stat_at(&current, filename)?;
        let after = source.metadata()?;
        if bytes.len() as u64 != before.len()
            || after.len() != before.len()
            || !same_entry(&after, &named)
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
        {
            return Err(denied());
        }
        Ok(Some(bytes))
    }

    fn open_custom_sidecar(&self, parent: &Path, dirname: &CStr) -> io::Result<File> {
        let mut folder = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&self.root)?;
        for part in parent.components() {
            let name = CString::new(part.as_os_str().to_str().ok_or_else(denied)?)?;
            folder = open_at(&folder, &name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        }
        open_at(&folder, dirname, libc::O_RDONLY | libc::O_DIRECTORY)
    }
}

fn absent<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn ordinary(entry: &libc::stat) -> bool {
    entry.st_mode & libc::S_IFMT == libc::S_IFREG
        && entry.st_nlink == 1
        && (0..=custom_metadata::MAX_BYTES as i64).contains(&entry.st_size)
}

fn same_entry(open: &Metadata, named: &libc::stat) -> bool {
    ordinary(named)
        && open.is_file()
        && open.nlink() == 1
        && open.dev() == named.st_dev as u64
        && open.ino() == named.st_ino as u64
        && open.len() == named.st_size as u64
}

fn open_at(folder: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // The descriptor and its terminated child name live through the syscall.
    let fd = unsafe {
        libc::openat(
            folder.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // openat returned a new descriptor owned by this caller.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn stat_at(folder: &File, name: &CStr) -> io::Result<libc::stat> {
    let mut result = MaybeUninit::uninit();
    // fstatat initializes this stat on success and does not follow leaf symlinks.
    if unsafe {
        libc::fstatat(
            folder.as_raw_fd(),
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { result.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new() -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "reader-custom-meta-{}-{suffix}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn nearby_custom_metadata_accepts_one_nested_named_source_and_denies_invalid_parents() {
        let scratch = Scratch::new();
        let root = scratch.0.join("Documents");
        let shelf = root.join("Maps");
        let outside = scratch.0.join("Outside");
        fs::create_dir_all(&shelf).unwrap();
        fs::create_dir(&outside).unwrap();
        let relative = Path::new("Maps/Edition.2.epub");
        fs::write(root.join(relative), []).unwrap();
        let library = Library::open(&root, &scratch.0.join("library.json")).unwrap();
        assert!(
            library
                .nearby_koreader_custom_metadata(relative)
                .unwrap()
                .is_none()
        );
        let valid_sdr = shelf.join("Edition.2.sdr");
        fs::create_dir(&valid_sdr).unwrap();
        let before = "return {custom_props={title='Coastal Ledger'}}";
        fs::write(valid_sdr.join("custom_metadata.lua"), before).unwrap();
        assert_eq!(
            library
                .nearby_koreader_custom_metadata(relative)
                .unwrap()
                .as_deref(),
            Some(before.as_bytes())
        );
        for root in [&root, &root.canonicalize().unwrap()] {
            assert_eq!(
                library
                    .nearby_koreader_custom_metadata(&root.join(relative))
                    .unwrap()
                    .as_deref(),
                Some(before.as_bytes())
            );
        }
        let external_sidecar = outside.join("Edition.2.sdr");
        fs::create_dir(&external_sidecar).unwrap();
        fs::write(
            external_sidecar.join("custom_metadata.lua"),
            "return {custom_props={title='Outside'}}",
        )
        .unwrap();
        symlink(&outside, root.join("Alias")).unwrap();
        for path in [
            Path::new("Alias/Edition.2.epub"),
            Path::new("../Outside/Edition.2.epub"),
            Path::new(".Hidden/Book.pdf"),
            &outside.join("Edition.2.epub"),
        ] {
            assert!(
                library.nearby_koreader_custom_metadata(path).is_err(),
                "{path:?}"
            );
        }
        fs::create_dir(root.join("Bad\nParent")).unwrap();
        assert!(
            library
                .nearby_koreader_custom_metadata(Path::new("Bad\nParent/Book.pdf"))
                .is_err()
        );
        assert!(
            library
                .nearby_koreader_custom_metadata(Path::new("Maps/Bad\nName.pdf"))
                .is_err()
        );
        symlink(&external_sidecar, shelf.join("Redirected.sdr")).unwrap();
        assert!(
            library
                .nearby_koreader_custom_metadata(Path::new("Maps/Redirected.epub"))
                .is_err()
        );
    }

    #[test]
    fn nearby_custom_metadata_rejects_linked_special_and_oversized_entries() {
        let scratch = Scratch::new();
        let root = scratch.0.join("Documents");
        let sdr = root.join("Edition.sdr");
        fs::create_dir_all(&sdr).unwrap();
        let external = scratch.0.join("outside.lua");
        fs::write(&external, "return {custom_props={title='External'}}").unwrap();
        let relative = Path::new("Edition.pdf");
        fs::write(root.join(relative), []).unwrap();
        let library = Library::open(&root, &scratch.0.join("library.json")).unwrap();
        let leaf = sdr.join("custom_metadata.lua");
        let refused = || assert!(library.nearby_koreader_custom_metadata(relative).is_err());
        symlink(&external, &leaf).unwrap();
        refused();
        fs::remove_file(&leaf).unwrap();
        fs::hard_link(&external, &leaf).unwrap();
        refused();
        fs::remove_file(&leaf).unwrap();
        fs::create_dir(&leaf).unwrap();
        refused();
        fs::remove_dir(&leaf).unwrap();
        let fifo = CString::new(leaf.as_os_str().as_encoded_bytes()).unwrap();
        // The path is a terminated string in this test's private directory.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        refused();
        fs::remove_file(&leaf).unwrap();
        File::create(&leaf)
            .unwrap()
            .set_len(custom_metadata::MAX_BYTES as u64 + 1)
            .unwrap();
        refused();
        assert_eq!(
            fs::read_to_string(&external).unwrap(),
            "return {custom_props={title='External'}}"
        );
    }
}
