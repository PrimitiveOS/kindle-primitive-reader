use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};
use reader::epub::{Book, Destination, TocEntry};
use roxmltree::Document;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

fn fixture(name: &str) -> PathBuf {
    std::env::var_os("READER_TEST_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .join(name)
}

struct TemporaryBook(PathBuf);

impl Drop for TemporaryBook {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn temporary_document(extension: &str) -> (TemporaryBook, File) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "primitiveos-reader-test-{}-{nonce}.{extension}",
        std::process::id()
    ));
    let output = File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    (TemporaryBook(path), output)
}

fn replace_navigation(version: u8, member: &str, navigation: &str) -> TemporaryBook {
    let input = File::open(fixture(&format!("field-notes-epub{version}.epub"))).unwrap();
    let mut input = ZipArchive::new(input).unwrap();
    let (path, output) = temporary_document("epub");
    let mut output = ZipWriter::new(output);
    for index in 0..input.len() {
        let entry = input.by_index(index).unwrap();
        if entry.name() == member {
            output
                .start_file(member, SimpleFileOptions::default())
                .unwrap();
            output.write_all(navigation.as_bytes()).unwrap();
        } else {
            output.raw_copy_file(entry).unwrap();
        }
    }
    output.finish().unwrap();
    path
}

fn destination(path: &str, fragment: Option<&str>) -> Destination {
    Destination {
        path: path.to_owned(),
        fragment: fragment.map(str::to_owned),
    }
}

fn entry(label: &str, depth: usize, destination: Option<Destination>) -> TocEntry {
    TocEntry {
        label: label.to_owned(),
        depth,
        destination,
    }
}

#[test]
fn epub_navigation_preserves_depth_order_headings_and_fragments() {
    let xhtml = r##"<?xml version="1.0"?>
    <!DOCTYPE html>
    <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
        <body>
            <nav epub:type="page-list"><ol><li><a href="chapters/shore.xhtml">First page</a></li></ol></nav>
            <nav epub:type="toc"><ol>
                <li><a href="chapters/shore.xhtml">Reading the Shore</a><ol>
                    <li><a href="chapters/shore.xhtml#tide%2Da">First tide</a></li>
                    <li><span>Observations</span><ol hidden="hidden">
                        <li><a href="./chapters/shore.xhtml#lantern">The <em>lantern</em></a></li>
                    </ol></li>
                </ol></li>
                <li><a href="chapters/dusk.xhtml#door">Returning at Dusk</a></li>
            </ol></nav>
        </body>
    </html>"##;
    let ncx = r##"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
        <navMap>
            <navPoint id="a"><navLabel><text>Reading the Shore</text></navLabel>
                <content src="chapters/shore.xhtml"/>
                <navPoint id="b"><navLabel><text>First tide</text></navLabel>
                    <content src="chapters/shore.xhtml#tide%2Da"/>
                    <navPoint id="c"><navLabel><text>The lantern</text></navLabel>
                        <content src="./chapters/shore.xhtml#lantern"/>
                    </navPoint>
                </navPoint>
            </navPoint>
            <navPoint id="d"><navLabel><text>Returning at Dusk</text></navLabel>
                <content src="chapters/dusk.xhtml#door"/>
            </navPoint>
        </navMap>
    </ncx>"##;

    let shore = "EPUB/chapters/shore.xhtml";
    let dusk = "EPUB/chapters/dusk.xhtml";
    for (version, member, markup) in [(2, "EPUB/toc.ncx", ncx), (3, "EPUB/nav.xhtml", xhtml)] {
        let file = replace_navigation(version, member, markup);
        let book = Book::open(&file.0).unwrap();
        let mut expected = vec![
            entry("Reading the Shore", 0, Some(destination(shore, None))),
            entry("First tide", 1, Some(destination(shore, Some("tide-a")))),
        ];
        if version == 3 {
            expected.push(entry("Observations", 1, None));
        }
        expected.extend([
            entry("The lantern", 2, Some(destination(shore, Some("lantern")))),
            entry(
                "Returning at Dusk",
                0,
                Some(destination(dusk, Some("door"))),
            ),
        ]);
        assert_eq!(book.toc, expected);
        assert_eq!(book.chapter_index(shore), Some(0));
        assert_eq!(book.chapter_index(dusk), Some(1));
        assert_eq!(book.chapter_index(member), None);
        assert_eq!(book.chapters[0].label, "Reading the Shore");
        assert_eq!(book.chapters[1].label, "Returning at Dusk");
    }
}

