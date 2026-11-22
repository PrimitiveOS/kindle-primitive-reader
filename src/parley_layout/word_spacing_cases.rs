use super::*;
use crate::WordSpacePercent;

fn shaped(
    text: &str,
    pair: Option<WordSpacePercent>,
    width: f32,
    preserved: &[Range<usize>],
) -> Layout<PaintBrush> {
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(40.0));
    let mut layout = builder.build(text);
    if let Some(pair) = pair {
        layout.set_word_spacing_percent(text, pair.scaling, pair.minimum, preserved);
    }
    layout.break_all_lines(Some(width));
    layout.align(Alignment::Start, AlignmentOptions::default());
    layout
}
fn advance(layout: &Layout<PaintBrush>, offset: usize) -> f32 {
    parley::Cluster::from_byte_index(layout, offset)
        .unwrap()
        .advance()
}

#[test]
fn percent_scales_selected_font_spaces_but_not_fixed_spaces_or_letters() {
    let text = "A B\u{a0}C\u{2003}D";
    let normal = shaped(text, None, 2000., &[]);
    let changed = shaped(
        text,
        Some(WordSpacePercent {
            scaling: 95,
            minimum: 100,
        }),
        2000.,
        &[],
    );
    for (at, ch) in text.char_indices() {
        let ratio = if matches!(ch, ' ' | '\u{a0}') {
            0.95
        } else {
            1.0
        };
        assert!(
            (advance(&changed, at) - advance(&normal, at) * ratio).abs() < 0.01,
            "{ch:?}"
        );
    }
}

#[test]
fn shrink_admits_a_word_only_when_needed_and_obeys_minimum() {
    let text = "alpha beta gamma";
    let pair = WordSpacePercent {
        scaling: 100,
        minimum: 50,
    };
    let wide = shaped(text, Some(pair), 2000., &[]);
    let natural = wide.get(0).unwrap().metrics().advance;
    let spaces: Vec<_> = text
        .match_indices(' ')
        .map(|(at, _)| (at, advance(&wide, at)))
        .collect();
    let capacity: f32 = spaces.iter().map(|(_, width)| width * 0.5).sum();
    let target = natural - capacity * 0.5;
    let fitted = shaped(text, Some(pair), target, &[]);
    let rigid = shaped(
        text,
        Some(WordSpacePercent {
            minimum: 100,
            ..pair
        }),
        target,
        &[],
    );
    assert_eq!(fitted.len(), 1);
    assert!(rigid.len() > 1);
    assert!((fitted.get(0).unwrap().metrics().advance - target).abs() < 0.02);
    for (at, width) in &spaces {
        let fitted_width = advance(&fitted, *at);
        assert!(fitted_width >= *width * 0.5 - 0.01);
        assert!(fitted_width < *width);
        assert!((advance(&wide, *at) - *width).abs() < 0.01);
    }
    let too_narrow = shaped(text, Some(pair), natural - capacity - 1.0, &[]);
    assert!(
        too_narrow.len() > 1,
        "must not exceed permitted condensing to fit the final word"
    );
}

#[test]
fn prefix_and_preformatted_locks_keep_their_gap_and_nbsp_remains_unbreakable() {
    let text = "\u{a0}— \" alpha\u{a0}beta gamma delta";
    let normal = shaped(text, None, 2000., &[]);
    let pair = WordSpacePercent {
        scaling: 100,
        minimum: 25,
    };
    let natural = normal.get(0).unwrap().metrics().advance;
    let fitted = shaped(text, Some(pair), natural - 2., &[]);
    assert_eq!(fitted.len(), 1);
    for at in [0, 5, 7] {
        assert!((advance(&fitted, at) - advance(&normal, at)).abs() < 0.01);
    }
    let nbsp = text.find("alpha").unwrap() + "alpha".len();
    assert!(advance(&fitted, nbsp) < advance(&normal, nbsp));
    let preserved = shaped(text, Some(WordSpacePercent::SMALL), 2000., &[0..text.len()]);
    assert_eq!(
        normal.get(0).unwrap().metrics().advance,
        preserved.get(0).unwrap().metrics().advance
    );
    for line in shaped("alpha\u{a0}beta gamma", Some(pair), 150., &[]).lines() {
        assert_ne!(line.text_range().end, "alpha\u{a0}".len());
    }
}

