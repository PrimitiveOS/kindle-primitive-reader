use super::*;
use crate::{Book, Engine, Probe};

#[test]
fn publisher_box_padding_shapes_wrapped_text_and_preserves_source_positions() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let cfg = Settings {
        width: 740,
        height: 1240,
        margin: 40,
        font_size: 30.,
        ..Default::default()
    };
    let open = |enabled| {
        Probe::from_book_content(
            Book::open(&path).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                publisher_styles: enabled,
                ..cfg
            },
            0,
            Some(include_str!("../../tests/fixtures/publisher-box-padding.xhtml").into()),
        )
        .unwrap()
    };
    let mut styled = open(true);
    let mut plain = open(false);
    assert_eq!(plain.coverage(), styled.coverage());
    assert_ne!(plain.capture(0).unwrap(), styled.capture(0).unwrap());
    for (id, phrase, displacement) in [
        ("ordinary", "Harbor lights", 0),
        ("sheltered", "Inside the sheltered", 52),
        ("nested", "Nested inside", 88),
        ("asymmetric", "Beyond the eastern", 12),
        ("plain", "Back in the open", 0),
    ] {
        let id_anchor = plain.fragment(id).unwrap();
        assert_eq!(styled.fragment(id).unwrap(), id_anchor);
        let a = plain.matches(phrase).remove(0);
        assert_eq!(styled.matches(phrase), [a.clone()]);
        let b = crate::Anchor {
            offset: a.offset + phrase.len(),
            ..a.clone()
        };
        let old = plain.select_range(&a, &b).unwrap();
        let new = styled.select_range(&a, &b).unwrap();
        assert_eq!(old, new);
        let page = styled.page_for(&a).unwrap();
        let rect = styled.selection_rects(page, &new)[0];
        let before = plain.selection_rects(plain.page_for(&a).unwrap(), &old)[0];
        assert_eq!(rect.x as i32 - before.x as i32, displacement, "{id}");
        let word = styled
            .word_at(
                page,
                rect.x as f32 + 3.,
                rect.y as f32 + rect.height as f32 / 2.,
            )
            .unwrap();
        assert_eq!(word.text, phrase.split_whitespace().next().unwrap());
    }
    if let Some(dir) = std::env::var_os("READER_PADDING_SCREENS") {
        for (name, probe) in [("plain", &mut plain), ("padded", &mut styled)] {
            image::save_buffer(
                std::path::PathBuf::from(&dir).join(format!("publisher-box-{name}.png")),
                &probe.capture(0).unwrap(),
                cfg.width,
                cfg.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "Supply a private original EPUB and one-based chapters to compare"]
fn compares_publisher_padding_in_an_original_document() {
    use crate::{Pagination, Renderer, book_style::BookStyle};
    use std::{collections::BTreeMap, path::PathBuf};
    let path = PathBuf::from(std::env::var_os("READER_PADDING_BOOK").unwrap());
    let chapters = std::env::var("READER_PADDING_CHAPTERS").unwrap();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let cfg = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.,
        ..Default::default()
    };
    let mut rows = Vec::new();
    let mut candidates = Vec::new();
    for item in chapters.split(',') {
        let n: usize = item.parse().unwrap();
        let mut styled = Probe::open(&path, Engine::Parley, &fonts, cfg, n - 1).unwrap();
        let mut plain = Probe::open(&path, Engine::Parley, &fonts, cfg, n - 1).unwrap();
        let mut book = Book::open(&path).unwrap();
        let context = crate::font_context(&fonts).unwrap();
        let mut fc = context.clone();
        let mut styles = BookStyle::load(&mut book, &plain.chapter, &plain.source, &mut fc)
            .compute(&plain.source.elements);
        for style in &mut styles {
            style.padding = Default::default();
        }
        let (layout, mut lines, height) = ParleyChapter::new(
            &mut book,
            &plain.chapter,
            &plain.source,
            fc,
            cfg,
            &styles,
            &Default::default(),
        )
        .unwrap();
        let edges = crate::publisher_breaks::geometry(&plain.source, &styles, &lines);
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        plain.pages =
            Pagination::with_breaks(&lines, cfg.text_height() as f32, height, &edges).unwrap();
        plain
            .pages
            .trim_unpainted_tail(layout.last_paint_bottom(&plain.source));
        plain.lines = lines;
        plain.renderer = Renderer::Parley(layout);
        assert_eq!(plain.coverage(), styled.coverage());
        let (Renderer::Parley(normal), Renderer::Parley(control)) =
            (&styled.renderer, &plain.renderer)
        else {
            panic!("Expected horizontal reflow")
        };
        assert_eq!(normal.paragraphs.len(), control.paragraphs.len());
        let mut located = 0;
        let mut narrower = 0;
        let mut wraps = 0;
        let mut shifts = BTreeMap::new();
        let mut pages = Vec::new();
        for (p, old) in normal.paragraphs.iter().zip(&control.paragraphs) {
            assert_eq!(p.key, old.key);
            let shift = (p.x - old.x).round() as i32;
            let changed = p.layout.len() != old.layout.len();
            let measure = p.layout.width() - old.layout.width();
            let changed_lines = p.layout.lines().zip(old.layout.lines()).any(|(new, old)| {
                new.text_range() != old.text_range()
                    || (new.metrics().offset - old.metrics().offset).abs() > 0.25
            });
            if shift == 0 && !changed && !changed_lines && measure.abs() < 0.5 {
                continue;
            }
            narrower += 1;
            wraps += usize::from(changed);
            if shift != 0 {
                *shifts.entry(shift).or_insert(0usize) += 1;
            }
            let block = &styled.source.blocks[styled
                .source
                .blocks
                .binary_search_by_key(&p.key, |b| b.key)
                .unwrap()];
            let mut words = block.text.split_whitespace();
            let Some(first) = words.next() else { continue };
            let start = styled.anchor(block.key, block.text.find(first).unwrap());
            let end = crate::Anchor {
                offset: start.offset + first.len(),
                ..start.clone()
            };
            let sel = styled.select_range(&start, &end).unwrap();
            assert_eq!(plain.select_range(&start, &end).unwrap(), sel);
            let old_page = plain.page_for(&start).unwrap();
            let page = styled.page_for(&start).unwrap();
            let a = plain.selection_rects(old_page, &sel);
            let b = styled.selection_rects(page, &sel);
            if a.is_empty() || b.is_empty() {
                continue;
            }
            located += 1;
            pages.push(page);
            let phrase = block
                .text
                .split_whitespace()
                .take(6)
                .collect::<Vec<_>>()
                .join(" ");
            // The stock check resolves these privately to the same source phrases.
            if located <= 18 && shift != 0 && phrase.split_whitespace().count() >= 4 {
                candidates.push(format!("{n}\t{}\t{phrase}", b[0].x as i32 - a[0].x as i32));
            }
        }
        pages.sort();
        pages.dedup();
        for &page in pages.iter().take(5) {
            assert!(
                styled
                    .capture(page)
                    .unwrap()
                    .iter()
                    .any(|&sample| sample < 210)
            );
            assert_eq!(
                styled.page_for(&styled.page_anchor(page).unwrap()),
                Some(page)
            );
        }
        rows.push(serde_json::json!({"chapter":n,"paragraphs_with_changed_geometry":narrower,"paragraphs_with_changed_line_count":wraps,
            "verified_source_selections":located,"left_shifts":shifts,"visible_pages_checked":pages.len().min(5),"control_pages":plain.page_count(),"published_pages":styled.page_count()}));
    }
    std::fs::write(
        std::env::var_os("READER_PADDING_REPORT").unwrap(),
        serde_json::to_vec(&rows).unwrap(),
    )
    .unwrap();
    if let Some(path) = std::env::var_os("READER_PADDING_PASSAGES") {
        std::fs::write(path, candidates.join("\n")).unwrap();
    }
}

#[test]
fn negative_first_lines_spend_available_physical_padding_before_shifting_other_lines() {
    let words = "One boat is tied to the pier and another is beyond the rocks. ".repeat(7);
    let text = format!(
        r#"<html><body><p id="balanced" style="padding-left:70px;text-indent:-70px">{words}</p>
        <p id="partial" style="padding-left:70px;text-indent:-25px">{words}</p>
        <p id="limit" style="padding-left:20px;text-indent:-65px">{words}</p>
        <p id="rtl" dir="auto" style="padding-right:70px;text-indent:-25px">אחד שנים שלושה ארבע חמש שש שבע שמונה תשע עשר</p></body></html>"#
    );
    let path = crate::test_fixture("field-notes-epub3.epub");
    let cfg = Settings {
        width: 520,
        height: 1000,
        margin: 24,
        font_size: 28.,
        ..Default::default()
    };
    let p = Probe::from_book_content(
        Book::open(&path).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        cfg,
        0,
        Some(text),
    )
    .unwrap();
    let crate::Renderer::Parley(chapter) = &p.renderer else {
        panic!()
    };
    for (id, first, following) in [
        ("balanced", 0., 70.),
        ("partial", 45., 70.),
        ("limit", 0., 65.),
    ] {
        let key = p.fragment(id).unwrap().block;
        let q = chapter.paragraphs.iter().find(|q| q.key == key).unwrap();
        assert!(
            (q.x + q.layout.get(0).unwrap().metrics().offset - first).abs() < 0.7,
            "{id}"
        );
        assert!(
            (q.x + q.layout.get(1).unwrap().metrics().offset - following).abs() < 0.7,
            "{id}"
        );
    }
    let key = p.fragment("rtl").unwrap().block;
    let rtl = chapter.paragraphs.iter().find(|q| q.key == key).unwrap();
    assert!(rtl.layout.is_rtl());
    let line = rtl.layout.get(0).unwrap();
    let edge = line.metrics().offset + line.metrics().advance;
    assert!((edge - (cfg.text_width() as f32 - 45.)).abs() < 1.);
}

#[test]
fn an_inset_paragraph_resolves_its_visible_link_at_the_new_word_position() {
    let html = r##"<html><body><p style="padding-left:120px"><a href="#destination">Harbor charts</a> and notes.</p><p id="destination">The next landing.</p></body></html>"##;
    let p = Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 640,
            height: 800,
            margin: 30,
            font_size: 26.,
            ..Default::default()
        },
        0,
        Some(html.into()),
    )
    .unwrap();
    let start = p.matches("Harbor charts").remove(0);
    let end = crate::Anchor {
        offset: start.offset + "Harbor".len(),
        ..start.clone()
    };
    let selection = p.select_range(&start, &end).unwrap();
    let page = p.page_for(&start).unwrap();
    let bounds = p.selection_rects(page, &selection)[0];
    let hit = p
        .link_at(
            page,
            (bounds.x + bounds.width / 2) as f32,
            (bounds.y + bounds.height / 2) as f32,
        )
        .unwrap();
    assert_eq!(hit.fragment.as_deref(), Some("destination"));
    assert_eq!(p.page_for(&p.fragment("destination").unwrap()), Some(page));
}

