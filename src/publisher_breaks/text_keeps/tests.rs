use crate::{Anchor, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 720;

fn specimen(
    chapter: usize,
    relax: bool,
    publisher_styles: bool,
    document: Option<String>,
) -> Probe {
    let path = test_fixture("render-text-keeps.epub");
    Probe::from_book_content(
        Book::open(&path).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            relax_oversized_publisher_keeps: relax,
            publisher_styles,
            ..Settings::default()
        },
        chapter,
        document,
    )
    .unwrap()
}

fn page(probe: &Probe, phrase: &str) -> usize {
    let anchors = probe.matches(phrase);
    assert_eq!(anchors.len(), 1, "{phrase}");
    probe.page_for(&anchors[0]).unwrap()
}

fn frame_equivalence(old: &mut Probe, new: &mut Probe) {
    assert_eq!(old.page_count(), new.page_count());
    for page in 0..old.page_count() {
        assert!(
            old.capture(page).unwrap() == new.capture(page).unwrap(),
            "page {page} differs"
        );
    }
}

fn capture(probe: &mut Probe, page: usize, name: &str) {
    if let Some(folder) = std::env::var_os("READER_TEXT_KEEP_CAPTURES") {
        image::save_buffer(
            PathBuf::from(folder).join(name),
            &probe.capture(page).unwrap(),
            WIDTH,
            HEIGHT,
            image::ColorType::L8,
        )
        .unwrap();
    }
}

#[test]
fn conditional_text_keeps_fill_the_current_page_and_restore_source_positions() {
    let mut regular = specimen(1, false, true, None);
    let mut bounded = specimen(1, true, true, None);
    assert_eq!(regular.coverage(), bounded.coverage());
    assert_eq!(page(&regular, "Long opening"), 1);
    assert_eq!(page(&bounded, "Long opening"), 0);
    assert_eq!((regular.page_count(), bounded.page_count()), (3, 2));
    for (marker, default_page, native_page) in [
        ("Before oversized group", 0, 0),
        ("Long opening", 1, 0),
        ("Long middle", 1, 1),
        ("Long ending", 2, 1),
        ("After oversized group", 2, 1),
    ] {
        assert_eq!(page(&regular, marker), default_page, "{marker}");
        assert_eq!(page(&bounded, marker), native_page, "{marker}");
    }
    let anchor = regular.matches("Long opening").remove(0);
    let stored = regular
        .select_range(
            &anchor,
            &Anchor {
                offset: anchor.offset + "Long".len(),
                ..anchor.clone()
            },
        )
        .unwrap();
    let saved = serde_json::to_vec(&stored).unwrap();
    let restored: TextSelection = serde_json::from_slice(&saved).unwrap();
    assert_eq!(
        bounded.select_range(&restored.start, &restored.end),
        Some(restored.clone())
    );
    let page = bounded.page_for(&restored.start).unwrap();
    let rect = bounded.selection_rects(page, &restored)[0];
    let hit = bounded.word_at(
        page,
        rect.x as f32 + rect.width as f32 * 0.5,
        rect.y as f32 + rect.height as f32 * 0.5,
    );
    assert_eq!(hit, Some(restored.clone()));
    assert_eq!(
        specimen(1, true, true, None).page_for(&restored.start),
        Some(0)
    );

    capture(&mut regular, 0, "reader-default-long-opening.png");
    capture(&mut bounded, 0, "reader-bounded-long-opening.png");
}

#[test]
fn conditional_text_keeps_keep_fitting_groups_and_preserve_publisher_styles_off() {
    let mut regular = specimen(0, false, true, None);
    let mut bounded = specimen(0, true, true, None);
    for (marker, expected) in [
        ("Before short group", 0),
        ("Short opening", 1),
        ("Short ending", 1),
        ("After short group", 1),
    ] {
        assert_eq!(page(&bounded, marker), expected);
    }
    frame_equivalence(&mut regular, &mut bounded);
    capture(&mut bounded, 1, "reader-bounded-short-opening.png");
    for chapter in 0..2 {
        frame_equivalence(
            &mut specimen(chapter, false, false, None),
            &mut specimen(chapter, true, false, None),
        );
    }
}

#[test]
fn conditional_text_keeps_preserve_borders_and_unhandled_descendant_rules() {
    let path = test_fixture("render-text-keeps.epub");
    let original = Book::open(&path).unwrap().chapter(1).unwrap();
    for (name, target, replacement) in [
        (
            "border",
            "<div class=\"keep\">",
            "<div class=\"keep\" style=\"border-top:2px solid black\">",
        ),
        (
            "descendant keep",
            "<p>Long line two</p>",
            "<p style=\"break-inside:avoid\">Long line two</p>",
        ),
        (
            "nested wrapper",
            "<p>Long line two</p>",
            "<section><p>Long line two</p></section>",
        ),
        (
            "paragraph line protection",
            "<p>Long line two</p>",
            "<p style=\"widows:2\">Long line two<br/>More words</p>",
        ),
        (
            "image",
            "<p>Long line two</p>",
            "<p>Long line two <img src=\"missing.png\" alt=\"Image\"/></p>",
        ),
    ] {
        assert!(original.contains(target), "{name}");
        let document = original.replace(target, replacement);
        let mut regular = specimen(1, false, true, Some(document.clone()));
        let mut bounded = specimen(1, true, true, Some(document));
        frame_equivalence(&mut regular, &mut bounded);
    }
}
