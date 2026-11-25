use crate::{Anchor, Book, Engine, Probe, PublisherRenderMode, Settings, user_styles};

const PHRASES: [&str; 13] = [
    "Late caption",
    "North berth",
    "North tide",
    "Attribute caption",
    "East berth",
    "East tide",
    "Author caption",
    "South berth",
    "South tide",
    "Override caption",
    "West berth",
    "West tide",
    "End of quay log.",
];

fn open(without_html: bool, height: u32) -> Probe {
    let mut data = user_styles::Data::default();
    if without_html {
        data.choose(1, "table_no_presentational_hints", Some(true))
            .unwrap();
    }
    let settings = PublisherRenderMode::Book.apply(Settings {
        width: 640,
        height,
        font_size: 22.,
        margin: 40,
        ..Settings::default()
    });
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("table-captions.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
        None,
        &crate::user_hyphenation::Rules::default(),
        &data.snapshot(1).unwrap(),
    )
    .unwrap()
}

fn anchor(probe: &Probe, phrase: &str) -> Anchor {
    let occurrences = probe.matches(phrase);
    assert_eq!(occurrences.len(), 1, "{phrase}");
    occurrences.into_iter().next().unwrap()
}

fn selection(probe: &Probe, phrase: &str) -> (usize, crate::selection::HighlightRect) {
    let start = anchor(probe, phrase);
    let end = Anchor {
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    let selected = probe.select_range(&start, &end).unwrap();
    assert_eq!(selected.text, phrase);
    let page = probe.page_for(&start).unwrap();
    let rects = probe.selection_rects(page, &selected);
    assert_eq!(rects.len(), 1, "{phrase}");
    (page, rects[0])
}

#[test]
fn table_caption_visual_order_and_link_survive_html_hints() {
    let baseline = open(false, 880);
    let anchors = PHRASES.map(|word| anchor(&baseline, word));
    // The late caption's physical movement does not change its source key.
    assert!(anchors[0].block > anchors[2].block);

    let mut paints = Vec::new();
    let mut link = None;
    for (name, ignore_html) in [("publisher", false), ("without-html-hints", true)] {
        let mut probe = open(ignore_html, 880);
        let rectangles = PHRASES.map(|word| {
            let (page, rect) = selection(&probe, word);
            assert_eq!(page, 0, "{word}");
            rect
        });
        let y = rectangles.map(|rect| rect.y);
        assert!(y[0] < y[1] && y[1] < y[2], "{y:?}");
        assert!(
            if ignore_html {
                y[3] < y[4]
            } else {
                y[5] < y[3]
            },
            "{y:?}"
        );
        assert!(y[7] < y[8] && y[8] < y[6], "{y:?}");
        assert!(y[9] < y[10] && y[10] < y[11] && y[11] < y[12], "{y:?}");
        for (index, word) in PHRASES.into_iter().enumerate() {
            assert_eq!(anchor(&probe, word), anchors[index]);
            assert_eq!(
                probe.restore_anchor(&anchors[index]),
                Some(anchors[index].clone())
            );
        }
        for (caption, row) in [(0, 1), (3, 4), (6, 7)] {
            assert!(
                rectangles[caption].x < rectangles[row].x,
                "{name} {rectangles:?}"
            );
        }
        let rect = rectangles[0];
        let target = probe.link_at(0, (rect.x + 4) as f32, (rect.y + 4) as f32);
        assert_eq!(
            target
                .as_ref()
                .and_then(|target| target.fragment.as_deref()),
            Some("finish")
        );
        if let Some(previous) = &link {
            assert_eq!(&target, previous);
        }
        link = Some(target);
        let frame = probe.capture(0).unwrap();
        let pixel = |x: usize, y: i32| frame[y as usize * 640 + x];
        for i in [0, 3, 6, 9] {
            let rect = rectangles[i];
            let y = rect.y + rect.height / 2;
            assert_eq!(pixel(230, y), 221, "{name} {:?}", PHRASES[i]);
            let stroke_x = if i == 9 { 40 } else { 43 };
            assert_eq!(pixel(stroke_x, y), 102, "{name} {:?}", PHRASES[i]);
        }
        assert_eq!(pixel(40, rectangles[0].y + rectangles[0].height / 2), 0);
        let openers = [if ignore_html { 3 } else { 4 }, 7, 9];
        let closers = [2, if ignore_html { 5 } else { 3 }, 6];
        for (end, start) in closers.into_iter().zip(openers) {
            assert!(
                (rectangles[end].y + rectangles[end].height..rectangles[start].y)
                    .any(|y| pixel(40, y) == 255),
                "Missing margin before {} ({name})",
                PHRASES[start]
            );
        }
        #[cfg(feature = "kindle")]
        if let Some(dir) = std::env::var_os("READER_TABLE_CAPTION_CAPTURES") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            primitive_app::write_png(
                &dir.join(format!("reader-table-captions-{name}.png")),
                640,
                880,
                &frame,
            )
            .unwrap();
        }
        paints.push(frame);
    }
    assert!(paints[0] != paints[1]);
    assert_eq!(open(false, 880).capture(0).unwrap(), paints[0]);
}

#[test]
fn table_caption_anchors_reopen_across_multiple_pages() {
    let original = open(false, 260);
    let relocated = open(true, 260);
    assert!(original.page_count() > 1 && relocated.page_count() > 1);
    for phrase in PHRASES {
        let position = anchor(&original, phrase);
        assert_eq!(relocated.restore_anchor(&position), Some(position.clone()));
        let (page, rect) = selection(&relocated, phrase);
        let top = relocated.anchor_top(&position).unwrap();
        assert!(relocated.page_range(page).contains(&top), "{phrase}");
        assert!(
            rect.y >= 0 && rect.y + rect.height <= 260,
            "{phrase} {rect:?}"
        );
    }
    assert!(
        relocated.anchor_top(&anchor(&original, "Late caption"))
            < relocated.anchor_top(&anchor(&original, "North berth"))
    );
}
