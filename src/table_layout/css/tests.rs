use crate::{BlockWidthMode, Book, Engine, Probe, Settings, selection::HighlightRect};

fn specimen(settings: Settings, excerpt: Option<&str>) -> Probe {
    let book = Book::open(&crate::test_fixture("render-css-tables.epub")).unwrap();
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    Probe::from_book_content(
        book,
        Engine::Parley,
        &[font],
        settings,
        0,
        excerpt.map(str::to_owned),
    )
    .unwrap()
}

fn viewport() -> Settings {
    Settings {
        width: 1264,
        height: 1606,
        margin: 54,
        render_dpi: 96,
        font_size: 40.0,
        ..Settings::default()
    }
}

fn rect(probe: &Probe, id: &str, word: &str) -> (usize, HighlightRect) {
    let start = probe.fragment(id).unwrap();
    let mut end = start.clone();
    end.offset += word.len();
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, word);
    let page = probe.page_for(&start).unwrap();
    let bounds = probe.selection_rects(page, &selection)[0];
    let x = (bounds.x + bounds.width / 2) as f32;
    let y = (bounds.y + bounds.height / 2) as f32;
    assert_eq!(probe.word_at(page, x, y).unwrap().text, word);
    assert_eq!(
        probe.text_position_at(page, x, y).unwrap().block,
        start.block
    );
    (page, bounds)
}

fn pair(probe: &Probe, first: &str, second: &str, x: i32) -> i32 {
    let (p1, left) = rect(probe, &first.to_lowercase(), first);
    let (p2, right) = rect(probe, &second.to_lowercase(), second);
    assert_eq!(p1, p2);
    assert_eq!(left.y, right.y, "{first}/{second}");
    assert_eq!(left.x, 64, "{first}");
    assert!(
        (right.x - x).abs() <= 1,
        "{second}: {} expected {x}",
        right.x
    );
    right.y
}

#[test]
fn repair_css_tables_keeps_source_selection_and_widths_across_modes() {
    let settings = viewport();
    let ordinary = specimen(settings, None);
    let (_, pier) = rect(&ordinary, "pier", "Pier");
    let (_, buoy) = rect(&ordinary, "buoy", "Buoy");
    assert!(pier.y < buoy.y);
    let old = ordinary.fragment("buoy").unwrap();
    let mut end = old.clone();
    end.offset += "Buoy".len();
    let quote = ordinary.select_range(&old, &end).unwrap();

    for (mode, width, right_x) in [
        ("flat", BlockWidthMode::Ignore, 642),
        ("web", BlockWidthMode::PublisherWithConstraints, 469),
    ] {
        let mut repaired = specimen(
            Settings {
                repair_css_tables: true,
                block_width: width,
                ..settings
            },
            None,
        );
        assert_eq!(repaired.page_count(), 1);
        assert_eq!(repaired.fragment("buoy"), Some(old.clone()));
        assert_eq!(repaired.select_range(&old, &end).unwrap(), quote);
        let first = pair(&repaired, "Pier", "Buoy", right_x);
        let grouped = pair(&repaired, "North", "South", right_x);
        let orphan_first = pair(&repaired, "Light", "Dark", 642);
        let orphan_second = pair(&repaired, "Sand", "Rock", 642);
        let split = pair(&repaired, "Rope", "Mast", right_x);
        let (_, ebb) = rect(&repaired, "ebb", "Ebb");
        let (_, flood) = rect(&repaired, "flood", "Flood");
        let (_, watch) = rect(&repaired, "watch", "Watch");
        let (_, dawn) = rect(&repaired, "dawn", "Dawn");
        assert_eq!(ebb.y, flood.y);
        assert!(
            (130..165).contains(&flood.x),
            "intrinsic column: {}",
            flood.x
        );
        assert!(first < grouped && grouped < ebb.y);
        assert!(ebb.y < orphan_first && orphan_first < orphan_second);
        assert!(orphan_second < watch.y && watch.y < dawn.y && dawn.y < split);
        assert!(repaired.matches("Hidden").is_empty());
        let (page, held) = rect(&repaired, "buoy", "Buoy");
        let x = (held.x + held.width / 2) as f32;
        let y = (held.y + held.height / 2) as f32;
        assert_eq!(
            repaired.link_at(page, x, y).unwrap().fragment.as_deref(),
            Some("after")
        );
        let pixels = repaired.capture(0).unwrap();
        assert!(pixels.contains(&187));
        assert!(pixels.contains(&221));
        if let Some(dir) = std::env::var_os("READER_CSS_TABLE_CAPTURES") {
            let output =
                std::path::Path::new(&dir).join(format!("render-css-tables-reader-{mode}.png"));
            image::save_buffer(
                output,
                &pixels,
                settings.width,
                settings.height,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn repair_css_tables_leaves_native_grids_and_unsupported_boxes_readable() {
    let html = r#"
        <html><head><style type="text/css">
        .grid { display:table } .cell { display:table-cell }
        </style></head><body>
        <table><tr><td id="port">Port</td><td id="shoal">Shoal</td></tr></table>
        <div class="grid"><div class="cell" id="outer">Outer</div>
        <div class="cell"><div class="grid"><div class="cell" id="nested">Nested</div>
        <div class="cell" id="channel">Channel</div></div></div></div>
        <p id="end">End</p></body></html>"#;
    let mut ordinary = specimen(viewport(), Some(html));
    let mut repaired = specimen(
        Settings {
            repair_css_tables: true,
            ..viewport()
        },
        Some(html),
    );
    assert_eq!(ordinary.capture(0).unwrap(), repaired.capture(0).unwrap());
    for (id, word) in [
        ("port", "Port"),
        ("shoal", "Shoal"),
        ("outer", "Outer"),
        ("nested", "Nested"),
        ("channel", "Channel"),
        ("end", "End"),
    ] {
        assert_eq!(ordinary.fragment(id), repaired.fragment(id));
        rect(&repaired, id, word);
    }

    let settings = Settings {
        publisher_styles: false,
        ..viewport()
    };
    let mut disabled = specimen(settings, None);
    let mut without_author_css = specimen(
        Settings {
            repair_css_tables: true,
            ..settings
        },
        None,
    );
    assert_eq!(
        disabled.capture(0).unwrap(),
        without_author_css.capture(0).unwrap()
    );
}

#[test]
fn repair_css_tables_keep_cells_selectable_after_page_breaks() {
    let settings = Settings {
        height: 315,
        repair_css_tables: true,
        ..viewport()
    };
    let mut repaired = specimen(settings, None);
    assert!(repaired.page_count() >= 3);
    let (_, first) = rect(&repaired, "pier", "Pier");
    for (a, b) in [
        ("Pier", "Buoy"),
        ("North", "South"),
        ("Light", "Dark"),
        ("Sand", "Rock"),
        ("Rope", "Mast"),
    ] {
        pair(&repaired, a, b, 642);
    }
    let (last, _) = rect(&repaired, "mast", "Mast");
    assert!(last > 0);
    assert!(first.x >= settings.margin as i32);
    for page in 0..repaired.page_count() {
        assert_eq!(
            repaired.capture(page).unwrap().len(),
            (settings.width * settings.height) as usize
        );
    }
}
