use crate::{Anchor, BlockMarginMode, Book, Engine, Probe, Settings, test_fixture};

fn specimen(mode: BlockMarginMode, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-parent-margins.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 1264,
            height: 1606,
            font_size: 40.0,
            line_height: 1.20,
            block_margins: mode,
            ..Settings::default()
        },
        0,
        document,
    )
    .unwrap()
}

#[test]
fn parent_margin_groups_match_the_native_contributions_and_preserve_text_positions() {
    use BlockMarginMode::{Collapse, CollapseNegative, Ignore, Separate};
    let pairs = [
        ("Natural first", "Natural second"),
        ("Before joined", "Joined first"),
        ("Joined first", "Joined last"),
        ("Joined last", "After joined"),
        ("Before signed", "Signed first"),
        ("Signed last", "After signed"),
        ("Before barrier", "Barrier first"),
        ("Barrier last", "After barrier"),
        ("Before text", "Intervening parent text"),
        ("Intervening parent text", "Text child"),
        ("Before border", "Border child"),
        ("Border child", "After border"),
        ("Before nested", "Nested child"),
        ("Nested child", "After nested"),
    ];
    let mut default = specimen(Ignore, None);
    let pitches = |probe: &Probe| {
        let y = |phrase| probe.anchor_top(&probe.matches(phrase)[0]).unwrap();
        pairs.map(|(a, b)| y(b) - y(a))
    };
    let baseline = pitches(&default);
    let anchor = default.matches("Nested child").remove(0);
    assert_eq!(default.page_for(&anchor), Some(0));
    let selection = default
        .select_range(
            &anchor,
            &Anchor {
                offset: anchor.offset + "Nested".len(),
                ..anchor.clone()
            },
        )
        .unwrap();
    let saved = serde_json::to_vec(&selection).unwrap();

    for (mode, stock) in [
        (
            Separate,
            [
                64, 190, 94, 178, 111, 79, 150, 157, 96, 98, 150, 157, 149, 137,
            ],
        ),
        (
            Collapse,
            [
                64, 120, 82, 108, 93, 64, 135, 137, 84, 98, 135, 137, 102, 94,
            ],
        ),
        (
            CollapseNegative,
            [
                64, 120, 82, 108, 93, 51, 135, 137, 84, 98, 135, 137, 102, 94,
            ],
        ),
    ] {
        let mut probe = specimen(mode, None);
        let measured = pitches(&probe);
        println!("{mode:?} Reader={measured:?} baseline={baseline:?} stock={stock:?}");
        for (index, native_pitch) in stock.into_iter().enumerate() {
            // Subtract stock's natural line pitch and the independent
            // padding or border before comparing the margin contribution.
            let physical_edge = match index {
                6 | 10 => 8.0,
                7 | 11 => 9.0,
                _ => 0.0,
            };
            let expected = native_pitch as f32 - 64.0 - physical_edge;
            let actual = measured[index] - baseline[index];
            assert!(
                (expected - actual).abs() < 0.01,
                "{mode:?} {:?}: {actual} vs {expected}",
                pairs[index]
            );
        }

        let restored: crate::selection::TextSelection = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            probe.select_range(&restored.start, &restored.end),
            Some(restored.clone())
        );
        assert_eq!(probe.matches("Nested child")[0], restored.start);
        let page = probe.page_for(&restored.start).unwrap();
        assert_eq!(page, 1);
        let rect = probe.selection_rects(page, &restored)[0];
        let x = rect.x as f32 + rect.width as f32 * 0.5;
        let y = rect.y as f32 + rect.height as f32 * 0.5;
        assert_eq!(probe.word_at(page, x, y), Some(restored));

        if mode == CollapseNegative
            && let Some(folder) = std::env::var_os("READER_PARENT_MARGIN_CAPTURES")
        {
            let path = std::path::PathBuf::from(folder).join("reader-web-parent-margins.png");
            let frame = probe.capture(0).unwrap();
            image::save_buffer(path, &frame, 1264, 1606, image::ColorType::L8).unwrap();
        }
    }

    let path = test_fixture("render-parent-margins.epub");
    let xhtml = Book::open(&path).unwrap().chapter(0).unwrap();
    let mut undeclared = String::new();
    for part in xhtml.split_inclusive(';') {
        if let Some(index) = part.rfind("margin")
            && ["margin:", "margin-top:", "margin-bottom:"]
                .iter()
                .any(|declaration| part[index..].starts_with(declaration))
        {
            undeclared.push_str(&part[..index]);
        } else {
            undeclared.push_str(part);
        }
    }
    let mut bare = specimen(Ignore, Some(undeclared));
    assert_eq!(default.page_count(), bare.page_count());
    for page in 0..default.page_count() {
        assert!(
            default.capture(page).unwrap() == bare.capture(page).unwrap(),
            "ordinary frame differs on page {page}"
        );
    }
}

#[test]
fn parent_margin_attachment_is_edge_specific_and_stops_at_other_box_boundaries() {
    use super::tests::{gap, spacing};
    let body = concat!(
        "<p>Before top padded wrapper</p>",
        "<section style='margin:20px 0;padding-top:5px'>",
        "<p style='display:none;margin:999px 0'>Hidden child</p>",
        "<p style='margin:30px 0 35px'>Inside top padded wrapper</p>",
        "</section>",
        "<p style='margin-top:10px'>After top padded wrapper</p>",
        "<div style='margin:20px 0;height:200px'>",
        "<p style='margin:30px 0 35px'>Inside sized wrapper</p>",
        "</div>",
        "<p>After sized wrapper</p>",
        "<div style='margin:14px 0'>",
        "<p style='margin:30px 0 28px'>Clearance wrapper</p>",
        "<p style='clear:left;margin-top:18px'>Last with clearance</p>",
        "</div>",
        "<p style='margin-top:32px'>After clearance wrapper</p>",
        "<div style='margin:14px 0'>",
        "<p style='margin:30px 0 28px'>Before visible empty</p>",
        "<div></div></div>",
        "<p style='margin-top:32px'>After visible empty</p>",
    );
    let gaps = spacing(body, BlockMarginMode::CollapseNegative);
    for (marker, expected) in [
        ("Inside top padded wrapper", 50.0),
        ("After top padded wrapper", 35.0),
        ("Inside sized wrapper", 50.0),
        ("After sized wrapper", 55.0),
        ("Clearance wrapper", 30.0),
        ("After clearance wrapper", 32.0),
        ("Before visible empty", 30.0),
        ("After visible empty", 60.0),
    ] {
        assert_eq!(gap(&gaps, marker), expected, "{marker}");
    }
}
