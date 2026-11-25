use super::*;
use crate::{Book, Engine, Pagination, Probe, Settings};
use std::{collections::BTreeMap, path::PathBuf};

#[test]
fn protects_each_end_of_a_paragraph_and_relaxes_impossible_keeps() {
    let chapter = "<html><body><p>Opening paragraph</p><p style='orphans:2;widows:3'>A measured paragraph</p></body></html>";
    let source = crate::source::Source::parse(chapter).unwrap();
    let mut styles = vec![Style::default(); source.elements.len()];
    let target = &source.blocks[1];
    styles[target.element].orphans = 2;
    styles[target.element].widows = 3;
    let lines: Vec<_> = (0..9)
        .map(|n| Line {
            block: if n < 2 {
                source.blocks[0].key
            } else {
                target.key
            },
            bytes: 0..1,
            spans: None,
            additional_sources: None,
            top: n as f32 * 10.,
            bottom: n as f32 * 10. + 8.,
            page_extension: [0.0; 2],
            missing_glyphs: 0,
        })
        .collect();
    let b = geometry(&source, &styles, &lines);
    let intervals: Vec<_> = lines.iter().map(|l| l.top..l.bottom).collect();
    let ends = boundaries(&intervals, &b);
    assert!(
        ends.iter()
            .enumerate()
            .all(|(n, v)| (*v == Edge::Avoid) == matches!(n, 3 | 7 | 8))
    );
    let easy = Pagination::with_breaks(&lines, 38., 88., &b).unwrap();
    assert_eq!(easy.bounds, [0., 40., 60., 88.]);
    let impossible = Pagination::with_breaks(&lines, 10., 88., &b).unwrap();
    assert_eq!(impossible.len(), 9);
}

#[test]
#[ignore = "Supply a private EPUB and the one-based chapters to measure"]
fn compare_original_line_end_boundaries() {
    let edition = PathBuf::from(std::env::var_os("READER_LINE_END_BOOK").unwrap());
    let selected = std::env::var("READER_LINE_END_CHAPTERS").unwrap();
    let chapters: Vec<usize> = selected.split(',').map(|c| c.parse().unwrap()).collect();
    let mut book = Book::open(&edition).unwrap();
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let cfg = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.,
        ..Default::default()
    };
    let mut report = Vec::new();
    let mut texts = Vec::new();
    for chapter in chapters {
        let mut p = Probe::open(&edition, Engine::Parley, &fonts, cfg, chapter - 1).unwrap();
        let author = crate::book_style::BookStyle::load(
            &mut book,
            &p.chapter,
            &p.source,
            &mut crate::font_context(&fonts).unwrap(),
        );
        let styles = author.compute(&p.source.elements);
        let mut control = styles.clone();
        for st in &mut control {
            st.orphans = 1;
            st.widows = 1;
        }
        let old_edges = geometry(&p.source, &control, &p.lines);
        let mut old = Pagination::with_breaks(
            &p.lines,
            cfg.text_height() as f32,
            *p.pages.bounds.last().unwrap(),
            &old_edges,
        )
        .unwrap();
        if let crate::Renderer::Parley(ch) = &p.renderer {
            old.trim_unpainted_tail(ch.last_paint_bottom(&p.source));
        }
        let mut blocks = BTreeMap::<_, Vec<_>>::new();
        for line in &p.lines {
            if line.bottom > line.top {
                blocks.entry(line.block).or_default().push(line);
            }
        }
        let mut styled = 0;
        let mut old_violations = 0;
        let mut old_orphans = 0;
        let mut old_widows = 0;
        let mut new_violations = 0;
        let mut restored = 0;
        let mut changed = Vec::new();
        for block in &p.source.blocks {
            let style = &styles[block.element];
            if style.widows <= 1 && style.orphans <= 1 {
                continue;
            }
            let Some(lines) = blocks.get(&block.key) else {
                continue;
            };
            if lines.len() < 2 {
                continue;
            }
            styled += 1;
            for (j, pair) in lines.windows(2).enumerate() {
                let protected = j + 1 < usize::from(style.orphans)
                    || lines.len() - j - 1 < usize::from(style.widows);
                if !protected {
                    continue;
                }
                let old_split = old.page(pair[0].top) != old.page(pair[1].top);
                let new_split = p.pages.page(pair[0].top) != p.pages.page(pair[1].top);
                old_violations += usize::from(old_split);
                old_orphans += usize::from(old_split && j + 1 < usize::from(style.orphans));
                old_widows +=
                    usize::from(old_split && lines.len() - j - 1 < usize::from(style.widows));
                new_violations += usize::from(new_split);
                if old_split && !new_split {
                    let a = p.anchor(block.key, pair[0].bytes.start);
                    let b = p.anchor(block.key, pair[1].bytes.start);
                    if !pair[0].bytes.is_empty() && !pair[1].bytes.is_empty() {
                        let n = p.page_for(&a).unwrap();
                        assert_eq!(p.page_for(&b), Some(n));
                        assert_eq!(p.page_for(&p.page_anchor(n).unwrap()), Some(n));
                        changed.push(n);
                        let left = block.text[pair[0].bytes.clone()]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let right = block.text[pair[1].bytes.clone()]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        if left.split_whitespace().count() >= 3
                            && right.split_whitespace().count() >= 3
                        {
                            texts.push(format!("{chapter}\t{left}\t{right}"));
                        }
                    }
                    restored += 1;
                }
            }
        }
        changed.sort();
        changed.dedup();
        for n in &changed {
            assert!(p.capture(*n).unwrap().iter().any(|&v| v < 220));
        }
        let item = serde_json::json!({"chapter":chapter,"styled_multiline_blocks":styled,"baseline_violations":old_violations,"baseline_orphan_breaks":old_orphans,"baseline_widow_breaks":old_widows,"new_violations":new_violations,"rejoined_pairs":restored,"verified_pages":changed.len(),"old_pages":old.len(),"new_pages":p.page_count()});
        report.push(item);
    }
    std::fs::write(
        std::env::var_os("READER_LINE_END_REPORT").unwrap(),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    if let Some(p) = std::env::var_os("READER_LINE_END_PASSAGES") {
        std::fs::write(p, texts.join("\n")).unwrap();
    }
}

