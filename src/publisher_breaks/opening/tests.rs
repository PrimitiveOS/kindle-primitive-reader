use crate::{Anchor, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 720;
const CASES: [(&str, &str); 8] = [
    ("Direct before previous", "Direct before next"),
    ("Direct after previous", "Direct after next"),
    ("Nested before previous", "Nested before next"),
    ("Nested after previous", "Nested after next"),
    ("Adjacent before previous", "Adjacent before next"),
    ("Adjacent after previous", "Adjacent after next"),
    ("Standalone after previous", "Standalone after next"),
    ("Standalone before previous", "Standalone before next"),
];

fn open(chapter: usize, prefer: bool, publisher_styles: bool, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-opening-breaks.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            prefer_publisher_opening_breaks: prefer,
            publisher_styles,
            ..Settings::default()
        },
        chapter,
        document,
    )
    .unwrap()
}

fn anchor(probe: &Probe, phrase: &str) -> Anchor {
    let matches = probe.matches(phrase);
    assert_eq!(matches.len(), 1, "{phrase}");
    matches.into_iter().next().unwrap()
}

fn equivalent(old: &mut Probe, updated: &mut Probe) {
    assert_eq!(old.page_count(), updated.page_count());
    for page in 0..old.page_count() {
        assert!(
            old.capture(page).unwrap() == updated.capture(page).unwrap(),
            "page {page} differs"
        );
    }
}

#[test]
fn publisher_opening_breaks_match_enhanced_native_directionality_and_resume() {
    assert!(!Settings::default().prefer_publisher_opening_breaks);
    let ordinary = [0, 0, 0, 0, 0, 0, 1, 1];
    let native = [1, 0, 1, 0, 1, 0, 1, 1];
    for (chapter, (before, after)) in CASES.into_iter().enumerate() {
        let mut regular = open(chapter, false, true, None);
        let mut updated = open(chapter, true, true, None);
        assert_eq!(regular.coverage(), updated.coverage());
        assert_eq!(regular.page_for(&anchor(&regular, before)), Some(0));
        assert_eq!(updated.page_for(&anchor(&updated, before)), Some(0));
        assert_eq!(
            regular.page_for(&anchor(&regular, after)),
            Some(ordinary[chapter]),
            "{after}"
        );
        let page = native[chapter];
        assert_eq!(
            updated.page_for(&anchor(&updated, after)),
            Some(page),
            "{after}"
        );
        assert_eq!(updated.page_count(), page + 1);
        if ordinary[chapter] == page {
            equivalent(&mut regular, &mut updated);
            continue;
        }

        let start = anchor(&regular, after);
        let first_word = after.split_whitespace().next().unwrap();
        let end = Anchor {
            raw_caret: None,
            offset: start.offset + first_word.len(),
            ..start.clone()
        };
        let saved = serde_json::to_vec(&regular.select_range(&start, &end).unwrap()).unwrap();
        let restored: TextSelection = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            updated.select_range(&restored.start, &restored.end),
            Some(restored.clone())
        );
        assert_eq!(updated.page_anchor(page), Some(restored.start.clone()));
        assert_eq!(
            open(chapter, true, true, None).page_for(&restored.start),
            Some(page)
        );
        let rect = updated.selection_rects(page, &restored)[0];
        let painted_word = updated.word_at(
            page,
            rect.x as f32 + rect.width as f32 * 0.5,
            rect.y as f32 + rect.height as f32 * 0.5,
        );
        assert_eq!(painted_word, Some(restored));

        if chapter == 2
            && let Some(folder) = std::env::var_os("READER_OPENING_BREAK_CAPTURES")
        {
            for (probe, frame_page, name) in [
                (&mut regular, 0, "reader-current.png"),
                (&mut updated, 1, "reader-enhanced.png"),
            ] {
                image::save_buffer(
                    PathBuf::from(&folder).join(name),
                    &probe.capture(frame_page).unwrap(),
                    WIDTH,
                    HEIGHT,
                    image::ColorType::L8,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn publisher_opening_breaks_keep_existing_frames_for_other_geometry_and_styles_off() {
    for chapter in 0..CASES.len() {
        equivalent(
            &mut open(chapter, false, false, None),
            &mut open(chapter, true, false, None),
        );
    }
    let original = Book::open(&test_fixture("render-opening-breaks.epub"))
        .unwrap()
        .chapter(0)
        .unwrap();
    for (from, to) in [
        (
            "class=\"force-before\"",
            "class=\"force-before\" style=\"margin-top:30px\"",
        ),
        (
            "class=\"force-before\"",
            "class=\"force-before\" style=\"border-top:2px solid black\"",
        ),
        (
            "class=\"keep\"",
            "class=\"keep\" style=\"padding-top:12px\"",
        ),
        ("class=\"keep\"", "class=\"keep\" style=\"background:#bbb\""),
        (
            "Direct before previous</p>",
            "Direct before previous<img src=\"missing.png\" alt=\"Image\"/></p>",
        ),
    ] {
        assert!(original.contains(from));
        let document = original.replace(from, to);
        equivalent(
            &mut open(0, false, true, Some(document.clone())),
            &mut open(0, true, true, Some(document)),
        );
    }
}
