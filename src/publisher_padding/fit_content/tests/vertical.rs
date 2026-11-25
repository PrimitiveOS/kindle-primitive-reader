use super::*;

const FIXTURE: &str = "render-fit-vertical.epub";

fn gray_at(frame: &[u8], x: usize, y: usize) -> u8 {
    frame[y * WIDTH as usize + x]
}

fn paragraph(probe: &Probe, key: usize) -> &crate::parley_layout::Paragraph {
    let crate::Renderer::Parley(layout) = &probe.renderer else {
        panic!("Expected Parley")
    };
    layout
        .paragraphs_in(0.0..f32::MAX)
        .find(|paragraph| paragraph.key == key)
        .unwrap()
}

#[test]
fn fit_content_vertical_padding_matches_stock_box_widths_and_source_hits() {
    let mut ordinary = open_fixture(FIXTURE, Settings::default(), None);
    let regular_frame = ordinary.capture(0).unwrap();
    for (mode, label) in [(FitContentMode::Book, "book"), (FitContentMode::Web, "web")] {
        let mut fitted = open_fixture(
            FIXTURE,
            Settings {
                fit_content: mode,
                ..Settings::default()
            },
            None,
        );
        let frame = fitted.capture(0).unwrap();
        capture(&format!("fit-vertical-reader-{label}-1.png"), &frame);
        for (phrase, shade, left, width, before, after) in [
            ("Upper ledge", 204, 478, 307, 29, 23),
            ("Lower ledge", 170, 480, 304, 34, 23),
            ("Long measure", 153, 427, 410, 25, 50),
            (
                "Sloping bank",
                136,
                if mode == FitContentMode::Web { 40 } else { 54 },
                316,
                23,
                11,
            ),
        ] {
            let canonical = quote(&ordinary, phrase);
            assert_eq!(
                fitted.select_range(&canonical.start, &canonical.end),
                Some(canonical.clone())
            );
            assert_eq!(fitted.page_for(&canonical.start), Some(0));
            let p = fitted
                .lines
                .iter()
                .find(|line| line.block == canonical.start.block)
                .unwrap();
            let wrapped = paragraph(&fitted, canonical.start.block);
            let outer = p.page_extent();
            let actual_before = wrapped.y - outer.start;
            let actual_after = outer.end - (wrapped.y + wrapped.layout.height()).max(p.bottom);
            assert!(
                (actual_before - before as f32).abs() < 0.02,
                "{phrase}: {actual_before}"
            );
            assert!(
                (actual_after - after as f32).abs() < 0.02,
                "{phrase}: {actual_after}"
            );
            let (x, w) = band(&frame, shade);
            assert!(
                x.abs_diff(left) <= 2 && w.abs_diff(width) <= 2,
                "{label} {phrase}: {x}/{w}"
            );
            assert!(band(&regular_frame, shade).1 > 1_000);
            let top =
                fitted.settings.top_margin() as usize + p.page_extent().start.floor() as usize;
            let center = x + w / 2;
            if phrase == "Upper ledge" {
                assert_eq!(gray_at(&frame, center, top), 51);
                assert_eq!(gray_at(&frame, center, top + 2), 51);
                assert_eq!(gray_at(&frame, center, top + 3), shade);
                assert_eq!(gray_at(&frame, x.saturating_sub(2), top + 10), 255);
                let rect = fitted.selection_rects(0, &canonical)[0];
                let x = rect.x as f32 + 14.0;
                assert!(fitted.link_at(0, x, top as f32 + 10.0).is_none());
                assert!(fitted.text_position_at(0, x, top as f32 + 10.0).is_none());
                assert_eq!(
                    fitted
                        .link_at(0, x, (rect.y + rect.height / 2) as f32)
                        .unwrap()
                        .fragment
                        .as_deref(),
                    Some("shore")
                );
            } else {
                assert_eq!(gray_at(&frame, center, top + 5), shade);
                assert_eq!(gray_at(&frame, x.saturating_sub(2), top + 5), 255);
            }
        }
    }
}

