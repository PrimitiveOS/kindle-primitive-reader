use super::*;
use crate::{Anchor, Engine, Probe, PublisherRenderMode, Renderer, user_styles};

const SOURCE: &str = include_str!("../../tests/fixtures/pseudo-first-line.xhtml");
const FIRST_LINE: &str = "no_pseudo_element_first_line";
const FIRST_LETTER: &str = "no_pseudo_element_first_letter";
const GENERATED: &str = "no_pseudo_element_before_after";

fn open(width: u32, source: &str, controls: &user_styles::Data) -> Probe {
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
            crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"),
        ],
        PublisherRenderMode::Book.apply(Settings {
            width,
            height: 900,
            margin: 30,
            font_size: 16.,
            automatic_hyphenation: false,
            ..Settings::default()
        }),
        0,
        Some(source.into()),
        &Default::default(),
        &controls.snapshot(1).unwrap(),
    )
    .unwrap()
}

fn found(probe: &Probe, word: &str) -> Anchor {
    let matches = probe.matches(word);
    assert_eq!(matches.len(), 1, "{word}");
    matches[0].clone()
}

fn paragraph<'a>(probe: &'a Probe, at: &Anchor) -> &'a Paragraph {
    let Renderer::Parley(chapter) = &probe.renderer else {
        unreachable!()
    };
    chapter
        .paragraphs
        .iter()
        .find(|block| block.key == at.block)
        .unwrap()
}

