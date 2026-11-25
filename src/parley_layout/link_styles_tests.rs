use super::*;
use crate::selection::HighlightRect;
use crate::{Engine, Probe, Renderer, user_styles};
use parley::FontStyle;
#[cfg(feature = "kindle")]
use std::path::Path;

const PIECES: [(&str, &str); 5] = [
    ("plain", "Wide plain link"),
    ("nested", "Held nested link"),
    ("marker", "Named anchor only"),
    ("outside", "Quiet shore text"),
    ("parent-ink", "Decorated parent link"),
];

fn open(preset: Option<&str>) -> Probe {
    let mut data = user_styles::Data::default();
    if let Some(id) = preset {
        data.choose(1, id, Some(true)).unwrap();
    }
    Probe::from_book_with_rules(
        Book::open(&crate::test_fixture("reading-link-styles.xhtml")).unwrap(),
        Engine::Parley,
        &[
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
        Settings {
            width: 650,
            height: 640,
            font_size: 40.,
            margin: 28,
            ..Settings::default()
        },
        0,
        None,
        &crate::user_hyphenation::Rules::default(),
        &data.snapshot(1).unwrap(),
    )
    .unwrap()
}

fn run_style(probe: &Probe, id: &str) -> (PaintBrush, f32, FontStyle) {
    let anchor = probe.fragment(id).unwrap();
    let Renderer::Parley(chapter) = &probe.renderer else {
        panic!("Expected the horizontal link layout");
    };
    let part = chapter
        .paragraphs
        .iter()
        .find(|p| p.key == anchor.block)
        .unwrap();
    for item in part.layout.lines().flat_map(|line| line.items()) {
        if let PositionedLayoutItem::GlyphRun(run) = item
            && run.run().clusters().any(|cluster| {
                cluster.style_index() == run.style_index()
                    && cluster.text_range().contains(&anchor.offset)
            })
        {
            let font = run.run().font_attrs();
            return (run.style().brush, font.weight.value(), font.style);
        }
    }
    panic!("Link specimen's first glyph run is missing");
}

fn region(probe: &Probe, id: &str, phrase: &str) -> HighlightRect {
    let begin = probe.fragment(id).unwrap();
    assert_eq!(probe.page_for(&begin), Some(0));
    let end = crate::Anchor {
        offset: begin.offset + phrase.len(),
        ..begin.clone()
    };
    let text = probe.select_range(&begin, &end).unwrap();
    assert_eq!(text.text, phrase);
    let areas = probe.selection_rects(0, &text);
    assert_eq!(areas.len(), 1);
    areas[0]
}

fn ink(frame: &[u8], width: usize, bounds: HighlightRect) -> u8 {
    (bounds.y..bounds.y + bounds.height)
        .map(|y| {
            let offset = y as usize * width + bounds.x as usize;
            *frame[offset..offset + bounds.width as usize]
                .iter()
                .min()
                .unwrap()
        })
        .min()
        .unwrap()
}

#[test]
fn reading_style_link_pairs_preserve_anchors_and_measure_the_painted_ink() {
    let baseline = open(None);
    let effects = [
        "link-black",
        "link-blue",
        "link-bold",
        "link-no-bold",
        "link-italic",
        "link-no-italic",
        "link-underline",
        "link-no-underline",
    ];
    let selections = PIECES.map(|(id, phrase)| {
        let rect = region(&baseline, id, phrase);
        baseline.link_at(0, rect.x as f32 + 4., rect.y as f32 + 5.)
    });
    assert!(selections[0].is_some() && selections[0] == selections[1]);
    assert!(selections[2].is_none() && selections[3].is_none());
    for preset in effects {
        let mut probe = open(Some(preset));
        let frame = probe.capture(0).unwrap();
        for ((id, phrase), destination) in PIECES.into_iter().zip(&selections) {
            let before = baseline.fragment(id).unwrap();
            assert_eq!(probe.restore_anchor(&before), Some(before.clone()));
            assert_eq!(probe.matches(phrase), baseline.matches(phrase));
            let area = region(&probe, id, phrase);
            assert_eq!(
                &probe.link_at(0, area.x as f32 + 4., area.y as f32 + 5.),
                destination,
                "{preset} {id}"
            );
            let (brush, weight, slant) = run_style(&probe, id);
            if id == "outside" {
                assert_eq!((brush, weight, slant), run_style(&baseline, id));
                assert_eq!(ink(&frame, 650, area), 0);
                continue;
            }
            match preset {
                "link-black" | "link-blue" => {
                    let gray = if preset == "link-black" { 0 } else { 63 };
                    assert_eq!(brush.link_ink, Some(gray));
                    assert_eq!(ink(&frame, 650, area), gray, "{preset} {id}");
                }
                "link-bold" => assert_eq!(weight, 700.),
                "link-no-bold" => assert_eq!(weight, 400.),
                "link-italic" => assert_eq!(slant, FontStyle::Italic),
                "link-no-italic" => assert_eq!(slant, FontStyle::Normal),
                "link-underline" => assert_eq!(brush.decoration.underline, id != "marker"),
                "link-no-underline" => assert_eq!(brush.decoration.underline, id == "parent-ink"),
                _ => unreachable!(),
            }
        }
        assert_eq!(probe.coverage(), baseline.coverage());
        #[cfg(feature = "kindle")]
        if let Some(dir) = std::env::var_os("READER_LINK_STYLE_CAPTURES") {
            primitive_app::write_png(
                &Path::new(&dir).join(format!("reader-{preset}.png")),
                650,
                640,
                &frame,
            )
            .unwrap();
        }
    }
}

#[cfg(feature = "kindle")]
#[test]
fn reading_style_link_ink_reaches_the_vertical_text_renderer() {
    let markup = r##"<html style="writing-mode:vertical-rl"><body>
        <p><a href="#plain">BLUE LINK</a></p><p id="plain">PLAIN TEXT</p>
        </body></html>"##;
    let mut styles = user_styles::Data::default();
    styles.choose(1, "link-blue", Some(true)).unwrap();
    let preset = styles.snapshot(1).unwrap();
    let epub = crate::test_fixture("field-notes-epub3.epub");
    let mut probe = Probe::from_book_with_rules(
        Book::open(&epub).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 650,
            height: 640,
            font_size: 40.,
            margin: 28,
            ..Settings::default()
        },
        0,
        Some(markup.into()),
        &crate::user_hyphenation::Rules::default(),
        &preset,
    )
    .unwrap();
    assert!(matches!(probe.renderer, Renderer::Vertical(_)));
    let page = probe.capture(0).unwrap();
    let rects = |text: &str| {
        let start = probe.matches(text)[0].clone();
        let end = crate::Anchor {
            offset: start.offset + text.len(),
            ..start.clone()
        };
        let selection = probe.select_range(&start, &end).unwrap();
        probe.selection_rects(0, &selection)
    };
    for (text, expected) in [("BLUE LINK", 63), ("PLAIN TEXT", 0)] {
        assert_eq!(
            rects(text)
                .into_iter()
                .map(|rect| ink(&page, 650, rect))
                .min(),
            Some(expected),
            "{text}"
        );
    }
}
