use crate::{Anchor, BlockWidthMode, Book, Engine, Probe, Settings, test_fixture};

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn settings() -> Settings {
    Settings {
        width: WIDTH,
        height: HEIGHT,
        font_size: 40.0,
        line_height: 1.20,
        ..Settings::default()
    }
}

fn longest_band(frame: &[u8], shade: u8) -> (usize, usize) {
    let mut longest = (0, 0);
    for row in frame.chunks_exact(WIDTH as usize) {
        let mut run = 0;
        for (x, &pixel) in row.iter().enumerate() {
            run = if pixel == shade { run + 1 } else { 0 };
            if run > longest.1 {
                longest = (x + 1 - run, run);
            }
        }
    }
    longest
}

#[test]
fn publisher_width_matches_the_pinned_web_text_boxes_and_restores_the_original_caret() {
    let path = test_fixture("render-modes.epub");
    let fonts = [test_fixture("fonts/NotoSans-Regular.ttf")];
    let open = |block_width, chapter| {
        Probe::open(
            &path,
            Engine::Parley,
            &fonts,
            Settings {
                block_width,
                ..settings()
            },
            chapter,
        )
        .unwrap()
    };
    let mut plain = open(BlockWidthMode::Ignore, 0);
    let mut bounded = open(BlockWidthMode::Publisher, 0);
    assert_eq!(plain.coverage(), bounded.coverage());
    let previous = plain.capture(0).unwrap();
    let next = bounded.capture(0).unwrap();
    for (phrase, shade, pinned_x, pinned_width) in [
        ("Centered box marker", 230, 389, 485),
        ("Pixel box marker", 221, 54, 260),
    ] {
        assert_eq!(longest_band(&previous, shade), (54, 1156));
        assert_eq!(longest_band(&next, shade), (pinned_x, pinned_width));
        let start = plain.matches(phrase).remove(0);
        let end = Anchor {
            raw_caret: None,
            offset: start.offset + phrase.len(),
            ..start.clone()
        };
        let quote = plain.select_range(&start, &end).unwrap();
        let saved = serde_json::to_string(&quote).unwrap();
        let quote: crate::selection::TextSelection = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            bounded.select_range(&quote.start, &quote.end),
            Some(quote.clone())
        );
        let page = bounded.page_for(&quote.start).unwrap();
        let bounds = bounded.selection_rects(page, &quote)[0];
        assert_eq!(bounds.x as usize, pinned_x);
        let first = bounded
            .word_at(
                page,
                bounds.x as f32 + 4.0,
                bounds.y as f32 + bounds.height as f32 / 2.0,
            )
            .unwrap();
        assert_eq!(first.start, start);
        assert_eq!(first.text, phrase.split_whitespace().next().unwrap());
    }
    let reopened = open(BlockWidthMode::Publisher, 0);
    assert_eq!(
        reopened.matches("Centered box marker"),
        bounded.matches("Centered box marker")
    );
    let mut no_widths = open(BlockWidthMode::Publisher, 1);
    let mut ordinary = open(BlockWidthMode::Ignore, 1);
    assert_eq!(no_widths.page_count(), ordinary.page_count());
    for i in 0..ordinary.page_count() {
        assert_eq!(no_widths.capture(i).unwrap(), ordinary.capture(i).unwrap());
    }
    if let Some(path) = std::env::var_os("READER_WEB_WIDTH_CAPTURE") {
        image::save_buffer(path, &next, WIDTH, HEIGHT, image::ColorType::L8).unwrap();
    }
}

