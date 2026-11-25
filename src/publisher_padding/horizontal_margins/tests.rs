use crate::{Anchor, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture};
use std::path::{Path, PathBuf};

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn spec(settings: Settings, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-horizontal-margins.epub")).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            ..settings
        },
        0,
        document,
    )
    .unwrap()
}

fn quote(probe: &Probe, phrase: &str) -> TextSelection {
    let start = probe.matches(phrase).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.split_whitespace().next().unwrap().len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

fn point(probe: &Probe, quote: &TextSelection) -> (usize, crate::selection::HighlightRect) {
    let page = probe.page_for(&quote.start).unwrap();
    (page, probe.selection_rects(page, quote)[0])
}

fn band(frame: &[u8], shade: u8) -> (usize, usize) {
    let mut best = (0, 0);
    for row in frame.chunks_exact(WIDTH as usize) {
        let mut length = 0;
        for (x, &pixel) in row.iter().enumerate() {
            length = if pixel == shade { length + 1 } else { 0 };
            if length > best.1 {
                best = (x + 1 - length, length);
            }
        }
    }
    best
}

fn capture(name: &str, frame: &[u8]) {
    if let Some(folder) = std::env::var_os("READER_HORIZONTAL_MARGIN_CAPTURES") {
        image::save_buffer(
            PathBuf::from(folder).join(name),
            frame,
            WIDTH,
            HEIGHT,
            image::ColorType::L8,
        )
        .unwrap();
    }
}

#[test]
fn publisher_horizontal_margins_match_pinned_word_origins_and_bands() {
    let mut regular = spec(Settings::default(), None);
    let mut inset = spec(
        Settings {
            publisher_horizontal_margins: true,
            ..Settings::default()
        },
        None,
    );
    assert!(!Settings::default().publisher_horizontal_margins);
    assert_eq!(regular.coverage(), inset.coverage());
    let regular_frame = regular.capture(0).unwrap();
    let inset_frame = inset.capture(0).unwrap();
    for (label, shade, x, width) in [
        ("Physical sides marker", 221, 94, 1092),
        ("Font and percent marker", 204, 109, 1078),
        ("Minimum inset marker", 187, 55, 1155),
    ] {
        assert_eq!(band(&regular_frame, shade), (54, 1156));
        assert_eq!(band(&inset_frame, shade), (x, width));
        let saved = serde_json::to_vec(&quote(&regular, label)).unwrap();
        let saved: TextSelection = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            inset.select_range(&saved.start, &saved.end),
            Some(saved.clone())
        );
        assert_eq!(point(&regular, &saved).1.x, 54);
        let (page, rect) = point(&inset, &saved);
        assert_eq!(rect.x, x as i32);
        assert_eq!(
            inset.word_at(
                page,
                rect.x as f32 + rect.width as f32 / 2.0,
                rect.y as f32 + rect.height as f32 / 2.0,
            ),
            Some(saved)
        );
    }
    assert_eq!(
        point(&inset, &quote(&inset, "Following ordinary text")).1.x,
        54
    );
    capture("reader-default.png", &regular_frame);
    capture("reader-horizontal-margins.png", &inset_frame);
}

#[test]
fn publisher_horizontal_margins_change_wrapping_and_link_targets() {
    let content = "<html><head><style>body,p{margin:0;padding:0;text-indent:0}</style></head><body>        <p style='margin:0 200px;background:#ddd'>The channel runs between the reeds and the low         bank. Six boats arrived before the evening wind and waited on the turning tide. The last         sounding was taken by the post with the lamp. <a href='#end'>Follow the final note</a>.</p>        <p id='end'>End of the short survey</p></body></html>";
    let before = spec(Settings::default(), Some(content.into()));
    let mut after = spec(
        Settings {
            publisher_horizontal_margins: true,
            ..Settings::default()
        },
        Some(content.into()),
    );
    let selection = quote(&before, "Follow the final note");
    let (page, old) = point(&before, &selection);
    let (new_page, new) = point(&after, &selection);
    assert_eq!(page, new_page);
    assert!(new.y > old.y);
    let tap = |probe: &Probe, rect: crate::selection::HighlightRect| {
        probe.link_at(
            page,
            rect.x as f32 + 3.0,
            rect.y as f32 + rect.height as f32 / 2.0,
        )
    };
    let link = tap(&before, old).unwrap();
    assert_eq!(tap(&after, new), Some(link));
    assert_eq!(band(&after.capture(page).unwrap(), 221), (254, 756));
}

