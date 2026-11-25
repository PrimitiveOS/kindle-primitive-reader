use crate::{
    Anchor, BlockWidthMode, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture,
};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn book(mode: BlockWidthMode, height: u32, styles: bool, authored: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-modes.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height,
            font_size: 40.0,
            line_height: 1.20,
            block_width: mode,
            publisher_styles: styles,
            ..Settings::default()
        },
        0,
        authored,
    )
    .unwrap()
}

fn word(probe: &Probe, phrase: &str) -> TextSelection {
    let start = probe.matches(phrase).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.split_whitespace().next().unwrap().len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

fn band(frame: &[u8], shade: u8) -> (usize, usize) {
    let mut best = (0, 0);
    for row in frame.chunks_exact(WIDTH as usize) {
        let mut run = 0;
        for (column, &pixel) in row.iter().enumerate() {
            run = if pixel == shade { run + 1 } else { 0 };
            if run > best.1 {
                best = (column + 1 - run, run);
            }
        }
    }
    best
}

fn exact_frames(mut first: Probe, mut second: Probe) {
    assert_eq!(first.page_count(), second.page_count());
    for page in 0..first.page_count() {
        assert!(
            first.capture(page).unwrap() == second.capture(page).unwrap(),
            "page {page} differs"
        );
    }
}

fn capture(name: &str, frame: &[u8], height: u32) {
    if let Some(folder) = std::env::var_os("READER_PUBLISHER_WRAPPER_CAPTURES") {
        image::save_buffer(
            PathBuf::from(folder).join(name),
            frame,
            WIDTH,
            height,
            image::ColorType::L8,
        )
        .unwrap();
    }
}

#[test]
fn publisher_wrappers_match_pinned_web_band_and_move_existing_source_text() {
    use BlockWidthMode::{Ignore, Publisher, PublisherWithWrappers};
    assert_eq!(Settings::default().block_width, Ignore);
    let mut regular = book(Ignore, HEIGHT, true, None);
    let mut paragraphs = book(Publisher, HEIGHT, true, None);
    let mut wrappers = book(PublisherWithWrappers, HEIGHT, true, None);
    assert_eq!(regular.coverage(), wrappers.coverage());
    let phrase = "Centered container marker";
    let initial = word(&regular, phrase);
    let saved = serde_json::to_vec(&initial).unwrap();
    let restored: TextSelection = serde_json::from_slice(&saved).unwrap();

    for probe in [&regular, &paragraphs] {
        let page = probe.page_for(&restored.start).unwrap();
        assert_eq!(probe.selection_rects(page, &restored)[0].x, 54);
    }
    let page = wrappers.page_for(&restored.start).unwrap();
    assert_eq!(page, 0);
    assert_eq!(
        wrappers.select_range(&restored.start, &restored.end),
        Some(restored.clone())
    );
    let text = wrappers.selection_rects(page, &restored)[0];
    assert_eq!(text.x, 389);
    assert_eq!(
        wrappers.word_at(
            page,
            text.x as f32 + text.width as f32 * 0.5,
            text.y as f32 + text.height as f32 * 0.5,
        ),
        Some(restored.clone())
    );
    assert_eq!(
        book(PublisherWithWrappers, HEIGHT, true, None).matches(phrase)[0],
        restored.start
    );

    let baseline = regular.capture(page).unwrap();
    let paragraph = paragraphs.capture(page).unwrap();
    let wrapper = wrappers.capture(page).unwrap();
    assert!(band(&baseline, 200).1 < 32);
    assert!(band(&paragraph, 200).1 < 32);
    assert_eq!(band(&wrapper, 200), (389, 485));
    assert_eq!(band(&paragraph, 230), (389, 485));
    assert_eq!(band(&wrapper, 230), (389, 485));
    capture("reader-default.png", &baseline, HEIGHT);
    capture("reader-web-wrappers.png", &wrapper, HEIGHT);
}

#[test]
fn publisher_wrappers_paint_wrapped_children_across_pages_and_ignore_suppressed_children() {
    use BlockWidthMode::{Publisher, PublisherWithWrappers};
    const SHORT_HEIGHT: u32 = 516;
    let hidden = "<p style='display:none'>Hidden paragraph with no rendered extent.</p>";
    let authored = format!(
        "<html><head><style>body,article,p{{margin:0;padding:0;text-indent:0}}</style></head>         <body><p>Outside opening</p>         <article style='width:42%;margin:0 auto;background:#c8c8c8'>         <p>Opening shoreline records continue into the same narrow column while the tide          passes the inlet during the afternoon. Repeated soundings from the dock show where          the submerged stones approach the surface. Pause here to mark the north bank.</p>         {hidden}<p>Closing passage follows the survey through the second bend and records          the deep channel at low water.</p>         </article><p>Outside ending</p></body></html>"
    );
    let narrow = book(
        PublisherWithWrappers,
        SHORT_HEIGHT,
        true,
        Some(authored.clone()),
    );
    let ordinary = book(Publisher, SHORT_HEIGHT, true, Some(authored.clone()));
    let start = narrow.matches("Opening shoreline").remove(0);
    let stop = narrow.matches("Closing passage").remove(0);
    let narrow_start = narrow.page_for(&start).unwrap();
    let narrow_stop = narrow.page_for(&stop).unwrap();
    assert!(narrow_stop > narrow_start);
    assert!(ordinary.page_for(&stop).unwrap() < narrow_stop);

    let mut without_hidden = book(
        PublisherWithWrappers,
        SHORT_HEIGHT,
        true,
        Some(authored.replace(hidden, "")),
    );
    let mut narrow = narrow;
    assert_eq!(narrow.page_count(), without_hidden.page_count());
    for page in 0..narrow.page_count() {
        assert!(narrow.capture(page).unwrap() == without_hidden.capture(page).unwrap());
    }
    for page in [narrow_start, narrow_stop] {
        let frame = narrow.capture(page).unwrap();
        assert_eq!(band(&frame, 200), (389, 485));
    }
    let outside = word(&narrow, "Outside ending");
    let page = narrow.page_for(&outside.start).unwrap();
    assert_eq!(narrow.selection_rects(page, &outside)[0].x, 54);
    let after = narrow.capture(narrow_stop).unwrap();
    capture("reader-continuation.png", &after, SHORT_HEIGHT);
}

#[test]
fn publisher_wrappers_keep_prior_frames_for_unsupported_boxes_and_styles_off() {
    use BlockWidthMode::{Ignore, PublisherWithWrappers};
    exact_frames(
        book(Ignore, HEIGHT, false, None),
        book(PublisherWithWrappers, HEIGHT, false, None),
    );
    for (style, body) in [
        ("width:150%;", "<p>Oversized width</p>"),
        ("width:1px;", "<p>Pinched width</p>"),
        (
            "width:42%;border-top:2px solid black;",
            "<p>Decorated edge</p>",
        ),
        ("width:42%;height:200px;", "<p>Sized height</p>"),
        ("width:42%;", "<section><p>Nested wrapper</p></section>"),
        (
            "width:42%;",
            "Direct text outside the paragraph<p>Later paragraph</p>",
        ),
        (
            "width:42%;",
            "<p><img src='missing.png' alt='Image'/>Image row</p>",
        ),
        (
            "width:42%;",
            "<p>Ordinary row</p><p style='width:260px'>Own width</p>",
        ),
    ] {
        let authored = format!(
            "<html><body><p>Before</p><div style='{style}margin:0 auto;background:#c8c8c8'>             {body}</div><p>After</p></body></html>"
        );
        exact_frames(
            book(Ignore, HEIGHT, true, Some(authored.clone())),
            book(PublisherWithWrappers, HEIGHT, true, Some(authored)),
        );
    }
}
