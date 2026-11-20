//! Index single-volume, independent RAR comic pages without extracting to disk.

use std::cell::RefCell;
use std::fs::{self, File, Metadata};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Range;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use rars::{
    Archive, ArchiveMemberRef, ArchiveReadOptions, ArchiveReader, ErrorKind, ExtractionDecision,
    ReadCancellation,
};

use crate::Result;
use crate::comic::{MAX_COMIC_BYTES, MAX_IMAGE_BYTES, natural_cmp, page_name, raster_input_limit};

pub(crate) const MAX_CBR_BYTES: u64 = MAX_COMIC_BYTES;
const MAX_HEADERS: u64 = 8192;
const MAX_HEADER_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WORKSPACE: u64 = 32 * 1024 * 1024;
const MAX_DICTIONARY: u64 = 16 * 1024 * 1024;
const MAX_BUFFERED_RAR5: u64 = 4 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(3);
const RAR4: &[u8] = b"Rar!\x1a\x07\x00";
const RAR5: &[u8] = b"Rar!\x1a\x07\x01\x00";
const UNSUPPORTED: &str =
    "Reader supports non-solid, unencrypted, single-volume RAR4 and RAR5 comics";
const CHANGED: &str = "This CBR changed after it was opened";

pub(crate) struct Cbr {
    path: PathBuf,
    stamp: ArchiveStamp,
    pages: Vec<Page>,
}

#[derive(PartialEq, Eq)]
struct ArchiveStamp {
    length: u64,
    modified: Option<SystemTime>,
}

#[derive(PartialEq, Eq)]
struct Page {
    name: String,
    index: usize,
    size: u64,
    packed: Range<usize>,
    integrity: Integrity,
}

#[derive(PartialEq, Eq)]
enum Integrity {
    Rar4 {
        header_crc: u16,
        data_crc: u32,
    },
    Rar5 {
        header_crc: u32,
        data_crc: Option<u32>,
        hash: Option<[u8; 32]>,
    },
}

impl Cbr {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let deadline = Deadline::start()?;
        let (archive, stamp) = read_archive(path, &deadline)?;
        let mut pages = Vec::new();
        for (index, member) in archive.member_refs().enumerate() {
            if let Some(page) = indexed_page(member, index)? {
                pages.push(page);
            }
        }
        if pages.is_empty() {
            return Err("This CBR contains no supported image pages".into());
        }
        pages.sort_by(|a, b| natural_cmp(&a.name, &b.name).then_with(|| a.index.cmp(&b.index)));
        if pages.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err("This CBR contains duplicate image paths".into());
        }
        Ok(Self {
            path: path.to_owned(),
            stamp,
            pages,
        })
    }

    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn page_name(&self, page: usize) -> Option<&str> {
        self.pages.get(page).map(|page| page.name.as_str())
    }

    /// Reading-state identity based on the verified header index and declared payload checksums.
    pub(crate) fn identity(&self) -> u64 {
        let mut hash = crate::revision_bytes(b"reader-cbr-page-index-v1\0");
        let mut add = |bytes: &[u8]| {
            for &byte in bytes {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        };
        for page in &self.pages {
            add(&(page.name.len() as u64).to_le_bytes());
            add(page.name.as_bytes());
            add(&(page.packed.len() as u64).to_le_bytes());
            add(&page.size.to_le_bytes());
            match &page.integrity {
                Integrity::Rar4 { data_crc, .. } => {
                    add(&[4]);
                    add(&data_crc.to_le_bytes());
                }
                Integrity::Rar5 { data_crc, hash, .. } => {
                    add(&[5, u8::from(data_crc.is_some()), u8::from(hash.is_some())]);
                    if let Some(crc) = data_crc {
                        add(&crc.to_le_bytes());
                    }
                    if let Some(digest) = hash {
                        add(digest);
                    }
                }
            }
        }
        hash
    }

    pub(crate) fn read_page(&self, page: usize) -> Result<Vec<u8>> {
        let expected = self
            .pages
            .get(page)
            .ok_or("That page is outside this CBR")?;
        let deadline = Deadline::start()?;
        let (archive, stamp) = read_archive(&self.path, &deadline)?;
        if stamp != self.stamp {
            return Err(CHANGED.into());
        }
        let current = archive.member_refs().nth(expected.index).ok_or(CHANGED)?;
        if indexed_page(current, expected.index)?.as_ref() != Some(expected) {
            return Err(CHANGED.into());
        }

        // The fixed writer is outside the decoder's workspace ledger and never grows.
        let buffer = vec![0; expected.size as usize].into_boxed_slice();
        let output = Rc::new(RefCell::new(Cursor::new(buffer)));
        let mut index = 0;
        archive
            .extract_with_control(deadline.options(expected.size), |_| {
                let decision = match index.cmp(&expected.index) {
                    std::cmp::Ordering::Less => ExtractionDecision::Skip,
                    std::cmp::Ordering::Equal => {
                        ExtractionDecision::Extract(Box::new(PageOutput(Rc::clone(&output))))
                    }
                    std::cmp::Ordering::Greater => ExtractionDecision::Stop,
                };
                index += 1;
                Ok(decision)
            })
            .map_err(archive_error)?;
        let cursor = Rc::try_unwrap(output)
            .map_err(|_| "CBR page output is still in use")?
            .into_inner();
        if cursor.position() != expected.size {
            return Err("The CBR page data is incomplete".into());
        }
        Ok(cursor.into_inner().into_vec())
    }
}

