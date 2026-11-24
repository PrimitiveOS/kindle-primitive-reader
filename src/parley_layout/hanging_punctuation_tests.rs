use super::*;
use crate::{Anchor, Engine, Probe, Renderer};

fn open(settings: Settings) -> Probe {
    Probe::open(
        &crate::test_fixture("hanging-punctuation.epub"),
        Engine::Parley,
        &[
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"),
        ],
        settings,
        0,
    )
    .unwrap()
}

fn paragraph<'a>(probe: &'a Probe, text: &str) -> &'a Paragraph {
    let Renderer::Parley(renderer) = &probe.renderer else {
        unreachable!()
    };
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.text.contains(text))
        .unwrap();
    renderer
        .paragraphs
        .iter()
        .find(|p| p.key == block.key)
        .unwrap()
}

#[test]
fn hanging_punctuation_uses_physical_edges_without_moving_source_boundaries() {
    let settings = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.0,
        ..Settings::default()
    };
    let mut off = open(settings);
    let mut on = open(Settings {
        hanging_punctuation: true,
        ..settings
    });
    let offsets = |probe: &Probe| {
        probe
            .lines
            .iter()
            .map(|line| (line.block, line.bytes.clone(), line.top))
            .collect::<Vec<_>>()
    };
    assert_eq!(offsets(&off), offsets(&on));
    assert_eq!(off.coverage(), on.coverage());
    assert_eq!(off.page_count(), on.page_count());
    let position = |probe: &Probe, word| {
        let paragraph = paragraph(probe, word);
        let line = paragraph.layout.get(0).unwrap();
        paragraph.x + line.metrics().inline_min_coord + line.metrics().offset
    };
    for (word, reference_shift) in [
        ("Harbor opening marker", -7.0),
        ("Onshore closing marker", 7.0),
        ("Centered marker", 0.0),
        ("Fixed marker", 0.0),
        ("Justified marker", -7.0),
        ("بالعالم", 3.0),
    ] {
        let shift = position(&on, word) - position(&off, word);
        if reference_shift == 0.0 {
            assert_eq!(shift, 0.0, "{word}");
        } else {
            assert!((shift - reference_shift).abs() <= 2.0, "{word}: {shift}");
        }
    }

    let p = paragraph(&on, "Harbor opening marker");
    let start = on.anchor(p.key, 0);
    let end = Anchor {
        offset: '“'.len_utf8(),
        ..start.clone()
    };
    let quote = on.select_range(&start, &end).unwrap();
    assert_eq!(quote.text, "“");
    let page = on.page_for(&start).unwrap();
    let rect = on.selection_rects(page, &quote)[0];
    let x = rect.x as f32 + 1.0;
    let y = rect.y as f32 + rect.height as f32 / 2.0;
    assert!(x < settings.left_margin() as f32, "{rect:?}");
    assert_eq!(
        on.link_at(page, x, y).unwrap().fragment.as_deref(),
        Some("target")
    );
    assert_eq!(on.text_position_at(page, x, y).unwrap().block, start.block);
    assert!(off.link_at(page, x, y).is_none());

    let baseline = off.capture(page).unwrap();
    let hanging = on.capture(page).unwrap();
    let stride = settings.width as usize;
    let has_margin_ink = |frame: &[u8], threshold: usize| {
        frame
            .chunks_exact(stride)
            .any(|row| row[..threshold].iter().any(|&gray| gray < 192))
    };
    assert!(!has_margin_ink(&baseline, settings.left_margin() as usize));
    assert!(has_margin_ink(&hanging, settings.left_margin() as usize));
    let has_right_ink = |frame: &[u8]| {
        frame.chunks_exact(stride).any(|row| {
            row[stride - settings.right_margin() as usize..]
                .iter()
                .any(|&gray| gray < 192)
        })
    };
    assert!(!has_right_ink(&baseline));
    assert!(has_right_ink(&hanging));
    assert_eq!(
        on.restore_anchor(&off.page_anchor(page).unwrap()),
        off.page_anchor(page)
    );
    #[cfg(feature = "kindle")]
    if let Some(directory) = std::env::var_os("READER_HANGING_PUNCTUATION_CAPTURES") {
        primitive_app::write_png(
            &std::path::Path::new(&directory).join("reader-hanging-punctuation.png"),
            settings.width,
            settings.height,
            &hanging,
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires an authorized original EPUB via READER_HANGING_PUNCTUATION_ORIGINAL"]
fn original_hanging_punctuation_keeps_source_bytes_and_page_anchors() {
    let source =
        std::path::PathBuf::from(std::env::var_os("READER_HANGING_PUNCTUATION_ORIGINAL").unwrap());
    let fonts = [
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"),
    ];
    let original = std::fs::read(&source).unwrap();
    let settings = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.0,
        ..Settings::default()
    };
    let mut shifts = 0;
    let mut count = 4;
    let mut chapter = 0;
    while chapter < count {
        let before = Probe::open(&source, Engine::Parley, &fonts, settings, chapter).unwrap();
        count = count.min(before.chapter_count());
        let after = Probe::open(
            &source,
            Engine::Parley,
            &fonts,
            Settings {
                hanging_punctuation: true,
                ..settings
            },
            chapter,
        )
        .unwrap();
        assert_eq!(before.page_count(), after.page_count());
        assert_eq!(before.coverage(), after.coverage());
        for page in 0..before.page_count() {
            assert_eq!(before.page_anchor(page), after.page_anchor(page));
        }
        if let Renderer::Parley(renderer) = after.renderer {
            shifts += renderer
                .paragraphs
                .iter()
                .filter(|p| p.layout.hanging_punctuation_overflow() != [0.0; 2])
                .count();
        }
        chapter += 1;
    }
    assert!(shifts > 0 && chapter > 0);
    assert_eq!(std::fs::read(source).unwrap(), original);
    println!(
        "{chapter} chapters, {shifts} shifted paragraphs, original bytes and page anchors matched"
    );
}
