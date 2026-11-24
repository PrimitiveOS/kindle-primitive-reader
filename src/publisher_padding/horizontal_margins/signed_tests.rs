use crate::{Anchor, Book, Engine, Probe, Settings, selection::TextSelection, test_fixture};
use std::path::PathBuf;

const WIDTH: u32 = 1264;
const HEIGHT: u32 = 1606;

fn open(settings: Settings, document: Option<String>) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture("render-signed-margins.epub")).unwrap(),
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

fn known_quote(probe: &Probe, phrase: &str) -> TextSelection {
    let start = probe.matches(phrase).pop().unwrap();
    let end = Anchor {
        offset: start.offset + phrase.len(),
        ..start.clone()
    };
    probe.select_range(&start, &end).unwrap()
}

fn widest_band(frame: &[u8], shade: u8) -> (usize, usize) {
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
    if let Some(folder) = std::env::var_os("READER_SIGNED_MARGIN_CAPTURES") {
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
fn publisher_signed_margins_match_native_bands_and_restore_visible_links_and_quotes() {
    let regular = Settings::default();
    assert!(!regular.publisher_signed_horizontal_margins);
    let book = Settings {
        publisher_horizontal_margins: true,
        ..regular
    };
    let web = Settings {
        publisher_signed_horizontal_margins: true,
        ..book
    };
    let mut default = open(regular, None);
    let mut clamped = open(book, None);
    let mut inset = open(web, None);
    let ordinary_frame = default.capture(0).unwrap();
    assert!(ordinary_frame == clamped.capture(0).unwrap());
    let inset_frame = inset.capture(0).unwrap();
    for (phrase, shade, left, outer_x, fragment) in [
        ("Ashore", 221, 34, 43, "west"),
        ("wind", 204, 66, 1222, "east"),
    ] {
        assert_eq!(widest_band(&ordinary_frame, shade), (54, 1156));
        assert_eq!(widest_band(&inset_frame, shade), (left, 1164));
        let saved = serde_json::to_string(&known_quote(&default, phrase)).unwrap();
        let saved: TextSelection = serde_json::from_str(&saved).unwrap();
        let page = inset.page_for(&saved.start).unwrap();
        let rect = inset.selection_rects(page, &saved)[0];
        assert!((rect.x..rect.x + rect.width).contains(&outer_x));
        let y = (rect.y + rect.height / 2) as f32;
        assert_eq!(default.link_at(page, outer_x as f32, y), None);
        assert_eq!(
            inset
                .link_at(page, outer_x as f32, y)
                .unwrap()
                .fragment
                .as_deref(),
            Some(fragment)
        );
        assert_eq!(inset.word_at(page, outer_x as f32, y), Some(saved.clone()));
        assert_eq!(
            inset
                .text_position_at(page, outer_x as f32, y)
                .unwrap()
                .block,
            saved.start.block
        );
        assert_eq!(inset.select_range(&saved.start, &saved.end), Some(saved));
        let outer = if outer_x < 54 { 34..54 } else { 1210..1230 };
        assert!((rect.y..rect.y + rect.height).any(|y| {
            outer
                .clone()
                .any(|x| inset_frame[(y as u32 * WIDTH + x) as usize] < 112)
        }));
        for x in [-1.0, 1.0, (WIDTH - 1) as f32, WIDTH as f32, f32::NAN] {
            assert!(inset.word_at(page, x, y).is_none());
            assert!(inset.text_position_at(page, x, y).is_none());
            assert!(inset.link_at(page, x, y).is_none());
        }
    }
    capture("reader-default.png", &ordinary_frame);
    capture("reader-signed-margins.png", &inset_frame);
}

fn same_frames(mut regular: Probe, mut inset: Probe) {
    assert_eq!(regular.page_count(), inset.page_count());
    for page in 0..regular.page_count() {
        assert!(regular.capture(page).unwrap() == inset.capture(page).unwrap());
    }
}

#[test]
fn publisher_signed_margins_leave_unsupported_or_disabled_frames_unchanged() {
    let regular = Settings::default();
    let inset = Settings {
        publisher_signed_horizontal_margins: true,
        ..regular
    };
    same_frames(
        open(
            Settings {
                publisher_styles: false,
                ..regular
            },
            None,
        ),
        open(
            Settings {
                publisher_styles: false,
                ..inset
            },
            None,
        ),
    );
    for css in [
        "margin-left:-55px",
        "margin-right:-55px",
        "margin-left:-1000000px",
        "margin-left:-10px;margin-right:-10px",
        "margin-left:-50px;margin-right:1190px",
        "margin-left:-10px;width:400px",
        "margin-left:-10px;max-width:400px",
        "margin-left:-10px;padding-left:4px",
        "margin-left:-10px;border-left:4px solid black",
        "margin-left:-10px;float:left",
        "margin-left:-10px;clear:both",
    ] {
        let document = format!(
            "<html><body><p style='{css};background:#ddd'>Fallback marker</p></body></html>"
        );
        same_frames(
            open(regular, Some(document.clone())),
            open(inset, Some(document)),
        );
    }
    let document = String::from(
        "<html><body><section><p style='margin-right:-20px;background:#ddd'>Nested marker</p></section></body></html>",
    );
    same_frames(
        open(regular, Some(document.clone())),
        open(inset, Some(document)),
    );
}