fn read_archive(path: &Path, deadline: &Deadline) -> Result<(Archive, ArchiveStamp)> {
    let named = fs::symlink_metadata(path)?;
    if !named.is_file() {
        return Err("Reader can only open a regular CBR file".into());
    }
    let mut file = File::open(path)?;
    let current = file.metadata()?;
    if !current.is_file() || !same_file(&named, &current) {
        return Err(CHANGED.into());
    }
    let stamp = ArchiveStamp {
        length: current.len(),
        modified: current.modified().ok(),
    };
    if stamp.length > MAX_CBR_BYTES {
        return Err("This CBR exceeds Reader's 512 MiB archive limit".into());
    }
    let mut signature = [0; 8];
    if file.read_exact(&mut signature).is_err() || !matches_signature(&signature) {
        return Err("This CBR has no supported RAR4 or RAR5 signature".into());
    }
    file.seek(SeekFrom::Start(0))?;
    // The owned-reader entry point in rars applies the parsing limits for both families.
    let archive = ArchiveReader::read_reader_with_options(file, deadline.options(MAX_IMAGE_BYTES))
        .map_err(archive_error)?;
    validate_archive(&archive, stamp.length)?;
    Ok((archive, stamp))
}

fn same_file(before: &Metadata, opened: &Metadata) -> bool {
    #[cfg(unix)]
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return false;
    }
    before.len() == opened.len() && before.modified().ok() == opened.modified().ok()
}

fn matches_signature(bytes: &[u8; 8]) -> bool {
    bytes.starts_with(RAR4) || bytes.starts_with(RAR5)
}

fn validate_archive(archive: &Archive, length: u64) -> Result<()> {
    let complete = match archive {
        Archive::Rar15To40(rar)
            if rar.sfx_offset == 0
                && !rar.main.is_solid()
                && !rar.main.is_volume()
                && !rar.main.has_encrypted_headers() =>
        {
            match rar.blocks.last() {
                Some(rars::rar15_40::Block::End(header)) => {
                    header.flags & 1 == 0
                        && header.offset as u64 + u64::from(header.head_size) == length
                }
                // Some legacy writers end at the final file's complete payload.
                Some(rars::rar15_40::Block::File(file)) => file.packed_range.end as u64 == length,
                _ => false,
            }
        }
        Archive::Rar50Plus(rar)
            if rar.sfx_offset == 0
                && !rar.main.is_solid()
                && !rar.main.is_volume()
                && !rar.main.encrypted_headers =>
        {
            matches!(rar.blocks.last(), Some(rars::rar50::Block::End(end))
                if !end.has_next_volume() && end.block.data_range.end as u64 == length)
        }
        _ => return Err(UNSUPPORTED.into()),
    };
    if !complete {
        return Err("This CBR has no complete final archive record".into());
    }
    for member in archive.member_refs() {
        let forbidden = match member {
            ArchiveMemberRef::Rar15To40(file) => file.is_encrypted() || file.is_split_after(),
            ArchiveMemberRef::Rar50Plus(file) => file.encrypted || file.is_split_after(),
            _ => true,
        };
        if forbidden || member.is_split_before() || member.is_solid() {
            return Err(UNSUPPORTED.into());
        }
    }
    Ok(())
}