fn glyph(probe: &Probe, word: &str, inside: usize) -> (usize, f32) {
    let anchor = found(probe, word);
    let p = paragraph(probe, &anchor);
    let source = anchor.offset + inside;
    let byte = p.inline_text.as_ref().map_or(source, |text| {
        text.layout_spans(&(source..source + 1))
            .next()
            .unwrap()
            .start
    });
    let (index, line) = p
        .layout
        .lines()
        .enumerate()
        .find(|(_, line)| line.text_range().contains(&byte))
        .unwrap_or_else(|| panic!("missing line for {word}:{inside} at {byte}"));
    let px = line
        .items()
        .find_map(|item| match item {
            PositionedLayoutItem::GlyphRun(run) if run.run().text_range().contains(&byte) => {
                Some(run.run().font_size())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing glyph for {word}:{inside} at {byte}"));
    (index, px)
}

fn capture(probe: &mut Probe, name: &str) -> Vec<u8> {
    let pixels = probe.capture(0).unwrap();
    #[cfg(feature = "kindle")]
    if let Some(dir) = std::env::var_os("READER_PSEUDO_FIRST_LINE_CAPTURES") {
        primitive_app::write_png(
            &std::path::Path::new(&dir).join(format!(
                "reader-pseudo-first-line-{}-{name}.png",
                probe.settings.width
            )),
            probe.settings.width,
            probe.settings.height,
            &pixels,
        )
        .unwrap();
    }
    #[cfg(not(feature = "kindle"))]
    let _ = name;
    pixels
}

#[test]
fn pseudo_first_line_preserves_logical_wrap_selection_and_independent_controls() {
    let quoted = "“É”merald";
    for width in [510, 650] {
        let mut controls = user_styles::Data::default();
        let mut styled = open(width, SOURCE, &controls);
        controls.choose(1, FIRST_LINE, Some(true)).unwrap();
        let mut plain = open(width, SOURCE, &controls);
        controls.choose(1, FIRST_LINE, Some(false)).unwrap();
        controls.choose(1, FIRST_LETTER, Some(true)).unwrap();
        let mut letter_off = open(width, SOURCE, &controls);
        controls.choose(1, FIRST_LETTER, Some(false)).unwrap();
        controls.choose(1, GENERATED, Some(true)).unwrap();
        let mut prefix_off = open(width, SOURCE, &controls);
        controls.choose(1, GENERATED, Some(false)).unwrap();
        let mut restored = open(width, SOURCE, &controls);
        let frame = capture(&mut styled, "publisher");
        assert_ne!(frame, capture(&mut plain, "line-off"));
        assert_ne!(frame, capture(&mut letter_off, "letter-off"));
        assert_ne!(frame, capture(&mut prefix_off, "prefix-off"));
        assert_eq!(frame, capture(&mut restored, "restored"));

        for word in [
            quoted,
            "Harbor",
            "Ternbird",
            "Sounding",
            "Current",
            "Sails",
            "Reef",
            "Shoal",
            "Channel",
            "Lagoon",
            "Lantern",
            "Beacon",
            "Shelter",
            "Bookcase",
            "صفحة",
            "كتاب",
            "دليل",
            "رحلة",
            "بحر",
            "ساحل",
            "طريق",
            "مرفأ",
            "Wavelengths",
            "Measuring",
            "Fluxgate",
            "Surveyor",
            "Anemone",
            "Metalwork",
            "Jurisdiction",
        ] {
            let original = found(&styled, word);
            for probe in [&plain, &letter_off, &prefix_off, &restored] {
                assert_eq!(found(probe, word), original);
                assert_eq!(probe.restore_anchor(&original), Some(original.clone()));
            }
        }
        let initial = found(&styled, quoted);
        let first = paragraph(&styled, &initial);
        let baseline = paragraph(&plain, &initial);
        let lead = first.inline_text.as_ref().unwrap();
        assert!(lead.text.starts_with("[Chart] “É”"));
        let styled_end = first.layout.get(0).unwrap().text_range().end;
        assert!(styled_end < baseline.layout.get(0).unwrap().text_range().end);
        assert_eq!(glyph(&styled, quoted, 0).1, glyph(&plain, quoted, 0).1);
        assert!(glyph(&styled, quoted, 8).1 > glyph(&plain, quoted, 8).1 * 1.5);
        assert!(glyph(&letter_off, quoted, 0).1 < glyph(&styled, quoted, 0).1);
        assert_eq!(glyph(&styled, quoted, 8), glyph(&letter_off, quoted, 8));
        assert!(glyph(&styled, "Lantern", 0).1 > glyph(&plain, "Lantern", 0).1);
        assert_eq!(glyph(&styled, "Shelter", 0), glyph(&plain, "Shelter", 0));
        assert_eq!(glyph(&styled, "Bookcase", 0), glyph(&plain, "Bookcase", 0));
        assert_eq!(
            glyph(&styled, "Anemone", 0).1,
            glyph(&plain, "Anemone", 0).1
        );
        assert_eq!(
            glyph(&styled, "Wavelengths", 0).1,
            glyph(&plain, "Wavelengths", 0).1
        );
        if width == 510 {
            assert!(glyph(&styled, "Fluxgate", 0).0 > glyph(&plain, "Fluxgate", 0).0);
        }
        assert!(glyph(&styled, "Metalwork", 0).1 > glyph(&plain, "Metalwork", 0).1 * 1.4);
        assert!(glyph(&styled, "صفحة", 0).1 > glyph(&plain, "صفحة", 0).1 * 1.5);
        let arabic = found(&styled, "صفحة");
        assert!(paragraph(&styled, &arabic).layout.is_rtl());
        assert!(
            paragraph(&styled, &arabic)
                .layout
                .get(0)
                .unwrap()
                .text_range()
                .end
                < paragraph(&plain, &arabic)
                    .layout
                    .get(0)
                    .unwrap()
                    .text_range()
                    .end
        );

        let end = found(&styled, "Lagoon");
        let end = Anchor {
            offset: end.offset + "Lagoon".len(),
            ..end
        };
        for probe in [&styled, &plain, &letter_off, &prefix_off] {
            let selected = probe.select_range(&initial, &end).unwrap();
            assert_eq!(
                selected.text,
                "“É”merald Harbor Ternbird Sounding Current Sails Reef Shoal Channel Lagoon"
            );
            assert!(probe.matches("Chart").is_empty());
            let page = probe.page_for(&initial).unwrap();
            assert!(!probe.selection_rects(page, &selected).is_empty());
            for word in ["Ternbird", "ساحل"] {
                let target = found(probe, word);
                let tail = Anchor {
                    offset: target.offset + word.len(),
                    ..target.clone()
                };
                let selected = probe.select_range(&target, &tail).unwrap();
                let page = probe.page_for(&target).unwrap();
                let rect = probe.selection_rects(page, &selected)[0];
                let x = rect.x as f32 + rect.width as f32 / 2.;
                let y = rect.y as f32 + rect.height as f32 / 2.;
                assert_eq!(probe.word_at(page, x, y).unwrap().start, target);
            }
        }
    }
}

#[test]
fn pseudo_first_line_retains_a_real_discretionary_hyphen_at_the_style_boundary() {
    let source = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>
        p { font-family: "Noto Sans"; font-size:26px; text-indent:0; }
        p::first-line { font-size:200%; }
        </style></head><body><p>Weather­proofing protects the archive.</p></body></html>"#;
    let styled = open(320, source, &user_styles::Data::default());
    let at = found(&styled, "Weather");
    let lines: Vec<_> = paragraph(&styled, &at).layout.lines().collect();
    assert!(lines[0].has_discretionary_hyphen());
    assert_eq!(lines[0].text_range().end, "Weather\u{ad}".len());
    assert_eq!(lines[1].text_range().start, lines[0].text_range().end);
    assert_eq!(glyph(&styled, "Weather", 0), (0, 52.));
    assert_eq!(glyph(&styled, "proofing", 0), (1, 26.));
}