fn same_frames(mut regular: Probe, mut inset: Probe) {
    assert_eq!(regular.page_count(), inset.page_count());
    for page in 0..regular.page_count() {
        assert!(regular.capture(page).unwrap() == inset.capture(page).unwrap());
    }
}

#[test]
fn publisher_horizontal_margins_bound_fallback_and_resolve_scaled_lengths() {
    let regular = Settings::default();
    let inset = Settings {
        publisher_horizontal_margins: true,
        ..regular
    };
    same_frames(
        spec(
            Settings {
                publisher_styles: false,
                ..regular
            },
            None,
        ),
        spec(
            Settings {
                publisher_styles: false,
                ..inset
            },
            None,
        ),
    );
    for attributes in [
        "margin:0 700px",
        "margin-left:50px;margin-right:-10px",
        "margin-left:50px;width:400px",
        "margin-left:50px;max-width:500px",
        "margin-left:50px;padding-left:12px",
        "margin-left:50px;border-left:3px solid black",
    ] {
        let document = format!(
            "<html><body><p style='{attributes};background:#ddd'>Fallback phrase</p></body></html>"
        );
        same_frames(
            spec(regular, Some(document.clone())),
            spec(inset, Some(document)),
        );
    }

    let mut scaled = spec(
        Settings {
            render_dpi: 144,
            ..inset
        },
        None,
    );
    assert_eq!(band(&scaled.capture(0).unwrap(), 221), (114, 1060));
    let dom = "<html style='font-size:125%'><body>
        <p style='font-size:200%;margin-left:1em;margin-right:1rem;background:#ddd'>Font relative margins</p>
        </body></html>";
    let mut relative = spec(inset, Some(dom.into()));
    assert_eq!(band(&relative.capture(0).unwrap(), 221), (154, 1006));
}

#[test]
#[ignore = "Requires an authorized local original EPUB and a private text manifest"]
fn original_publisher_horizontal_margins_retain_quotes_and_links() {
    let source = std::env::var_os("READER_HORIZONTAL_MARGIN_ORIGINAL").unwrap();
    let cases =
        std::fs::read_to_string(std::env::var_os("READER_HORIZONTAL_MARGIN_CASES").unwrap())
            .unwrap();
    let fonts = [test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut checks = 0;
    for (index, row) in cases.lines().enumerate() {
        let mut parts = row.splitn(3, '\t');
        let chapter = parts.next().unwrap().parse::<usize>().unwrap() - 1;
        let dx = parts.next().unwrap().parse::<i32>().unwrap();
        let phrase = parts.next().unwrap();
        let open = |publisher_horizontal_margins| {
            Probe::open(
                Path::new(&source),
                Engine::Parley,
                &fonts,
                Settings {
                    width: WIDTH,
                    height: HEIGHT,
                    font_size: 40.0,
                    line_height: 1.20,
                    publisher_horizontal_margins,
                    ..Default::default()
                },
                chapter,
            )
            .unwrap()
        };
        let regular = open(false);
        let inset = open(true);
        let selection = quote(&regular, phrase);
        assert!(
            inset
                .select_range(&selection.start, &selection.end)
                .is_some()
        );
        let (old_page, old) = point(&regular, &selection);
        let (new_page, new) = point(&inset, &selection);
        assert_eq!(new.x - old.x, dx, "case {}", index + 1);
        let tap = |probe: &Probe, page, rect: crate::selection::HighlightRect| {
            probe.link_at(
                page,
                rect.x as f32 + 3.0,
                rect.y as f32 + rect.height as f32 / 2.0,
            )
        };
        assert_eq!(tap(&regular, old_page, old), tap(&inset, new_page, new));
        checks += 1;
        println!(
            "private margin case {} shift={} source-restored=true",
            index + 1,
            dx
        );
    }
    assert!(checks > 0);
}
