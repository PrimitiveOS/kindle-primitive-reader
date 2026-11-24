//! Resolve the selected book's custom metadata under Documents or its KOReader peer.
use super::{DocumentFormat, Library, denied, safe_path};
use crate::koreader_sidecar::{
    self,
    custom_metadata::{self, Location},
};
use std::ffi::{CStr, CString};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path};

const CUSTOM_FILENAME: &str = "custom_metadata.lua";

impl Library {
    pub(crate) fn koreader_custom_metadata(&self, book: &Path) -> io::Result<Option<Vec<u8>>> {
        let relative = if book.is_absolute() {
            book.strip_prefix(&self.root)
                .or_else(|_| book.strip_prefix(&self.original_root))
                .map_err(|_| denied())?
        } else {
            book
        };
        let format = DocumentFormat::for_path(relative).ok_or_else(denied)?;
        if !safe_path(relative) {
            return Err(denied());
        }
        if crate::epub_members::is_folder_book(relative) || relative.extension().is_none() {
            return Ok(None);
        }
        let parent = relative.parent().ok_or_else(denied)?;
        self.browse_folder(parent)?;
        if relative
            .file_name()
            .is_none_or(|name| name.to_string_lossy().chars().any(char::is_control))
        {
            return Err(denied());
        }
        let documents = open_directory(&self.root)?;
        let beside = relative.with_extension("sdr").join(CUSTOM_FILENAME);
        let Some(stock) = self.stock_metadata_root(&documents) else {
            return read_literal(&documents, &beside, custom_metadata::MAX_BYTES);
        };
        for location in settings_location(&stock).order() {
            let bytes = match location {
                Location::Document => read_literal(&documents, &beside, custom_metadata::MAX_BYTES),
                Location::Directory => self.central_custom_metadata(&stock, &beside),
                Location::Hash => hash_custom_metadata(&stock, &documents, relative, format),
            }?;
            if bytes.is_some() {
                return Ok(bytes);
            }
        }
        Ok(None)
    }

    fn stock_metadata_root(&self, documents: &File) -> Option<File> {
        // Match the local shared-storage convention for KOReader dictionaries.
        if self.original_root.file_name()? != "documents" {
            return None;
        }
        let mount = open_directory(self.root.parent()?).ok()?;
        let stock = descend(&mount, Path::new("koreader")).ok()?;
        (stock.metadata().ok()?.dev() == documents.metadata().ok()?.dev()).then_some(stock)
    }

    fn central_custom_metadata(&self, stock: &File, beside: &Path) -> io::Result<Option<Vec<u8>>> {
        let central = Path::new("docsettings");
        let native = central
            .join(self.root.strip_prefix("/").map_err(|_| denied())?)
            .join(beside);
        let mounted = central.join("mnt/us/documents").join(beside);
        let exact = read_literal(stock, &native, custom_metadata::MAX_BYTES)?;
        if native == mounted {
            return Ok(exact);
        }
        let mirrored = read_literal(stock, &mounted, custom_metadata::MAX_BYTES)?;
        if exact.is_some() && mirrored.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Ambiguous KOReader metadata paths",
            ));
        }
        Ok(exact.or(mirrored))
    }
}

fn settings_location(stock: &File) -> Location {
    for name in ["settings.reader.lua", "settings.reader.lua.old"] {
        if let Ok(Some(bytes)) =
            read_literal(stock, Path::new(name), koreader_sidecar::MAX_SIDECAR_BYTES)
            && let Ok(location) = custom_metadata::storage_preference(&bytes)
        {
            return location;
        }
    }
    Location::default()
}

