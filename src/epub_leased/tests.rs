use super::*;
use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(
            std::env::temp_dir().join(format!("reader-leased-{}-{nonce}.epub", std::process::id())),
        )
    }

    fn stamped(&self) -> FileStamp {
        FileStamp::read(&self.0).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn reader(path: &Path, stamp: FileStamp) -> (LeasedReader, Health) {
    LeasedReader::open(
        path,
        stamp,
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}

#[test]
fn leased_epub_zip_reads_small_members_without_copying_a_large_archive() {
    let fixture = Fixture::new();
    let file = File::create(&fixture.0).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", options).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("chapter.xhtml", options).unwrap();
    zip.write_all(b"<p>Small chapter</p>").unwrap();
    zip.start_file("large-unrelated-image.bin", options)
        .unwrap();
    let block = [37_u8; CHUNK_BYTES];
    for _ in 0..144 {
        zip.write_all(&block).unwrap();
    }
    zip.finish().unwrap();
    assert!(fs::metadata(&fixture.0).unwrap().len() > 8 * 1024 * 1024);
    let before = fs::metadata(&fixture.0).unwrap().len();
    let (reader, health) = reader(&fixture.0, fixture.stamped());
    let mut zip = zip::ZipArchive::new(reader).unwrap();
    assert!(zip.by_name("optional-missing.css").is_err());
    health.check().unwrap();
    let mut text = String::new();
    zip.by_name("chapter.xhtml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "<p>Small chapter</p>");
    health.verify().unwrap();
    assert!(health.0.bytes.load(Ordering::Acquire) < 1024 * 1024);
    assert_eq!(fs::metadata(&fixture.0).unwrap().len(), before);
}

#[test]
fn leased_epub_reads_and_seeks_in_bounded_chunks() {
    let fixture = Fixture::new();
    let mut file = File::create(&fixture.0).unwrap();
    let data = vec![42_u8; CHUNK_BYTES * 2 + 17];
    file.write_all(&data).unwrap();
    drop(file);
    let (mut reader, health) = reader(&fixture.0, fixture.stamped());
    let mut output = vec![0; data.len()];
    assert_eq!(reader.read(&mut output).unwrap(), CHUNK_BYTES);
    assert_eq!(
        reader.seek(SeekFrom::Current(2)).unwrap(),
        (CHUNK_BYTES + 2) as u64
    );
    assert_eq!(
        reader.seek(SeekFrom::End(-17)).unwrap(),
        (data.len() - 17) as u64
    );
    assert_eq!(reader.read(&mut output).unwrap(), 17);
    assert_eq!(reader.read(&mut output).unwrap(), 0);
    assert!(output[..17].iter().all(|&byte| byte == 42));
    health.verify().unwrap();
}

#[test]
fn leased_epub_cancellation_during_read_is_sticky_and_never_exposes_bytes() {
    let fixture = Fixture::new();
    fs::write(&fixture.0, vec![31_u8; 128]).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (mut reader, health) = LeasedReader::open(
        &fixture.0,
        fixture.stamped(),
        cancelled.clone(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    reader.after_read = Some(Box::new(move || cancelled.store(true, Ordering::Relaxed)));
    let mut output = [99_u8; 64];
    let error = reader.read(&mut output).unwrap_err();
    assert_ne!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(reader.position, 0);
    assert_eq!(output, [99_u8; 64]);
    assert!(health.check().is_err());
    assert!(reader.read(&mut output).is_err());
}

#[test]
fn leased_epub_replacement_during_read_is_sticky_and_does_not_advance() {
    let fixture = Fixture::new();
    let replacement = Fixture::new();
    fs::write(&fixture.0, vec![11_u8; 128]).unwrap();
    fs::write(&replacement.0, vec![22_u8; 128]).unwrap();
    let (mut reader, health) = reader(&fixture.0, fixture.stamped());
    let path = fixture.0.clone();
    let other = replacement.0.clone();
    reader.after_read = Some(Box::new(move || fs::rename(other, path).unwrap()));
    let mut output = [99_u8; 64];
    assert!(reader.read(&mut output).is_err());
    assert_eq!(reader.position, 0);
    assert_eq!(output, [99_u8; 64]);
    assert!(health.check().is_err());
}

#[test]
fn leased_epub_swallowed_io_errors_remain_fatal() {
    let fixture = Fixture::new();
    fs::write(&fixture.0, b"original resource").unwrap();
    let (mut reader, health) = reader(&fixture.0, fixture.stamped());
    fs::remove_file(&fixture.0).unwrap();
    let _ignored_optional_resource_error = reader.read(&mut [0; 8]);
    assert!(health.check().is_err());
    assert!(health.verify().is_err());
}

#[test]
fn leased_epub_enforces_byte_and_operation_budgets() {
    let fixture = Fixture::new();
    fs::write(&fixture.0, vec![7_u8; 256]).unwrap();
    let limits = Limits {
        bytes: 64,
        operations: 10,
    };
    let (mut reader, health) = LeasedReader::open_with_limits(
        &fixture.0,
        fixture.stamped(),
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
        limits,
    )
    .unwrap();
    assert_eq!(reader.read(&mut [0; 128]).unwrap(), 64);
    assert!(reader.read(&mut [0; 1]).is_err());
    assert_eq!(reader.position, 64);
    assert!(health.check().is_err());

    let limits = Limits {
        bytes: 256,
        operations: 1,
    };
    let (mut reader, health) = LeasedReader::open_with_limits(
        &fixture.0,
        fixture.stamped(),
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
        limits,
    )
    .unwrap();
    assert_eq!(reader.seek(SeekFrom::Start(4)).unwrap(), 4);
    assert!(reader.read(&mut [0; 1]).is_err());
    assert_eq!(reader.position, 4);
    assert!(health.check().is_err());
}

#[test]
fn leased_epub_rejects_expiry_and_invalid_seek_without_moving() {
    let fixture = Fixture::new();
    fs::write(&fixture.0, b"book").unwrap();
    assert!(
        LeasedReader::open(
            &fixture.0,
            fixture.stamped(),
            Arc::new(AtomicBool::new(false)),
            Instant::now(),
        )
        .is_err()
    );
    let (mut reader, health) = reader(&fixture.0, fixture.stamped());
    assert!(reader.seek(SeekFrom::Current(-1)).is_err());
    assert_eq!(reader.position, 0);
    assert!(health.check().is_err());
}

/// Runs only against a user-authorized Mac-local original. The path is never
/// included in output, panic messages, or assertion diagnostics.
#[test]
#[ignore = "requires a user-authorized EPUB path in READER_LEASED_ORIGINAL"]
fn leased_epub_authorized_original_matches_regular_members_and_keeps_source_hash() {
    use crate::epub::Book;
    use crate::epub_members::Members;
    use crate::source::{Source, Stylesheet};
    use ring::digest::{Context, SHA256};

    fn hash(path: &Path, stamp: &FileStamp, len: u64) -> Vec<u8> {
        let mut context = Context::new(&SHA256);
        let mut offset = 0_u64;
        while offset < len {
            let count = (len - offset).min(CHUNK_BYTES as u64) as usize;
            let chunk = with_storage(|| {
                let mut file = File::open(path)?;
                if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(stamp) {
                    return Err(io::Error::other("source changed"));
                }
                file.seek(SeekFrom::Start(offset))?;
                let mut chunk = vec![0; count];
                file.read_exact(&mut chunk)?;
                if FileStamp::from_metadata(&file.metadata()?).as_ref() != Some(stamp)
                    || FileStamp::read(path).as_ref() != Some(stamp)
                {
                    return Err(io::Error::other("source changed"));
                }
                Ok(chunk)
            });
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(_) => panic!("authorized source could not be hashed safely"),
            };
            context.update(&chunk);
            offset += count as u64;
        }
        context.finish().as_ref().to_vec()
    }

    fn digest(bytes: &[u8]) -> Vec<u8> {
        ring::digest::digest(&SHA256, bytes).as_ref().to_vec()
    }

    fn sample(
        book: &mut Book,
    ) -> Option<(
        u64,
        Vec<(String, bool, bool)>,
        Vec<(Option<Vec<u8>>, Vec<Option<Vec<u8>>>)>,
    )> {
        let count = book.chapters.len();
        if count == 0 {
            return None;
        }
        let spine = book
            .chapters
            .iter()
            .map(|chapter| (chapter.href.clone(), chapter.nonlinear, chapter.is_svg))
            .collect();
        let mut members = Vec::new();
        for index in [0, count / 2, count - 1] {
            let chapter_path = book.chapters[index].href.clone();
            let Ok(xhtml) = book.chapter(index) else {
                members.push((None, Vec::new()));
                continue;
            };
            let mut styles = Vec::new();
            if let Ok(source) = Source::parse(&xhtml) {
                for sheet in source.stylesheets.iter().take(4) {
                    if let Stylesheet::Linked(href) = sheet {
                        let value = Book::resolve(&chapter_path, href)
                            .ok()
                            .and_then(|path| book.resource_limited(&path, 96 * 1024).ok())
                            .map(|bytes| digest(&bytes));
                        styles.push(value);
                    }
                }
            }
            members.push((Some(digest(xhtml.as_bytes())), styles));
        }
        Some((book.revision(), spine, members))
    }

    let path = std::env::var_os("READER_LEASED_ORIGINAL")
        .map(PathBuf::from)
        .expect("set READER_LEASED_ORIGINAL to an authorized EPUB");
    let metadata = match with_storage(|| fs::metadata(&path)) {
        Ok(metadata) => metadata,
        Err(_) => panic!("authorized source metadata is unavailable"),
    };
    let len = metadata.len();
    assert!(
        len > 8 * 1024 * 1024 && len <= MAX_ARCHIVE,
        "authorized source is outside the leased archive-size range"
    );
    let stamp = match FileStamp::from_metadata(&metadata) {
        Some(stamp) => stamp,
        None => panic!("authorized source is not a regular stamped file"),
    };
    let before = hash(&path, &stamp, len);
    let expected = with_storage(|| {
        let mut book = Book::open(&path).map_err(io::Error::other)?;
        sample(&mut book).ok_or_else(|| io::Error::other("empty EPUB"))
    });
    let expected = match expected {
        Ok(expected) => expected,
        Err(_) => panic!("authorized source cannot be opened by the regular reader"),
    };
    assert!(
        expected.2.iter().any(|(chapter, _)| chapter.is_some()),
        "authorized EPUB has no readable sampled chapter"
    );
    let (reader, health) = match LeasedReader::open(
        &path,
        stamp.clone(),
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
    ) {
        Ok(value) => value,
        Err(_) => panic!("authorized source cannot be opened under a lease"),
    };
    let archive = match crate::epub_zip::open_leased(reader) {
        Ok(archive) => archive,
        Err(_) => panic!("authorized source is ineligible for bounded background ZIP reading"),
    };
    assert!(
        health.check().is_ok(),
        "leased ZIP open encountered an I/O failure"
    );
    let mut leased = match Book::from_archive(Members::Zip(archive)) {
        Ok(book) => book,
        Err(_) => panic!("authorized source cannot be parsed under a lease"),
    };
    assert!(
        health.check().is_ok(),
        "leased EPUB metadata encountered an I/O failure"
    );
    let actual = match sample(&mut leased) {
        Some(actual) => actual,
        None => panic!("leased EPUB has no readable spine"),
    };
    assert!(
        health.verify().is_ok(),
        "leased EPUB member reading encountered an I/O failure"
    );
    assert!(
        actual == expected,
        "leased and regular EPUB member data differ"
    );
    assert!(
        hash(&path, &stamp, len) == before,
        "authorized EPUB source bytes changed"
    );
}
