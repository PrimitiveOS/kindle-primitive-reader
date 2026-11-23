use super::*;
use std::path::Path;

#[test]
#[ignore = "requires a directory of numbered original reflow books"]
fn inventory_cjk_source_characters() {
    let dir = std::env::var_os("READER_CJK_CORPUS").unwrap();
    let mut results = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u16>().ok())
        else {
            continue;
        };
        let Ok(mut book) = Book::open(&path) else {
            continue;
        };
        let mut samples = Vec::new();
        let mut total = 0;
        let mut readable = 0;
        let mut characters = 0;
        for index in 0..book.chapters.len() {
            let Ok(xml) = book.chapter(index) else {
                continue;
            };
            let Ok(source) = Source::parse(&xml) else {
                continue;
            };
            readable += 1;
            let mut count = 0;
            let mut largest = 0;
            let mut key = 0;
            for block in &source.blocks {
                characters += block.text.chars().count();
                let eligible = block
                    .text
                    .chars()
                    .filter(|c| cjk_width::eligible(*c))
                    .count();
                count += eligible;
                if eligible > largest {
                    largest = eligible;
                    key = block.key;
                }
            }
            total += count;
            if count > 0 {
                samples.push((index, count, key, largest));
            }
        }
        results.push(serde_json::json!({"id":id,"count":total,"parsed":readable,"characters":characters,"chapters":samples}));
    }
    let out = std::env::var_os("READER_CJK_REPORT").unwrap();
    std::fs::write(out, serde_json::to_vec(&results).unwrap()).unwrap();
}

fn paragraph<'a>(probe: &'a crate::Probe, key: usize) -> &'a Layout<PaintBrush> {
    let crate::Renderer::Parley(chapter) = &probe.renderer else {
        panic!("Expected horizontal text")
    };
    &chapter
        .paragraphs
        .iter()
        .find(|p| p.key == key)
        .unwrap()
        .layout
}

fn glyph_origins(layout: &Layout<PaintBrush>) -> Vec<(u32, i32)> {
    let mut result = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(run) = item {
                result.extend(
                    run.positioned_glyphs()
                        .map(|g| (g.id, (g.x * 4.).round() as i32)),
                );
            }
        }
    }
    result
}

#[test]
fn cjk_ranges_and_fullwidth_quotes_follow_the_selected_shape() {
    for s in [
        "港",
        "한",
        "ｶ",
        "Ａ",
        "“",
        "‘",
        "𠀀",
        "港\u{e0100}",
        "か\u{3099}",
    ] {
        assert_eq!(cjk_width::extra(s, 42., 42., 150), 21, "{s}");
    }
    for s in [
        "A",
        "0",
        ".",
        "é",
        "\u{1100}",
        "\u{e0100}",
        "\u{fe0f}",
        "\u{e000}",
    ] {
        assert_eq!(cjk_width::extra(s, 42., 42., 150), 0, "{s}");
    }
    assert!(cjk_width::policy("Latin “quote”", 150).is_none());
    assert!(cjk_width::policy("港", 100).is_none());
    assert_eq!(cjk_width::extra("“", 22., 42., 150), 0);
    assert_eq!(cjk_width::extra("ｶ", 21., 42., 125), 5);
    assert_eq!(cjk_width::extra("港", 42., 42., 101), 0);
    assert_eq!(cjk_width::extra("港", 0., 42., 150), 0);
    assert!(
        Settings {
            cjk_width_percent: 151,
            ..Settings::default()
        }
        .validate()
        .is_err()
    );
    let old: crate::library::BookPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.cjk_width_percent, None);
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("cjk_width_percent")
    );
}

