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

#[test]
fn multilingual_hyphenation_renders_and_selects_the_original_word() {
    let words: &[&[&str]] = &[
        &["información", "lingüística", "murciélago"],
        &["straordinario", "elettroencefalogramma", "acqua"],
        &["informação", "linguística", "extraordinário", "saúde"],
    ];
    assert_dictionary_rendering("multilingual-hyphenation", 570, words);
}

#[test]
fn multilingual_hyphenation_renders_and_selects_estonian_and_finnish() {
    let words: &[&[&str]] = &[
        &[
            "õigekeelsussõnaraamat",
            "raudteejaam",
            "lühiülevaade",
            "päikesepaisteline",
        ],
        &[
            "työskenteleminen",
            "ääntäminen",
            "kansainvälinen",
            "taloudellinen",
            "kalastus",
        ],
    ];
    assert_dictionary_rendering("estonian-finnish-hyphenation", 574, words);
}

#[test]
fn multilingual_hyphenation_renders_danish_and_dutch_with_original_selections() {
    let words: &[&[&str]] = &[
        &[
            "læseoplevelse",
            "bibliotekernes",
            "samarbejdsmuligheder",
            "påmindelser",
            "færgeforbindelse",
        ],
        &[
            "coördinatie",
            "bibliotheek",
            "schilderstalent",
            "waterstaatsingenieur",
            "maatschappij",
        ],
    ];
    assert_dictionary_rendering("danish-dutch-hyphenation", 578, words);
}

fn assert_dictionary_rendering(specimen: &str, width: u32, words: &[&[&str]]) {
    let path = crate::test_fixture(&format!("{specimen}.xhtml"));
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let settings = Settings {
        width,
        height: 864,
        margin: 16,
        font_size: 24.0,
        ..Settings::default()
    };
    let open = |settings| Probe::open(&path, Engine::Parley, &fonts, settings, 0).unwrap();
    let mut dictionary = open(settings);
    let mut limited = open(Settings {
        hyphenation_limits: Some(HyphenationLimits { left: 4, right: 4 }),
        ..settings
    });
    let mut off = open(Settings {
        disable_soft_hyphens: true,
        automatic_hyphenation: false,
        ..settings
    });
    let Renderer::Parley(renderer) = &dictionary.renderer else {
        unreachable!()
    };
    let mut split_words = Vec::new();
    for word in words.iter().flat_map(|language| language.iter().copied()) {
        let start = dictionary.matches(word).remove(0);
        let end = crate::Anchor {
            offset: start.offset + word.len(),
            ..start.clone()
        };
        let paragraph = renderer
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.key == start.block)
            .unwrap();
        let split = paragraph.layout.lines().any(|line| {
            let at = line.text_range().end;
            start.offset < at && at < end.offset
        });
        if split {
            split_words.push(word);
        }
        for (probe, hyphenation_off) in [(&dictionary, false), (&limited, false), (&off, true)] {
            assert_eq!(probe.coverage(), dictionary.coverage());
            assert_eq!(probe.matches(word)[0], start);
            assert_eq!(probe.restore_anchor(&start).unwrap(), start);
            let selection = probe.select_range(&start, &end).unwrap();
            assert_eq!(selection.text, word);
            let page = probe.page_for(&start).unwrap();
            let parts = probe.selection_rects(page, &selection);
            assert!(!parts.is_empty());
            if hyphenation_off {
                assert!(parts.iter().all(|rect| rect.y == parts[0].y));
            }
            for rect in parts {
                let hit = probe
                    .word_at(
                        page,
                        (rect.x + rect.width / 2) as f32,
                        (rect.y + rect.height / 2) as f32,
                    )
                    .unwrap();
                assert_eq!(hit, selection);
            }
        }
    }
    assert!(
        words
            .iter()
            .all(|language| { language.iter().any(|word| split_words.contains(word)) }),
        "{split_words:?}"
    );
    assert_eq!(dictionary.coverage().0, dictionary.coverage().1);
    let automatic_frame = dictionary.capture(0).unwrap();
    let limited_frame = limited.capture(0).unwrap();
    let off_frame = off.capture(0).unwrap();
    assert!(automatic_frame != off_frame);
    assert!(automatic_frame != limited_frame);
    #[cfg(feature = "kindle")]
    if let Some(folder) = std::env::var_os("READER_MULTILINGUAL_HYPHENATION_CAPTURE") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (suffix, pixels) in [
            ("", automatic_frame),
            ("-limits", limited_frame),
            ("-off", off_frame),
        ] {
            primitive_app::write_png(
                &folder.join(format!("reader-{specimen}{suffix}.png")),
                settings.width,
                settings.height,
                &pixels,
            )
            .unwrap();
        }
    }
}
