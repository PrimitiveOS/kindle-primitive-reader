use super::*;

#[test]
#[ignore = "requires an original EPUB and independently identified source coordinates"]
fn original_footnote_and_formula_baselines() {
    use crate::{Anchor, Engine, Probe, Renderer};
    use std::collections::HashMap;
    let name = std::path::PathBuf::from(std::env::var_os("READER_SCRIPT_EDITION").unwrap());
    let data =
        std::fs::read_to_string(std::env::var_os("READER_SCRIPT_LOCATIONS").unwrap()).unwrap();
    let fonts: Vec<_> =
        std::env::split_paths(&std::env::var_os("READER_TEST_FONTS").unwrap()).collect();
    let mut book = Book::open(&name).unwrap();
    let mut targets = std::collections::BTreeMap::new();
    for (index, row) in data.lines().enumerate() {
        let cols: Vec<_> = row.split('\t').collect();
        assert_eq!(cols.len(), 5);
        let left = crate::cre_pointer::resolve(&mut book, cols[1], Some(20240114)).unwrap();
        let right = crate::cre_pointer::resolve(&mut book, cols[3], Some(20240114)).unwrap();
        assert_eq!(left.chapter, right.chapter);
        targets.entry(left.chapter).or_insert_with(Vec::new).push((
            index + 1,
            cols[0],
            left.anchor,
            right.anchor,
        ));
    }
    let mut verified = 0;
    for (chapter, pairs) in targets {
        let probe = Probe::open(
            &name,
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
        let Renderer::Parley(render) = &probe.renderer else {
            panic!("Expected a horizontal reading page")
        };
        let mut geometry = HashMap::new();
        for p in &render.paragraphs {
            for line in p.layout.lines() {
                let base = line.metrics().baseline;
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    for cluster in run
                        .run()
                        .clusters()
                        .filter(|c| c.style_index() == run.style_index())
                    {
                        let key = (p.key, cluster.text_range().start);
                        geometry.insert(key, (base - run.baseline(), run.run().font_size()));
                    }
                }
            }
        }
        for (index, kind, normal, script) in pairs {
            let position = |a: &Anchor| {
                geometry
                    .get(&(a.block, a.offset))
                    .expect("Source position must paint a glyph")
            };
            assert!(probe.page_for(&normal).is_some() && probe.page_for(&script).is_some());
            let (a, normal_size) = position(&normal);
            let (b, script_size) = position(&script);
            assert!(matches!(kind, "sup" | "sub"));
            let shift = b - a;
            assert!(if kind == "sup" {
                shift > 0.0
            } else {
                shift < 0.0
            });
            assert!(script_size < normal_size);
            println!(
                "Script {index} {kind} shift={:.2} normal={normal_size:.2} script={script_size:.2}",
                shift
            );
            verified += 1;
        }
    }
    println!("Mapped {verified} pairs from original source to glyphs");
}

#[test]
fn raises_footnotes_and_lowers_subscripts_without_changing_the_source() {
    let xml = r##"<html><body><p>A<sup id="fn"><a href="#fn"><em>1</em></a></sup>B<sub id="water">2</sub>C<span style="vertical-align: super">D</span>E<sup style="vertical-align: baseline; font-size: 100%">F</sup>G<sup>X<sup>Y</sup></sup></p></body></html>"##;
    let source = Source::parse(xml).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let href = book.chapters[0].href.clone();
    let mut fonts =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let styles = crate::book_style::BookStyle::load(&mut book, &href, &source, &mut fonts)
        .compute(&source.elements);
    let b = &source.blocks[0];
    assert_eq!(b.text, "A1B2CDEFGXY");
    assert_eq!(source.fragments["fn"], (b.key, 1));
    assert_eq!(b.links[0].bytes, 1..2);
    let settings = Settings {
        font_size: 40.,
        width: 1000,
        height: 1000,
        margin: 40,
        ..Default::default()
    };
    let mut context = LayoutContext::new();
    let (layout, _) = layout_paragraph(&mut context, &mut fonts, b, 900, settings, &styles);
    let line = layout.get(0).unwrap();
    let at = |index: usize| {
        line.items()
            .find_map(|item| {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    return None;
                };
                let contains = run.run().clusters().any(|c| {
                    c.text_range().contains(&index) && c.style_index() == run.style_index()
                });
                contains.then(|| (run.baseline(), run.run().font_size()))
            })
            .unwrap()
    };
    let (base, size) = at(0);
    assert!(
        (base - at(1).0) >= 5.,
        "superscript must rise from the baseline"
    );
    assert!(at(3).0 - base >= 3., "subscript must sit below the line");
    assert!((at(1).1 / size - 0.7).abs() < 0.01);
    assert!((at(3).1 / size - 0.7).abs() < 0.01);
    assert!(
        at(5).0 < base && at(5).1 == size,
        "CSS shift keeps source font size"
    );
    assert_eq!(
        at(7),
        at(0),
        "explicit baseline and size override the defaults"
    );
    assert!(at(10).0 < at(9).0 && at(10).1 < at(9).1);
}
