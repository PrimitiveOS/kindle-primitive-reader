use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::{Anchor, Engine, Probe, Renderer};

fn painted_decoration(probe: &Probe, anchor: &Anchor) -> TextDecoration {
    let Renderer::Parley(renderer) = &probe.renderer else {
        panic!("Expected horizontal text");
    };
    let p = renderer
        .paragraphs
        .iter()
        .find(|p| p.key == anchor.block)
        .unwrap();
    for line in p.layout.lines() {
        if !line.text_range().contains(&anchor.offset) {
            continue;
        }
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyphs) = item else {
                continue;
            };
            if glyphs.run().clusters().any(|cluster| {
                cluster.style_index() == glyphs.style_index()
                    && cluster.text_range().contains(&anchor.offset)
            }) {
                return glyphs.style().brush;
            }
        }
    }
    panic!("Source character has no painted run");
}

fn decoration(kind: &str) -> TextDecoration {
    TextDecoration {
        underline: kind.contains("underline"),
        overline: kind.contains("overline"),
        strike: kind.contains("line-through"),
    }
}

#[test]
fn publisher_decoration_preserves_links_selection_and_painted_lines() {
    let settings = Settings {
        width: 840,
        height: 980,
        font_size: 32.,
        margin: 24,
        margin_left: Some(52),
        margin_right: Some(17),
        ..Default::default()
    };
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let book = crate::test_fixture("field-notes-epub3.epub");
    let xml = r##"<html><head><style>
    a[href].quiet {text-decoration:none     } .under {text-decoration-line:underline}
    .above {text-decoration:overline solid} .two {text-decoration:underline line-through}
    a:link {text-decoration:underline} .anchor[href] {text-decoration:line-through}
    </style></head><body><p><a id="plain" href="#end" class="quiet">Quiet link</a> and
    <a id="marked" href="#end">usual link</a>, <span class="under"><em id="in" style="text-decoration:none">emphasis</em></span>,
    <span id="mean" class="above">mean</span>, <del id="old">old</del>, <ins id="new">new</ins>,
    <span id="both" class="two">two lines</span>, <span id="empty" class="anchor">no target</span>.</p>
    <p id="end">Landing</p></body></html>"##;
    let open = |markup: &str| {
        Probe::from_book_content(
            Book::open(&book).unwrap(),
            Engine::Parley,
            &fonts,
            settings,
            0,
            Some(markup.into()),
        )
        .unwrap()
    };
    let mut probe = open(xml);
    for (id, expected) in [
        ("plain", "none"),
        ("marked", "underline"),
        ("in", "underline"),
        ("mean", "overline"),
        ("old", "line-through"),
        ("new", "underline"),
        ("both", "underline line-through"),
        ("empty", "none"),
    ] {
        let a = probe.fragment(id).unwrap();
        assert_eq!(painted_decoration(&probe, &a), decoration(expected), "{id}");
        assert!(probe.page_for(&a).is_some());
    }
    let plain = probe.fragment("plain").unwrap();
    let landing = probe.fragment("end").unwrap();
    let alternate = xml.replace(
        "text-decoration:none     } .under",
        "text-decoration:underline} .under",
    );
    let mut underlined = open(&alternate);
    let underlined_link = underlined.fragment("plain").unwrap();
    let underlined_target = underlined.fragment("end").unwrap();
    assert_eq!(
        (plain.block, plain.offset),
        (underlined_link.block, underlined_link.offset)
    );
    assert_eq!(
        (landing.block, landing.offset),
        (underlined_target.block, underlined_target.offset)
    );
    assert_eq!(open(xml).fragment("plain").unwrap(), plain);
    assert_eq!(probe.source.blocks[0].links.len(), 2);
    assert_eq!(probe.source.blocks[0].links[0].href, "#end");
    assert_eq!(probe.matches("Quiet link").len(), 1);
    let original = probe.capture(0).unwrap();
    let with_line = underlined.capture(0).unwrap();
    let width = settings.width as usize;
    let changed: Vec<_> = original
        .iter()
        .zip(&with_line)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .collect();
    assert!(changed.len() >= 20);
    assert!(changed.iter().all(|(i, _)| settings.text_columns().contains(&(i % width))));
    let ymin = changed[0].0 / width;
    let ymax = changed.last().unwrap().0 / width;
    assert!(ymax - ymin <= 4, "Only the link underline should move ink");
    assert!(changed.iter().all(|(_, (_, b))| **b == 48));
}

#[test]
#[ignore = "requires an original EPUB and independently identified source coordinates"]
fn original_publisher_decorations_follow_verified_source_passages() {
    let edition = PathBuf::from(std::env::var_os("READER_DECORATION_EDITION").unwrap());
    let locations =
        std::fs::read_to_string(std::env::var_os("READER_DECORATION_LOCATIONS").unwrap()).unwrap();
    let fonts: Vec<_> =
        std::env::split_paths(&std::env::var_os("READER_TEST_FONTS").unwrap()).collect();
    let mut book = Book::open(&edition).unwrap();
    let mut chapters = BTreeMap::new();
    for (i, row) in locations.lines().enumerate() {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let a = crate::cre_pointer::resolve(&mut book, fields[1], Some(20240114)).unwrap();
        let b = crate::cre_pointer::resolve(&mut book, fields[2], Some(20240114)).unwrap();
        assert_eq!(a.chapter, b.chapter);
        assert_eq!(a.anchor.block, b.anchor.block);
        assert!(a.anchor.offset < b.anchor.offset);
        chapters
            .entry(a.chapter)
            .or_insert_with(Vec::new)
            .push((i + 1, fields[0], a.anchor));
    }
    for (chapter, coordinates) in chapters {
        let probe = Probe::open(
            &edition,
            Engine::Parley,
            &fonts,
            Settings {
                width: 1264,
                height: 1606,
                margin: 54,
                font_size: 42.,
                ..Default::default()
            },
            chapter,
        )
        .unwrap();
        for (index, kind, anchor) in coordinates {
            assert!(probe.page_for(&anchor).is_some());
            assert_eq!(
                painted_decoration(&probe, &anchor),
                decoration(kind),
                "position {index}"
            );
            println!("Decoration {index} {kind} matched");
        }
    }
}
