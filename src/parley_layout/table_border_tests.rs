use super::*;
use crate::{Anchor, Engine, Probe, Renderer};

fn render(html: &str, cfg: Settings) -> Probe {
    Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        cfg,
        0,
        Some(html.into()),
    )
    .unwrap()
}
fn config() -> Settings {
    Settings {
        width: 900,
        height: 1010,
        margin: 35,
        font_size: 34.,
        ..Default::default()
    }
}
fn placed(p: &Probe, id: &str, word: &str) -> (Anchor, crate::selection::HighlightRect) {
    let start = p.fragment(id).unwrap();
    let e = Anchor {
        offset: start.offset + word.len(),
        ..start.clone()
    };
    let selection = p.select_range(&start, &e).unwrap();
    assert_eq!(selection.text, word);
    let page = p.page_for(&start).unwrap();
    let bounds = p.selection_rects(page, &selection)[0];
    let near = p
        .word_at(
            page,
            (bounds.x + bounds.width / 2) as f32,
            (bounds.y + bounds.height / 2) as f32,
        )
        .unwrap();
    assert_eq!(near, selection);
    (start, bounds)
}
fn sample(frame: &[u8], cfg: Settings, x: i32, y: i32) -> u8 {
    frame[y as usize * cfg.width as usize + x as usize]
}