#[test]
fn explicit_100_100_is_pixel_identity_and_all_settings_keep_canonical_source() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let make = |pair| {
        crate::Probe::open(
            &path,
            crate::Engine::Parley,
            &fonts,
            Settings {
                percent_word_spacing: pair,
                ..Settings::default()
            },
            0,
        )
        .unwrap()
    };
    let mut old = make(None);
    let mut identity = make(Some(WordSpacePercent {
        scaling: 100,
        minimum: 100,
    }));
    assert_eq!(old.page_count(), identity.page_count());
    for page in 0..old.page_count() {
        assert_eq!(old.page_anchor(page), identity.page_anchor(page));
        assert_eq!(old.capture(page).unwrap(), identity.capture(page).unwrap());
    }
    for pair in [
        WordSpacePercent::SMALL,
        WordSpacePercent::MEDIUM,
        WordSpacePercent::LARGE,
    ] {
        let changed = make(Some(pair));
        assert_eq!(old.source.revision, changed.source.revision);
        for (a, b) in old.source.blocks.iter().zip(&changed.source.blocks) {
            assert_eq!((a.key, &a.text), (b.key, &b.text));
            for word in crate::selection::word_ranges(&a.text).take(5) {
                let start = old.anchor(a.key, word.start);
                let end = old.anchor(a.key, word.end);
                let selected = changed.select_range(&start, &end).unwrap();
                assert_eq!(selected.text, a.text[word]);
                let page = changed.page_for(&start).unwrap();
                let boxes = changed.selection_rects(page, &selected);
                assert!(!boxes.is_empty());
            }
        }
    }
}

#[test]
fn multiline_condensing_justification_and_tracking_keep_copy_word_hits_and_links() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let paragraph="The returning boats cross the harbor while careful readers follow every word along the shore. ".repeat(8);
    let markup = format!(
        "<html><body><p id='start'>Before the voyage <a href='#end'>returning boats</a> {paragraph}</p><p id='end'>Arrival.</p></body></html>"
    );
    for pair in [
        WordSpacePercent::SMALL,
        WordSpacePercent::MEDIUM,
        WordSpacePercent::LARGE,
    ] {
        let probe = crate::Probe::from_book_content(
            Book::open(&path).unwrap(),
            crate::Engine::Parley,
            &fonts,
            Settings {
                width: 420,
                height: 460,
                margin: 12,
                font_size: 38.,
                percent_word_spacing: Some(pair),
                alignment: crate::TextAlignment::Justify,
                word_expansion: 15,
                letter_spacing: 0.5,
                automatic_hyphenation: false,
                disable_soft_hyphens: true,
                ..Settings::default()
            },
            0,
            Some(markup.clone()),
        )
        .unwrap();
        assert!(probe.page_count() > 1);
        let block = &probe.source.blocks[0];
        let all = probe
            .select_range(
                &probe.anchor(block.key, 0),
                &probe.anchor(block.key, block.text.len()),
            )
            .unwrap();
        assert_eq!(all.text, block.text);
        let mut hits = 0;
        for range in crate::selection::word_ranges(&block.text)
            .filter(|range| range.len() > 4)
            .take(35)
        {
            let start = probe.anchor(block.key, range.start);
            let end = probe.anchor(block.key, range.end);
            let selected = probe.select_range(&start, &end).unwrap();
            let page = probe.page_for(&start).unwrap();
            let boxes = probe.selection_rects(page, &selected);
            if let [rect] = &boxes[..] {
                let (x, y) = (
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                );
                assert_eq!(probe.word_at(page, x, y).unwrap(), selected);
                if block
                    .links
                    .iter()
                    .any(|link| link.bytes.contains(&range.start))
                {
                    assert_eq!(
                        probe.link_at(page, x, y).unwrap().fragment.as_deref(),
                        Some("end")
                    );
                }
                hits += 1;
            }
        }
        assert!(
            hits >= 20,
            "enough independent source-to-pixel-to-source word checks"
        );
    }
}