fn indexed_page(member: ArchiveMemberRef<'_>, index: usize) -> Result<Option<Page>> {
    if member.is_directory() || member.is_redirection() {
        return Ok(None);
    }
    let legacy = matches!(member, ArchiveMemberRef::Rar15To40(_));
    let decoded = member.decoded_name(None).map_err(archive_error)?;
    let Ok(name) = std::str::from_utf8(&decoded) else {
        return Ok(None);
    };
    let name = if legacy && name.contains('\\') {
        name.replace('\\', "/")
    } else {
        name.to_owned()
    };
    if !page_name(&name) {
        return Ok(None);
    }
    let (size, packed, integrity) = match member {
        ArchiveMemberRef::Rar15To40(file) => {
            if file.host_os == 3 && !regular_unix_mode(u64::from(file.attr)) {
                return Ok(None);
            }
            if file.method != 0x30 && (!(0x31..=0x35).contains(&file.method) || file.unp_ver != 29)
            {
                return Err("This CBR page needs an unsupported RAR4 decoder".into());
            }
            (
                file.unp_size,
                file.packed_range.clone(),
                Integrity::Rar4 {
                    header_crc: file.block.head_crc,
                    data_crc: file.file_crc,
                },
            )
        }
        ArchiveMemberRef::Rar50Plus(file) => {
            if file.host_os == 1 && !regular_unix_mode(file.attributes) {
                return Ok(None);
            }
            let compress = file.decoded_compression_info().map_err(archive_error)?;
            if compress.method > 5 || (compress.method != 0 && compress.algorithm_version != 0) {
                return Err("This CBR page needs an unsupported RAR5 decoder".into());
            }
            if compress.method != 0 && compress.dictionary_size > MAX_DICTIONARY {
                return Err("This CBR page exceeds Reader's 16 MiB RAR5 dictionary limit".into());
            }
            let size = file
                .known_unpacked_size()
                .ok_or("This CBR page declares no output size")?;
            let hash = match &file.hash {
                None => None,
                Some(hash) if hash.hash_type == 0 => Some(
                    hash.data
                        .as_slice()
                        .try_into()
                        .map_err(|_| "This CBR page has an invalid checksum")?,
                ),
                Some(_) => return Err("This CBR page uses an unsupported checksum".into()),
            };
            if file.data_crc32.is_none() && hash.is_none() {
                return Err("This CBR page declares no checksum".into());
            }
            (
                size,
                file.block.data_range.clone(),
                Integrity::Rar5 {
                    header_crc: file.block.header_crc,
                    data_crc: file.data_crc32,
                    hash,
                },
            )
        }
        _ => return Err(UNSUPPORTED.into()),
    };
    if !(1..=raster_input_limit(Path::new(&name))).contains(&size)
        || !(1..=MAX_IMAGE_BYTES).contains(&(packed.len() as u64))
    {
        return Err("This CBR page exceeds Reader's image limits or is empty".into());
    }
    Ok(Some(Page {
        name,
        index,
        size,
        packed,
        integrity,
    }))
}

fn regular_unix_mode(mode: u64) -> bool {
    matches!(mode & 0o170000, 0 | 0o100000)
}

fn archive_error(error: rars::Error) -> &'static str {
    match error.kind() {
        ErrorKind::UnsupportedFeature
        | ErrorKind::UnsupportedFormat
        | ErrorKind::PasswordRequired => UNSUPPORTED,
        ErrorKind::ResourceLimit => "This CBR exceeds Reader's archive or decompression limits",
        ErrorKind::Cancelled => "Reading this CBR exceeded Reader's time limit",
        ErrorKind::ChecksumMismatch => "This CBR failed its checksum",
        _ => "This CBR archive or page data is invalid",
    }
}

struct PageOutput(Rc<RefCell<Cursor<Box<[u8]>>>>);

