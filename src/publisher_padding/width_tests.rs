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
            offset: start.offset + name.len(),
            ..start.clone()
        };
        let quote = bounded.select_range(&start, &end).unwrap();
        let page = bounded.page_for(&start).unwrap();
        assert_eq!(bounded.selection_rects(page, &quote)[0].x, x, "{name}");
    }
}
