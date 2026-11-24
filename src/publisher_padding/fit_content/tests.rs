use crate::{
    Anchor, BlockWidthMode, Book, Engine, FitContentMode, Probe, Settings,
    selection::TextSelection, test_fixture,
};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn open(settings: Settings, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-fit-content.epub")).unwrap(),
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
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
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
fn fit_content_leaves_decorated_boxes_and_sized_ancestors_on_the_normal_path() {
    let ordinary = Settings::default();
    let web = Settings {
        fit_content: FitContentMode::Web,
        ..ordinary
    };
    for body in [
        "<p style='width:fit-content;padding:10px;background:#ddd'>Harbor marker</p>",
        "<p style='width:fit-content;border:5px solid black;background:#ddd'>Harbor marker</p>",
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
