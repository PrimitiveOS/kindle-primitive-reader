use super::*;

fn authored_book(scroll: bool, overlap_footer: bool) -> Reading {
    let config = BookConfig {
        viewport: ReadingViewport::PORTRAIT,
        user_styles: Default::default(),
        user_hyphenation: Default::default(),
        engine: Engine::Parley,
        book: crate::test_fixture("field-notes-epub3.epub"),
        fonts: vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
        settings: Settings {
            width: WIDTH,
            height: if overlap_footer { HEIGHT } else { PAGE_HEIGHT },
            font_size: 52.0,
            ..Settings::default()
        },
        scroll,
        facing_pages: false,
        hide_nonlinear: false,
        hidden_flows: Default::default(),
        chapter_offsets: Default::default(),
        screen_pages: Default::default(),
    };
    Reading::open(config).unwrap()
}

fn find_repeated_source(reading: &mut Reading) -> TextSelection {
    reading.jump(0, Some("tide-a")).unwrap();
    let earlier = reading.anchor().unwrap();
    reading.jump(0, Some("tide-b")).unwrap();
    let mut tide = reading.anchor().unwrap();
    assert_ne!(tide.block, earlier.block);
    tide.offset += "The ".len();
    reading.jump_to_anchor(0, &tide).unwrap();
    let mut end = tide.clone();
    end.offset += "tide".len();
    let selection = reading.anchored_quote(&tide, &end).unwrap();
    assert_eq!(selection.text, "tide");
    selection
}

fn verify_hit(reading: &Reading, selection: &TextSelection) -> HighlightRect {
    let rect = reading.highlight_rects(selection)[0];
    let word = reading
        .word_at(
            rect.x as f32 + rect.width as f32 / 2.0,
            rect.y as f32 + rect.height as f32 / 2.0,
        )
        .unwrap();
    assert_eq!(word.text, selection.text);
    assert_eq!(word.start, selection.start);
    assert_eq!(word.end, selection.end);
    let (w, h) = reading.config.viewport.dimensions();
    assert!(rect.x >= 0 && rect.x + rect.width <= w as i32);
    assert!(rect.y >= 0 && rect.y + rect.height <= h as i32);
    rect
}

fn render_frame(reading: &Reading, selection: &TextSelection, name: &str) -> Vec<u8> {
    let viewport = reading.config.viewport;
    let (w, h) = viewport.dimensions();
    let fonts = reading
        .config
        .fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(w, h, &fonts).unwrap();
    canvas.begin();
    reading.draw_page(
        &mut canvas,
        Some("Exit"),
        false,
        None,
        viewport.caption(viewport.done().x.saturating_sub(60), 28),
        CAPTION,
        &[],
        &reading.highlight_rects(selection),
        PagePaint {
            visible: 0..reading.frame.height,
            ..PagePaint::full(false)
        },
        true,
    );
    let pixels = canvas.finish(false);
    let footer = viewport.done();
    let footer_has_ink = (footer.y..footer.y + footer.h).any(|y| {
        let start = y as usize * w as usize + footer.x as usize;
        pixels[start..start + footer.w as usize]
            .iter()
            .any(|&pixel| pixel < 125)
    });
    assert!(footer_has_ink, "footer is inside the new viewport");
    assert_eq!(pixels.len(), (w * h) as usize);
    if let Some(out) = std::env::var_os("READER_REFLOW_VIEWPORT_CAPTURE_DIR") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        primitive_app::write_png(&out.join(name), w, h, &pixels).unwrap();
    }
    pixels
}

