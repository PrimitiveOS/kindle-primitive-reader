use super::*;
use crate::{Engine, HyphenationLimits, Probe, Renderer};

fn open(settings: Settings) -> Probe {
    Probe::open(
        &crate::test_fixture("hyphenation-options.epub"),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
    )
    .unwrap()
}

fn first_line_offsets(probe: &Probe) -> Vec<usize> {
    let Renderer::Parley(renderer) = &probe.renderer else {
        unreachable!()
    };
    renderer
        .paragraphs
        .iter()
        .map(|paragraph| {
            let block = probe
                .source
                .blocks
                .iter()
                .find(|block| block.key == paragraph.key)
                .unwrap();
            let end = paragraph.layout.get(0).unwrap().text_range().end;
            block.text[..end].chars().count()
        })
        .collect()
}

#[test]
fn hyphenation_options_choose_fitting_authored_breaks_without_suppressing_fallback() {
    let settings = Settings {
        width: 166,
        height: 720,
        margin: 16,
        font_size: 42.0,
        ..Settings::default()
    };
    let baseline = open(settings);
    let trusted = open(Settings {
        trust_soft_hyphens: true,
        ..settings
    });
    let limits = open(Settings {
        trust_soft_hyphens: true,
        hyphenation_limits: Some(HyphenationLimits { left: 5, right: 5 }),
        ..settings
    });
    let algorithmic = open(Settings {
        trust_soft_hyphens: true,
        algorithmic_hyphenation: true,
        ..settings
    });
    let authored = open(Settings {
        trust_soft_hyphens: true,
        automatic_hyphenation: false,
        ..settings
    });
    let profiles = [&baseline, &trusted, &limits, &algorithmic, &authored].map(first_line_offsets);
    let [before, prefer, limited, algorithm, soft_only] = &profiles;
    assert_eq!(before.len(), 5, "{profiles:?}");
    assert!(before[0] > 3, "{profiles:?}");
    assert_eq!(prefer[0], 3, "{profiles:?}");
    assert_eq!(prefer[1], before[1], "{profiles:?}");
    assert_eq!(prefer[2], before[2], "{profiles:?}");
    assert_eq!(limited[0], 3, "{profiles:?}");
    assert_ne!(limited[2], before[2], "{profiles:?}");
    assert_eq!(algorithm[0], 3, "{profiles:?}");
    assert_ne!(algorithm[4], before[4], "{profiles:?}");
    assert_eq!(soft_only[0], 3, "{profiles:?}");
    let start = baseline.matches("adventure").remove(0);
    for candidate in [&trusted, &limits, &algorithmic, &authored] {
        assert_eq!(candidate.coverage(), baseline.coverage());
        let same_passage = candidate.restore_anchor(&start).unwrap();
        assert_eq!(same_passage, start);
        assert!(candidate.page_for(&same_passage).is_some());
    }
}
