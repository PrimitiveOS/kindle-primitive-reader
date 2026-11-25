use crate::{Anchor, Book, Engine, Probe, PublisherRenderMode, Settings, user_styles};

fn open(markup: &str, style: &user_styles::Snapshot) -> Probe {
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        PublisherRenderMode::Web.apply(Settings {
            width: 617,
            height: 1020,
            margin: 30,
            font_size: 30.,
            ..Settings::default()
        }),
        0,
        Some(markup.into()),
        &Default::default(),
        style,
    )
    .unwrap()
}

#[test]
fn reading_style_table_nowrap_preserves_cascade_breaks_and_source_positions() {
    let contents = include_str!("../../tests/fixtures/table-nowrap.xhtml");
    let mut styles = user_styles::Data::default();
    let choice = "table_no_presentational_hints";
    let phrases = [
        ("harbor", "Harbor"),
        ("beam", "beam"),
        ("early", "Early"),
        ("ferry", "ferry"),
        ("west", "West"),
        ("signal", "signal"),
        ("first", "First watch"),
        ("second", "Second shift"),
        ("storm", "Storm"),
        ("watch", "watch"),
        ("south", "South"),
        ("searchlight", "searchlight"),
    ];
    let mut locations = Vec::new();
    let mut sample = |styles: &user_styles::Data, name| {
        let protected = !styles.enabled(1, choice);
        let mut probe = open(contents, &styles.snapshot(1).unwrap());
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut anchors = Vec::new();
        for (id, phrase) in phrases {
            let start = probe.fragment(id).unwrap();
            let end = Anchor {
                raw_caret: None,
                offset: start.offset + phrase.len(),
                ..start.clone()
            };
            let selection = probe.select_range(&start, &end).unwrap();
            assert_eq!(selection.text, phrase);
            assert_eq!(probe.page_for(&start), Some(0));
            let geometry = probe.selection_rects(0, &selection);
            if protected {
                assert_eq!(geometry.len(), 1, "{name}/{id}");
            }
            let first = geometry[0];
            assert_eq!(
                probe
                    .word_at(0, first.x as f32 + 3., (first.y + first.height / 2) as f32)
                    .unwrap()
                    .start,
                start,
                "{name}/{id}"
            );
            anchors.push(start);
            x.push(first.x);
            y.push(first.y);
        }
        let long = "Storm reports show heavy rain and rough water past the lighthouse through the evening watch";
        let start = probe.fragment("storm").unwrap();
        for word in crate::selection::word_ranges(long) {
            let from = Anchor {
                raw_caret: None,
                offset: start.offset + word.start,
                ..start.clone()
            };
            let to = Anchor {
                raw_caret: None,
                offset: start.offset + word.end,
                ..start.clone()
            };
            let selection = probe.select_range(&from, &to).unwrap();
            assert_eq!(selection.text, long[word]);
            if protected {
                assert_eq!(
                    probe.selection_rects(0, &selection).len(),
                    1,
                    "{name}/{}",
                    selection.text
                );
            }
        }
        if locations.is_empty() {
            locations = anchors;
        } else {
            assert_eq!(locations, anchors);
        }
        #[cfg(feature = "kindle")]
        if let Some(path) = std::env::var_os("READER_TABLE_NOWRAP_CAPTURES") {
            primitive_app::write_png(
                &std::path::Path::new(&path).join(format!("reader-table-nowrap-{name}.png")),
                probe.settings.width,
                probe.settings.height,
                &probe.capture(0).unwrap(),
            )
            .unwrap();
        }
        eprintln!("{name}: x {x:?}, y {y:?}");
        (x, y)
    };

    let publisher = sample(&styles, "publisher");
    styles.choose(1, choice, Some(true)).unwrap();
    let filtered = sample(&styles, "without-html-hints");
    styles.choose(1, choice, Some(false)).unwrap();
    assert_eq!(sample(&styles, "restored"), publisher);
    assert_eq!(publisher.1[0], publisher.1[1]);
    assert!(filtered.1[0] < filtered.1[1]);
    for (_, y) in [publisher, filtered] {
        assert_eq!(y[2], y[3]);
        assert!(y[4] < y[5]);
        assert!(y[6] < y[7]);
        assert!(y[8] < y[9]);
        assert!(y[10] < y[11]);
    }
}

#[test]
fn reading_style_table_nowrap_bounds_an_oversize_unbroken_word() {
    let word = "ABCDEFGHIJK".repeat(8);
    let markup = format!(
        "<html><body><table style='width:210px;border-collapse:collapse'>\
         <tr><td id='sample' nowrap='nowrap'>{word}</td><td>End</td></tr>\
         </table></body></html>"
    );
    let probe = open(&markup, &Default::default());
    let start = probe.fragment("sample").unwrap();
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + word.len(),
        ..start.clone()
    };
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, word);
    let page = probe.page_for(&start).unwrap();
    let rects = probe.selection_rects(page, &selection);
    assert!(rects.len() > 3);
    assert!(rects.iter().all(|rect| rect.x + rect.width <= 240));
}
