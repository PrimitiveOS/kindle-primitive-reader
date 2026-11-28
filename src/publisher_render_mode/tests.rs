use super::*;
use crate::{Anchor, Engine, Probe, selection::HighlightRect, test_fixture};
use std::path::Path;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;
const MODES: [PublisherRenderMode; 4] = [
    PublisherRenderMode::Legacy,
    PublisherRenderMode::Flat,
    PublisherRenderMode::Book,
    PublisherRenderMode::Web,
];

fn settings() -> Settings {
    Settings {
        width: WIDTH,
        height: HEIGHT,
        font_size: 40.0,
        line_height: 1.20,
        ..Settings::default()
    }
}

fn open(book: &str, chapter: usize, settings: Settings) -> Probe {
    Probe::open(
        &test_fixture(book),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        settings,
        chapter,
    )
    .unwrap()
}

fn mark(probe: &Probe, phrase: &str) -> (Anchor, usize, HighlightRect) {
    let start = probe.matches(phrase).remove(0);
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    let selection = probe.select_range(&start, &end).unwrap();
    assert_eq!(selection.text, phrase);
    let page = probe.page_for(&start).unwrap();
    let ink = probe.selection_rects(page, &selection).remove(0);
    (start, page, ink)
}

fn gray_band(frame: &[u8], tone: u8) -> (usize, usize) {
    let mut result = (0, 0);
    for row in frame.chunks_exact(WIDTH as usize) {
        let mut length = 0;
        for (x, pixel) in row.iter().copied().enumerate() {
            if pixel == tone {
                length += 1;
                if length > result.1 {
                    result = (x + 1 - length, length);
                }
            } else {
                length = 0;
            }
        }
    }
    result
}

fn capture(mode: PublisherRenderMode, part: &str, frame: &[u8]) {
    if let Some(directory) = std::env::var_os("READER_RENDER_MODE_CAPTURES") {
        let name = format!("render-composition-reader-{mode:?}-{part}.png").to_lowercase();
        image::save_buffer(
            Path::new(&directory).join(name),
            frame,
            WIDTH,
            HEIGHT,
            image::ColorType::L8,
        )
        .unwrap();
    }
}

fn check_composed_mode(
    mode: PublisherRenderMode,
    expected_pitch: [i32; 3],
    expected_breaks: usize,
) {
    let book = "render-mode-composition.epub";
    let mut view = open(book, 0, mode.apply(settings()));
    for (before, after, expected) in [
        (
            "Before joined margins",
            "After joined margins",
            expected_pitch[0],
        ),
        (
            "Before negative margins",
            "After negative margins",
            expected_pitch[1],
        ),
        (
            "Before net negative gap",
            "After net negative gap",
            expected_pitch[2],
        ),
    ] {
        let (_, p1, head) = mark(&view, before);
        let (_, p2, tail) = mark(&view, after);
        assert_eq!(p1, p2, "{mode:?} {after}");
        assert!(tail.y.abs_diff(head.y + expected) <= 1, "{mode:?} {after}");
    }
    let (neighbor, page, prose) = mark(&view, "Neighbor starts here");
    let (_, card_page, card) = mark(&view, "Floating card marker");
    assert_eq!(page, card_page);
    if matches!(mode, PublisherRenderMode::Book | PublisherRenderMode::Web) {
        assert_eq!((prose.x - card.x, prose.y), (416, card.y), "{mode:?}");
    } else {
        assert_eq!(prose.x, card.x, "{mode:?}");
        assert!(prose.y > card.y);
    }
    let baseline = open(book, 0, settings());
    assert_eq!(
        mark(&baseline, "Neighbor starts here").0,
        neighbor,
        "{mode:?}"
    );

    let (_, table_page, first) = mark(&view, "First table cell");
    let (_, second_page, second) = mark(&view, "Second table cell");
    assert_eq!(table_page, second_page);
    if mode == PublisherRenderMode::Legacy {
        // Reader retains selectable orphan cells as ordinary text.
        assert!(second.y > first.y);
    } else {
        assert_eq!(second.y, first.y, "{mode:?}");
        assert!(second.x > first.x);
    }

    let (_, inline_page, inline) = mark(&view, "Inside inline cluster");
    let (_, following_page, following) = mark(&view, "after inline cluster");
    assert_eq!(inline_page, following_page);
    assert_eq!(inline.y, following.y);
    match mode {
        PublisherRenderMode::Legacy => assert!(following.x - inline.x < 390),
        _ => assert!(
            (485..=506).contains(&(following.x - inline.x)),
            "{mode:?}: inline text advance {}",
            following.x - inline.x
        ),
    }

    let (_, link_page, linked) = mark(&view, "After pixel box marker");
    let link = view
        .link_at(
            link_page,
            (linked.x + 6) as f32,
            (linked.y + linked.height / 2) as f32,
        )
        .unwrap();
    assert_eq!(link.fragment.as_deref(), Some("source-target"));
    if mode == PublisherRenderMode::Web {
        assert_eq!(linked.x, 34);
    } else {
        assert_eq!(linked.x, 54);
    }

    let (_, first_page, _) = mark(&view, "Centered box marker");
    let first_frame = view.capture(first_page).unwrap();
    if mode == PublisherRenderMode::Web {
        assert_eq!(gray_band(&first_frame, 230), (389, 485), "{mode:?}");
        assert_eq!(gray_band(&first_frame, 200), (389, 485), "{mode:?}");
        assert_eq!(gray_band(&first_frame, 221), (54, 260), "{mode:?}");
    } else {
        assert_eq!(gray_band(&first_frame, 230), (54, 1156), "{mode:?}");
        assert_eq!(gray_band(&first_frame, 221), (54, 1156), "{mode:?}");
    }
    capture(mode, "main", &first_frame);

    let mut breaks = open(book, 1, mode.apply(settings()));
    let (_, before, _) = mark(&breaks, "Before empty breaks");
    let (_, after, _) = mark(&breaks, "After empty breaks");
    let (_, ordinary, _) = mark(&breaks, "After nonempty break");
    assert_eq!(after - before, expected_breaks, "{mode:?}");
    assert_eq!(ordinary - after, 1, "{mode:?}");
    if matches!(mode, PublisherRenderMode::Legacy | PublisherRenderMode::Web) {
        let intermediate = breaks.capture(before + 1).unwrap();
        assert!(intermediate.iter().any(|&pixel| pixel < 245), "{mode:?}");
        capture(mode, "break", &intermediate);
    }
}

