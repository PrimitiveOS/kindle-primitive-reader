use crate::{
    Anchor, BlockWidthMode, Book, Engine, FitContentMode, Probe, Settings,
    selection::TextSelection, test_fixture,
};
use std::path::PathBuf;

mod height;
mod vertical;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn open(settings: Settings, document: Option<String>) -> Probe {
    open_fixture("render-fit-content.epub", settings, document)
}

fn open_fixture(fixture: &str, settings: Settings, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture(fixture)).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            render_dpi: 96,
            ..settings
        },
        0,
        document,
    )
    .unwrap()
}

fn quote(probe: &Probe, phrase: &str) -> TextSelection {
    let start = probe.matches(phrase).pop().unwrap();
    let end = Anchor {
        raw_caret: None,
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

fn band(frame: &[u8], shade: u8) -> (usize, usize) {
    band_colors(frame, &[shade])
}

fn band_colors(frame: &[u8], shades: &[u8]) -> (usize, usize) {
    let mut best = (0, 0);
    for row in frame.chunks_exact(WIDTH as usize) {
        let mut length = 0;
        for (x, &pixel) in row.iter().enumerate() {
            length = if shades.contains(&pixel) {
                length + 1
            } else {
                0
            };
            if length > best.1 {
                best = (x + 1 - length, length);
            }
        }
    }
    best
}

fn capture(name: &str, frame: &[u8]) {
    if let Some(folder) = std::env::var_os("READER_FIT_CONTENT_CAPTURES") {
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
fn fit_content_resolves_book_and_web_widths_and_preserves_quotes() {
    let ordinary = Settings::default();
    let mut default = open(ordinary, None);
    capture(
        "fit-content-reader-default.png",
        &default.capture(0).unwrap(),
    );

    for (mode, name) in [(FitContentMode::Book, "book"), (FitContentMode::Web, "web")] {
        let web = mode == FitContentMode::Web;
        let mut fitted = open(
            Settings {
                fit_content: mode,
                ..ordinary
            },
            None,
        );
        capture(
            &format!("fit-content-reader-{name}.png"),
            &fitted.capture(0).unwrap(),
        );
        for (phrase, shade, book, web_band) in [
            ("Shore marker", 221, (54, 258), (54, 258)),
            ("Harbor marker", 211, (491, 282), (491, 282)),
            ("Anchor marker", 201, (871, 283), (871, 283)),
            ("Beacon marker", 191, (557, 150), (557, 150)),
            ("Needle marker", 181, (422, 420), (422, 420)),
            ("Sounding marker", 141, (54, 328), (30, 328)),
            ("Current marker", 131, (74, 1124), (74, 1124)),
            ("Ledger marker", 121, (491, 281), (459, 346)),
            ("Quartz marker", 111, (54, 275), (30, 1234)),
            ("Turn marker", 81, (974, 236), (974, 236)),
        ] {
            let saved = quote(&default, phrase);
            assert_eq!(
                fitted.select_range(&saved.start, &saved.end),
                Some(saved.clone()),
                "{name} {phrase}"
            );
            let page = fitted.page_for(&saved.start).unwrap();
            let measured = band(&fitted.capture(page).unwrap(), shade);
            let native = if web { web_band } else { book };
            let font_tolerance = match (phrase, mode) {
                ("Shore marker" | "Turn marker", _)
                | ("Ledger marker" | "Quartz marker", FitContentMode::Book) => 2,
                _ => 0,
            };
            assert!(
                measured.0.abs_diff(native.0) <= font_tolerance
                    && measured.1.abs_diff(native.1) <= font_tolerance,
                "{name} {phrase}: Reader {measured:?}, stock {native:?}"
            );
            if phrase == "Sounding marker" {
                let word = quote(&fitted, "Sounding");
                let rect = fitted.selection_rects(page, &word)[0];
                let x = if web { 42.0 } else { 66.0 };
                let y = (rect.y + rect.height / 2) as f32;
                assert_eq!(fitted.word_at(page, x, y), Some(word));
                assert_eq!(
                    fitted.link_at(page, x, y).unwrap().fragment.as_deref(),
                    Some("channel")
                );
                assert!(fitted.link_at(page, -1.0, y).is_none());
            }
        }
    }
}

#[test]
fn fit_content_keeps_earlier_valid_width_with_policy_disabled() {
    let document = "<html><body><p style='width:320px;width:fit-content;background:#ddd;text-indent:0'>Fallback</p></body></html>";
    let defaults = Settings {
        block_width: BlockWidthMode::PublisherWithConstraints,
        ..Settings::default()
    };
    let mut fallback = open(defaults, Some(document.into()));
    assert_eq!(band(&fallback.capture(0).unwrap(), 221), (54, 320));
    let mut fitted = open(
        Settings {
            fit_content: FitContentMode::Web,
            ..defaults
        },
        Some(document.into()),
    );
    let fitted_band = band(&fitted.capture(0).unwrap(), 221);
    assert_eq!(fitted_band.0, 54);
    assert!((120..320).contains(&fitted_band.1));
    let mut styles_off = open(
        Settings {
            publisher_styles: false,
            ..defaults
        },
        None,
    );
    let mut styles_off_web = open(
        Settings {
            publisher_styles: false,
            fit_content: FitContentMode::Web,
            ..defaults
        },
        None,
    );
    assert_eq!(styles_off.page_count(), styles_off_web.page_count());
    for page in 0..styles_off.page_count() {
        assert!(styles_off.capture(page).unwrap() == styles_off_web.capture(page).unwrap());
    }
}

#[test]
fn fit_content_leaves_unsupported_edges_and_sized_ancestors_on_the_normal_path() {
    let ordinary = Settings::default();
    let web = Settings {
        fit_content: FitContentMode::Web,
        ..ordinary
    };
    for body in [
        "<p style='width:fit-content;padding-top:500px;background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;border:5px dashed black;background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;border:5px solid rgba(0,0,0,.5);background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;padding-left:100%;background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;max-width:1px;background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;text-indent:-20px;background:#ddd'>Harbor marker</p>",
        "<section style='width:50%'><p style='width:fit-content;background:#ddd'>Harbor marker</p></section>",
        "<section style='border:5px solid black'><p style='width:fit-content;background:#ddd'>Harbor marker</p></section>",
    ] {
        let document = format!("<html><body>{body}</body></html>");
        let mut regular = open(ordinary, Some(document.clone()));
        let mut fitted = open(web, Some(document));
        assert_eq!(regular.page_count(), fitted.page_count(), "{body}");
        for page in 0..regular.page_count() {
            assert!(
                regular.capture(page).unwrap() == fitted.capture(page).unwrap(),
                "{body}"
            );
        }
    }
}

#[test]
fn fit_content_decorations_keep_default_frames_and_align_strokes_during_scroll() {
    let document = "<html><body><p style='width:fit-content;box-sizing:border-box;max-width:420px;padding-left:20px;border:4px solid #444;background:#ddd'>Framed passage</p></body></html>";
    let mut baseline = open(
        Settings::default(),
        Some(document.replace("width:fit-content;box-sizing:border-box;", "")),
    );
    let mut preserved = open(Settings::default(), Some(document.into()));
    assert!(baseline.capture(0).unwrap() == preserved.capture(0).unwrap());

    let mut fitted = open_fixture(
        "render-fit-decorated.epub",
        Settings {
            fit_content: FitContentMode::Web,
            ..Settings::default()
        },
        None,
    );
    let key = quote(&fitted, "Beacon marker").start.block;
    let cfg = fitted.settings;
    let crate::Renderer::Parley(layout) = &mut fitted.renderer else {
        panic!("Expected Parley")
    };
    let paragraph = layout
        .paragraphs_in(0.0..1500.0)
        .find(|paragraph| paragraph.key == key)
        .unwrap();
    let outer_top = paragraph.y - 4.0;
    let scroll_top = (outer_top.fract() / 2.0).max(0.0625);
    let expected = cfg.top_margin() as usize + (outer_top - scroll_top).floor() as usize;
    let frame = layout
        .capture(scroll_top..scroll_top + 1400.0, cfg)
        .unwrap();
    let rows = |y| &frame[y * WIDTH as usize + 766..y * WIDTH as usize + 1186];
    assert!((expected..expected + 4).all(|y| rows(y).iter().all(|&gray| gray == 68)));
    assert!(rows(expected - 1).iter().any(|&gray| gray != 68));
    assert!(rows(expected + 4).iter().any(|&gray| gray != 68));
}

#[test]
fn fit_content_decorations_match_stock_outer_bands_inner_hits_and_constraints() {
    let fixture = "render-fit-decorated.epub";
    let mut default = open_fixture(fixture, Settings::default(), None);
    capture(
        "fit-decoration-reader-default.png",
        &default.capture(0).unwrap(),
    );
    for (mode, label) in [(FitContentMode::Book, "book"), (FitContentMode::Web, "web")] {
        let web = mode == FitContentMode::Web;
        let mut fitted = open_fixture(
            fixture,
            Settings {
                fit_content: mode,
                ..Settings::default()
            },
            None,
        );
        capture(
            &format!("fit-decoration-reader-{label}.png"),
            &fitted.capture(0).unwrap(),
        );
        for (phrase, shades, book, web_case, tolerance) in [
            (
                "Compass marker",
                &[221][..],
                (441, 382, 461),
                (441, 382, 461),
                2,
            ),
            (
                "Channel marker",
                &[211],
                (516, 232, 536),
                (532, 200, 552),
                0,
            ),
            ("Ripple marker", &[201], (532, 200, 552), (548, 168, 568), 0),
            (
                "Measure marker",
                &[191],
                (416, 432, 473),
                (416, 432, 473),
                2,
            ),
            (
                "Boundary marker",
                &[181, 51, 85],
                (54, 409, 77),
                (30, 409, 53),
                2,
            ),
            (
                "Ledger marker",
                &[171],
                (471, 321, 483),
                (449, 366, 461),
                if web { 0 } else { 2 },
            ),
            (
                "Beacon marker",
                &[161, 68],
                (766, 420, 778),
                (766, 420, 778),
                0,
            ),
            ("Wide marker", &[151], (54, 1156, 73), (54, 1188, 73), 1),
        ] {
            let saved = quote(&default, phrase);
            assert_eq!(
                fitted.select_range(&saved.start, &saved.end),
                Some(saved.clone()),
                "{label} {phrase}"
            );
            let page = fitted.page_for(&saved.start).unwrap();
            let frame = fitted.capture(page).unwrap();
            let observed = band_colors(&frame, shades);
            let (left, width, word_x) = if web { web_case } else { book };
            assert!(
                observed.0.abs_diff(left) <= tolerance && observed.1.abs_diff(width) <= tolerance,
                "{label} {phrase}: {observed:?}, expected ({left}, {width})"
            );
            let first = fitted.selection_rects(page, &saved)[0];
            assert!(
                (first.x as usize).abs_diff(word_x) <= tolerance,
                "{label} {phrase}: first selection x={}, expected {word_x}",
                first.x
            );
            if phrase == "Boundary marker" {
                let y = (first.y + first.height / 2) as usize;
                let row = &frame[y * WIDTH as usize..(y + 1) * WIDTH as usize];
                assert!(row[left..left + 5].iter().all(|&ink| ink == 51));
                assert!(
                    row[observed.0 + observed.1 - 7..observed.0 + observed.1]
                        .iter()
                        .all(|&ink| ink == 85)
                );
                let y = y as f32;
                assert_eq!(
                    fitted.word_at(page, first.x as f32 + 10.0, y),
                    Some(quote(&fitted, "Boundary"))
                );
                assert_eq!(
                    fitted
                        .link_at(page, first.x as f32 + 10.0, y)
                        .unwrap()
                        .fragment
                        .as_deref(),
                    Some("shore")
                );
                for x in [left as f32 + 2.0, left as f32 + 12.0] {
                    assert!(fitted.word_at(page, x, y).is_none());
                    assert!(fitted.link_at(page, x, y).is_none());
                    assert!(fitted.text_position_at(page, x, y).is_none());
                }
            } else if phrase == "Beacon marker" {
                let full_stroke_rows = frame
                    .chunks_exact(WIDTH as usize)
                    .filter(|row| row[left..left + width].iter().all(|&ink| ink == 68))
                    .count();
                assert_eq!(full_stroke_rows, 8);
            }
        }
    }
}

#[test]
fn fit_content_measures_percentage_indent_against_its_font() {
    let settings = Settings {
        fit_content: FitContentMode::Book,
        ..Settings::default()
    };
    let measured = |indent| {
        let document = format!(
            "<html><body><p style='width:fit-content;text-indent:{indent};background:#ddd;font-size:150%'>Harbor marker</p></body></html>"
        );
        band(&open(settings, Some(document)).capture(0).unwrap(), 221)
    };
    let (left, no_indent) = measured("0");
    assert_eq!(measured("50%"), (left, no_indent + 30));
}
