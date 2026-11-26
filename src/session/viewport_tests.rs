use super::*;

fn authored_config(scroll: bool, overlap_footer: bool) -> BookConfig {
    BookConfig {
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
        quote_prefixes: Default::default(),
        screen_pages: Default::default(),
        eager_layouts: Default::default(),
        folder_snapshot: None,
    }
}

fn authored_book(scroll: bool, overlap_footer: bool) -> Reading {
    Reading::open(authored_config(scroll, overlap_footer)).unwrap()
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
        let mut config = authored_config(scroll, overlap_footer);
        config.facing_pages = scroll;
        let mut reading = Reading::open(config).unwrap();
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
        assert_eq!(landscape.config.facing_pages, scroll);
        assert!(!landscape.uses_facing_pages());
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

fn first_pane_hit<T>(
    width: u32,
    height: u32,
    hit: impl Fn(f32, f32) -> Option<T>,
) -> Option<(f32, f32, T)> {
    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            if let Some(item) = hit(x as f32, y as f32) {
                return Some((x as f32, y as f32, item));
            }
        }
    }
    None
}

fn verify_spread(reading: &mut Reading, rtl: bool) -> Vec<TextSelection> {
    use crate::highlight_style::{HighlightColor, HighlightStyle, NoteMarker, Paint};

    assert!(reading.uses_facing_pages());
    assert_eq!(reading.probe.pages_right_to_left(), rtl);
    let (width, height) = reading.config.viewport.dimensions();
    let leaf_width = (width - 32) / 2;
    let right = leaf_width + 32;
    let first = reading.first_visible_page();
    assert_eq!(reading.last_visible_page(), first + 1);
    assert_eq!(reading.probe.settings.width, leaf_width);
    assert_eq!(
        (reading.frame.width, reading.frame.height),
        (width, height - 74)
    );
    assert_eq!(reading.frame.pixels.len(), (width * (height - 74)) as usize);
    assert!(
        reading
            .frame
            .pixels
            .chunks_exact(width as usize)
            .all(|row| row[leaf_width as usize..right as usize]
                .iter()
                .all(|&pixel| pixel == 255))
    );
    let mid = width as f32 / 2.0;
    assert!(reading.word_at(mid, 160.0).is_none());
    assert!(reading.link_at(mid, 160.0).is_none());

    let origins = if rtl { [right, 0] } else { [0, right] };
    let mut words = Vec::new();
    let mut link_hits = 0;
    let mut arabic_hits = 0;
    for (logical, x) in origins.into_iter().enumerate() {
        let page = first + logical;
        let expected = reading.probe.capture(page).unwrap();
        for (row, leaf) in reading
            .frame
            .pixels
            .chunks_exact(width as usize)
            .zip(expected.chunks_exact(leaf_width as usize))
        {
            assert_eq!(&row[x as usize..(x + leaf_width) as usize], leaf);
        }
        let (wx, wy, word) = first_pane_hit(leaf_width, height, |px, py| {
            reading.probe.word_at(page, px, py)
        })
        .unwrap();
        assert_eq!(reading.word_at(wx + x as f32, wy), Some(word.clone()));
        let rect = verify_hit(reading, &word);
        assert!(rect.x >= x as i32 && rect.x + rect.width <= (x + leaf_width) as i32);
        let marker = NoteMarker::Sideline;
        let margin = reading.note_marker_margin(rect, marker);
        assert_eq!(
            margin,
            (x + leaf_width - reading.probe.settings.right_margin()) as i32 + 11
        );
        let marked = reading.marked_frame(
            &[(
                rect,
                Paint::Annotation {
                    style: HighlightStyle::Underline,
                    color: HighlightColor::default(),
                    note: true,
                    first: true,
                    night: false,
                },
            )],
            &[],
            &PagePaint {
                note_marker: marker,
                visible: 0..reading.frame.height,
                ..PagePaint::full(false)
            },
        );
        assert_ne!(
            marked.pixels[rect.y as usize * width as usize + margin as usize],
            255
        );
        words.push(word);
        if rtl
            && let Some((wx, wy, arabic)) = first_pane_hit(leaf_width, height, |px, py| {
                reading.probe.word_at(page, px, py).filter(|word| {
                    word.text
                        .chars()
                        .any(|ch| ('\u{0600}'..='\u{06ff}').contains(&ch) && ch.is_alphabetic())
                })
            })
        {
            assert_eq!(reading.word_at(wx + x as f32, wy), Some(arabic));
            arabic_hits += 1;
        }
        if !reading
            .probe
            .links_in(reading.probe.page_range(page))
            .is_empty()
        {
            let (lx, ly, target) = first_pane_hit(leaf_width, height, |px, py| {
                reading.probe.link_at(page, px, py)
            })
            .unwrap();
            assert_eq!(reading.link_at(lx + x as f32, ly), Some(target));
            link_hits += 1;
        }
    }
    assert!(
        !rtl || link_hits > 0,
        "The authored RTL spread includes a translation link"
    );
    assert!(
        !rtl || arabic_hits > 0,
        "The authored RTL spread includes selectable Arabic"
    );
    words
}