#[test]
fn fit_content_vertical_paint_keeps_outer_ends_across_forced_and_natural_breaks() {
    let ordinary = open_fixture(FIXTURE, Settings::default(), None);
    for (mode, label) in [(FitContentMode::Book, "book"), (FitContentMode::Web, "web")] {
        let mut fitted = open_fixture(
            FIXTURE,
            Settings {
                fit_content: mode,
                ..Settings::default()
            },
            None,
        );
        let first = quote(&ordinary, "Sounding 01");
        let last = quote(&ordinary, "Sounding 42");
        let next = quote(&ordinary, "After the passage");
        for (saved, page) in [(&first, 1), (&last, 2), (&next, 3)] {
            assert_eq!(
                fitted.select_range(&saved.start, &saved.end),
                Some(saved.clone())
            );
            assert_eq!(fitted.page_for(&saved.start), Some(page));
        }
        let mut lines = fitted
            .lines
            .iter()
            .filter(|line| line.block == first.start.block);
        let first_line = lines.next().unwrap();
        let last_line = lines.next_back().unwrap();
        assert_eq!(
            fitted.pages.range(1).unwrap().start,
            first_line.page_extent().start
        );
        let wrapped = paragraph(&fitted, first.start.block);
        assert!((wrapped.y - first_line.page_extent().start - 31.0).abs() < 0.02);
        let text_end = (wrapped.y + wrapped.layout.height()).max(last_line.bottom);
        assert!((last_line.page_extent().end - text_end - 40.0).abs() < 0.02);
        assert!(lines.all(|line| line.page_extension == [0.0; 2]));
        let whole = fitted.select_range(&first.start, &last.end).unwrap();
        assert_eq!(
            ordinary.select_range(&first.start, &last.end),
            Some(whole.clone())
        );
        assert!(!fitted.selection_rects(1, &whole).is_empty());
        assert!(!fitted.selection_rects(2, &whole).is_empty());
        let top = fitted.settings.top_margin() as usize;
        for (page, ending) in [(1, false), (2, true)] {
            let frame = fitted.capture(page).unwrap();
            capture(
                &format!(
                    "fit-vertical-reader-{label}-{}.png",
                    if ending { 9 } else { 6 }
                ),
                &frame,
            );
            let (x, width) = band(&frame, 187);
            assert!(
                x.abs_diff(470) <= 2 && width.abs_diff(323) <= 2,
                "{label} page {page}: {x}/{width}"
            );
            let full_rows = frame
                .chunks_exact(WIDTH as usize)
                .enumerate()
                .filter(|(_, row)| row[x - 3..x + width + 3].iter().all(|&p| p == 68))
                .map(|(y, _)| y)
                .collect::<Vec<_>>();
            assert_eq!(full_rows.len(), 3);
            assert_eq!(gray_at(&frame, x - 1, top + 8), 68);
            assert_eq!(gray_at(&frame, x - 5, top + 8), 255);
            if ending {
                assert!(full_rows[0] > top + 40);
                assert_eq!(gray_at(&frame, x + width - 10, full_rows[0] - 8), 187);
            } else {
                assert_eq!(full_rows, [top, top + 1, top + 2]);
                assert_eq!(gray_at(&frame, x + width - 10, top + 23), 187);
            }
        }
    }
}

#[test]
fn fit_content_vertical_padding_preserves_ordinary_page_paint() {
    let document = "<html><body><p style='width:fit-content;padding:3% 16px 2%;border:4px solid #444;background:#bbb'>Leading and trailing edges</p><p>Next paragraph</p></body></html>";
    let mut ordinary = open(
        Settings::default(),
        Some(document.replace("width:fit-content;", "")),
    );
    let mut opted_out = open(Settings::default(), Some(document.into()));
    assert_eq!(ordinary.page_count(), opted_out.page_count());
    assert!(ordinary.capture(0).unwrap() == opted_out.capture(0).unwrap());
}

#[test]
fn fit_content_vertical_edge_overflow_moves_the_text_with_the_decoration() {
    let line = |top, bottom, page_extension| crate::Line {
        block: 0,
        bytes: 0..1,
        spans: None,
        additional_sources: None,
        raw_whitespace: None,
        top,
        bottom,
        page_extension,
        missing_glyphs: 0,
    };
    let lines = [
        line(0.0, 78.0, [0.0; 2]),
        line(90.0, 96.0, [10.0, 16.0]),
        line(120.0, 132.0, [0.0; 2]),
    ];
    let pages = crate::Pagination::new(&lines, 100.0, 132.0).unwrap();
    assert_eq!(pages.range(0), Some(0.0..80.0));
    assert_eq!(pages.range(1), Some(80.0..132.0));
    assert_eq!((lines[1].top, lines[1].bottom), (90.0, 96.0));
}