#[test]
fn publisher_width_cascades_margin_shorthand_and_wraps_only_supported_text_boxes() {
    let path = test_fixture("field-notes-epub3.epub");
    let css = r#"<html><head><style>
        p { margin: 0 auto; width: 260px; }
        .right { margin-right:0; }
        .left { margin-left:initial; }
        .oversized { width:150%; }
        .small { width:1px; }
        .mixed { margin-left:-20px; }
        .padded { padding-left:12px; }
        .max { max-width:100px; }
    </style></head><body>
        <p>Centered title wraps because it has an authored column. Read this whole sentence.</p>
        <p class="right">Right title</p><p class="left">Left title</p>
        <p class="oversized">Oversized title</p><p class="small">Small title</p>
        <p class="mixed">Signed margin title</p><p class="padded">Padded title</p>
        <p class="max">Maximum title</p><div><p>Nested title</p></div>
    </body></html>"#;
    let open = |block_width| {
        Probe::from_book_content(
            Book::open(&path).unwrap(),
            Engine::Parley,
            &[test_fixture("fonts/NotoSans-Regular.ttf")],
            Settings {
                block_width,
                base_stylesheet: crate::BaseStylesheet::None,
                ..settings()
            },
            0,
            Some(css.into()),
        )
        .unwrap()
    };
    let plain = open(BlockWidthMode::Ignore);
    let bounded = open(BlockWidthMode::Publisher);
    let phrase =
        "Centered title wraps because it has an authored column. Read this whole sentence.";
    let start = plain.matches(phrase).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    let quote = plain.select_range(&start, &end).unwrap();
    assert_eq!(bounded.select_range(&start, &end), Some(quote.clone()));
    let from = plain.selection_rects(plain.page_for(&start).unwrap(), &quote);
    let to = bounded.selection_rects(bounded.page_for(&start).unwrap(), &quote);
    assert!(to.len() > from.len());
    assert_eq!(to[0].x, 502);
    for (name, x) in [
        ("Right title", 950),
        ("Left title", 54),
        ("Oversized title", 54),
        ("Small title", 54),
        ("Signed margin title", 54),
        ("Padded title", 66),
        ("Maximum title", 54),
        ("Nested title", 54),
    ] {
        let start = bounded.matches(name).remove(0);
        let end = Anchor {
            raw_caret: None,
            offset: start.offset + name.len(),
            ..start.clone()
        };
        let quote = bounded.select_range(&start, &end).unwrap();
        let page = bounded.page_for(&start).unwrap();
        assert_eq!(bounded.selection_rects(page, &quote)[0].x, x, "{name}");
    }
}

fn open_constraints(config: Settings, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-width-margins.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        config,
        0,
        document,
    )
    .unwrap()
}

fn constrained() -> Settings {
    Settings {
        block_width: BlockWidthMode::PublisherWithConstraints,
        ..settings()
    }
}