fn hash_custom_metadata(
    stock: &File,
    documents: &File,
    book: &Path,
    format: DocumentFormat,
) -> io::Result<Option<Vec<u8>>> {
    let Some(hash_root) = absent(descend(stock, Path::new("hashdocsettings")))? else {
        return Ok(None);
    };
    let digest = read_stable(documents, book, format.max_bytes(), |source, _| {
        koreader_sidecar::partial_md5(source)
    });
    // Stock reuses the adjacent path if calculating the hash fails.
    let Ok(Some(digest)) = digest else {
        return Ok(None);
    };
    let digest = format!("{:x}", md5::Digest(digest));
    let relative = Path::new(&digest[..2])
        .join(format!("{digest}.sdr"))
        .join(CUSTOM_FILENAME);
    read_literal(&hash_root, &relative, custom_metadata::MAX_BYTES)
}

fn open_directory(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)
}

fn descend(root: &File, relative: &Path) -> io::Result<File> {
    let device = root.metadata()?.dev();
    let mut folder = root.try_clone()?;
    for part in relative.components() {
        let Component::Normal(name) = part else {
            return Err(denied());
        };
        let name = name
            .to_str()
            .filter(|name| !name.chars().any(char::is_control))
            .ok_or_else(denied)?;
        folder = open_at(
            &folder,
            &CString::new(name)?,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        if folder.metadata()?.dev() != device {
            return Err(denied());
        }
    }
    Ok(folder)
}

fn read_literal(root: &File, relative: &Path, limit: usize) -> io::Result<Option<Vec<u8>>> {
    read_stable(root, relative, limit as u64, |source, len| {
        let mut bytes = Vec::with_capacity(len as usize);
        source.take(len + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != len {
            return Err(denied());
        }
        Ok(bytes)
    })
}

fn read_stable<T>(
    root: &File,
    relative: &Path,
    limit: u64,
    read: impl FnOnce(&mut File, u64) -> io::Result<T>,
) -> io::Result<Option<T>> {
    let parent = relative.parent().ok_or_else(denied)?;
    let name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(denied)?;
    let filename = CString::new(name)?;
    let Some(folder) = absent(descend(root, parent))? else {
        return Ok(None);
    };
    let Some(named) = absent(stat_at(&folder, &filename))? else {
        return Ok(None);
    };
    let device = root.metadata()?.dev();
    if !ordinary(&named, device, limit) {
        return Err(denied());
    }
    let mut source = open_at(&folder, &filename, libc::O_RDONLY | libc::O_NONBLOCK)?;
    let before = source.metadata()?;
    if !same_entry(&before, &named) {
        return Err(denied());
    }
    let value = read(&mut source, before.len())?;
    let named = stat_at(&descend(root, parent)?, &filename)?;
    let after = source.metadata()?;
    if !ordinary(&named, device, limit)
        || !same_entry(&after, &named)
        || after.len() != before.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err(denied());
    }
    Ok(Some(value))
}

fn absent<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn ordinary(entry: &libc::stat, device: u64, limit: u64) -> bool {
    entry.st_mode & libc::S_IFMT == libc::S_IFREG
        && entry.st_nlink == 1
        && entry.st_dev as u64 == device
        && u64::try_from(entry.st_size).is_ok_and(|len| len <= limit)
}

fn same_entry(open: &Metadata, named: &libc::stat) -> bool {
    open.is_file()
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SCRATCH_ID: AtomicUsize = AtomicUsize::new(0);

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new() -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "reader-custom-meta-{}-{suffix}-{id}",
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
                .koreader_custom_metadata(relative)
                .unwrap()
                .is_none()
        );
        let valid_sdr = shelf.join("Edition.2.sdr");
        fs::create_dir(&valid_sdr).unwrap();
        let before = "return {custom_props={title='Coastal Ledger'}}";
        fs::write(valid_sdr.join("custom_metadata.lua"), before).unwrap();
        assert_eq!(
            library
                .koreader_custom_metadata(relative)
                .unwrap()
                .as_deref(),
            Some(before.as_bytes())
        );
        for root in [&root, &root.canonicalize().unwrap()] {
            assert_eq!(
                library
                    .koreader_custom_metadata(&root.join(relative))
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
            assert!(library.koreader_custom_metadata(path).is_err(), "{path:?}");
        }
        fs::create_dir(root.join("Bad\nParent")).unwrap();
        assert!(
            library
                .koreader_custom_metadata(Path::new("Bad\nParent/Book.pdf"))
                .is_err()
        );
        assert!(
            library
                .koreader_custom_metadata(Path::new("Maps/Bad\nName.pdf"))
                .is_err()
        );
        symlink(&external_sidecar, shelf.join("Redirected.sdr")).unwrap();
        assert!(
            library
                .koreader_custom_metadata(Path::new("Maps/Redirected.epub"))
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
        let refused = || assert!(library.koreader_custom_metadata(relative).is_err());
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

    fn write_sidecar(directory: &Path, bytes: &[u8]) {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join(CUSTOM_FILENAME), bytes).unwrap();
    }

    #[test]
    fn central_custom_metadata_resolves_all_priorities_by_stock_path_and_fingerprint() {
        let scratch = Scratch::new();
        let docs = scratch.0.join("documents");
        let ko = scratch.0.join("koreader");
        let book = Path::new("Shelf/Edition.2.epub");
        fs::create_dir_all(docs.join("Shelf")).unwrap();
        let physical = docs.join(book);
        fs::write(&physical, b"authorial edition 1").unwrap();
        let adjacent = physical.with_extension("sdr");
        let central = ko.join("docsettings/mnt/us/documents/Shelf/Edition.2.sdr");
        let digest = koreader_sidecar::partial_md5(&mut File::open(&physical).unwrap()).unwrap();
        let digest = format!("{:x}", md5::Digest(digest));
        let hashed = ko
            .join("hashdocsettings")
            .join(&digest[..2])
            .join(format!("{digest}.sdr"));
        write_sidecar(&adjacent, b"beside");
        write_sidecar(&central, b"central");
        write_sidecar(&hashed, b"hash");
        let settings = ko.join("settings.reader.lua");
        let l = Library::open(&docs, &scratch.0.join("library.json")).unwrap();
        let lookup = || l.koreader_custom_metadata(book).unwrap().unwrap();
        assert_eq!(lookup(), b"beside");
        for (mode, expected) in [
            ("doc", b"beside".as_slice()),
            ("dir", b"central"),
            ("hash", b"hash"),
        ] {
            fs::write(
                &settings,
                format!("return {{document_metadata_folder='{mode}'}}"),
            )
            .unwrap();
            assert_eq!(lookup(), expected, "{mode}");
        }
        // Renaming an edition keeps its sampled hash. A different edition
        // whose path shares the old filename must reach the lower priorities.
        fs::rename(&physical, docs.join("Shelf/Renamed.epub")).unwrap();
        assert_eq!(
            l.koreader_custom_metadata(Path::new("Shelf/Renamed.epub"))
                .unwrap()
                .unwrap(),
            b"hash"
        );
        fs::write(&physical, b"authorial edition 2").unwrap();
        assert_eq!(lookup(), b"beside");
        fs::remove_file(adjacent.join(CUSTOM_FILENAME)).unwrap();
        assert_eq!(lookup(), b"central");
        fs::remove_file(central.join(CUSTOM_FILENAME)).unwrap();
        assert!(l.koreader_custom_metadata(book).unwrap().is_none());
        assert_eq!(fs::read(hashed.join(CUSTOM_FILENAME)).unwrap(), b"hash");
        assert_eq!(
            fs::read_to_string(settings).unwrap(),
            "return {document_metadata_folder='hash'}"
        );
    }

    #[test]
    fn central_custom_metadata_rejects_ambiguous_paths_and_untrusted_mount_entries() {
        let scratch = Scratch::new();
        let mount = scratch.0.join("Mount");
        let docs = mount.join("documents");
        let external = scratch.0.join("Outside");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir(&external).unwrap();
        let physical = docs.join("Book.pdf");
        let book = Path::new("Book.pdf");
        fs::write(&physical, b"book").unwrap();
        let l = Library::open(&docs, &scratch.0.join("library.json")).unwrap();
        let ko = mount.join("koreader");
        let adjacent = physical.with_extension("sdr");
        write_sidecar(&adjacent, b"beside");
        let central = external.join("docsettings/mnt/us/documents/Book.sdr");
        write_sidecar(&central, b"outside");
        fs::write(
            external.join("settings.reader.lua"),
            "return {document_metadata_folder='dir'}",
        )
        .unwrap();
        symlink(&external, &ko).unwrap();
        assert_eq!(
            l.koreader_custom_metadata(book).unwrap().unwrap(),
            b"beside"
        );
        fs::remove_file(&ko).unwrap();
        fs::create_dir(&ko).unwrap();
        let settings = ko.join("settings.reader.lua");
        let backup = ko.join("settings.reader.lua.old");
        fs::write(&backup, "return {document_metadata_folder='dir'}").unwrap();
        symlink(external.join("settings.reader.lua"), &settings).unwrap();
        let local = ko
            .join("docsettings")
            .join(docs.canonicalize().unwrap().strip_prefix("/").unwrap())
            .join("Book.sdr");
        write_sidecar(&local, b"native");
        assert_eq!(
            l.koreader_custom_metadata(book).unwrap().unwrap(),
            b"native"
        );
        fs::remove_file(&settings).unwrap();
        fs::write(
            &settings,
            "return {document_metadata_folder='dir', extra=do_work()}",
        )
        .unwrap();
        assert_eq!(
            l.koreader_custom_metadata(book).unwrap().unwrap(),
            b"native"
        );
        let mirrored = ko.join("docsettings/mnt/us/documents/Book.sdr");
        write_sidecar(&mirrored, b"mirror");
        assert!(
            l.koreader_custom_metadata(book)
                .unwrap_err()
                .to_string()
                .contains("Ambiguous")
        );
        fs::remove_file(mirrored.join(CUSTOM_FILENAME)).unwrap();
        fs::remove_file(local.join(CUSTOM_FILENAME)).unwrap();
        symlink(central.join(CUSTOM_FILENAME), local.join(CUSTOM_FILENAME)).unwrap();
        assert!(l.koreader_custom_metadata(book).is_err());
        assert_eq!(fs::read(central.join(CUSTOM_FILENAME)).unwrap(), b"outside");
    }

    #[test]
    fn central_custom_metadata_uses_pinned_stock_fingerprints_for_authored_formats() {
        let scratch = Scratch::new();
        let docs = scratch.0.join("documents");
        let ko = scratch.0.join("koreader");
        fs::create_dir(&docs).unwrap();
        fs::create_dir(&ko).unwrap();
        fs::write(
            ko.join("settings.reader.lua"),
            "return {document_metadata_folder='hash'}",
        )
        .unwrap();
        let l = Library::open(&docs, &scratch.0.join("library.json")).unwrap();
        // KOReader's native util.partialMD5 on the two authored files.
        for (name, fixture, fingerprint) in [
            (
                "Prose.epub",
                "field-notes-epub3.epub",
                "71bcf61d42cc5f9e83a2bceb144ad66f",
            ),
            (
                "Scan.pdf",
                "scan-reflow/vector.pdf",
                "e5bc48795bd17d2b2a42dcd017d67131",
            ),
        ] {
            fs::copy(crate::test_fixture(fixture), docs.join(name)).unwrap();
            let located = ko
                .join("hashdocsettings")
                .join(&fingerprint[..2])
                .join(format!("{fingerprint}.sdr"));
            write_sidecar(&located, name.as_bytes());
            assert_eq!(
                l.koreader_custom_metadata(Path::new(name))
                    .unwrap()
                    .unwrap(),
                name.as_bytes()
            );
        }
    }
}
