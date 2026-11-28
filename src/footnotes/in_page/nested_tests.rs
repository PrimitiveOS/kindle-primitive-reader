use super::*;
use crate::{Engine, Probe, test_fixture};

fn settings() -> Settings {
    Settings {
        width: 720,
        height: 480,
        margin: 32,
        font_size: 25.0,
        line_height: 1.2,
        in_page_footnotes: true,
        ..Settings::default()
    }
}

#[test]
fn a_nested_epub_note_shares_its_outer_note_band_in_source_order() {
    let probe = Probe::open(
        &test_fixture("in-page-nested-notes.epub"),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        settings(),
        0,
    )
    .unwrap();
    let pages = probe
        .in_page_notes
        .as_ref()
        .expect("nested notes should compose in-page");
    let inner = probe.fragment("n2").unwrap();
    let inner_top = probe.anchor_top(&inner).unwrap();
    let outer = probe.fragment("n1").unwrap();
    let outer_top = probe.anchor_top(&outer).unwrap();
    let page = pages
        .iter()
        .position(|page| {
            page.parts
                .iter()
                .any(|part| part.y > 0 && part.source.contains(&inner_top))
        })
        .expect("the inner note should appear in a band");
    let band: Vec<_> = pages[page]
        .parts
        .iter()
        .filter(|part| part.y > 0)
        .collect();
    let outer_index = band
        .iter()
        .position(|part| part.source.contains(&outer_top))
        .expect("the outer note should share the band");
    let inner_index = band
        .iter()
        .position(|part| part.source.contains(&inner_top))
        .unwrap();
    assert!(outer_index < inner_index, "the outer note leads its nested note");
}
