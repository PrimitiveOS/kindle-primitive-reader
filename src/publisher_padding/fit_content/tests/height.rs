use super::*;

const FIXTURE: &str = "render-fit-height.epub";

fn book(mode: FitContentMode, dpi: u16) -> Probe {
    Probe::from_book_content(
        Book::open(&test_fixture(FIXTURE)).unwrap(),
        Engine::Parley,
        &[test_fixture("fonts/NotoSans-Regular.ttf")],
        Settings {
            width: WIDTH,
            height: HEIGHT,
            font_size: 40.0,
            line_height: 1.20,
            render_dpi: dpi,
            fit_content: mode,
            ..Settings::default()
        },
        0,
        None,
    )
    .unwrap()
}

fn box_bounds(frame: &[u8], gray: u8, text_y: usize, tones: [u8; 2]) -> (usize, usize) {
    let (left, width) = band(frame, gray);
    assert!(width > 20);
    let x = left + width - 8;
    let pixel = |y| frame[y * WIDTH as usize + x];
    let mut top = text_y;
    let mut end = text_y;
    assert_eq!(pixel(text_y), gray);
    while top > 0 && pixel(top - 1) == gray {
        top -= 1;
    }
    while end < HEIGHT as usize && pixel(end) == gray {
        end += 1;
    }
    while top > 0 && pixel(top - 1) == tones[0] {
        top -= 1;
    }
    while end < HEIGHT as usize && pixel(end) == tones[1] {
        end += 1;
    }
    assert_eq!([pixel(top), pixel(end - 1)], tones);
    (top, end)
}

#[test]
fn fit_content_height_uses_stock_box_sizing_limits_and_resolves_units() {
    let mut proposal = book(FitContentMode::Web, 96);
    let mut fitted_book = book(FitContentMode::Book, 96);
    let first = proposal.capture(0).unwrap();
    let second = proposal.capture(1).unwrap();
    capture("fit-height-reader-web-1.png", &first);
    capture("fit-height-reader-web-9.png", &second);
    for (phrase, shade, page, expected) in [
        ("Content bay", 221, 0, 225),
        ("Border bay", 204, 0, 180),
        ("Narrow basin", 187, 0, 145),
        ("North channel", 170, 0, 200),
        ("Quiet water", 153, 0, 181),
        ("Deep reach", 221, 1, 205),
        ("Open shore", 204, 1, 205),
        ("Bright harbor", 136, 1, 184),
    ] {
        let original = quote(&fitted_book, phrase);
        assert_eq!(
            proposal.select_range(&original.start, &original.end),
            Some(original.clone())
        );
        assert_eq!(proposal.page_for(&original.start), Some(page));
        let rect = proposal.selection_rects(page, &original)[0];
        let y = (rect.y + rect.height / 2) as usize;
        let (top, end) = box_bounds(if page == 0 { &first } else { &second }, shade, y, [51, 85]);
        assert!(
            (end - top).abs_diff(expected) <= 1,
            "{phrase}: got {}, expected {expected}",
            end - top
        );
        let book_rect = fitted_book.selection_rects(page, &original)[0];
        let book_frame = fitted_book.capture(page).unwrap();
        let (book_top, book_end) = box_bounds(
            &book_frame,
            shade,
            (book_rect.y + book_rect.height / 2) as usize,
            [51, 85],
        );
        assert!(
            book_end - book_top + 35 < expected,
            "Book must ignore {phrase}'s declared height"
        );
        if phrase == "Content bay" {
            let x = rect.x as f32 + 14.0;
            assert_eq!(
                proposal
                    .link_at(page, x, y as f32)
                    .unwrap()
                    .fragment
                    .as_deref(),
                Some("route")
            );
            assert!(proposal.link_at(page, x, (end - 12) as f32).is_none());
            assert!(
                proposal
                    .text_position_at(page, x, (end - 12) as f32)
                    .is_none()
            );
        }
    }
    let crate::Renderer::Parley(renderer) = &proposal.renderer else {
        panic!("Expected Parley")
    };
    for phrase in ["Current line", "Twin banks", "Lower bank", "Tidal fall"] {
        let saved = quote(&fitted_book, phrase);
        let element = proposal
            .source
            .blocks
            .iter()
            .find(|block| block.key == saved.start.block)
            .unwrap()
            .element;
        assert!(
            !renderer
                .fitted_height_tails()
                .iter()
                .any(|tail| tail.element == element),
            "{phrase} has no added height"
        );
        assert_eq!(proposal.select_range(&saved.start, &saved.end), Some(saved));
    }
    let mut high_dpi = book(FitContentMode::Web, 144);
    let physical = quote(&high_dpi, "Bright harbor");
    let page = high_dpi.page_for(&physical.start).unwrap();
    let rect = high_dpi.selection_rects(page, &physical)[0];
    let frame = high_dpi.capture(page).unwrap();
    let (top, end) = box_bounds(&frame, 136, (rect.y + rect.height / 2) as usize, [51, 85]);
    assert!((end - top).abs_diff(276) <= 1);
}

#[test]
fn fit_content_height_keeps_bounded_spacer_visible_around_forced_and_natural_turns() {
    let mut ordinary = book(FitContentMode::Ignore, 96);
    let mut proposal = book(FitContentMode::Web, 96);
    let mut fitted_book = book(FitContentMode::Book, 96);
    for (phrase, book_page, web_page) in [
        ("Far headland", 2, 2),
        ("After the headland", 2, 3),
        ("Last headland", 3, 4),
        ("After the last headland", 4, 6),
    ] {
        let quote = quote(&ordinary, phrase);
        assert_eq!(fitted_book.page_for(&quote.start), Some(book_page));
        assert_eq!(proposal.page_for(&quote.start), Some(web_page), "{phrase}");
        assert_eq!(proposal.select_range(&quote.start, &quote.end), Some(quote));
    }
    let height = proposal.settings.text_height() as f32;
    for (opening, closing, gray) in [(2, 3, 170), (4, 5, 153)] {
        let a = proposal.pages.range(opening).unwrap();
        let b = proposal.pages.range(closing).unwrap();
        assert_eq!(a.end, b.start);
        assert!((a.end - a.start - height).abs() < 0.02);
        let first = proposal.capture(opening).unwrap();
        let last = proposal.capture(closing).unwrap();
        let top = proposal.settings.top_margin() as usize;
        let row = |frame: &[u8], y| frame[y * WIDTH as usize + 632];
        assert_eq!(row(&first, top), 68);
        assert_eq!(row(&first, top + height as usize - 1), gray);
        assert_eq!(row(&last, top), gray);
        let last_edge = (top..HEIGHT as usize)
            .find(|&y| row(&last, y) == 68)
            .unwrap();
        assert!(last_edge > top + 60 && last_edge < top + 115);
        for y in last_edge..last_edge + 3 {
            assert_eq!(row(&last, y), 68);
        }
        assert_eq!(row(&last, last_edge + 3), 255);
        assert!(
            proposal
                .text_position_at(closing, 632.0, (top + 20) as f32)
                .is_none()
        );
        capture(&format!("fit-height-reader-web-{closing}.png"), &last);
    }
    assert!(proposal.page_anchor(5).is_none());
    assert_eq!(proposal.page_count(), 7);
    assert_eq!(fitted_book.page_count(), 5);
    assert_eq!(ordinary.page_count(), 5);
    assert!(ordinary.capture(0).unwrap() != fitted_book.capture(0).unwrap());
}