#[test]
fn fractional_em_padding_matches_signed_fixed_point_lengths_at_both_font_sizes() {
    let words = "All along the dunes the tide has left small white shells and pebbles. ".repeat(7);
    let html = format!(
        r#"<html><body><p id="large" style="padding-left:2.35em;text-indent:-0.65em">{words}</p>
        <div style="font-size:85%"><p id="small" style="padding-left:2.35em;text-indent:-0.65em">{words}</p></div>
        </body></html>"#
    );
    let p = Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: 760,
            height: 1680,
            margin: 30,
            font_size: 42.,
            ..Default::default()
        },
        0,
        Some(html),
    )
    .unwrap();
    let crate::Renderer::Parley(engine) = &p.renderer else {
        panic!()
    };
    for (id, first, rest) in [("large", 70., 98.), ("small", 59., 82.)] {
        let key = p.fragment(id).unwrap().block;
        let q = engine.paragraphs.iter().find(|q| q.key == key).unwrap();
        assert_eq!(q.x + q.layout.get(0).unwrap().metrics().offset, first);
        assert_eq!(q.x + q.layout.get(1).unwrap().metrics().offset, rest);
    }
}

#[test]
#[ignore = "Supply an original EPUB and private source passage list"]
fn compares_original_table_coordinates_and_source_hits() {
    use crate::{Pagination, Renderer, book_style::BookStyle};
    use std::path::PathBuf;
    let path = PathBuf::from(std::env::var_os("READER_TABLE_BOOK").unwrap());
    let entries =
        std::fs::read_to_string(std::env::var_os("READER_TABLE_PASSAGES").unwrap()).unwrap();
    let chapter = entries
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
    let mut result = Vec::new();
    let mut anchors = Vec::new();
    for mode in [
        "published",
        "table_unpadded",
        "cells_unpadded",
        "unpadded",
        "auto_cells",
        "separate",
        "legacy",
        "legacy_unpadded",
    ] {
        let mut book = Book::open(&path).unwrap();
        let mut probe = Probe::open(&path, Engine::Parley, &fonts, cfg, chapter).unwrap();
        let mut fc = crate::font_context(&fonts).unwrap();
        let mut styles = BookStyle::load(&mut book, &probe.chapter, &probe.source, &mut fc)
            .compute(&probe.source.elements);
        for (element, style) in probe.source.elements.iter().zip(&mut styles) {
            let cell = matches!(element.tag.as_str(), "th" | "td");
            if mode == "unpadded"
                || mode == "legacy_unpadded"
                || (mode == "cells_unpadded" && cell)
                || (mode == "table_unpadded" && element.tag == "table")
            {
                style.padding = Default::default();
            }
            if cell && (mode == "auto_cells" || mode.starts_with("legacy")) {
                style.image_width = None;
            }
            if element.tag == "table" && (mode == "separate" || mode.starts_with("legacy")) {
                style.border_collapse = false;
            }
        }
        if let Some(dest) = std::env::var_os("READER_TABLE_INTRINSICS")
            && mode == "published"
        {
            let mut ctx = LayoutContext::new();
            let mut rows = Vec::new();
            for block in &probe.source.blocks {
                let Some(cell) = block.table_cell else {
                    continue;
                };
                let indent = LineIndent::resolve(
                    &styles[block.element],
                    cfg.text_width(),
                    cfg.text_width(),
                    cfg.font_size,
                    0,
                );
                let layout = make_paragraph(
                    &mut ctx,
                    &mut fc,
                    &LayoutBlock::identity(block),
                    None,
                    cfg,
                    &styles,
                    &[],
                    &[],
                    false,
                    indent,
                    &Default::default(),
                );
                let demand = layout.calculate_content_widths();
                rows.push(serde_json::json!({"r":cell.row,"c":cell.column,"chars":block.text.chars().count(),"longest":block.text.split_whitespace().map(|s|s.chars().count()).max(),"size":cfg.font_size*styles[block.element].size_scale,"min":demand.min,"max":demand.max}));
            }
            std::fs::write(dest, serde_json::to_vec(&rows).unwrap()).unwrap();
        }
        let (layout, mut lines, height) = ParleyChapter::new(
            &mut book,
            &probe.chapter,
            &probe.source,
            fc,
            cfg,
            &styles,
            &Default::default(),
        )
        .unwrap();
        let edges = crate::publisher_breaks::geometry(&probe.source, &styles, &lines);
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        probe.pages =
            Pagination::with_breaks(&lines, cfg.text_height() as f32, height, &edges).unwrap();
        probe
            .pages
            .trim_unpainted_tail(layout.last_paint_bottom(&probe.source));
        probe.lines = lines;
        probe.renderer = Renderer::Parley(layout);
        let mut placements = Vec::new();
        for (i, entry) in entries.lines().enumerate() {
            let phrase = entry.splitn(3, '\t').nth(2).unwrap();
            let mut hits = probe.matches(phrase);
            assert_eq!(hits.len(), 1, "Source passage {}", i + 1);
            let start = hits.remove(0);
            let word = phrase.split_whitespace().next().unwrap();
            let end = crate::Anchor {
                offset: start.offset + word.len(),
                ..start.clone()
            };
            if mode == "published" {
                anchors.push((start.clone(), end.clone()));
            }
            assert_eq!((&start, &end), (&anchors[i].0, &anchors[i].1));
            let selection = probe.select_range(&start, &end).unwrap();
            assert_eq!(selection.text, word);
            let page = probe.page_for(&start).unwrap();
            let rect = probe.selection_rects(page, &selection)[0];
            let hit = probe
                .word_at(
                    page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            // A selection can include opening quotation marks omitted by word hit testing.
            assert_eq!(hit.start.block, start.block);
            assert!(hit.start.offset < end.offset && hit.end.offset > start.offset);
            let block = &probe.source.blocks[probe
                .source
                .blocks
                .binary_search_by_key(&start.block, |b| b.key)
                .unwrap()];
            let cell = block.table_cell.unwrap();
            let Renderer::Parley(renderer) = &probe.renderer else {
                panic!()
            };
            let p = renderer
                .paragraphs
                .iter()
                .find(|p| p.key == start.block)
                .unwrap();
            placements.push(serde_json::json!({"row":cell.row,"col":cell.column,"source":[start.block,start.offset,end.offset],"page":page,"x":rect.x,"column":[p.column.start,p.column.end],"paragraph_x":p.x}));
        }
        result.push(serde_json::json!({"mode":mode,"placements":placements}));
    }
    std::fs::write(
        std::env::var_os("READER_TABLE_REPORT").unwrap(),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
}

#[test]
fn collapsed_table_uses_its_header_widths_and_preserves_cross_column_word_hits() {
    let html = std::fs::read_to_string(crate::test_fixture("column-widths.xhtml")).unwrap();
    let render = |source: String| {
        Probe::from_book_content(
            Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
            Engine::Parley,
            &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                width: 900,
                height: 1000,
                margin: 30,
                font_size: 30.,
                ..Default::default()
            },
            0,
            Some(source),
        )
        .unwrap()
    };
    let mut published = render(html.clone());
    let zero = render(html.replace("padding:0 2em", "padding:0"));
    let mut auto = render(
        html.replace("width:68%", "width:auto")
            .replace("width:20%", "width:auto")
            .replace("width:12%", "width:auto"),
    );
    let narrow = render(html.replace("width:95%", "width:60%"));
    // Real bold headers can exceed a percentage track's minimum content width.
    // Keep a separate short normal-weight header case for exact percentages.
    let percent = render(
        html.replace("th.depth {", "th.depth {font-weight:normal;")
            .replace("th.buoy {", "th.buoy {font-weight:normal;"),
    );
    let rect = |p: &Probe, id: &str, word: &str| {
        let start = p.fragment(id).unwrap();
        let end = crate::Anchor {
            offset: start.offset + word.len(),
            ..start.clone()
        };
        let selection = p.select_range(&start, &end).unwrap();
        assert_eq!(selection.text, word);
        let page = p.page_for(&start).unwrap();
        let r = p.selection_rects(page, &selection)[0];
        let hit = p
            .word_at(
                page,
                (r.x + r.width / 2) as f32,
                (r.y + r.height / 2) as f32,
            )
            .unwrap();
        assert_eq!(hit.start, start);
        assert_eq!(hit.end, end);
        r
    };
    for (id, word) in [
        ("buoy-note", "Whitecaps"),
        ("buoy-name", "North"),
        ("sample-depth", "8"),
    ] {
        let p = rect(&published, id, word);
        assert_eq!(rect(&zero, id, word), p);
        assert_eq!(rect(&narrow, id, word), p);
        if id == "buoy-name" {
            let proportional = rect(&percent, id, word);
            assert!((proportional.x - (30 + 840 * 68 / 100 + 8)).abs() < 4);
            assert!(p.x <= proportional.x);
            assert!(p.x.abs_diff(rect(&auto, id, word).x) > 8);
        }
    }
    if let Some(dir) = std::env::var_os("READER_TABLE_SCREENSHOTS") {
        for (name, probe) in [("published", &mut published), ("automatic", &mut auto)] {
            image::save_buffer(
                std::path::PathBuf::from(&dir).join(format!("table-{name}.png")),
                &probe.capture(0).unwrap(),
                900,
                1000,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn intrinsic_measurement_reserves_an_entire_word_across_inline_formatting() {
    let html = r#"<html><body><table style="border-collapse:collapse"><tr><td id="long">Inter<em>continental</em></td><td id="short">One word. Several short words.</td></tr></table></body></html>"#;
    let cfg = Settings {
        width: 630,
        height: 900,
        font_size: 33.,
        margin: 30,
        ..Default::default()
    };
    let p = Probe::from_book_content(
        Book::open(&crate::test_fixture("field-notes-epub3.epub")).unwrap(),
        Engine::Parley,
        &[crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        cfg,
        0,
        Some(html.into()),
    )
    .unwrap();
    let start = p.fragment("long").unwrap();
    let end = crate::Anchor {
        offset: "Intercontinental".len(),
        ..start.clone()
    };
    let selection = p.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, "Intercontinental");
    let page = p.page_for(&start).unwrap();
    let r = p.selection_rects(page, &selection);
    assert_eq!(r.len(), 1, "The unbreakable word fits on one line");
    let next = p.fragment("short").unwrap();
    let crate::Renderer::Parley(renderer) = &p.renderer else {
        panic!()
    };
    let col = renderer
        .paragraphs
        .iter()
        .find(|q| q.key == next.block)
        .unwrap()
        .x;
    assert!(r[0].x + r[0].width <= cfg.margin as i32 + col.round() as i32);
    assert!(col > cfg.font_size * 4.);
}
