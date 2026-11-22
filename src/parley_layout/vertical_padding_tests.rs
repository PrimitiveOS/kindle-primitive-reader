use super::*;
use crate::{Anchor, Engine, Pagination, Probe, Renderer};
use std::collections::BTreeMap;

fn config() -> Settings {
    Settings {
        width: 800,
        height: 1040,
        margin: 45,
        font_size: 32.,
        ..Default::default()
    }
}
fn authored(html: &str, cfg: Settings) -> Probe {
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
fn row_y(p: &Probe, id: &str) -> i32 {
    p.anchor_top(&p.fragment(id).unwrap()).unwrap().round() as i32
}
const TABLE: &str = include_str!("../../tests/fixtures/publisher-vertical-padding.xhtml");
#[test]
fn seven_rows_add_top_and_bottom_padding_once_and_keep_source_checkpoints() {
    let cfg = config();
    let mut normal = authored(TABLE, cfg);
    let mut flat = authored(&TABLE.replace("10px 8px", "0px 8px"), cfg);
    for (id, expected) in [
        ("station", 10),
        ("quay", 30),
        ("rocks", 50),
        ("pool", 70),
        ("islet", 90),
        ("creek", 110),
        ("ridge", 130),
        ("end", 140),
    ] {
        assert_eq!(row_y(&normal, id) - row_y(&flat, id), expected);
        assert_eq!(
            normal.fragment(id).unwrap().offset,
            flat.fragment(id).unwrap().offset
        );
    }
    let frames = [normal.capture(0).unwrap(), flat.capture(0).unwrap()];
    // The published header paints through its padding up to the next row's rule.
    let x = (cfg.margin + 6) as usize;
    let samples: Vec<_> = frames[0]
        .chunks(cfg.width as usize)
        .map(|row| row[x])
        .collect();
    let first = samples.iter().position(|v| *v == 221).unwrap();
    let divider = samples[first..].iter().position(|v| *v == 85).unwrap() + first;
    assert!(divider - first > 48);
    assert!(samples[first..divider].iter().all(|v| *v == 221));
    assert_ne!(frames[0], frames[1]);
    if let Some(dir) = std::env::var_os("READER_VERTICAL_SCREENS") {
        for (frame, name) in frames.iter().zip(["publisher", "cleared"]) {
            image::save_buffer(
                std::path::PathBuf::from(&dir).join(format!("vertical-cell-{name}.png")),
                frame,
                cfg.width,
                cfg.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn separated_table_reserves_its_outer_padding_and_collapsed_table_discards_it() {
    let cfg = config();
    for (model, top, after) in [("separate", 13, 30), ("collapse", 0, 0)] {
        let html = TABLE.replace(
            "border-collapse:collapse",
            &format!("border-collapse:{model};padding:13px 0 17px;border-spacing:0"),
        );
        let padded = authored(&html, cfg);
        let clear = authored(&html.replace("13px 0 17px", "0"), cfg);
        for id in ["station", "quay", "ridge"] {
            assert_eq!(row_y(&padded, id) - row_y(&clear, id), top);
        }
        assert_eq!(row_y(&padded, "end") - row_y(&clear, "end"), after);
    }
}
#[test]
fn nested_and_percentage_padding_wraps_content_once_and_fills_its_own_background() {
    let cfg = config();
    let h = r#"<html><body><div style="padding:7px 0 9px"><p id="a" style="padding:3% 0 4px;background-color:#aaa">Morning</p><p id="b">Afternoon</p></div><p id="c">Evening</p></body></html>"#;
    let mut padded = authored(h, cfg);
    let reference = authored(&h.replace("7px 0 9px", "0").replace("3% 0 4px", "0"), cfg);
    let percent = (f64::from(cfg.text_width()) * 0.03).floor() as i32;
    assert_eq!(row_y(&padded, "a") - row_y(&reference, "a"), 7 + percent);
    assert_eq!(
        row_y(&padded, "b") - row_y(&reference, "b"),
        7 + percent + 4
    );
    assert_eq!(
        row_y(&padded, "c") - row_y(&reference, "c"),
        7 + percent + 4 + 9
    );
    let image = padded.capture(0).unwrap();
    let x = (cfg.margin + 3) as usize;
    let at = |y| image[((cfg.top_margin() + y) * cfg.width) as usize + x];
    assert_eq!(at(2), 255);
    assert_eq!(at(8), 170);
    assert_eq!(at(7 + percent as u32 - 1), 170);
    let pair = r#"<html><body><div style="padding-top:8px"><table><tr><td><p id="one">First</p><p id="two">Second</p></td><td id="three">Third</td></tr></table></div><p id="last">Last</p></body></html>"#;
    let wrap = authored(pair, cfg);
    let plain = authored(&pair.replace("8px", "0"), cfg);
    for id in ["one", "two", "three", "last"] {
        assert_eq!(row_y(&wrap, id) - row_y(&plain, id), 8);
    }
}
#[test]
fn cross_page_checkpoint_reopens_at_same_offset_after_vertical_reflow() {
    let cfg = Settings {
        height: 450,
        ..config()
    };
    let mut padded = authored(TABLE, cfg);
    let mut flat = authored(&TABLE.replace("10px 8px", "0 8px"), cfg);
    let ids = [
        "station", "quay", "rocks", "pool", "islet", "creek", "ridge", "end",
    ];
    let id = ids
        .into_iter()
        .find(|id| {
            flat.page_for(&flat.fragment(id).unwrap())
                != padded.page_for(&padded.fragment(id).unwrap())
        })
        .expect("at least one entry moves to a different page");
    let mark = padded.fragment(id).unwrap();
    let saved = serde_json::to_vec(&mark).unwrap();
    let page = padded.page_for(&mark).unwrap();
    let first = padded.capture(page).unwrap();
    padded.capture(page.saturating_sub(1)).unwrap();
    assert_eq!(first, padded.capture(page).unwrap());
    let _ = flat
        .capture(flat.page_for(&flat.fragment(id).unwrap()).unwrap())
        .unwrap();
    drop(padded);
    let mut reopened = authored(TABLE, cfg);
    let mark: Anchor = serde_json::from_slice(&saved).unwrap();
    assert_eq!(reopened.page_for(&mark), Some(page));
    assert_eq!(reopened.capture(page).unwrap(), first);
}
fn original(path: &std::path::Path, chapter: usize, mode: &str, cfg: Settings) -> Probe {
    let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut book = Book::open(path).unwrap();
    let mut p = Probe::open(path, Engine::Parley, &fonts, cfg, chapter).unwrap();
    let mut fc = crate::font_context(&fonts).unwrap();
    let mut styles = crate::book_style::BookStyle::load(&mut book, &p.chapter, &p.source, &mut fc)
        .compute(&p.source.elements);
    for (e, s) in p.source.elements.iter().zip(&mut styles) {
        if matches!(e.tag.as_str(), "td" | "th") {
            if mode == "cleared" {
                s.vertical_padding = Default::default();
            }
            if mode == "top_only" {
                s.vertical_padding[1] = Default::default();
            }
        } else if e.tag == "table" && mode == "table_cleared" {
            s.vertical_padding = Default::default();
        }
    }
    let (layout, mut lines, h) = ParleyChapter::new(
        &mut book,
        &p.chapter,
        &p.source,
        fc,
        cfg,
        &styles,
        &Default::default(),
    )
    .unwrap();
    let splits = crate::publisher_breaks::geometry(&p.source, &styles, &lines);
    lines.sort_by(|a, b| a.top.total_cmp(&b.top));
    p.pages = Pagination::with_breaks(&lines, cfg.text_height() as f32, h, &splits).unwrap();
    p.pages
        .trim_unpainted_tail(layout.last_paint_bottom(&p.source));
    p.lines = lines;
    p.renderer = Renderer::Parley(layout);
    p
}
#[test]
#[ignore = "The original EPUB, user-provided source passages and numeric stock measurements are private"]
fn compares_real_vertical_cells_and_saved_checkpoint_with_installed_reference() {
    let path = std::path::PathBuf::from(std::env::var_os("READER_TABLE_BOOK").unwrap());
    let inputs =
        std::fs::read_to_string(std::env::var_os("READER_TABLE_PASSAGES").unwrap()).unwrap();
    let stock: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("READER_VERTICAL_REFERENCE").unwrap()).unwrap(),
    )
    .unwrap();
    let reference = stock["sources"]["061"]["stock"]["spans"]
        .as_array()
        .unwrap();
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
    let cfg = Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        font_size: 42.,
        ..Default::default()
    };
    let mut probes = BTreeMap::new();
    let mut anchors = BTreeMap::new();
    let mut frames = BTreeMap::new();
    for mode in [
        "published",
        "cleared",
        "top_only",
        "table_cleared",
        "restored",
    ] {
        let mut p = original(&path, chapter, mode, cfg);
        let mut geometry = BTreeMap::new();
        let mut page_frames = BTreeMap::new();
        for line in inputs.lines() {
            let mut parts = line.splitn(3, '\t');
            parts.next();
            let index = parts.next().unwrap().parse::<usize>().unwrap();
            let phrase = parts.next().unwrap();
            if !reference
                .iter()
                .any(|v| v["source"].as_u64() == Some(index as u64))
            {
                continue;
            }
            let found = p.matches(phrase);
            assert_eq!(found.len(), 1, "unique source {index}");
            let a = &found[0];
            let end = Anchor {
                offset: a.offset + phrase.split_whitespace().next().unwrap().len(),
                ..a.clone()
            };
            let word = p.select_range(a, &end).unwrap();
            let page = p.page_for(a).unwrap();
            let rect = p.selection_rects(page, &word)[0];
            let hit = p
                .word_at(
                    page,
                    (rect.x + rect.width / 2) as f32,
                    (rect.y + rect.height / 2) as f32,
                )
                .unwrap();
            assert_eq!(hit.start.block, a.block);
            assert!(hit.start.offset < end.offset && hit.end.offset > a.offset);
            let saved = serde_json::to_vec(a).unwrap();
            let ink = p.anchor_top(a).unwrap().round() as i64;
            if mode == "published" {
                anchors.insert(index, saved);
            } else {
                assert!(anchors[&index] == saved, "source moved: {index}");
            }
            let recovered: Anchor = serde_json::from_slice(&anchors[&index]).unwrap();
            assert_eq!(p.page_for(&recovered), Some(page));
            geometry.insert(
                index,
                [page as i64, i64::from(rect.x), i64::from(rect.y), ink],
            );
            page_frames
                .entry(page)
                .or_insert_with(|| p.capture(page).unwrap());
        }
        if mode == "published" {
            frames = page_frames;
        } else if mode == "restored" {
            assert!(frames == page_frames, "restored frames differed");
        }
        probes.insert(mode, geometry);
    }
    assert_eq!(probes["published"], probes["restored"]);
    assert_eq!(probes["published"], probes["table_cleared"]);
    let mut changed = 0;
    for v in reference {
        let i = v["source"].as_u64().unwrap() as usize;
        for mode in ["published", "top_only", "table_cleared"] {
            let rust = probes[mode][&i][3] - probes["cleared"][&i][3];
            let expected = v[mode][3].as_i64().unwrap() - v["cleared"][3].as_i64().unwrap();
            assert_eq!(rust, expected, "source {i}, configuration {mode}");
        }
        changed += usize::from(probes["published"][&i][0] != probes["cleared"][&i][0]);
    }
    let (&source_index, _) = probes["published"]
        .iter()
        .find(|(i, point)| point[0] != probes["cleared"][i][0])
        .expect("at least one real source checkpoint changes page");
    let mark: Anchor = serde_json::from_slice(&anchors[&source_index]).unwrap();
    let old_page = probes["cleared"][&source_index][0] as usize;
    let new_page = probes["published"][&source_index][0] as usize;
    let report = std::path::PathBuf::from(std::env::var_os("READER_VERTICAL_REPORT").unwrap());
    let scratch = report
        .parent()
        .unwrap()
        .join(format!("vertical-checkpoint-{}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let file = scratch.join("library.json");
    let root = path.parent().unwrap();
    {
        use crate::library::{Bookmark, Library, ReadingPosition};
        let mut library = Library::open(root, &file).unwrap();
        let entry = library.resolve(path.file_name().unwrap().as_ref()).unwrap();
        let state = library.track(&entry, mark.book);
        let old = ReadingPosition {
            chapter,
            page: old_page,
            anchor: Some(mark.clone()),
        };
        state.position = Some(old.clone());
        state.bookmarks.push(Bookmark {
            label: "Checkpoint".into(),
            position: old,
        });
        library
            .save_with(|path, bytes| std::fs::write(path, bytes))
            .unwrap();
    }
    let library = crate::library::Library::open(root, &file).unwrap();
    let state = library.state(mark.book).unwrap();
    let mut fresh = original(&path, chapter, "published", cfg);
    for position in [
        state.position.as_ref().unwrap(),
        &state.bookmarks[0].position,
    ] {
        assert_eq!(position.page, old_page);
        let restored = position.anchor.as_ref().unwrap();
        assert!(restored == &mark);
        assert_eq!(fresh.page_for(restored), Some(new_page));
    }
    fresh.capture(new_page.saturating_sub(1)).unwrap();
    assert!(fresh.capture(new_page).unwrap() == frames[&new_page]);
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(scratch).unwrap();
    let results = serde_json::json!({
        "cases":probes,"regular_view_page_changes":changed,"saved_offsets_checked":anchors.len(),
        "exact_restored_frames":frames.len(),"reopened_library_bookmark":[source_index,old_page,new_page]
    });
    std::fs::write(report, serde_json::to_vec(&results).unwrap()).unwrap();
}
