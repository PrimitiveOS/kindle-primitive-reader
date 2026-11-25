use std::path::PathBuf;

use reader::epub::Book;
use reader::{Anchor, Engine, Probe, Settings};

fn fixture(name: &str) -> PathBuf {
    std::env::var_os("READER_TEST_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
        .join(name)
}

#[test]
fn publisher_styles_off_changes_rendering_without_losing_anchors_links_or_images() {
    let book = fixture("field-notes-epub3.epub");
    let fonts = [fixture("fonts/Silkscreen-Regular.ttf")];
    let settings = Settings {
        width: 650,
        height: 760,
        margin: 25,
        ..Settings::default()
    };
    let mut styled = Probe::open(&book, Engine::Parley, &fonts, settings, 0).unwrap();
    let mut plain = Probe::open(
        &book,
        Engine::Parley,
        &fonts,
        Settings {
            publisher_styles: false,
            ..settings
        },
        0,
    )
    .unwrap();
    assert_ne!(styled.capture(0).unwrap(), plain.capture(0).unwrap());
    assert_eq!(styled.coverage(), plain.coverage());
    for name in ["shore", "tide-a", "tide-b", "note-ref", "tide-note"] {
        let saved = styled.fragment(name).unwrap();
        assert_eq!(plain.fragment(name).as_ref(), Some(&saved));
        assert!(plain.page_for(&saved).is_some());
    }
    let start = plain.matches("[1]").remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + "[1]".len(),
        ..start.clone()
    };
    let selection = plain.select_range(&start, &end).unwrap();
    let page = plain.page_for(&start).unwrap();
    let rect = plain.selection_rects(page, &selection)[0];
    let link = plain
        .link_at(
            page,
            rect.x as f32 + rect.width as f32 / 2.0,
            rect.y as f32 + rect.height as f32 / 2.0,
        )
        .unwrap();
    assert_eq!(link.fragment.as_deref(), Some("tide-note"));
    assert!(
        plain
            .open_illustration(&book, "EPUB/images/shore.png")
            .is_ok()
    );
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
                ..settings
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

#[test]
fn vertical_pages_keep_links_selection_and_source_anchors() {
    let font = fixture("fonts/Silkscreen-Regular.ttf");
    let book = fixture("vertical-chapters.epub");
    let settings = Settings {
        width: 650,
        height: 760,
        font_size: 26.0,
        margin: 25,
        ..Settings::default()
    };
    let fonts = std::slice::from_ref(&font);
    let mut probe = Probe::open(&book, Engine::Parley, fonts, settings, 1).unwrap();
    let (covered, total) = probe.coverage();
    assert_eq!(covered, total);
    let word = probe.matches("office").remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: word.offset + "office".len(),
        ..word.clone()
    };
    let selection = probe.select_range(&word, &end).unwrap();
    let page = probe.page_for(&word).unwrap();
    let bounds = probe.selection_rects(page, &selection);
    let rect = bounds.first().unwrap();
    assert!(rect.height > rect.width);
    let x = rect.x as f32 + rect.width as f32 / 2.0;
    let y = rect.y as f32 + rect.height as f32 / 2.0;
    assert_eq!(probe.word_at(page, x, y), Some(selection));
    assert_eq!(
        probe.link_at(page, x, y),
        Some(reader::epub::Destination {
            path: "EPUB/chapters/home.xhtml".into(),
            fragment: Some("landing".into()),
        })
    );

    let last = probe.fragment("entry-24").unwrap();
    let later_page = probe.page_for(&last).unwrap();
    assert!(later_page > page);
    assert!(
        probe
            .capture(later_page)
            .unwrap()
            .iter()
            .any(|&pixel| pixel < 200)
    );
    let reflowed = Probe::open(
        &book,
        Engine::Parley,
        fonts,
        Settings {
            font_size: 38.0,
            ..settings
        },
        1,
    )
    .unwrap();
    assert_eq!(reflowed.fragment("entry-24"), Some(last.clone()));
    assert!(reflowed.page_for(&last).is_some());
}