impl Write for PageOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Deadline {
    token: ReadCancellation,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Deadline {
    fn start() -> io::Result<Self> {
        let token = ReadCancellation::new();
        let cancel = token.clone();
        let (stop, exit) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("reader-cbr-watchdog".to_owned())
            .spawn(move || {
                if matches!(
                    exit.recv_timeout(READ_TIMEOUT),
                    Err(RecvTimeoutError::Timeout)
                ) {
                    cancel.cancel();
                }
            })?;
        Ok(Self {
            token,
            stop,
            worker: Some(worker),
        })
    }

    fn options(&self, output: u64) -> ArchiveReadOptions<'_> {
        ArchiveReadOptions::new()
            .with_cancellation(&self.token)
            .with_max_header_count(MAX_HEADERS)
            .with_max_header_bytes(MAX_HEADER_BYTES)
            .with_max_reader_workspace_bytes(MAX_WORKSPACE)
            .with_rar50_dictionary_size_limit(MAX_DICTIONARY)
            .with_rar50_buffered_decode_limit(MAX_BUFFERED_RAR5)
            .with_max_member_output_bytes(output)
            .with_max_total_output_bytes(output)
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::comic::Comic;

    struct Fixture(PathBuf);

    impl Fixture {
        fn write(bytes: &[u8]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-cbr-{}-{}.cbr",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            let fixture = Self(path);
            file.write_all(bytes).unwrap();
            fixture
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb88320 } else { 0 };
            }
        }
        !crc
    }

    fn rar4(members: &[(&str, &[u8])], archive_flags: u16, file_flags: u16) -> Vec<u8> {
        let mut archive = RAR4.to_vec();
        let mut block = |kind: u8, flags: u16, fields: &[u8], data: &[u8]| {
            let mut header = vec![kind];
            header.extend(flags.to_le_bytes());
            header.extend((fields.len() as u16 + 7).to_le_bytes());
            header.extend_from_slice(fields);
            archive.extend((crc32(&header) as u16).to_le_bytes());
            archive.extend(header);
            archive.extend_from_slice(data);
        };
        block(0x73, archive_flags, &[0; 6], &[]);
        for &(name, data) in members {
            let mut fields = Vec::new();
            fields.extend((data.len() as u32).to_le_bytes());
            fields.extend((data.len() as u32).to_le_bytes());
            fields.push(3); // Unix
            fields.extend(crc32(data).to_le_bytes());
            fields.extend([0; 4]); // DOS timestamp
            fields.extend([29, 0x30]); // Unpack version and stored method
            fields.extend((name.len() as u16).to_le_bytes());
            fields.extend(0o100644u32.to_le_bytes());
            fields.extend_from_slice(name.as_bytes());
            block(0x74, 0x8000 | file_flags, &fields, data);
        }
        block(0x7b, 0, &[], &[]);
        archive
    }

    fn vint(bytes: &mut Vec<u8>, mut value: u64) {
        while value >= 128 {
            bytes.push((value as u8 & 127) | 128);
            value >>= 7;
        }
        bytes.push(value as u8);
    }

    fn rar5(members: &[(&str, &[u8])], archive_flags: u8, codec: u64) -> Vec<u8> {
        let mut archive = RAR5.to_vec();
        let mut block = |kind: u8, flags: u8, fields: &[u8], data: &[u8]| {
            let mut body = vec![kind, flags];
            if flags & 2 != 0 {
                vint(&mut body, data.len() as u64);
            }
            body.extend_from_slice(fields);
            let mut header = Vec::new();
            vint(&mut header, body.len() as u64);
            header.extend(body);
            archive.extend(crc32(&header).to_le_bytes());
            archive.extend(header);
            archive.extend_from_slice(data);
        };
        block(1, 0, &[archive_flags], &[]);
        for &(name, data) in members {
            let mut fields = vec![4]; // Data CRC present
            vint(&mut fields, data.len() as u64);
            vint(&mut fields, 0o100644);
            fields.extend(crc32(data).to_le_bytes());
            vint(&mut fields, codec);
            fields.push(1); // Unix
            vint(&mut fields, name.len() as u64);
            fields.extend_from_slice(name.as_bytes());
            block(2, 2, &fields, data);
        }
        block(5, 0, &[0], &[]);
        archive
    }

    #[test]
    fn cbr_opens_rar4_and_rar5_images_in_natural_order_through_comic() {
        let pages: [(&str, &[u8]); 5] = [
            ("pages/10.pgm", b"P5\n2 1\n255\n\x10\x11"),
            ("pages/2.pgm", b"P5\n2 1\n255\n\x20\x21"),
            ("pages/1.pgm", b"P5\n2 1\n255\n\x30\x31"),
            ("../unsafe.pgm", b"ignored"),
            ("notes.txt", b"ignored"),
        ];
        let legacy = rar4(&pages, 0, 0);
        let no_end = legacy[..legacy.len() - 7].to_vec();
        for bytes in [legacy, no_end, rar5(&pages, 0, 0)] {
            let fixture = Fixture::write(&bytes);
            let book = Comic::open(&fixture.0).unwrap();
            assert_eq!(book.page_count(), 3);
            assert_eq!(book.page_name(0), Some("pages/1.pgm"));
            assert_eq!(book.page_name(1), Some("pages/2.pgm"));
            assert_eq!(book.page_name(2), Some("pages/10.pgm"));
            assert_eq!(
                book.decode_page(1).unwrap().fit(2, 1, 0).unwrap(),
                [0x20, 0x21]
            );
            assert_eq!(
                book.decode_page(0).unwrap().fit(2, 1, 0).unwrap(),
                [0x30, 0x31]
            );
        }
    }

    #[test]
    fn cbr_random_pages_skip_other_payloads_and_selected_crc_is_verified() {
        let pages = [
            ("2.png", &b"earlier corrupted bytes"[..]),
            ("1.png", &b"later valid bytes"[..]),
        ];
        for mut bytes in [rar4(&pages, 0, 0), rar5(&pages, 0, 0)] {
            let corrupt = bytes
                .windows(pages[0].1.len())
                .position(|part| part == pages[0].1)
                .unwrap();
            bytes[corrupt] ^= 1;
            let fixture = Fixture::write(&bytes);
            let book = Cbr::open(&fixture.0).unwrap();
            assert_eq!(book.read_page(0).unwrap(), pages[1].1);
            let error = book.read_page(1).unwrap_err();
            assert!(error.to_string().contains("checksum"), "{error}");
        }
    }

    #[test]
    fn cbr_identity_ignores_file_path_and_detects_revised_page_crc() {
        let original = rar5(&[("page.png", b"original")], 0, 0);
        let a = Fixture::write(&original);
        let b = Fixture::write(&original);
        let changed = Fixture::write(&rar5(&[("page.png", b"revision")], 0, 0));
        let id = Cbr::open(&a.0).unwrap().identity();
        assert_eq!(Cbr::open(&b.0).unwrap().identity(), id);
        assert_ne!(Cbr::open(&changed.0).unwrap().identity(), id);
    }

    #[test]
    fn cbr_rejects_multi_volume_solid_encrypted_duplicate_and_incomplete_archives() {
        let pages = [("1.png", &b"image"[..])];
        let duplicate = [pages[0], pages[0]];
        for invalid in [
            rar4(&pages, 1, 0),    // volume
            rar4(&pages, 8, 0),    // archive solid
            rar4(&pages, 0, 0x10), // member solid
            rar4(&pages, 0, 4),    // member encrypted
            rar4(&pages, 0, 2),    // split member
            rar5(&pages, 4, 0),    // archive solid
            rar4(&duplicate, 0, 0),
            rar5(&duplicate, 0, 0),
            rar5(&[("readme.txt", b"only text")], 0, 0),
        ] {
            let fixture = Fixture::write(&invalid);
            assert!(Cbr::open(&fixture.0).is_err());
        }
        let mut bytes = rar5(&pages, 0, 0);
        bytes.pop();
        let fixture = Fixture::write(&bytes);
        assert!(Cbr::open(&fixture.0).is_err());

        let bytes = rar5(&pages, 0, (1 << 7) | (8 << 10)); // 32 MiB window
        let fixture = Fixture::write(&bytes);
        assert!(
            Cbr::open(&fixture.0)
                .err()
                .unwrap()
                .to_string()
                .contains("dictionary limit")
        );
    }
}
