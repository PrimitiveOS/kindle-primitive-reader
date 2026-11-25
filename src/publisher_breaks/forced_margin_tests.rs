use crate::{
    Anchor, BlockMarginMode, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture,
};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 720;

fn specimen(
    chapter: usize,
    mode: BlockMarginMode,
    preserve: bool,
    author: bool,
    document: Option<String>,
) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-forced-page-margins.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            block_margins: mode,
            preserve_forced_page_margins: preserve,
            publisher_styles: author,
            ..Settings::default()
        },
        chapter,
        document,
    )
    .unwrap()
}

fn anchor(probe: &Probe, phrase: &str) -> Anchor {
    let hits = probe.matches(phrase);
    assert_eq!(hits.len(), 1, "{phrase}");
    hits.into_iter().next().unwrap()
}

fn position(probe: &Probe, phrase: &str) -> (usize, f32, f32) {
    let anchor = anchor(probe, phrase);
    let page = probe.page_for(&anchor).unwrap();
    let top = probe.anchor_top(&anchor).unwrap();
    let cut = probe.pages.range(page).unwrap().start;
    (page, top, top - cut)
}

fn top_band(frame: &[u8]) -> usize {
    frame
        .chunks_exact(WIDTH as usize)
        .position(|row| row[1209] == 221)
        .unwrap()
}

#[test]
fn forced_page_margins_follow_both_unambiguous_publisher_directions() {
    for mode in [BlockMarginMode::Collapse, BlockMarginMode::CollapseNegative] {
        let normal = specimen(2, mode, true, true, None);
        let (_, a, _) = position(&normal, "Ordinary margin previous");
        let (_, b, _) = position(&normal, "Ordinary margin next");
        for (chapter, previous, next) in [
            (0, "Margin before previous", "Margin before next"),
            (1, "Margin after previous", "Margin after next"),
        ] {
            let mut regular = specimen(chapter, mode, false, true, None);
            let mut retained = specimen(chapter, mode, true, true, None);
            assert_eq!(regular.coverage(), retained.coverage());
            assert_eq!((regular.page_count(), retained.page_count()), (2, 2));
            let (page, current_top, current_offset) = position(&regular, next);
            let (new_page, new_top, new_offset) = position(&retained, next);
            let (_, preceding_top, _) = position(&retained, previous);
            assert_eq!((page, new_page), (1, 1));
            assert!((current_offset - 0.0).abs() < 0.01);
            assert!((new_offset - 68.0).abs() < 0.01);
            assert!((current_top - new_top - (94.0 - 68.0)).abs() < 0.01);
            assert!((b - a - (new_top - preceding_top) - (94.0 - 68.0)).abs() < 0.01);

            let old_frame = regular.capture(1).unwrap();
            let new_frame = retained.capture(1).unwrap();
            assert_eq!(top_band(&new_frame) - top_band(&old_frame), 68);
            assert_eq!(retained.page_anchor(1), Some(anchor(&retained, next)));
            assert!(retained.word_at(1, 72.0, 90.0).is_none());
            let start = anchor(&regular, next);
            let end = Anchor {
                raw_caret: None,
                offset: start.offset + "Margin".len(),
                ..start.clone()
            };
            let selection = regular.select_range(&start, &end).unwrap();
            let saved = serde_json::to_vec(&selection).unwrap();
            let quote: TextSelection = serde_json::from_slice(&saved).unwrap();
            assert_eq!(retained.select_range(&start, &end), Some(quote.clone()));
            let rect = retained.selection_rects(1, &quote)[0];
            assert_eq!(
                retained.word_at(
                    1,
                    rect.x as f32 + rect.width as f32 * 0.5,
                    rect.y as f32 + rect.height as f32 * 0.5,
                ),
                Some(quote)
            );
            if let Some(folder) = std::env::var_os("READER_FORCED_MARGIN_CAPTURES")
                && mode == BlockMarginMode::CollapseNegative
                && chapter == 0
            {
                for (name, frame) in [
                    ("reader-current.png", old_frame),
                    ("reader-retained.png", new_frame),
                ] {
                    image::save_buffer(
                        PathBuf::from(&folder).join(name),
                        &frame,
                        WIDTH,
                        HEIGHT,
                        image::ColorType::L8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
fn forced_page_margins_drop_an_outgoing_margin_when_incoming_is_zero() {
    let mode = BlockMarginMode::Collapse;
    let regular = specimen(3, mode, false, true, None);
    let retained = specimen(3, mode, true, true, None);
    let (page, original, original_offset) = position(&regular, "Zero margin next");
    let (new_page, shifted, new_offset) = position(&retained, "Zero margin next");
    assert_eq!((page, new_page), (1, 1));
    assert!((original_offset - 0.0).abs() < 0.01);
    assert!((new_offset - 0.0).abs() < 0.01);
    assert!((original - shifted - 94.0).abs() < 0.01);
}

fn same_frames(mut old: Probe, mut updated: Probe) {
    assert_eq!(old.page_count(), updated.page_count());
    for page in 0..old.page_count() {
        assert!(
            old.capture(page).unwrap() == updated.capture(page).unwrap(),
            "page {page} differs"
        );
    }
}

#[test]
fn forced_page_margins_preserve_conflicting_edges_disabled_styles_and_other_modes() {
    use BlockMarginMode::{Collapse, CollapseNegative, Ignore, Separate};
    assert!(!Settings::default().preserve_forced_page_margins);
    for (chapter, mode, author) in [
        (0, Ignore, true),
        (0, Separate, true),
        (0, Collapse, false),
        (2, Collapse, true),
        (4, CollapseNegative, true),
    ] {
        same_frames(
            specimen(chapter, mode, false, author, None),
            specimen(chapter, mode, true, author, None),
        );
    }
    let original = Book::open(&test_fixture("render-forced-page-margins.epub"))
        .unwrap()
        .chapter(0)
        .unwrap();
    for (from, to) in [
        (
            "margin-bottom:94px",
            "margin-bottom:94px;page-break-after:avoid",
        ),
        (
            "margin-top:68px",
            "margin-top:68px;border-top:2px solid black",
        ),
        ("margin-top:68px", "margin-top:-12px"),
    ] {
        let document = original.replace(from, to);
        same_frames(
            specimen(0, CollapseNegative, false, true, Some(document.clone())),
            specimen(0, CollapseNegative, true, true, Some(document)),
        );
    }
}
