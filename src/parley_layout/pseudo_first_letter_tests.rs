use crate::{Anchor, Book, Engine, Probe, PublisherRenderMode, Renderer, Settings, user_styles};

const FIXTURE: &str = include_str!("../../tests/fixtures/pseudo-first-letter.xhtml");
const FIRST: &str = "no_pseudo_element_first_letter";
const GENERATED: &str = "no_pseudo_element_before_after";

fn open(snapshot: &user_styles::Snapshot) -> Probe {
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        PublisherRenderMode::Book.apply(Settings {
            width: 617,
            height: 1000,
            margin: 30,
            font_size: 26.,
            ..Settings::default()
        }),
        0,
        Some(FIXTURE.into()),
        &Default::default(),
        snapshot,
    )
    .unwrap()
}

fn anchor(probe: &Probe, word: &str) -> Anchor {
    let found = probe.matches(word);
    assert_eq!(found.len(), 1, "{word}");
    found[0].clone()
}

fn rect(probe: &Probe, within: &str, offset: usize, len: usize) -> crate::selection::HighlightRect {
    let original = anchor(probe, within);
    let start = Anchor {
        raw_caret: None,
        offset: original.offset + offset,
        ..original.clone()
    };
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + len,
        ..original
    };
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, &within[offset..offset + len]);
    probe.selection_rects(probe.page_for(&start).unwrap(), &selection)[0]
}

fn capture(probe: &mut Probe, name: &str) -> Vec<u8> {
    let frame = probe.capture(0).unwrap();
    #[cfg(feature = "kindle")]
    if let Some(dir) = std::env::var_os("READER_PSEUDO_FIRST_LETTER_CAPTURES") {
        primitive_app::write_png(
            &std::path::Path::new(&dir).join(format!("reader-pseudo-first-letter-{name}.png")),
            probe.settings.width,
            probe.settings.height,
            &frame,
        )
        .unwrap();
    }
    frame
}

#[test]
fn pseudo_first_letter_preserves_original_text_and_generated_offsets_across_toggles() {
    let mut controls = user_styles::Data::default();
    let mut publisher = open(&controls.snapshot(1).unwrap());
    controls.choose(1, FIRST, Some(true)).unwrap();
    let mut off = open(&controls.snapshot(1).unwrap());
    controls.choose(1, GENERATED, Some(true)).unwrap();
    assert!(controls.enabled(1, FIRST) && controls.enabled(1, GENERATED));
    let both = open(&controls.snapshot(1).unwrap());
    controls.choose(1, FIRST, Some(false)).unwrap();
    let unprefixed = open(&controls.snapshot(1).unwrap());
    controls.choose(1, GENERATED, Some(false)).unwrap();
    let mut restored = open(&controls.snapshot(1).unwrap());
    for word in [
        "“É”lan",
        "Breakwater",
        "Kestrel",
        "Figural",
        "Reeds",
        "Flock",
        "Marsh",
    ] {
        let source = anchor(&publisher, word);
        for probe in [&off, &both, &unprefixed, &restored] {
            assert_eq!(anchor(probe, word), source);
            assert_eq!(probe.restore_anchor(&source), Some(source.clone()));
        }
    }

    let decorated = |word, offset, len| rect(&publisher, word, offset, len).width;
    let plain = |word, offset, len| rect(&off, word, offset, len).width;
    let same_width = |styled: i32, original: i32| styled.abs_diff(original) <= 1;
    assert!(decorated("“É”lan", 0, 9) > plain("“É”lan", 0, 9) * 2);
    assert!(same_width(decorated("“É”lan", 9, 1), plain("“É”lan", 9, 1)));
    assert!(same_width(
        decorated("Breakwater", 0, 1),
        plain("Breakwater", 0, 1)
    ));
    assert!(decorated("Kestrel", 0, 1) > plain("Kestrel", 0, 1) * 2);
    assert!(decorated("(", 0, 1) > plain("(", 0, 1) * 2);
    assert!(same_width(
        decorated("Figural", 0, 1),
        plain("Figural", 0, 1)
    ));
    assert!(decorated("Reeds", 0, 1) > plain("Reeds", 0, 1) * 2);
    assert!(same_width(
        decorated("Reeds", 0, 1),
        rect(&unprefixed, "Reeds", 0, 1).width
    ));
    assert!(rect(&publisher, "Reeds", 0, 1).x > rect(&unprefixed, "Reeds", 0, 1).x + 50);
    assert!(rect(&off, "Reeds", 0, 1).x > rect(&both, "Reeds", 0, 1).x + 50);
    let Renderer::Parley(chapter) = &publisher.renderer else {
        unreachable!()
    };
    let reeds = anchor(&publisher, "Reeds");
    let paragraph = chapter
        .paragraphs
        .iter()
        .find(|part| part.key == reeds.block)
        .unwrap();
    assert!(
        paragraph
            .inline_text
            .as_ref()
            .unwrap()
            .text
            .starts_with("[Map] Reeds")
    );
    let hit = rect(&publisher, "Reeds", 0, 1);
    assert_eq!(
        publisher
            .word_at(
                0,
                (hit.x + hit.width / 2) as f32,
                (hit.y + hit.height / 2) as f32
            )
            .unwrap()
            .start,
        reeds
    );
    let frame = capture(&mut publisher, "publisher");
    assert_ne!(capture(&mut off, "off"), frame);
    assert_eq!(capture(&mut restored, "restored"), frame);
}

#[test]
fn pseudo_first_letter_floats_and_sinks_with_selectable_original_initials() {
    let mut controls = user_styles::Data::default();
    let publisher = open(&controls.snapshot(1).unwrap());
    controls.choose(1, FIRST, Some(true)).unwrap();
    let off = open(&controls.snapshot(1).unwrap());
    let Renderer::Parley(chapter) = &publisher.renderer else {
        unreachable!()
    };
    for word in ["Flock", "Marsh"] {
        let first = rect(&publisher, word, 0, 1);
        let next = rect(&publisher, word, 1, 1);
        assert!(
            first.height > rect(&off, word, 0, 1).height * 2,
            "{word}: {first:?}"
        );
        assert!(
            first.x + first.width <= next.x,
            "{word}: {first:?}/{next:?}"
        );
        let expected = anchor(&publisher, word);
        let paragraph = chapter
            .paragraphs
            .iter()
            .find(|part| part.key == expected.block)
            .unwrap();
        let initial = paragraph
            .inline_text
            .as_ref()
            .and_then(super::inline_blocks::InlineText::initial_bounds);
        assert!(initial.is_some(), "{word}");
        let picked = publisher.word_at(
            publisher.page_for(&expected).unwrap(),
            (first.x + first.width / 2) as f32,
            (first.y + first.height / 2) as f32,
        );
        assert_eq!(picked.unwrap().start, expected);
    }
    let starts = |probe: &Probe| {
        let Renderer::Parley(chapter) = &probe.renderer else {
            unreachable!()
        };
        let target = anchor(probe, "Marsh");
        chapter
            .paragraphs
            .iter()
            .find(|part| part.key == target.block)
            .unwrap()
            .layout
            .lines()
            .map(|line| line.metrics().inline_min_coord)
            .collect::<Vec<_>>()
    };
    let wrapped = starts(&publisher);
    let plain = starts(&off);
    assert!(
        wrapped.len() >= 3 && wrapped[0] > 100.0 && wrapped[1] == wrapped[0],
        "{wrapped:?}"
    );
    assert!(wrapped[2..].iter().all(|&edge| edge == 0.0), "{wrapped:?}");
    assert!(plain.iter().all(|&edge| edge == 0.0), "{plain:?}");
}