#[test]
fn facing_viewports_reflow_both_leaves_and_keep_the_selected_source_in_each_direction() {
    for (file, rtl) in [
        ("field-notes-epub3.epub", false),
        ("rtl-notebook.epub", true),
    ] {
        let mut config = authored_config(false, false);
        config.book = crate::test_fixture(file);
        config.facing_pages = true;
        config.settings.font_size = if rtl { 46.0 } else { 76.0 };
        if rtl {
            config
                .fonts
                .push(crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"));
        }
        let mut portrait = Reading::open(config).unwrap();
        let first_pair = verify_spread(&mut portrait, rtl);
        let selected = &first_pair[1];
        let anchor = selected.start.clone();
        portrait.jump_to_anchor(0, &anchor).unwrap();
        assert_eq!(portrait.page, 1);
        let original_hit = verify_hit(&portrait, selected);
        let name = if rtl { "rtl-facing" } else { "ltr-facing" };
        let original = render_frame(&portrait, selected, &format!("{name}-portrait.png"));

        let mut landscape = portrait
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .unwrap();
        assert_eq!(landscape.anchor(), Some(anchor.clone()));
        assert_eq!(
            landscape
                .anchored_quote(&selected.start, &selected.end)
                .unwrap(),
            *selected
        );
        assert_eq!(portrait.anchor(), Some(anchor.clone()));
        verify_spread(&mut landscape, rtl);
        let wide_hit = verify_hit(&landscape, selected);
        assert_ne!(wide_hit, original_hit);
        let frame = landscape.frame.pixels.clone();
        landscape
            .reflow(Settings {
                page_overlap_enabled: !landscape.config.settings.page_overlap_enabled,
                ..landscape.config.settings
            })
            .unwrap();
        assert_eq!(landscape.probe.settings.width, 824);
        assert_eq!(landscape.frame.pixels, frame);
        assert_eq!(verify_hit(&landscape, selected), wide_hit);
        render_frame(&landscape, selected, &format!("{name}-landscape.png"));

        let returned = landscape
            .prepare_viewport(ReadingViewport::PORTRAIT)
            .unwrap();
        assert_eq!(returned.anchor(), Some(anchor));
        assert_eq!(returned.page, 1);
        assert_eq!(verify_hit(&returned, selected), original_hit);
        assert_eq!(
            render_frame(&returned, selected, &format!("{name}-returned.png")),
            original
        );

        let start = landscape.first_visible_page();
        assert!(landscape.probe.page_count() > start + 2);
        let y = landscape.frame.height as f32 / 2.0;
        let (forward, back) = if rtl { (16.0, 1664.0) } else { (1664.0, 16.0) };
        for (input, page, change) in [
            (Input::Tap(1060.0, y), start, Change::None),
            (Input::Tap(forward, y), start + 2, Change::Partial),
            (Input::Tap(back, y), start, Change::Partial),
            (Input::PageForward, start + 2, Change::Partial),
            (Input::PageBack, start, Change::Partial),
        ] {
            landscape.turns.next_refresh = Instant::now();
            assert_eq!(landscape.input(input), change);
            assert_eq!(landscape.first_visible_page(), page);
        }
    }
}

#[test]
fn mixed_chapters_keep_their_layout_preferences_after_vertical_resize() {
    let mut config = authored_config(false, false);
    config.book = crate::test_fixture("vertical-chapters.epub");
    config.facing_pages = true;
    let portrait = Reading::open(config).unwrap();
    let mut landscape = portrait
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    assert!(landscape.uses_facing_pages());
    assert_eq!(landscape.visible_parts().len(), 1);
    assert!(
        landscape
            .frame
            .pixels
            .chunks_exact(1680)
            .all(|row| row[856..].iter().all(|&pixel| pixel == 255))
    );
    assert_eq!(landscape.turn(1).unwrap(), Change::Partial);
    assert_eq!(landscape.chapter, 1);
    assert!(landscape.probe.is_vertical());
    assert!(landscape.config.facing_pages);
    assert!(!landscape.uses_facing_pages());
    assert_eq!(landscape.frame.width, 1680);
    assert_eq!(landscape.probe.settings.width, 1680);
    let anchor = landscape.anchor();
    let frame = landscape.frame.pixels.clone();
    let returned = landscape
        .prepare_viewport(ReadingViewport::PORTRAIT)
        .unwrap();
    assert!(returned.probe.is_vertical());
    assert_eq!(returned.probe.settings.width, 1264);
    assert_eq!(returned.anchor(), anchor);
    assert_eq!(landscape.anchor(), anchor);
    assert_eq!(landscape.frame.pixels, frame);
    assert_eq!(landscape.turn(-1).unwrap(), Change::Partial);
    assert_eq!(landscape.chapter, 0);
    assert!(landscape.uses_facing_pages());
    assert_eq!(landscape.probe.settings.width, 824);
}

fn vertical_config(scroll: bool) -> BookConfig {
    let mut config = authored_config(scroll, false);
    config.book = crate::test_fixture("harbor-vertical-viewport.epub");
    config.facing_pages = true;
    config.settings.font_size = 52.0;
    config
}

fn vertical_link_selection(reading: &Reading, id: &str, text: &str) -> TextSelection {
    let start = reading.probe.fragment(id).unwrap();
    let mut end = start.clone();
    end.offset += text.len();
    let selection = reading.anchored_quote(&start, &end).unwrap();
    assert_eq!(selection.text, text);
    selection
}

fn verify_vertical_link(reading: &Reading, selected: &TextSelection, href: &str) -> HighlightRect {
    let (width, height) = reading.config.viewport.dimensions();
    assert!(reading.probe.is_vertical());
    assert!(reading.probe.pages_right_to_left());
    assert_eq!(reading.probe.settings.width, width);
    assert_eq!(
        (reading.frame.width, reading.frame.height),
        (width, height - 74)
    );
    assert_eq!(reading.visible_parts().len(), 1);
    assert_eq!(reading.visible_parts()[0].x, 0);
    assert!(!reading.uses_facing_pages());
    assert!(!reading.is_continuous());
    assert_eq!(reading.probe.page_for(&selected.start), Some(reading.page));
    let (painted, available) = reading.probe.coverage();
    assert_eq!(painted, available);

    let rect = verify_hit(reading, selected);
    let x = rect.x as f32 + rect.width as f32 / 2.0;
    let y = rect.y as f32 + rect.height as f32 / 2.0;
    assert_eq!(reading.link_href_at(x, y), Some(href));
    assert_eq!(
        reading.link_at(x, y),
        Book::resolve_link(&reading.probe.chapter, href).ok()
    );
    assert!(
        reading
            .link_at(x, reading.probe.settings.top_margin() as f32 - 1.0)
            .is_none()
    );
    assert!(reading.word_at(x, (height - 74) as f32).is_none());
    assert!(reading.link_at(width as f32 - 1.0, y).is_none());
    assert!((rect.y..rect.y + rect.height).any(|row| {
        let left = (row as u32 * width) as usize + rect.x as usize;
        reading.frame.pixels[left..left + rect.width as usize]
            .iter()
            .any(|&ink| ink < 96)
    }));

    // Source bytes advance toward the bottom of each vertical column.
    let mut middle = selected.start.clone();
    middle.offset += selected.text.chars().next().unwrap().len_utf8();
    let first = reading.anchored_quote(&selected.start, &middle).unwrap();
    let second = reading.anchored_quote(&middle, &selected.end).unwrap();
    let first = reading.highlight_rects(&first)[0];
    let second = reading.highlight_rects(&second)[0];
    assert_eq!(first.x, second.x);
    assert!(first.y < second.y);
    rect
}

#[test]
fn vertical_cjk_round_trip_preserves_the_link_and_both_selection_endpoints() {
    for (chapter, language, text, href, scroll) in [
        (0, "ja", "灯台", "chinese.xhtml#landing", false),
        (1, "zh", "港口", "japanese.xhtml#landing", true),
    ] {
        let mut portrait = Reading::restore(vertical_config(scroll), chapter, 0, None).unwrap();
        let earlier = vertical_link_selection(&portrait, "earlier", text);
        let selected = vertical_link_selection(&portrait, "later", text);
        assert_ne!(earlier.start.block, selected.start.block);
        portrait.jump_to_anchor(chapter, &selected.start).unwrap();
        let anchor = portrait.anchor();
        let page = portrait.page;
        let original_hit = verify_vertical_link(&portrait, &selected, href);
        let original_pixels = render_frame(
            &portrait,
            &selected,
            &format!("vertical-{language}-portrait.png"),
        );

        let mut landscape = portrait
            .prepare_viewport(ReadingViewport::LANDSCAPE)
            .unwrap();
        assert_eq!(landscape.anchor(), anchor);
        assert_eq!(portrait.anchor(), anchor);
        assert!(landscape.config.facing_pages);
        assert_eq!(landscape.config.scroll, scroll);
        let wide_hit = verify_vertical_link(&landscape, &selected, href);
        assert_ne!(wide_hit, original_hit);
        let (x, y, word) = first_pane_hit(416, landscape.frame.height, |x, y| {
            landscape.probe.word_at(landscape.page, 1264.0 + x, y)
        })
        .unwrap();
        assert_eq!(landscape.word_at(1264.0 + x, y), Some(word));
        assert_eq!(
            landscape
                .anchored_quote(&selected.start, &selected.end)
                .unwrap(),
            selected
        );
        render_frame(
            &landscape,
            &selected,
            &format!("vertical-{language}-landscape.png"),
        );
        let returned = landscape
            .prepare_viewport(ReadingViewport::PORTRAIT)
            .unwrap();
        assert_eq!(returned.anchor(), anchor);
        assert_eq!(returned.page, page);
        assert_eq!(
            verify_vertical_link(&returned, &selected, href),
            original_hit
        );
        assert_eq!(
            render_frame(
                &returned,
                &selected,
                &format!("vertical-{language}-returned.png")
            ),
            original_pixels
        );

        let start = landscape.page;
        assert!(start + 1 < landscape.probe.page_count());
        for (event, page) in [
            (Input::Tap(16.0, 500.0), start + 1),
            (Input::Tap(1664.0, 500.0), start),
            (Input::PageForward, start + 1),
            (Input::PageBack, start),
        ] {
            landscape.turns.next_refresh = Instant::now();
            assert_eq!(landscape.input(event), Change::Partial);
            assert_eq!(landscape.page, page);
        }
        landscape.jump_to_anchor(chapter, &selected.start).unwrap();
        verify_vertical_link(&landscape, &selected, href);
        let destination = landscape
            .link_at(
                wide_hit.x as f32 + wide_hit.width as f32 / 2.0,
                wide_hit.y as f32 + wide_hit.height as f32 / 2.0,
            )
            .unwrap();
        let chapter = landscape.probe.chapter_index(&destination.path).unwrap();
        landscape
            .jump(chapter, destination.fragment.as_deref())
            .unwrap();
        assert!(landscape.probe.is_vertical());
        assert_eq!(landscape.probe.settings.width, 1680);
        assert_eq!(landscape.anchor(), landscape.probe.fragment("landing"));
    }
}

#[test]
fn vertical_book_with_wide_columns_ignores_the_saved_two_page_choice() {
    let mut config = vertical_config(false);
    config.settings.font_size = 120.0;
    config.settings.line_height = 3.0;
    config.settings.margin = 100;
    let portrait = Reading::open(config).unwrap();
    assert!(portrait.probe.is_vertical());
    assert!(!portrait.uses_facing_pages());
    assert_eq!(portrait.probe.settings.width, 1264);
    let landscape = portrait
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .unwrap();
    assert!(landscape.probe.is_vertical());
    assert!(!landscape.uses_facing_pages());
    assert_eq!(landscape.probe.settings.width, 1680);
    assert_eq!(landscape.anchor(), portrait.anchor());
}

#[test]
fn resize_refuses_a_vertical_ruby_column_that_no_longer_fits() {
    let mut portrait = Reading::restore(vertical_config(false), 2, 0, None).unwrap();
    assert!(portrait.probe.is_vertical());
    portrait.queue_page_turns([1, -1]);
    let anchor = portrait.anchor();
    let original = portrait.frame.pixels.clone();
    let error = portrait
        .prepare_viewport(ReadingViewport::LANDSCAPE)
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "The new viewport could not keep this chapter's writing mode"
    );
    assert_eq!(portrait.anchor(), anchor);
    assert_eq!(portrait.frame.pixels, original);
    assert_eq!(portrait.turns.pending.len(), 2);
}
