use super::*;
use crate::{Engine, Probe, user_hyphenation};
use std::fs::File;
use std::io::{Cursor, Write};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn copy() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "reader-quote-worker-{}-{nonce}.epub",
            std::process::id()
        ));
        let bytes = super::super::tests::epub("open-quote", 82, 4096);
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn context(path: &Path) -> (Request, ForegroundQuotes) {
    let config = super::super::tests::config(path);
    let probe = config.chapter(75).unwrap();
    let need = config.quote_prefixes.need().unwrap();
    let request = Request::from_need(
        1,
        path,
        &config.quote_prefixes,
        &need,
        probe.book,
        probe.chapter_entries(),
        75,
        probe.settings,
    )
    .expect("deep jump should leave an incomplete quote marker");
    (request, config.quote_prefixes)
}

fn probe(book: Book, request: &Request) -> Probe {
    Probe::from_book_with_rules(
        book,
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        request.settings,
        request.chapter,
        None,
        &user_hyphenation::Rules::default(),
        &request.styles,
    )
    .unwrap()
}

#[test]
fn quote_worker_exact_prefix_matches_snapshot_and_preserves_source() {
    let fixture = Fixture::copy();
    let before = std::fs::read(&fixture.0).unwrap();
    let (request, cache) = context(&fixture.0);
    let prepared = request
        .clone()
        .run(&Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert!(prepared.install(&request, &cache).unwrap());
    assert!(cache.need().is_some());

    let mut foreground = Book::open(&fixture.0).unwrap();
    let stamp = cache.prepare(&fixture.0, &mut foreground).unwrap().unwrap();
    let mut opened = Some(foreground);
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut actual = Probe::layout_opened_book(
        &mut opened,
        Engine::Parley,
        request.settings,
        request.chapter,
        None,
        &user_hyphenation::Rules::default(),
        &request.styles,
        || crate::font_context(&fonts),
    )
    .unwrap();
    cache
        .save(&fixture.0, opened.as_mut().unwrap(), stamp)
        .unwrap();
    assert!(cache.need().is_none());
    let mut expected = probe(
        Book::open_snapshot(before.clone().into()).unwrap(),
        &request,
    );
    assert_eq!(actual.capture(0).unwrap(), expected.capture(0).unwrap());
    assert_eq!(std::fs::read(&fixture.0).unwrap(), before);
}

#[test]
fn quote_worker_large_epub_uses_leased_reads_and_keeps_the_same_anchor() {
    let original = Fixture::copy();
    let padded = Fixture::copy();
    let bytes = std::fs::read(&original.0).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(File::create(&padded.0).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        output.start_file(entry.name(), options).unwrap();
        std::io::copy(&mut entry, &mut output).unwrap();
    }
    output.start_file("unrelated-image.bin", options).unwrap();
    let chunk = [42_u8; 64 * 1024];
    for _ in 0..144 {
        output.write_all(&chunk).unwrap();
    }
    output.finish().unwrap();
    let before = std::fs::read(&padded.0).unwrap();
    assert!(before.len() as u64 > ARCHIVE_LIMIT);
    let (request, cache) = context(&padded.0);
    let prepared = request
        .clone()
        .run(&Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert!(prepared.install(&request, &cache).unwrap());
    let mut foreground = Book::open(&padded.0).unwrap();
    let stamp = cache.prepare(&padded.0, &mut foreground).unwrap().unwrap();
    let mut opened = Some(foreground);
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut actual = Probe::layout_opened_book(
        &mut opened,
        Engine::Parley,
        request.settings,
        request.chapter,
        None,
        &user_hyphenation::Rules::default(),
        &request.styles,
        || crate::font_context(&fonts),
    )
    .unwrap();
    cache
        .save(&padded.0, opened.as_mut().unwrap(), stamp)
        .unwrap();
    let anchor = actual.matches("Quoted chapter 75").remove(0);
    let mut expected = probe(
        Book::open_snapshot(before.clone().into()).unwrap(),
        &request,
    );
    assert_eq!(actual.page_for(&anchor), expected.page_for(&anchor));
    assert_eq!(actual.capture(0).unwrap(), expected.capture(0).unwrap());
    assert_eq!(std::fs::read(&padded.0).unwrap(), before);
}

#[test]
fn quote_worker_rejects_stale_epoch_target_profile_and_edition() {
    let fixture = Fixture::copy();
    let (original, cache) = context(&fixture.0);
    let changed = [
        {
            let mut v = original.clone();
            v.epoch += 1;
            v
        },
        {
            let mut v = original.clone();
            v.chapter -= 1;
            v
        },
        {
            let mut v = original.clone();
            v.settings.font_size += 1.0;
            v
        },
        {
            let mut v = original.clone();
            v.edition ^= 1;
            v
        },
        {
            let mut v = original.clone();
            v.main_language = "fr".into();
            v
        },
        {
            let mut v = original.clone();
            v.spine.reverse();
            v
        },
    ];
    for current in changed {
        let prepared = original
            .clone()
            .run(&Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert!(!prepared.install(&current, &cache).unwrap());
        assert!(cache.need().is_some());
    }
    let prepared = original
        .clone()
        .run(&Arc::new(AtomicBool::new(false)))
        .unwrap();
    cache.clear();
    assert!(!prepared.install(&original, &cache).unwrap());
}

#[test]
fn quote_worker_cancels_and_rechecks_replaced_file_before_install() {
    let fixture = Fixture::copy();
    let (request, cache) = context(&fixture.0);
    assert!(
        request
            .clone()
            .run(&Arc::new(AtomicBool::new(true)))
            .is_err()
    );
    let prepared = request
        .clone()
        .run(&Arc::new(AtomicBool::new(false)))
        .unwrap();
    let metadata = std::fs::metadata(&fixture.0).unwrap();
    let modified = metadata.modified().unwrap();
    let bytes = std::fs::read(&fixture.0).unwrap();
    std::fs::write(&fixture.0, &bytes).unwrap();
    std::fs::File::open(&fixture.0)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(prepared.install(&request, &cache).is_err());
    assert!(cache.need().is_some());
    assert_eq!(std::fs::read(&fixture.0).unwrap(), bytes);
}

#[test]
fn quote_worker_rejects_oversized_archive_before_reading_it() {
    let fixture = Fixture::copy();
    let (mut request, _) = context(&fixture.0);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fixture.0)
        .unwrap()
        .set_len(MAX_ARCHIVE + 1)
        .unwrap();
    request.stamp = FileStamp::read(&fixture.0).unwrap();
    assert!(request.run(&Arc::new(AtomicBool::new(false))).is_err());
}

#[test]
fn quote_worker_job_cancel_cannot_install_a_finished_result() {
    let fixture = Fixture::copy();
    let (request, _) = context(&fixture.0);
    let job = Job::start(request).unwrap();
    job.cancel();
    while !job.finished() {
        std::thread::yield_now();
    }
    assert!(job.collect().is_err());
}

#[test]
fn quote_worker_charges_each_repeated_spine_item_against_the_decoded_limit() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in [
        ("mimetype", "application/epub+zip".to_owned()),
        ("META-INF/container.xml", r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_owned()),
        ("book.opf", format!(r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:quote-repeat</dc:identifier><dc:title>Repeated spine</dc:title></metadata><manifest><item id="same" href="same.xhtml" media-type="application/xhtml+xml"/><item id="tail" href="tail.xhtml" media-type="application/xhtml+xml"/></manifest><spine>{}<itemref idref="tail"/></spine></package>"#, "<itemref idref=\"same\"/>".repeat(35))),
        ("same.xhtml", format!("<html><body><p>repeated</p><!--{}--></body></html>", "x".repeat(480 * 1024))),
        ("tail.xhtml", "<html><body><p><q>tail</q></p></body></html>".to_owned()),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    assert!(bytes.len() < ARCHIVE_LIMIT as usize);
    let mut book = Book::open_snapshot(bytes.into()).unwrap();
    assert_eq!(book.chapters.len(), 36);
    let error = match quotes::prepare_exact(
        &mut book,
        35,
        Settings::default(),
        &user_styles::Snapshot::default(),
        None,
        "en",
        &|| Ok(()),
    ) {
        Ok(_) => panic!("repeated expanded text must exceed the worker budget"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("decoded text limit"));
}