#[test]
fn transparent_table_rules_keep_source_geometry_and_remove_only_the_strokes() {
    let source = include_str!("../../tests/fixtures/harbor-survey-table.xhtml");
    let cfg = config();
    let mut published = render(source, cfg);
    let mut clear = render(&source.replace("#404040", "transparent"), cfg);
    let mut separate = render(
        &source.replace(
            "border-collapse:collapse",
            "border-collapse:separate; border-spacing:8px 6px",
        ),
        cfg,
    );
    let mut separate_clear = render(
        &source.replace("#404040", "transparent").replace(
            "border-collapse:collapse",
            "border-collapse:separate; border-spacing:8px 6px",
        ),
        cfg,
    );
    for (id, word) in [
        ("north", "North"),
        ("whitecaps", "Whitecaps"),
        ("low", "Low"),
        ("west", "West"),
    ] {
        let (_, base) = placed(&published, id, word);
        assert_eq!(base, placed(&clear, id, word).1);
        assert_eq!(
            placed(&separate, id, word).1,
            placed(&separate_clear, id, word).1
        );
    }
    let ink = published.capture(0).unwrap();
    let no_ink = clear.capture(0).unwrap();
    let apart = separate.capture(0).unwrap();
    for (paint, plain) in [
        (&ink, &no_ink),
        (&apart, &separate_clear.capture(0).unwrap()),
    ] {
        let mut changed = 0;
        for (a, b) in paint.iter().zip(plain) {
            if a != b {
                assert_eq!(*a, 64);
                changed += 1;
            }
        }
        assert!(changed > 6000);
    }
    let (_, sample_cell) = placed(&published, "north", "North");
    let y = sample_cell.y + sample_cell.height / 2;
    for x in cfg.margin..cfg.margin + 4 {
        assert_eq!(sample(&ink, cfg, x as i32, y), 64);
    }
    if let Some(dir) = std::env::var_os("READER_BORDER_SCREENS") {
        for (name, frame) in [
            ("collapsed", ink),
            ("separate", apart),
            ("transparent", no_ink),
        ] {
            image::save_buffer(
                std::path::PathBuf::from(&dir).join(format!("table-border-{name}.png")),
                &frame,
                cfg.width,
                cfg.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn collapsed_boundary_uses_the_owning_cell_and_wider_outer_table_rule() {
    let cfg = config();
    let h = r#"<html><body><table style="border:4px solid #333;border-collapse:collapse;width:100%"><tr><td id="a" style="width:50%;border-right:16px solid black">East</td><td id="b" style="width:50%;border-left:3px solid #888">West</td></tr><tr><td id="c">North</td><td>South</td></tr></table></body></html>"#;
    let mut p = render(h, cfg);
    let (_, b) = placed(&p, "b", "West");
    let frame = p.capture(0).unwrap();
    let y = b.y + b.height / 2;
    assert_eq!(sample(&frame, cfg, b.x - 1, y), 136);
    assert_eq!(sample(&frame, cfg, b.x - 3, y), 136);
    assert_eq!(sample(&frame, cfg, b.x - 4, y), 255);
    for x in 0..4 {
        assert_eq!(sample(&frame, cfg, cfg.margin as i32 + x, y), 51);
    }
    let hidden = render(&h.replace("3px solid #888", "3px hidden #888"), cfg);
    let clear = render(&h.replace("3px solid #888", "3px solid transparent"), cfg);
    assert_eq!(placed(&clear, "b", "West").1.x, b.x);
    assert!(placed(&hidden, "b", "West").1.x < b.x);
}

#[test]
fn separate_double_and_dashed_rules_remain_distinct_and_visible() {
    let cfg = config();
    let h = r#"<html><body><table style="border-collapse:separate;border-spacing:0"><tr><td id="a" style="width:50%;border-right:6px double #777">East</td><td id="b" style="width:50%;border-left:4px dashed #333">West</td></tr></table></body></html>"#;
    let mut p = render(h, cfg);
    let (_, b) = placed(&p, "b", "West");
    let f = p.capture(0).unwrap();
    let base = b.x - 4;
    let y = b.y + b.height / 2;
    assert_eq!(sample(&f, cfg, base - 1, y), 119);
    assert_eq!(sample(&f, cfg, base - 3, y), 255);
    assert_eq!(sample(&f, cfg, base - 5, y), 119);
    assert!((b.y..b.y + b.height).any(|yy| sample(&f, cfg, base, yy) == 51));
    assert!((b.y..b.y + b.height).any(|yy| sample(&f, cfg, base, yy) == 255));
}

fn compare_page_masks(
    a: &std::collections::BTreeMap<usize, Vec<u8>>,
    b: &std::collections::BTreeMap<usize, Vec<u8>>,
    width: usize,
) -> [usize; 5] {
    let mut values = [0; 5];
    for (page, pixels) in a {
        let other = &b[page];
        let mut runs = vec![0; width];
        let mut h = 0;
        for (k, (first, second)) in pixels.iter().zip(other).enumerate() {
            let x = k % width;
            if x == 0 {
                h = 0;
            }
            if first != second {
                values[0] += 1;
                h += 1;
                runs[x] += 1;
                values[1] += usize::from(h == 64);
                values[2] += usize::from(runs[x] == 64);
                values[3] = values[3].max(h);
                values[4] = values[4].max(runs[x]);
            } else {
                h = 0;
                runs[x] = 0;
            }
        }
    }
    values
}

#[test]
#[ignore = "An original EPUB and private passage list are supplied by the document owner"]
fn compares_original_table_borders_and_source_geometry() {
    use crate::{
        Pagination,
        book_style::{BookStyle, borders::Pattern},
    };
    use std::{collections::BTreeMap, path::PathBuf};
    let path = PathBuf::from(std::env::var_os("READER_TABLE_BOOK").unwrap());
    let inputs =
        std::fs::read_to_string(std::env::var_os("READER_TABLE_PASSAGES").unwrap()).unwrap();
    let chapter = inputs
        .lines()
        .next()
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap()
        - 1;
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let cfg = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.,
        ..Default::default()
    };
    let mut baseline = Vec::new();
    let mut geometry = BTreeMap::new();
    let mut images = BTreeMap::new();
    for mode in [
        "published",
        "transparent",
        "hidden",
        "separate",
        "separate_clear",
        "reset",
    ] {
        let mut book = Book::open(&path).unwrap();
        let mut p = Probe::open(&path, Engine::Parley, &fonts, cfg, chapter).unwrap();
        let mut fc = crate::font_context(&fonts).unwrap();
        let mut styles =
            BookStyle::load(&mut book, &p.chapter, &p.source, &mut fc).compute(&p.source.elements);
        for (elem, style) in p.source.elements.iter().zip(&mut styles) {
            if mode.starts_with("separate") && elem.tag == "table" {
                style.border_collapse = false;
                style.border_spacing = Default::default();
            }
            if let Some(b) = &mut style.borders {
                for e in std::sync::Arc::make_mut(b) {
                    if mode == "hidden" {
                        e.pattern = Pattern::Hidden;
                    }
                    if mode == "transparent" || mode == "separate_clear" {
                        e.gray = None;
                    }
                }
            }
        }
        let (layout, mut lines, height) = ParleyChapter::new(
            &mut book,
            &p.chapter,
            &p.source,
            fc,
            cfg,
            &styles,
            &Default::default(),
        )
        .unwrap();
        let edges = crate::publisher_breaks::geometry(&p.source, &styles, &lines);
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        p.pages =
            Pagination::with_breaks(&lines, cfg.text_height() as f32, height, &edges).unwrap();
        p.pages
            .trim_unpainted_tail(layout.last_paint_bottom(&p.source));
        p.lines = lines;
        p.renderer = Renderer::Parley(layout);
        let mut rects = Vec::new();
        let mut frames = BTreeMap::new();
        for (i, line) in inputs.lines().enumerate() {
            let phrase = line.splitn(3, '\t').nth(2).unwrap();
            let found = p.matches(phrase);
            assert_eq!(found.len(), 1);
            let start = found[0].clone();
            let end = Anchor {
                offset: start.offset + phrase.split_whitespace().next().unwrap().len(),
                ..start.clone()
            };
            let word = p.select_range(&start, &end).unwrap();
            let page = p.page_for(&start).unwrap();
            let rect = p.selection_rects(page, &word)[0];
            let hit = p
                .word_at(
                    page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            assert_eq!(hit.start.block, start.block);
            assert!(hit.start.offset < end.offset && hit.end.offset > start.offset);
            if mode == "published" {
                baseline.push(word.clone());
            } else {
                assert_eq!(baseline[i], word);
            }
            rects.push((page, rect.x, rect.y));
            frames
                .entry(page)
                .or_insert_with(|| p.capture(page).unwrap());
        }
        geometry.insert(mode, rects);
        images.insert(mode, frames);
    }
    assert_eq!(geometry["published"], geometry["transparent"]);
    assert_eq!(geometry["separate"], geometry["separate_clear"]);
    assert_eq!(geometry["published"], geometry["reset"]);
    assert_eq!(images["published"], images["reset"]);
    let collapsed = compare_page_masks(
        &images["published"],
        &images["transparent"],
        cfg.width as usize,
    );
    let separate = compare_page_masks(
        &images["separate"],
        &images["separate_clear"],
        cfg.width as usize,
    );
    for m in [collapsed, separate] {
        assert!(m[0] > 1000 && m[1] > 0 && m[2] > 0 && m[3] > 150 && m[4] > 80);
    }
    assert!(separate[0] > collapsed[0]);
    std::fs::write(std::env::var_os("READER_BORDER_REPORT").unwrap(),serde_json::to_vec(&serde_json::json!({
        "cases":geometry,"collapsed":collapsed,"separate":separate,"pages":images["published"].len(),"exact_reset":true
    })).unwrap()).unwrap();
}

#[test]
fn spanning_edge_survives_page_turns_and_does_not_draw_through_merged_cells() {
    let cfg = Settings {
        width: 520,
        height: 420,
        margin: 25,
        font_size: 27.,
        ..Default::default()
    };
    let words = "The local tide carries fresh water toward the shore. ".repeat(12);
    let h = format!(
        r#"<html><body><table style="border-collapse:collapse;border:3px solid #555"><tr>
    <td rowspan="2" id="label" style="width:30%;border:3px solid #555">Harbor</td><td id="reading" style="width:70%;border:3px solid #555">Morning {words}</td></tr>
    <tr><td id="afternoon" style="border:3px solid #555">Afternoon {words}</td></tr><tr><td colspan="2" style="border:3px solid #555" id="end">End of the survey</td></tr></table></body></html>"#
    );
    let mut painted = render(&h, cfg);
    let mut clear = render(&h.replace("#555", "transparent"), cfg);
    assert!(painted.page_count() > 2);
    assert_eq!(painted.page_count(), clear.page_count());
    for (id, w) in [
        ("label", "Harbor"),
        ("reading", "Morning"),
        ("afternoon", "Afternoon"),
        ("end", "End"),
    ] {
        let (a, r) = placed(&painted, id, w);
        let (other, plain) = placed(&clear, id, w);
        assert_eq!(a.offset, other.offset);
        assert_eq!(r, plain);
    }
    let a = painted.fragment("afternoon").unwrap();
    let e = painted.fragment("end").unwrap();
    let before = painted.page_for(&a).unwrap();
    let end = painted.page_for(&e).unwrap();
    assert!(end > before);
    let frame = painted.capture(before).unwrap();
    assert_ne!(frame, clear.capture(before).unwrap());
    let other = painted.capture(end).unwrap();
    assert_ne!(other, frame);
    assert_eq!(frame, painted.capture(before).unwrap());
    // The horizontal divider of the second row ends at the side of the spanning first cell.
    let loc = placed(&painted, "afternoon", "Afternoon").1;
    let mut found = false;
    for page in 0..painted.page_count() {
        let frame = painted.capture(page).unwrap();
        for y in cfg.top_margin() as i32..(cfg.height - cfg.bottom_margin()) as i32 {
            if sample(&frame, cfg, loc.x + 15, y) == 85
                && sample(&frame, cfg, loc.x + 60, y) == 85
                && sample(&frame, cfg, cfg.margin as i32 + 24, y) == 255
            {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "The page sequence contains a divider limited to the second column"
    );
}

#[test]
fn separate_side_widths_colors_and_hidden_resets_obey_the_publisher_cascade() {
    let source=Source::parse(r#"<html><head><style>td {border:5px dotted red !important; border-right-style:solid !important; border-color:rgb(0 0 0 / 50%) !important}
    #chosen {border-left:0 none;color:black} td {border-bottom-color:transparent !important}</style></head>
    <body><table><tr><td id="chosen" style="border-top:1px hidden #123 !important; border-right-width:7px !important">East</td><td>West</td></tr></table></body></html>"#).unwrap();
    let mut font =
        crate::font_context(&[crate::test_fixture("fonts/NotoSans-Regular.ttf")]).unwrap();
    let mut book = Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap();
    let id = source
        .elements
        .iter()
        .position(|e| e.id.as_deref() == Some("chosen"))
        .unwrap();
    let chapter = book.chapters[0].href.clone();
    let styles = crate::book_style::BookStyle::load(&mut book, &chapter, &source, &mut font)
        .compute(&source.elements);
    let s = styles[id].borders.as_ref().unwrap();
    use crate::book_style::borders::Pattern;
    assert_eq!(s[0].pattern, Pattern::Hidden);
    assert_eq!(s[1].pattern, Pattern::Solid);
    assert_eq!(s[3].pattern, Pattern::Dotted);
    assert_eq!(s[1].width.box_pixels(0, 42.), 7.);
    assert_eq!(s[3].width.box_pixels(0, 42.), 5.);
    assert_eq!(s[1].gray, Some((0, 128)));
    assert_eq!(s[2].gray, None);
}