fn find_quoted(probe: &Probe, label: &str) -> crate::selection::TextSelection {
    let start = probe.matches(label).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + label.len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

#[test]
fn publisher_width_constraints_match_native_web_geometry_and_link_targets() {
    let mut ordinary = open_constraints(settings(), None);
    let mut wide = open_constraints(constrained(), None);
    let frame = wide.capture(0).unwrap();
    let baseline = ordinary.capture(0).unwrap();
    for (label, shade, x, width) in [
        ("Fixed marker", 221, 94, 320),
        ("Trailing marker", 204, 692, 462),
        ("Maximum marker", 187, 442, 380),
        ("Minimum marker", 170, 424, 416),
        ("Auto cap marker", 153, 422, 420),
        ("Overflow marker", 136, 134, 1000),
        ("Direction marker", 119, 890, 320),
        ("Natural marker", 102, 106, 1092),
    ] {
        assert_eq!(longest_band(&baseline, shade), (54, 1156));
        assert_eq!(longest_band(&frame, shade), (x, width), "{label}");
        let saved = find_quoted(&ordinary, label);
        let quote = wide.select_range(&saved.start, &saved.end).unwrap();
        assert_eq!(quote, saved);
        let page = wide.page_for(&quote.start).unwrap();
        let rect = wide.selection_rects(page, &quote)[0];
        let y = (rect.y + rect.height / 2) as f32;
        assert!(rect.x as usize >= x && (rect.x + rect.width) as usize <= x + width);
        if label != "Direction marker" {
            assert_eq!(rect.x as usize, x);
        }
        let hit = wide.word_at(page, rect.x as f32 + 8.0, y).unwrap();
        assert_eq!(hit.text, label.split_whitespace().next().unwrap());
        assert!(wide.text_position_at(page, x as f32 - 1.0, y).is_none());
        assert!(wide.word_at(page, (x + width) as f32, y).is_none());
        if label == "Fixed marker" {
            let href = wide.link_at(page, rect.x as f32 + 8.0, y).unwrap();
            assert_eq!(href.fragment.as_deref(), Some("west"));
        }
    }
    if let Some(path) = std::env::var_os("READER_CONSTRAINED_WIDTH_CAPTURE_DIR") {
        let path = std::path::PathBuf::from(path);
        for (name, pixels) in [
            ("reader-default.png", baseline),
            ("reader-constrained.png", frame),
        ] {
            image::save_buffer(
                path.join(name),
                &pixels,
                WIDTH,
                HEIGHT,
                image::ColorType::L8,
            )
            .unwrap();
        }
    }
}

#[test]
fn publisher_width_constraints_repaginate_at_the_saved_source_anchor() {
    let text = "The team measured how the wind pushed water against the edge. ".repeat(25);
    let html = format!(
        "<html><body><p style='width:400px;max-width:240px;margin:0 25px 0 auto'>{text}Final heading at the river.</p></body></html>"
    );
    let regular = open_constraints(
        Settings {
            height: 520,
            ..settings()
        },
        Some(html.clone()),
    );
    let mut wide = open_constraints(
        Settings {
            height: 520,
            ..constrained()
        },
        Some(html),
    );
    let quote = find_quoted(&regular, "Final heading at the river");
    let old_page = regular.page_for(&quote.start).unwrap();
    let page = wide.page_for(&quote.start).unwrap();
    assert!(page > old_page && wide.page_count() > regular.page_count());
    assert_eq!(
        wide.select_range(&quote.start, &quote.end),
        Some(quote.clone())
    );
    assert!(
        wide.selection_rects(page, &quote)
            .iter()
            .all(|rect| rect.x >= 945)
    );
    let saved = serde_json::to_vec(&quote.start).unwrap();
    let focus: Anchor = serde_json::from_slice(&saved).unwrap();
    assert_eq!(wide.restore_anchor(&focus), Some(quote.start));
    assert_eq!(wide.capture(page).unwrap().len(), WIDTH as usize * 520);
}

#[test]
fn publisher_width_constraints_respect_cascade_and_keep_fallback_frames() {
    let html = "<html><head><style>body { min-width:11em }
        p { width:8em; min-width:13em; margin-left:1em; text-indent:0; background:#ddd }
        .normal { min-width:initial }
        .inset { width:16em; min-width:13em; max-width:9em; background:#ccc }
        .inherited { font-size:150%; width:180px; min-width:inherit; background:#bbb }
        </style></head><body><p class='normal'>Reset minimum</p><p class='inset'>Own minimum</p>
        <p class='inherited'>Inherited minimum</p></body></html>";
    let mut applied = open_constraints(constrained(), Some(html.into()));
    for (phrase, x) in [("Reset minimum", 94), ("Own minimum", 94)] {
        let quote = find_quoted(&applied, phrase);
        assert_eq!(applied.selection_rects(0, &quote)[0].x, x);
    }
    let frame = applied.capture(0).unwrap();
    for (shade, x, width) in [(221, 94, 320), (204, 94, 520), (187, 114, 440)] {
        assert_eq!(longest_band(&frame, shade), (x, width));
    }
    let default = Settings {
        publisher_styles: false,
        ..settings()
    };
    let opt_in = Settings {
        publisher_styles: false,
        ..constrained()
    };
    assert!(
        open_constraints(default, None).capture(0).unwrap()
            == open_constraints(opt_in, None).capture(0).unwrap()
    );

    let previous = Settings {
        block_width: BlockWidthMode::PublisherWithWrappers,
        ..settings()
    };
    for css in [
        "width:80px;min-width:1400px",
        "width:600px;margin-left:800px",
        "width:300px;margin-left:-20px",
        "width:300px;margin:0 20px;padding-left:8px",
        "width:300px;margin:0 20px;border-left:8px solid black",
    ] {
        let doc = format!(
            "<html><body><p style='{css};background:#ddd'>Guarded paragraph</p></body></html>"
        );
        let old = open_constraints(previous, Some(doc.clone()))
            .capture(0)
            .unwrap();
        let new = open_constraints(constrained(), Some(doc))
            .capture(0)
            .unwrap();
        assert!(old == new, "{css}");
    }
}