#[test]
fn authored_paragraph_ends_move_complete_lines_and_keep_the_source_anchors() {
    let path = crate::test_fixture("field-notes-epub3.epub");
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let text = include_str!("../../tests/fixtures/paragraph-ends.xhtml");
    let height = 480;
    {
        let cfg = Settings {
            width: 630,
            height,
            margin: 35,
            font_size: 30.,
            ..Default::default()
        };
        let open = |publisher_styles| {
            Probe::from_book_content(
                Book::open(&path).unwrap(),
                Engine::Parley,
                &fonts,
                Settings {
                    publisher_styles,
                    ..cfg
                },
                0,
                Some(text.into()),
            )
            .unwrap()
        };
        let mut old = open(false);
        let mut new = open(true);
        assert_eq!(old.lines.len(), new.lines.len());
        // Select a genuine orphan: the previous page holds just the first line
        // of its paragraph and the rest begins overleaf.
        for i in 0..old.lines.len() - 1 {
            let a = &old.lines[i];
            let b = &old.lines[i + 1];
            if a.block != b.block
                || a.bytes.start != 0
                || old.pages.page(a.top) == old.pages.page(b.top)
            {
                continue;
            }
            let page = old.pages.page(a.top);
            let aa = old.anchor(a.block, a.bytes.start);
            let bb = old.anchor(b.block, b.bytes.start);
            if new.page_for(&aa) != Some(page + 1) || new.page_for(&bb) != Some(page + 1) {
                continue;
            }
            let old_range = old.select_range(&aa, &bb).unwrap();
            assert_eq!(new.select_range(&aa, &bb).unwrap(), old_range);
            assert_eq!(
                new.page_for(&new.page_anchor(page + 1).unwrap()),
                Some(page + 1)
            );
            for (probe, name) in [(&mut old, "before"), (&mut new, "after")] {
                if let Some(dir) = std::env::var_os("READER_LINE_END_SCREENS") {
                    for n in 0..=1 {
                        let path = PathBuf::from(&dir).join(format!("{name}-{n}.png"));
                        image::save_buffer(
                            path,
                            &probe.capture(page + n).unwrap(),
                            cfg.width,
                            cfg.height,
                            image::ColorType::L8,
                        )
                        .unwrap();
                    }
                }
            }
            eprintln!(
                "Authored line-end control: {height} pixels, page {}",
                page + 1
            );
            return;
        }
    }
    panic!("Expected a single first line to move from page one to page two");
}
