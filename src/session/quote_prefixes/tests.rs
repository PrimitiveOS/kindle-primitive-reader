use super::*;
use crate::book_style::pseudo::quotes::take_historical_items;
use crate::session::{BookConfig, Reading, ReadingViewport};
use crate::{BaseStylesheet, Engine, Probe, Settings};
use primitive_app::App;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

const CHAPTERS: usize = 82;
const EARLY: usize = 8;
const LATE: usize = 75;

struct Fixture(PathBuf);

impl Fixture {
    fn write(bytes: &[u8]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "reader-quote-session-{}-{nonce}.epub",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(crate) fn epub(seed: &str, chapters: usize, padding: usize) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut member = |name: &str, text: &str| {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    };
    member("mimetype", "application/epub+zip");
    member(
        "META-INF/container.xml",
        r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    );
    let mut manifest = String::new();
    let mut spine = String::new();
    for index in 0..chapters {
        manifest.push_str(&format!(r#"<item id="p{index}" href="chapter-{index}.xhtml" media-type="application/xhtml+xml"/>"#));
        spine.push_str(&format!(r#"<itemref idref="p{index}"/>"#));
    }
    member(
        "book.opf",
        &format!(
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:primitiveos:quote-session</dc:identifier><dc:title>Quote session</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-11-25T00:00:00Z</meta></metadata><manifest>{manifest}<item id="css" href="seed.css" media-type="text/css"/></manifest><spine>{spine}</spine></package>"#
        ),
    );
    member("seed.css", &format!(".seed::before {{ content: {seed}; }}"));
    let padding = "p".repeat(padding);
    for index in 0..chapters {
        let content = if index == 0 {
            r#"<span class="seed">Opening seed</span>"#.to_string()
        } else if index % 7 == 0 {
            // A chapter without an active quote still extends a known boundary.
            format!("Unquoted chapter {index}")
        } else {
            format!("<q>Quoted chapter {index}</q>")
        };
        member(
            &format!("chapter-{index}.xhtml"),
            &format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en"><head><link rel="stylesheet" href="seed.css"/></head><body><p>{content}</p><!--{padding}--></body></html>"#
            ),
        );
    }
    writer.finish().unwrap().into_inner()
}

pub(super) fn config(path: &Path) -> BookConfig {
    BookConfig {
        viewport: ReadingViewport::PORTRAIT,
        user_styles: Default::default(),
        user_hyphenation: Default::default(),
        engine: Engine::Parley,
        book: path.to_owned(),
        fonts: vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings: Settings {
            width: 860,
            height: 920,
            font_size: 22.0,
            base_stylesheet: BaseStylesheet::Traditional,
            ..Settings::default()
        },
        scroll: false,
        facing_pages: false,
        hide_nonlinear: false,
        hidden_flows: Default::default(),
        chapter_offsets: Default::default(),
        quote_prefixes: Default::default(),
        screen_pages: Default::default(),
        eager_layouts: Default::default(),
        folder_snapshot: None,
    }
}

fn exact(config: &BookConfig, bytes: Arc<[u8]>, chapter: usize) -> Probe {
    Probe::from_book_with_rules(
        Book::open_snapshot(bytes).unwrap(),
        config.engine,
        &config.fonts,
        config.settings,
        chapter,
        None,
        &config.user_hyphenation,
        &config.user_styles,
    )
    .unwrap()
}

#[test]
fn quote_marks_foreground_cold_far_jump_keeps_partial_boundaries_and_warm_turns() {
    let bytes: Arc<[u8]> = epub("open-quote", CHAPTERS, 32 * 1024).into();
    let fixture = Fixture::write(&bytes);
    let config = config(&fixture.0);
    let expected = exact(&config, bytes, LATE).capture(0).unwrap();
    take_historical_items();
    let cold_start = Instant::now();
    let cold = config.chapter(LATE).unwrap().capture(0).unwrap();
    let elapsed = cold_start.elapsed();
    let cold_items = take_historical_items();
    assert!((1..=24).contains(&cold_items));
    assert_eq!(config.quote_prefixes.latest(), Some(cold_items));
    assert_ne!(cold, expected);

    for _ in 0..LATE {
        let prior = config.quote_prefixes.latest().unwrap();
        if prior > LATE {
            break;
        }
        let mut tentative = config.chapter(LATE).unwrap();
        let next = config.quote_prefixes.latest().unwrap();
        assert!(next > prior);
        if next > LATE {
            assert_eq!(tentative.capture(0).unwrap(), expected);
        }
    }
    assert_eq!(config.quote_prefixes.latest(), Some(LATE + 1));
    take_historical_items();
    let mut reading = Reading::restore(config.clone(), LATE, 0, None).unwrap();
    for index in [LATE + 1, LATE, LATE + 1, LATE + 2, LATE + 1, LATE] {
        reading.jump(index, None).unwrap();
    }
    assert_eq!(take_historical_items(), 0);
    assert_eq!(reading.probe.capture(0).unwrap(), expected);
    eprintln!(
        "foreground quote cold jump, completed {cold_items} of {LATE} predecessors in {elapsed:?}"
    );
}

#[test]
fn quote_marks_foreground_invalidates_same_size_same_mtime_changes_and_usb() {
    let original = epub("open-quote", 16, 0);
    let changed = epub("none      ", 16, 0);
    assert_eq!(original.len(), changed.len());
    let fixture = Fixture::write(&original);
    let config = config(&fixture.0);
    let mut reading = Reading::restore(config.clone(), EARLY, 0, None).unwrap();
    let before = reading.probe.capture(0).unwrap();
    let revision = reading.probe.book;
    assert_eq!(config.quote_prefixes.latest(), Some(EARLY + 1));
    take_historical_items();
    let mut style = config.clone();
    style.settings.typography.respect_embedded = false;
    style.chapter(EARLY).unwrap();
    assert_eq!(take_historical_items(), EARLY);
    config.chapter(EARLY).unwrap();
    assert_eq!(take_historical_items(), 0);

    let modified = std::fs::metadata(&fixture.0).unwrap().modified().unwrap();
    std::fs::write(&fixture.0, changed).unwrap();
    std::fs::File::open(&fixture.0)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert_eq!(
        std::fs::metadata(&fixture.0).unwrap().modified().unwrap(),
        modified
    );
    let mut after = config.chapter(EARLY).unwrap();
    assert_eq!(after.book, revision);
    assert_eq!(take_historical_items(), EARLY);
    assert_ne!(after.capture(0).unwrap(), before);
    reading.storage_returned().unwrap();
    assert_eq!(config.quote_prefixes.latest(), None);
    config.chapter(EARLY).unwrap();
    assert_eq!(take_historical_items(), EARLY);
    std::fs::remove_file(&fixture.0).unwrap();
    assert!(config.chapter(EARLY).is_err());
    assert_eq!(config.quote_prefixes.latest(), None);
}

#[test]
fn quote_marks_foreground_sequential_work_compared_to_independent_reopens() {
    let chapters = 36;
    let bytes: Arc<[u8]> = epub("open-quote", chapters, 12 * 1024).into();
    let fixture = Fixture::write(&bytes);
    let config = config(&fixture.0);
    take_historical_items();
    let start = Instant::now();
    let mut baseline_items = 0;
    for chapter in 0..chapters {
        exact(&config, bytes.clone(), chapter);
        baseline_items += take_historical_items();
    }
    let baseline = start.elapsed();
    // Every seventh chapter has no quote and needs no incoming plan.
    let expected: usize = (1..chapters).filter(|chapter| chapter % 7 != 0).sum();
    assert_eq!(baseline_items, expected);
    let start = Instant::now();
    for chapter in 0..chapters {
        config.chapter(chapter).unwrap();
    }
    let sequential = start.elapsed();
    let sequential_items = take_historical_items();
    assert_eq!(sequential_items, 0);
    assert_eq!(config.quote_prefixes.latest(), Some(chapters));
    eprintln!(
        "foreground quote benchmark, {chapters} chapters: independent {baseline_items} prior items in {baseline:?}, shared {sequential_items} prior items in {sequential:?}"
    );
}