#[test]
fn link_destinations_keep_local_fragments_without_redecoding_or_accepting_remote_sources() {
    let base = "EPUB/chapters/shore.xhtml";
    for (href, path, fragment) in [
        ("#tide-note", base, Some("tide-note")),
        ("#note-ref", base, Some("note-ref")),
        ("#", base, None),
        ("dusk.xhtml#door", "EPUB/chapters/dusk.xhtml", Some("door")),
        (
            "./caf%C3%A9%20notes.xhtml?x=1#%E7%AB%A0%3F%2523",
            "EPUB/chapters/café notes.xhtml",
            Some("章?%23"),
        ),
    ] {
        assert_eq!(
            Book::resolve_link(base, href).unwrap(),
            destination(path, fragment)
        );
    }
    for href in [
        "https://example.invalid/doc.xhtml#door",
        "//example.invalid/doc.xhtml",
        "/docs/doc.xhtml",
        "../../../doc.xhtml",
        "../%2e%2e/../doc.xhtml",
        "#incomplete%2",
        "#invalid%FF",
        "#control%00",
    ] {
        assert!(Book::resolve_link(base, href).is_err(), "{href}");
    }
    assert!(Book::resolve_link("../outside.xhtml", "#id").is_err());
}

#[test]
fn standalone_text_and_xhtml_use_a_single_chapter_without_resource_access() {
    let (text_file, mut data) = temporary_document("TXT");
    data.write_all(b"Reading & <tag>\r\nNext line\r\n\r\nOne more paragraph")
        .unwrap();
    drop(data);
    let mut text = Book::open(&text_file.0).unwrap();
    assert_eq!(
        text.title,
        text_file.0.file_stem().unwrap().to_str().unwrap()
    );
    assert_eq!(text.chapters.len(), 1);
    assert_eq!(text.chapter_index("document.xhtml"), Some(0));
    let xhtml = text.chapter(0).unwrap();
    assert!(xhtml.contains("<p>Reading &amp; &lt;tag&gt;<br />Next line</p>"));
    assert!(xhtml.contains("<p>One more paragraph</p>"));
    assert!(text.chapter(1).is_err());
    assert!(text.resource("document.xhtml").is_err());
    assert!(text.toc.is_empty());

    let (html_file, mut data) = temporary_document("html");
    data.write_all(
        br##"<!doctype html><html><head><title>Field Notes</title></head><body><p id="start">Keep reading <a href="#start">here</a>.</p><p><img src="cover.png" /></p></body></html>"##,
    )
    .unwrap();
    drop(data);
    let mut html = Book::open(&html_file.0).unwrap();
    assert_eq!(html.title, "Field Notes");
    assert_eq!(html.chapters[0].href, "document.xhtml");
    assert!(html.chapter(0).unwrap().contains("id=\"start\""));
    assert!(html.resource("cover.png").is_err());

    let (invalid_text, mut data) = temporary_document("txt");
    data.write_all(&[0x00, 0xFF]).unwrap();
    drop(data);
    assert!(Book::open(&invalid_text.0).is_err());
}

#[test]
fn fictionbook_opens_a_local_chapter_with_readable_notes() {
    let (file, mut data) = temporary_document("FB2");
    data.write_all(
        br##"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"
        xmlns:xlink="http://www.w3.org/1999/xlink">
        <description><title-info><book-title>Field Notes</book-title></title-info></description>
        <body><section id="primitiveos-fb2-section-1"><title><p>Shore</p></title>
            <section><title><p>First <emphasis>observation</emphasis></p></title>
                <p>An observation<a xlink:href="#note">[1]</a>.</p>
            </section>
            <section><title><p>Second observation</p></title><p>Reading onward.</p></section>
        </section></body>
        <body name="notes"><section id="note"><p>First note.</p></section></body>
    </FictionBook>"##,
    )
    .unwrap();
    drop(data);
    let mut book = Book::open(&file.0).unwrap();
    assert_eq!(book.title, "Field Notes");
    assert_eq!(book.chapter_index("document.xhtml"), Some(0));
    let toc_ids = [
        "primitiveos-fb2-section-1",
        "primitiveos-fb2-section-2",
        "primitiveos-fb2-section-3",
    ];
    assert_eq!(
        book.toc(),
        [
            entry(
                "Shore",
                0,
                Some(destination("document.xhtml", Some(toc_ids[0])))
            ),
            entry(
                "First observation",
                1,
                Some(destination("document.xhtml", Some(toc_ids[1])))
            ),
            entry(
                "Second observation",
                1,
                Some(destination("document.xhtml", Some(toc_ids[2])))
            ),
        ],
    );
    let xhtml = book.chapter(0).unwrap();
    let chapter = Document::parse(&xhtml).unwrap();
    for id in toc_ids {
        assert_eq!(
            chapter
                .descendants()
                .filter(|node| node.attribute("id") == Some(id))
                .count(),
            1,
        );
    }
    assert!(
        chapter
            .descendants()
            .any(|node| node.attribute("id") == Some("note"))
    );
    assert!(
        chapter
            .descendants()
            .any(|node| node.attribute("href") == Some("#note"))
    );
    assert!(book.resource("cover.png").is_err());
    let (renamed, mut data) = temporary_document("fb2");
    data.write_all(&fs::read(&file.0).unwrap()).unwrap();
    drop(data);
    let mut reopened = Book::open(&renamed.0).unwrap();
    assert_eq!(reopened.toc(), book.toc());
    assert_eq!(reopened.chapter(0).unwrap(), xhtml);
}

