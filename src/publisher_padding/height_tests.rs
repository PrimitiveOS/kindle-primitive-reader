use crate::{Anchor, BlockWidthMode, Book, Engine, Probe, Renderer, Settings, test_fixture};

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;
const COMPACT_HEIGHT: u32 = 690;

fn settings(height: u32, enabled: bool) -> Settings {
    Settings {
        width: WIDTH,
        height,
        font_size: 40.0,
        line_height: 1.20,
        block_width: if enabled {
            BlockWidthMode::Publisher
        } else {
            BlockWidthMode::Ignore
        },
        publisher_block_height: enabled,
        ..Settings::default()
    }
}

fn specimen(settings: Settings, document: Option<String>) -> Probe {
    let book = Book::open(&test_fixture("render-text-heights.epub")).unwrap();
    Probe::from_book_content(
        book,
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        0,
        document,
    )
    .unwrap()
}

fn quote(probe: &Probe, phrase: &str) -> crate::selection::TextSelection {
    let start = probe.matches(phrase).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

fn pixel(frame: &[u8], x: usize, y: usize) -> u8 {
    frame[y * WIDTH as usize + x]
}

fn band(frame: &[u8], x: usize, gray: u8) -> (usize, usize) {
    let height = frame.len() / WIDTH as usize;
    let first = (0..height).find(|&y| pixel(frame, x, y) == gray).unwrap();
    let last = (first..height)
        .find(|&y| pixel(frame, x, y) != gray)
        .unwrap_or(height);
    (first, last)
}

#[test]
fn publisher_text_height_matches_stock_paint_retains_text_and_restores_saved_quotes() {
    let mut stock_default = specimen(settings(HEIGHT, false), None);
    let mut web = specimen(settings(HEIGHT, true), None);
    assert_eq!((stock_default.page_count(), web.page_count()), (1, 2));
    assert_eq!(stock_default.coverage(), web.coverage());

    let plain = stock_default.capture(0).unwrap();
    let painted = web.capture(0).unwrap();
    for (x, gray, native_height) in [(1209, 224, 164), (473, 194, 153)] {
        let (top, bottom) = band(&painted, x, gray);
        assert_eq!(bottom - top, native_height);
        assert_ne!(pixel(&painted, x + 1, top + 2), gray);
        let (plain_top, plain_bottom) = band(&plain, x, gray);
        assert!(plain_bottom - plain_top < native_height);
    }
    let (_, bottom) = band(&painted, 1209, 224);
    assert!(web.word_at(0, 75.0, (bottom - 15) as f32).is_none());
    let tall = quote(&web, "Tall height");
    let follows = quote(&web, "Tall follows");
    let tall_rect = web.selection_rects(0, &tall)[0];
    let following_rect = web.selection_rects(0, &follows)[0];
    // The next Parley text frame retains its own leading above the glyphs.
    assert_eq!(following_rect.y - tall_rect.y, 168);

    for gray in [235, 214, 204] {
        let (a, b) = band(&plain, 1209, gray);
        let (c, d) = band(&painted, 1209, gray);
        assert!(((d - c) as isize - (b - a) as isize).abs() <= 1);
    }
    let short = quote(&web, "Short height");
    let continuation = quote(&web, "Short continuation");
    assert!(web.selection_rects(0, &continuation)[0].y > web.selection_rects(0, &short)[0].y);

    let start = quote(&stock_default, "Last line");
    let saved = serde_json::to_string(&start).unwrap();
    let restored: crate::selection::TextSelection = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        web.select_range(&restored.start, &restored.end),
        Some(restored.clone())
    );
    assert_eq!(stock_default.page_for(&restored.start), Some(0));
    assert_eq!(web.page_for(&restored.start), Some(1));
    assert_eq!(
        specimen(settings(HEIGHT, true), None).page_for(&restored.start),
        Some(1)
    );
    capture("reader-web-height.png", &painted, HEIGHT);
}