#[test]
fn embedded_cjk_width_reflows_repositions_ink_and_preserves_source_ranges() {
    let fixture = crate::test_fixture("harbour-cjk-width.epub");
    let regular = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let settings = Settings {
        width: 610,
        height: 900,
        margin: 30,
        font_size: 42.,
        alignment: crate::TextAlignment::Start,
        ..Settings::default()
    };
    let open = |percent| {
        crate::Probe::open(
            &fixture,
            crate::Engine::Parley,
            std::slice::from_ref(&regular),
            Settings {
                cjk_width_percent: percent,
                ..settings
            },
            0,
        )
        .unwrap()
    };
    let mut normal = open(100);
    let mut wide = open(150);
    assert_eq!(normal.missing_glyphs(), 0);
    assert_eq!(wide.missing_glyphs(), 0);
    let sample = normal.fragment("sample").unwrap();
    let latin = normal.fragment("latin").unwrap();
    let sample_text = "港波朝空海川山光";
    let mut measured = Vec::new();
    for (probe, expected) in [(&normal, 42.), (&wide, 63.)] {
        let layout = paragraph(probe, sample.block);
        assert_eq!(layout.lines().count(), 1);
        let positions = glyph_origins(layout);
        assert_eq!(positions.len(), 8);
        for offset in sample_text.char_indices().map(|(at, _)| at) {
            let cluster = parley::Cluster::from_byte_index(layout, offset).unwrap();
            assert!((cluster.advance() - expected).abs() < 0.1);
        }
        assert!((layout.calculate_content_widths().max - 8. * expected).abs() < 1.);
        measured.push(positions);
    }
    for (i, ((a, x), (b, y))) in measured[0].iter().zip(&measured[1]).enumerate() {
        assert_eq!(a, b);
        assert_eq!(
            y - x,
            4 * (10 + 21 * i as i32),
            "shape centered in measured advance"
        );
    }
    assert_eq!(
        glyph_origins(paragraph(&normal, latin.block)),
        glyph_origins(paragraph(&wide, latin.block))
    );
    let text = normal.matches("小さな船");
    assert_eq!(text, wide.matches("小さな船"));
    assert!(wide.page_count() > normal.page_count());
    let target = text.last().unwrap();
    let p0 = normal.page_for(target).unwrap();
    let p1 = wide.page_for(target).unwrap();
    assert!(p1 > p0);
    let mut end = sample.clone();
    end.offset = sample.offset + "港".len();
    let selected = normal.select_range(&sample, &end).unwrap();
    let a = normal.selection_rects(0, &selected);
    let b = wide.selection_rects(0, &selected);
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].x, b[0].x);
    assert_eq!(b[0].width - a[0].width, 21);
    let light = normal.capture(0).unwrap();
    let spread = wide.capture(0).unwrap();
    let mut ink = 0;
    for y in a[0].y..a[0].y + a[0].height {
        let s = y as usize * settings.width as usize + a[0].x as usize;
        let w = a[0].width as usize;
        assert_eq!(light[s..s + w], spread[s + 10..s + 10 + w]);
        ink += light[s..s + w].iter().filter(|p| **p < 128).count();
    }
    assert!(ink > 50);
    if let Some(dir) = std::env::var_os("READER_CJK_CAPTURES") {
        #[cfg(feature = "kindle")]
        for (name, pixels) in [("normal", light), ("wide", spread)] {
            primitive_app::write_png(
                &Path::new(&dir).join(format!("cjk-{name}.png")),
                settings.width,
                settings.height,
                &pixels,
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "requires the original public or private publication"]
fn original_publication_width_and_source_positions() {
    let path = std::env::var_os("READER_CJK_BOOK").unwrap();
    let chapter = std::env::var("READER_CJK_CHAPTER")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    if let Some(path) = std::env::var_os("READER_CJK_FONT") {
        fonts.push(path.into());
    }
    let target = std::env::var("READER_CJK_QUERY").unwrap();
    let positive = std::env::var("READER_CJK_EXPECT").unwrap() == "wider";
    let open = |percent| {
        crate::Probe::open(
            Path::new(&path),
            crate::Engine::Parley,
            &fonts,
            Settings {
                width: 610,
                height: 900,
                font_size: 42.,
                margin: 30,
                cjk_width_percent: percent,
                ..Default::default()
            },
            chapter,
        )
        .unwrap()
    };
    let mut normal = open(100);
    let mut wide = open(150);
    let hits = normal.matches(&target);
    assert!(!hits.is_empty());
    assert_eq!(hits, wide.matches(&target));
    let mut pairs = Vec::new();
    for h in hits.iter().take(40) {
        let mut end = h.clone();
        end.offset += target.len();
        let selected = normal.select_range(h, &end).unwrap();
        assert_eq!(selected.text, target);
        let (p0, p1) = (normal.page_for(h).unwrap(), wide.page_for(h).unwrap());
        let measure = |p: &crate::Probe, page: usize| {
            p.selection_rects(page, &selected)
                .iter()
                .map(|r| r.width)
                .sum::<i32>()
        };
        let a = measure(&normal, p0);
        let b = measure(&wide, p1);
        if a > 0 && b > 0 {
            pairs.push((a, b, p0, p1));
        }
    }
    assert!(!pairs.is_empty());
    let changes = pairs.iter().filter(|v| v.1 > v.0).count();
    if positive {
        assert!(changes > 0);
        assert_ne!(normal.capture(0).unwrap(), wide.capture(0).unwrap());
    } else {
        assert!(pairs.iter().all(|v| v.0 == v.1 && v.2 == v.3));
        assert_eq!(normal.page_count(), wide.page_count());
        assert_eq!(normal.capture(0).unwrap(), wide.capture(0).unwrap());
    }
    let data = serde_json::json!({"source_hits":hits.len(),"span_pairs":pairs,"wider_pairs":changes,"normal_pages":normal.page_count(),"wide_pages":wide.page_count(),"source_anchor_preserved":true});
    std::fs::write(
        std::env::var_os("READER_CJK_REPORT").unwrap(),
        serde_json::to_vec(&data).unwrap(),
    )
    .unwrap();
}
