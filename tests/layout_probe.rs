use std::path::PathBuf;

use reader::epub::Book;
use reader::{Anchor, Engine, Probe, Settings};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn repeated_text_resolves_to_the_same_passages_after_reflow() {
    let font = fixture("fonts/Silkscreen-Regular.ttf");
    let settings = Settings {
        width: 650,
        height: 760,
        font_size: 34.0,
        margin: 25,
        ..Settings::default()
    };
    for engine in [Engine::Blitz, Engine::Parley] {
        for book in ["field-notes-epub2.epub", "field-notes-epub3.epub"] {
            let book = fixture(book);
            let mut initial =
                Probe::open(&book, engine, std::slice::from_ref(&font), settings, 0).unwrap();
            assert_eq!(initial.chapter_count(), 2);
            let matches = initial.matches("The tide keeps its own time.");
            assert_eq!(matches.len(), 2);
            assert_eq!(initial.fragment("tide-a").as_ref(), Some(&matches[0]));
            assert_eq!(initial.fragment("tide-b").as_ref(), Some(&matches[1]));
            assert_ne!(matches[0], matches[1]);
            let first = initial.page_for(&matches[0]).unwrap();
            let last = initial.page_for(&matches[1]).unwrap();
            assert!(first <= last);
            let mut package = Book::open(&book).unwrap();
            let xhtml = package.chapter(0).unwrap();
            let document = roxmltree::Document::parse(&xhtml).unwrap();
            let reference = document
                .descendants()
                .find(|node| node.attribute("id") == Some("note-ref"))
                .unwrap();
            let note_href = reference.attribute("href").unwrap();
            assert_eq!(
                Book::resolve(&initial.chapter, note_href).unwrap(),
                initial.chapter
            );
            let note_id = note_href.strip_prefix('#').unwrap();
            let note = initial.fragment(note_id).unwrap();
            assert!(initial.page_for(&note).is_some());
            let return_href = document
                .descendants()
                .find(|node| node.attribute("id") == Some(note_id))
                .unwrap()
                .descendants()
                .find(|node| node.has_tag_name("a"))
                .unwrap()
                .attribute("href")
                .unwrap();
            assert_eq!(
                Book::resolve(&initial.chapter, return_href).unwrap(),
                initial.chapter
            );
            let returned = initial
                .fragment(return_href.strip_prefix('#').unwrap())
                .unwrap();
            assert!(initial.page_for(&returned).is_some());
            let frame = initial.capture(first).unwrap();
            assert_eq!(frame.len(), (settings.width * settings.height) as usize);
            assert!(frame.iter().any(|&value| value < 240));
            let (covered, total) = initial.coverage();
            assert_eq!(covered, total, "{engine:?}, {book:?}");
            drop(initial);

            let changed = Settings {
                width: 760,
                height: 650,
                font_size: 48.0,
                margin: 46,
                line_height: 1.6,
            };
            let mut reopened =
                Probe::open(&book, engine, std::slice::from_ref(&font), changed, 0).unwrap();
            for original in &matches {
                let saved = serde_json::to_vec(original).unwrap();
                let restored: Anchor = serde_json::from_slice(&saved).unwrap();
                assert!(
                    reopened
                        .matches("The tide keeps its own time.")
                        .contains(&restored)
                );
                let page = reopened.page_for(&restored).unwrap();
                assert!(
                    reopened
                        .capture(page)
                        .unwrap()
                        .iter()
                        .any(|&value| value < 240)
                );
            }
        }
    }
}

#[test]
fn long_chapter_pages_reach_the_end_without_losing_source_text() {
    let font = fixture("fonts/Silkscreen-Regular.ttf");
    let settings = Settings {
        width: 650,
        height: 760,
        font_size: 34.0,
        margin: 25,
        ..Settings::default()
    };
    for engine in [Engine::Blitz, Engine::Parley] {
        let mut probe = Probe::open(
            &fixture("long-chapter.epub"),
            engine,
            std::slice::from_ref(&font),
            settings,
            0,
        )
        .unwrap();
        assert!(probe.page_count() > 10, "{engine:?}");
        let (covered, total) = probe.coverage();
        assert_eq!(covered, total, "{engine:?}");
        let last = probe.page_count() - 1;
        let anchor = probe.page_anchor(last).unwrap();
        assert_eq!(probe.page_for(&anchor), Some(last));
        let first = probe.capture(0).unwrap();
        let end = probe.capture(last).unwrap();
        assert!(end.iter().any(|&value| value < 240));
        assert_ne!(first, end, "{engine:?}");
        assert!(probe.capture(last + 1).is_err());
    }
}