#[test]
fn publisher_text_height_can_split_painted_space_without_creating_text_anchors() {
    let mut web = specimen(settings(COMPACT_HEIGHT, true), None);
    let ordinary = specimen(settings(COMPACT_HEIGHT, false), None);
    assert_eq!((ordinary.page_count(), web.page_count()), (2, 3));
    let last = quote(&web, "Last line");
    assert_eq!(web.page_for(&last.start), Some(2));
    assert_eq!(web.page_anchor(2), Some(last.start.clone()));
    let second = web.capture(1).unwrap();
    let third = web.capture(2).unwrap();
    let (start, stop) = band(&second, 1209, 184);
    let (continued, end) = band(&third, 1209, 184);
    assert_eq!(stop, (COMPACT_HEIGHT - 54) as usize);
    assert_eq!(continued, 54);
    assert_eq!((stop - start) + (end - continued), 600);
    assert!(web.word_at(2, 70.0, (end - 20) as f32).is_none());
    let last_rect = web.selection_rects(2, &last)[0];
    assert_eq!(
        web.word_at(2, 68.0, last_rect.y as f32 + last_rect.height as f32 / 2.0)
            .unwrap()
            .start,
        last.start
    );
    capture("reader-compact-height-page-2.png", &second, COMPACT_HEIGHT);
    capture("reader-compact-height-page-3.png", &third, COMPACT_HEIGHT);
}

#[test]
fn publisher_text_height_preserves_default_frames_and_bounds_unsupported_boxes() {
    let path = test_fixture("render-text-heights.epub");
    let mut without_declarations = Book::open(&path).unwrap().chapter(0).unwrap();
    for declaration in [
        "height:164px;",
        "height:24px;",
        "height:45%;",
        "height:153px;",
        "height:600px;",
    ] {
        without_declarations = without_declarations.replace(declaration, "");
    }
    let mut plain = specimen(settings(HEIGHT, false), None);
    let mut undeclared = specimen(settings(HEIGHT, false), Some(without_declarations));
    assert_eq!(plain.page_count(), undeclared.page_count());
    assert!(
        plain.capture(0).unwrap() == undeclared.capture(0).unwrap(),
        "ordinary frames differ with unused CSS heights"
    );

    let unsupported = Some(
        r#"<html><body>
        <p style="height:400px;max-height:250px;">Maximum</p>
        <p style="height:400px;padding-top:12px;">Padded</p>
        <p style="height:400px;break-inside:avoid;">Kept</p>
        <section><p style="height:400px;">Nested</p></section>
        <p style="height:400px;border:solid 1px black;">Border</p>
        <p style="height:400px;display:none;">Hidden</p>
        <p style="height:200%;">Relative</p></body></html>"#
            .to_string(),
    );
    let mut plain = specimen(settings(HEIGHT, false), unsupported.clone());
    let mut web = specimen(settings(HEIGHT, true), unsupported);
    assert_eq!(plain.page_count(), web.page_count());
    for page in 0..plain.page_count() {
        assert!(
            plain.capture(page).unwrap() == web.capture(page).unwrap(),
            "unsupported geometry changed page {page}"
        );
    }
    let huge = specimen(
        settings(COMPACT_HEIGHT, true),
        Some(
            "<html><body><p style='height:900000px'>Bounded</p><p>Still readable</p></body></html>"
                .into(),
        ),
    );
    let Renderer::Parley(layout) = &huge.renderer else {
        panic!("unexpected provider")
    };
    let [space] = layout.publisher_height_spaces() else {
        panic!("expected bounded fill")
    };
    assert!(space.end - space.start <= huge.settings.text_height() as f32);
    assert_eq!(huge.page_count(), 2);
    assert_eq!(huge.matches("Still readable").len(), 1);
}

#[test]
fn publisher_text_height_resolves_physical_units_and_leaves_active_float_flow_alone() {
    let document = r#"<html><head><style>
        body, p, div { margin:0; padding:0; text-indent:0; }
        .card { float:left; width:200px; height:3em; }
        </style></head><body>
        <p style="height:2in;background:#e0e0e0;">Physical height</p>
        <div class="card"><p>Float card</p></div>
        <p style="height:250px;">Real text currently beside a float</p>
        </body></html>"#;
    let mut probe = specimen(
        Settings {
            float_mode: crate::FloatMode::PublisherHeight,
            render_dpi: 144,
            ..settings(HEIGHT, true)
        },
        Some(document.into()),
    );
    let Renderer::Parley(renderer) = &probe.renderer else {
        panic!("unexpected provider")
    };
    assert!(renderer.uses_float_geometry());
    assert_eq!(renderer.publisher_height_spaces().len(), 1);
    let page = probe.capture(0).unwrap();
    let (top, bottom) = band(&page, 1209, 224);
    assert_eq!(bottom - top, 288);
}

fn capture(name: &str, frame: &[u8], height: u32) {
    if let Some(folder) = std::env::var_os("READER_WEB_HEIGHT_CAPTURES") {
        image::save_buffer(
            std::path::Path::new(&folder).join(name),
            frame,
            WIDTH,
            height,
            image::ColorType::L8,
        )
        .unwrap();
    }
}
