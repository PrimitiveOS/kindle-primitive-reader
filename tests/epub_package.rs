use std::path::PathBuf;

use reader::epub::Book;
use roxmltree::Document;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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