fn write_fictionbook(extension: &str, xhtml: &str) -> TemporaryBook {
    let (file, mut output) = temporary_document(extension);
    match extension {
        "fb2" => output.write_all(xhtml.as_bytes()).unwrap(),
        "fb2.gz" => {
            let mut gzip = GzEncoder::new(output, Compression::fast());
            gzip.write_all(xhtml.as_bytes()).unwrap();
            gzip.finish().unwrap();
        }
        "fb2.zip" | "fbz" => {
            let mut zip = ZipWriter::new(output);
            zip.start_file(
                "folder/Field notes.FB2",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
            zip.write_all(xhtml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        _ => unreachable!(),
    }
    file
}

#[test]
fn packed_fictionbook_cover_and_images_resolve_to_local_resources() {
    let fixture = File::open(fixture("field-notes-epub3.epub")).unwrap();
    let mut fixture = ZipArchive::new(fixture).unwrap();
    let mut image_bytes = Vec::new();
    fixture
        .by_name("EPUB/images/shore.png")
        .unwrap()
        .read_to_end(&mut image_bytes)
        .unwrap();
    let base64 = STANDARD.encode(&image_bytes);
    let (prefix, suffix) = base64.split_at(128);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&image::RgbImage::from_pixel(3, 2, image::Rgb([33; 3])))
        .unwrap();
    let encoded_jpeg = STANDARD.encode(&jpeg);
    let file_content = format!(
        r##"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"
        xmlns:xlink="http://www.w3.org/1999/xlink">
        <description><title-info><book-title>Field Notes</book-title>
          <coverpage><image xlink:href="#shore%2Dpicture" alt="Shore cover"/></coverpage>
        </title-info></description>
        <body><section>
          <title><p>Shore</p></title>
          <p>An observation<image xlink:href="#shore-picture" alt="Shore illustration"/>.</p>
          <image xlink:href="#photo-picture" alt="Shore photo"/>
          <image xlink:href="#wrong-type" alt="Unusable illustration"/>
          <image xlink:href="https://example.invalid/image.png" alt="Remote illustration"/>
        </section></body>
        <binary id="shore-picture" content-type="image/png">
          {prefix}
          {suffix}
        </binary>
        <binary id="photo-picture" content-type="image/jpeg">{encoded_jpeg}</binary>
        <binary id="wrong-type" content-type="image/jpeg">{base64}</binary>
    </FictionBook>"##,
    );

    for extension in ["fb2", "fb2.gz", "fb2.zip", "fbz"] {
        let file = write_fictionbook(extension, &file_content);
        let mut book = Book::open(&file.0).unwrap();
        assert_eq!(book.title, "Field Notes");
        let xhtml = book.chapter(0).unwrap();
        let chapter = Document::parse(&xhtml).unwrap();
        let images: Vec<_> = chapter
            .descendants()
            .filter(|node| node.has_tag_name("img"))
            .collect();
        assert_eq!(images.len(), 3, "{extension}");
        assert_eq!(images[0].attribute("alt"), Some("Shore cover"));
        assert_eq!(images[1].attribute("alt"), Some("Shore illustration"));
        assert_eq!(images[2].attribute("alt"), Some("Shore photo"));
        assert!(images[0].parent().unwrap().has_tag_name("figure"));
        assert!(images[1].parent().unwrap().has_tag_name("p"));
        assert!(xhtml.contains("Unusable illustration"));
        assert!(xhtml.contains("Remote illustration"));
        for (index, image) in images.into_iter().enumerate() {
            let src = image.attribute("src").unwrap();
            let path = Book::resolve(&book.chapters[0].href, src).unwrap();
            let bytes = book.resource(&path).unwrap();
            let (expected, dimensions) = if index == 2 {
                (&jpeg, (3, 2))
            } else {
                (&image_bytes, (144, 96))
            };
            assert_eq!(&bytes, expected);
            let image = image::load_from_memory(&bytes).unwrap();
            assert_eq!((image.width(), image.height()), dimensions);
        }
        assert!(book.resource("fb2-images/3.jpg").is_err());
    }
}

#[test]
fn packed_fictionbook_rejects_ambiguous_zip_and_oversized_decompression() {
    let (file, output) = temporary_document("fb2.zip");
    let mut zip = ZipWriter::new(output);
    for name in ["first.fb2", "second.fb2"] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(b"test").unwrap();
    }
    zip.finish().unwrap();
    let error = Book::open(&file.0).err().unwrap().to_string();
    assert!(error.contains("more than one"));

    let (file, output) = temporary_document("fb2.gz");
    let mut gzip = GzEncoder::new(output, Compression::fast());
    let block = [b'x'; 8192];
    for _ in 0..=1024 {
        gzip.write_all(&block).unwrap();
    }
    gzip.finish().unwrap();
    let error = Book::open(&file.0).err().unwrap().to_string();
    assert!(error.contains("decoded document exceeds"));
}

#[test]
fn epub_2_and_3_expose_titles_spine_and_local_assets() {
    for version in [2, 3] {
        let path = fixture(&format!("field-notes-epub{version}.epub"));
        let mut book = Book::open(&path).unwrap();
        assert_eq!(book.title, format!("Field Notes at Dusk (EPUB {version})"));
        let chapter_paths = ["EPUB/chapters/shore.xhtml", "EPUB/chapters/dusk.xhtml"];
        let chapter_labels = ["Reading the Shore", "Returning at Dusk"];
        assert_eq!(book.chapters.len(), chapter_paths.len());
        for (index, (path, label)) in chapter_paths.iter().zip(chapter_labels).enumerate() {
            assert_eq!(book.chapters[index].href, *path);
            assert_eq!(book.chapters[index].label, label);
        }

        let first = book.chapter(0).unwrap();
        let first = Document::parse(&first).unwrap();
        let linked_path = |tag: &str, attr: &str| {
            let href = first
                .descendants()
                .find(|node| node.has_tag_name(tag))
                .unwrap()
                .attribute(attr)
                .unwrap();
            Book::resolve(chapter_paths[0], href).unwrap()
        };
        let css = book.resource(&linked_path("link", "href")).unwrap();
        assert!(String::from_utf8(css).unwrap().contains(".notice"));
        let image = book.resource(&linked_path("img", "src")).unwrap();
        let image = image::load_from_memory(&image).unwrap();
        assert_eq!((image.width(), image.height()), (144, 96));

        if version == 3 {
            let font = book.resource("EPUB/fonts/Silkscreen-Regular.ttf").unwrap();
            assert_eq!(&font[..4], &[0, 1, 0, 0]);
        }
        assert!(book.chapter(chapter_paths.len()).is_err());
    }
}

#[test]
fn local_archive_paths_handle_uri_escapes_and_reject_outside_links() {
    let base = "EPUB/chapters/shore.xhtml";
    for (href, expected) in [
        ("#tide-note", base),
        ("dusk.xhtml#door", "EPUB/chapters/dusk.xhtml"),
        ("../styles/reader.css", "EPUB/styles/reader.css"),
        (
            "./caf%C3%A9%20reading.xhtml",
            "EPUB/chapters/café reading.xhtml",
        ),
        (
            "cover%23a%25.png?token=x#image",
            "EPUB/chapters/cover#a%.png",
        ),
    ] {
        assert_eq!(Book::resolve(base, href).unwrap(), expected);
    }

    let mut book = Book::open(&fixture("field-notes-epub3.epub")).unwrap();
    for invalid in [
        "../../../documents/book.epub",
        "%2e%2e/%2e%2e/%2e%2e/documents/book.epub",
        "https://example.invalid/cover.png",
        "//example.invalid/cover.png",
        "/etc/passwd",
        "nested%2Fcover.png",
        "nested%5ccover.png",
        "bad%00cover.png",
        "bad%FFcover.png",
        "bad%Ac%Qzcover.png",
    ] {
        assert!(Book::resolve(base, invalid).is_err(), "{invalid}");
        assert!(book.resource(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn long_and_many_chapter_books_keep_accessible_endpoints() {
    let mut long = Book::open(&fixture("long-chapter.epub")).unwrap();
    assert_eq!(long.chapters.len(), 1);
    assert!(long.chapter(0).unwrap().contains("End of the shore walk."));

    let mut many = Book::open(&fixture("many-chapters.epub")).unwrap();
    assert_eq!(many.chapters.len(), 48);
    assert_eq!(many.chapters[0].href, "EPUB/chapters/day-01.xhtml");
    assert_eq!(many.chapters[47].href, "EPUB/chapters/day-48.xhtml");
    assert!(many.chapter(0).is_ok());
    assert!(many.chapter(47).is_ok());
}

#[test]
fn damaged_packages_and_missing_assets_return_recoverable_errors() {
    for name in ["malformed-container.epub", "truncated.epub"] {
        assert!(Book::open(&fixture(name)).is_err(), "{name}");
    }

    let mut book = Book::open(&fixture("missing-image.epub")).unwrap();
    assert!(book.resource("EPUB/images/shore.png").is_err());
    assert!(book.chapter(0).unwrap().contains("Reading the Shore"));
    assert!(book.chapter(1).unwrap().contains("Returning at Dusk"));
}