#[test]
fn render_modes_compose_legacy_geometry() {
    check_composed_mode(PublisherRenderMode::Legacy, [188, 140, 75], 3);
}

#[test]
fn render_modes_compose_flat_geometry() {
    check_composed_mode(PublisherRenderMode::Flat, [148, 107, 68], 1);
}

#[test]
fn render_modes_compose_book_geometry() {
    check_composed_mode(PublisherRenderMode::Book, [148, 107, 68], 1);
}

#[test]
fn render_modes_compose_web_geometry() {
    check_composed_mode(PublisherRenderMode::Web, [148, 107, 50], 3);
}

#[test]
fn render_modes_keep_independent_settings_and_avoid_changing_plain_frames() {
    let plain = Settings {
        publisher_styles: false,
        ..settings()
    };
    let mut baseline = open("render-mode-composition.epub", 0, plain);
    let original = baseline.capture(0).unwrap();
    for mode in MODES {
        let flags = mode.apply(plain);
        assert_eq!(flags.render_dpi, plain.render_dpi);
        assert_eq!(flags.typography, plain.typography);
        assert_eq!(flags.in_page_footnotes, plain.in_page_footnotes);
        assert!(!flags.publisher_styles);
        assert_eq!(mode.apply(flags), flags);
        assert_eq!(
            open("render-mode-composition.epub", 0, flags)
                .capture(0)
                .unwrap(),
            original,
            "{mode:?}"
        );
    }
}

#[cfg(feature = "kindle")]
#[test]
fn render_mode_switch_preserves_the_focused_quote_and_returns_to_the_original_frame() {
    use crate::session::{BookConfig, Reading, ReadingViewport};
    let initial = PublisherRenderMode::Web.apply(settings());
    let book = "render-mode-composition.epub";
    let original = open(book, 0, initial);
    let (anchor, page, rect) = mark(&original, "Neighbor starts here");
    let config = BookConfig {
        viewport: ReadingViewport::PORTRAIT,
        book: test_fixture(book),
        fonts: vec![test_fixture("fonts/NotoSans-Regular.ttf")],
        font_roles: Default::default(),
        engine: Engine::Parley,
        settings: initial,
        scroll: false,
        facing_pages: false,
        hide_nonlinear: false,
        user_styles: Default::default(),
        user_hyphenation: Default::default(),
        chapter_offsets: Default::default(),
        quote_prefixes: Default::default(),
        screen_pages: Default::default(),
        eager_layouts: Default::default(),
        folder_snapshot: None,
        hidden_flows: Default::default(),
    };
    let mut live = Reading::restore(config, 0, page, Some(&anchor)).unwrap();
    let saved = serde_json::to_vec(&live.anchor().unwrap()).unwrap();
    let pixels = live.probe.capture(live.page).unwrap();
    for mode in MODES {
        live.reflow(mode.apply(live.config.settings)).unwrap();
        let restored = live.anchor().unwrap();
        assert_eq!(serde_json::to_vec(&restored).unwrap(), saved, "{mode:?}");
        let (_, page, mark) = mark(&live.probe, "Neighbor starts here");
        assert_eq!(live.page, page);
        assert_eq!(
            mark.x,
            if matches!(mode, PublisherRenderMode::Book | PublisherRenderMode::Web) {
                470
            } else {
                54
            }
        );
    }
    assert_eq!(mark(&live.probe, "Neighbor starts here").2, rect);
    assert_eq!(live.probe.capture(live.page).unwrap(), pixels);
}