#[test]
fn reflow_viewport_round_trip_keeps_exact_repeated_source_and_reprojects_selection() {
    for (scroll, overlap_footer) in [(false, false), (true, true)] {
        let mut reading = authored_book(scroll, overlap_footer);
        let selected = find_repeated_source(&mut reading);
        let anchor = reading.anchor().unwrap();
        assert_eq!(anchor, selected.start);
        let original_hit = verify_hit(&reading, &selected);
        let mode = if scroll { "scroll" } else { "page" };
        let portrait_pixels = render_frame(&reading, &selected, &format!("{mode}-portrait.png"));

        let mut landscape = reading
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .unwrap();
        assert_eq!(reading.config.viewport, ReadingViewport::PORTRAIT);
        assert_eq!(reading.anchor(), Some(anchor.clone()));
        assert_eq!(landscape.config.viewport.dimensions(), (1680, 1264));
        assert_eq!(landscape.frame.width, 1680);
        assert_eq!(
            landscape.frame.height,
            if overlap_footer { 1264 } else { 1190 }
        );
        assert_eq!(landscape.anchor(), Some(anchor.clone()));
        assert_eq!(landscape.chapter, reading.chapter);
        assert_eq!(landscape.is_continuous(), scroll);
        let visible = 110..290;
        let (area, clipped) = PagePaint {
            visible: visible.clone(),
            ..PagePaint::full(false)
        }
        .apply(landscape.frame.clone());
        assert_eq!(area, Rect::new(0, 110, 1680, 180));
        let stride = landscape.frame.width as usize;
        assert_eq!(
            clipped.pixels,
            landscape.frame.pixels[visible.start as usize * stride..visible.end as usize * stride],
        );
        let wide_hit = verify_hit(&landscape, &selected);
        let landscape_pixels =
            render_frame(&landscape, &selected, &format!("{mode}-landscape.png"));
        assert_ne!(landscape_pixels, portrait_pixels);
        if !scroll {
            assert_ne!(original_hit, wide_hit);
        }

        if !scroll {
            assert_eq!(landscape.input(Input::Tap(1000.0, 300.0)), Change::None);
            assert!(landscape.turns.pending.is_empty());
            let exit = ReadingViewport::LANDSCAPE.done();
            assert_eq!(
                landscape.input(Input::Tap(
                    (exit.x + exit.w / 2) as f32,
                    (exit.y + exit.h / 2) as f32
                )),
                Change::Exit,
            );
        }

        let returned = landscape
            .prepare_viewport(ReadingViewport::PORTRAIT)
            .unwrap();
        assert_eq!(returned.config.viewport.dimensions(), (1264, 1680));
        assert_eq!(returned.anchor(), Some(anchor));
        assert_eq!(returned.page, reading.page);
        assert_eq!(verify_hit(&returned, &selected), original_hit);
        assert_eq!(
            render_frame(&returned, &selected, &format!("{mode}-returned.png")),
            portrait_pixels,
        );
    }
}

#[test]
fn a_refused_viewport_does_not_change_current_pixels_position_or_input_queue() {
    let mut reading = authored_book(false, false);
    reading
        .reflow(Settings {
            margin_bottom: Some(380),
            ..reading.config.settings
        })
        .unwrap();
    let _selection = find_repeated_source(&mut reading);
    reading.queue_page_turns([-1, 1]);
    let anchor = reading.anchor();
    let page = reading.page;
    let frame = reading.frame.pixels.clone();
    let error = reading
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "Reading layout settings are out of range"
    );
    assert_eq!(reading.config.viewport, ReadingViewport::PORTRAIT);
    assert_eq!(reading.anchor(), anchor);
    assert_eq!(reading.page, page);
    assert_eq!(reading.frame.pixels, frame);
    assert_eq!(reading.turns.pending.len(), 2);
}

#[test]
fn facing_pages_refuse_to_transition_until_the_spread_uses_explicit_geometry() {
    let mut reading = authored_book(false, false);
    reading.set_facing_pages(true).unwrap();
    let frame = reading.frame.pixels.clone();
    assert!(
        reading
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .is_err()
    );
    assert_eq!(reading.config.viewport, ReadingViewport::PORTRAIT);
    assert!(reading.uses_facing_pages());
    assert_eq!(reading.frame.pixels, frame);
}
