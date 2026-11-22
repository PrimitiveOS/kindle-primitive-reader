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
