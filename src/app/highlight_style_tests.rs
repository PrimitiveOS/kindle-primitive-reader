use super::*;
use crate::app::saved_highlight_popup::tests::{center, fixture};
use crate::highlight_style::HighlightStyle;

fn choose_default(app: &mut Reader) {
    app.open_reading_edge(true, 630.);
    if app.top_group != Some(TopGroup::Settings) {
        let options = app.reading_button("Settings");
        app.input(center(options));
    }
    let picker = app.reading_button("Highlight style");
    assert_eq!(app.input(center(picker)), Change::Partial);
    let popup = app.saved_popup.as_ref().unwrap();
    let button = popup.style_cell(1);
    app.input(center(button));
    assert_eq!(app.effective_highlight_style(), HighlightStyle::Underline);
    let close = app.saved_popup.as_ref().unwrap().close();
    app.input(center(close));
}

fn check_styles(
    app: &mut Reader,
    font: &Path,
    hit: Input,
    rect: Rect,
    fill_rows: std::ops::Range<i32>,
    original: &[u8],
    external: bool,
    bookmark_note: &str,
) {
    let position = app.current_history_location().unwrap();
    let fixed = app.fixed.is_some();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    assert_eq!(app.mark_style(0), Some(HighlightStyle::Underline));
    let mut checks = 0;
    for (index, style) in HighlightStyle::ALL.into_iter().enumerate() {
        assert_eq!(app.input(hit), Change::Partial);
        assert!(matches!(
            app.saved_popup.as_ref().unwrap().view,
            View::Note(0)
        ));
        let actions = app.saved_popup.as_ref().unwrap().button(1, 2);
        app.input(center(actions));
        let show = app.saved_popup.as_ref().unwrap().style_action();
        app.input(center(show));
        let popup = app.saved_popup.as_ref().unwrap();
        assert!(matches!(popup.view, View::Styles(Some(0))));
        if !external && !fixed && index == 1 {
            crate::app::tests::capture_test_screen(
                app,
                &[font.to_owned()],
                "READER_HIGHLIGHT_STYLE_SCREEN",
                false,
            );
        }
        let choose = popup.style_cell(index);
        app.input(center(choose));
        assert!(app.saved_popup.is_none());
        assert_eq!(app.mark_style(0), Some(style));
        assert_eq!(app.mark_text(0).unwrap().1, Some(bookmark_note));
        assert_eq!(app.current_history_location().unwrap(), position);
        for night in [false, true] {
            app.controls.night_pages = night;
            let pixels = primitive_app::frame(app, &mut canvas, night);
            let x0 = rect.x + 2;
            let x1 = rect.right() - 2;
            let y0 = rect.y - 2;
            let y1 = rect.bottom() + 5;
            for y in y0.max(0)..y1.min(PAGE_HEIGHT as i32) {
                for x in x0.max(0)..x1.min(WIDTH as i32) {
                    let k = (y as usize) * WIDTH as usize + x as usize;
                    let src = original[k];
                    let changed = match style {
                        HighlightStyle::Lighten if fill_rows.contains(&y) => {
                            ((u16::from(src) * 204 + 127) / 255) as u8
                        }
                        HighlightStyle::Invert if fill_rows.contains(&y) => 255 - src,
                        HighlightStyle::Underline
                            if (rect.bottom() - 1..rect.bottom() + 3).contains(&y) =>
                        {
                            68
                        }
                        HighlightStyle::Strikethrough => {
                            let start = rect.y + rect.h / 2 + 1 + if fixed { 2 } else { 0 };
                            if (start..start + 3).contains(&y) {
                                0
                            } else {
                                src
                            }
                        }
                        _ => src,
                    };
                    let expected = if night { 255 - changed } else { changed };
                    assert_eq!(
                        pixels[k], expected,
                        "{style:?} fixed={fixed} night={night} rect={rect:?} x={x} y={y}"
                    );
                    checks += 1;
                }
            }
        }
        app.controls.night_pages = false;
    }
    assert!(checks > 120);
    let id = app.open_book_id().unwrap();
    app.save().unwrap();
    assert_eq!(
        app.library.state(id).unwrap().highlight_style,
        Some(HighlightStyle::Underline)
    );
    println!(
        "highlight-style-proof fixed={fixed} style_cases=4 palettes=2 sample_pixels={checks} source_restored=1 note_retained=1"
    );
}

#[test]
fn highlight_styles_epub_all_four_drawers_follow_the_reading_bar_and_source() {
    let (_scratch, mut app, font, external) = fixture(Some("READER_STYLE_EPUB"), false);
    let chapter = std::env::var("READER_STYLE_CHAPTER")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let probe = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .chapter(chapter)
        .unwrap();
    let (block, word) = probe
        .source
        .blocks
        .iter()
        .filter(|b| b.links.is_empty() && b.text.len() > 20)
        .find_map(|b| {
            crate::selection::word_ranges(&b.text)
                .find(|w| b.text[w.clone()].chars().count() >= 5)
                .map(|w| (b, w))
        })
        .unwrap();
    let mark = probe
        .select_range(
            &probe.anchor(block.key, word.start),
            &probe.anchor(block.key, word.end),
        )
        .unwrap();
    app.restore_position(&ReadingPosition {
        chapter,
        page: 0,
        anchor: Some(mark.start.clone()),
    })
    .unwrap();
    choose_default(&mut app);
    let bounds = app.active.as_ref().unwrap().reading.highlight_rects(&mark)[0];
    let rect = Rect::new(bounds.x, bounds.y, bounds.width, bounds.height);
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    app.notice = None;
    let original = primitive_app::frame(&app, &mut canvas, false);
    let note = "Return to this sentence.";
    app.selection = Some(mark.clone());
    app.save_highlight(Some(note.into())).unwrap();
    app.notice = None;
    let hit = center(rect);
    check_styles(
        &mut app,
        &font,
        hit,
        rect,
        rect.y..rect.bottom(),
        &original,
        external,
        note,
    );
    let id = app.open_book_id().unwrap();
    assert_eq!(app.library.state(id).unwrap().highlights[0].selection, mark);
    let reopened = Reader::open(&_scratch.root, &_scratch.state, vec![font]).unwrap();
    let saved = reopened.library.state(id).unwrap();
    assert_eq!(saved.highlights[0].style, HighlightStyle::Invert);
    assert_eq!(saved.highlights[0].selection, mark);
    assert_eq!(saved.highlights[0].note.as_deref(), Some(note));
}

#[test]
fn highlight_styles_pdf_at_high_zoom_preserve_page_note_and_pixels() {
    let (_scratch, mut app, font, external) = fixture(Some("READER_STYLE_PDF"), true);
    let page = std::env::var("READER_STYLE_PAGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let fixed = app.fixed.as_mut().unwrap();
    let text = fixed.reading.page_text(page).unwrap();
    let word = text
        .words
        .iter()
        .find(|w| w.bounds.x1 - w.bounds.x0 > 0.007 && w.bounds.y1 - w.bounds.y0 > 0.004)
        .unwrap();
    let bounds = word.bounds;
    let quote = text.text[word.bytes.clone()].to_owned();
    let mut view = fixed.reading.preferences();
    view.zoom = 4;
    fixed.reading.restore_view(view).unwrap();
    app.jump_fixed(page, FixedJump::regions(&[bounds])).unwrap();
    app.pdf_result_regions.clear();
    choose_default(&mut app);
    let fixed = app.fixed.as_ref().unwrap();
    let r = fixed.reading.visible_rectangle(page, bounds).unwrap();
    let rect = Rect::new(
        r.x0.floor() as i32,
        r.y0.floor() as i32,
        r.width().ceil() as i32,
        r.height().ceil() as i32,
    );
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    app.notice = None;
    let original = primitive_app::frame(&app, &mut canvas, false);
    let note = "Check this figure.";
    let state = app
        .library
        .remember(&fixed.entry, fixed.id, &fixed.entry.title);
    state
        .add_pdf_highlight(PdfHighlight {
            style: state.highlight_style.unwrap(),
            page,
            bounds: vec![bounds],
            text: quote,
            note: Some(note.into()),
            continuation: Vec::new(),
        })
        .unwrap();
    check_styles(
        &mut app,
        &font,
        center(rect),
        rect,
        ((r.y0 - 0.5).ceil() as i32)..((r.y1 - 0.5).ceil() as i32),
        &original,
        external,
        note,
    );
    let mark = &app
        .library
        .state(app.open_book_id().unwrap())
        .unwrap()
        .pdf_highlights[0];
    assert_eq!((mark.page, &mark.bounds), (page, &vec![bounds]));
    assert_eq!(app.fixed.as_ref().unwrap().reading.preferences().zoom, 4);
}

#[test]
fn highlight_styles_overlap_changes_only_the_selected_saved_mark_and_survive_old_state() {
    let (_scratch, mut app, font, _) = fixture(None, false);
    let chapter = 1;
    let probe = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .chapter(chapter)
        .unwrap();
    let anchor = probe.fragment("door").unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.key == anchor.block)
        .unwrap();
    let words: Vec<_> = crate::selection::word_ranges(&block.text).take(7).collect();
    let span = |lo: usize, hi: usize| {
        probe
            .select_range(
                &probe.anchor(block.key, words[lo].start),
                &probe.anchor(block.key, words[hi].end),
            )
            .unwrap()
    };
    let first = span(0, 2);
    let second = span(1, 3);
    let middle = span(1, 1);
    app.restore_position(&ReadingPosition {
        chapter,
        page: 0,
        anchor: Some(middle.start.clone()),
    })
    .unwrap();
    let id = app.open_book_id().unwrap();
    let b = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&middle)[0];
    let hit = center(Rect::new(b.x, b.y, b.width, b.height));
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let original = primitive_app::frame(&app, &mut canvas, false);
    for (mark, note) in [(&first, "First note"), (&second, "Second note")] {
        app.selection = Some(mark.clone());
        app.save_highlight(Some(note.into())).unwrap();
    }
    app.notice = None;
    let state = app.library.state(id).unwrap();
    let old = serde_json::to_value(state).unwrap();
    assert!(old.get("highlight_style").is_none());
    assert!(
        old["highlights"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h.get("style").is_none())
    );
    let loaded: crate::library::BookState = serde_json::from_value(old.clone()).unwrap();
    assert!(
        loaded
            .highlights
            .iter()
            .all(|m| m.style == HighlightStyle::Lighten)
    );
    assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
    for index in [1, 0] {
        app.input(hit);
        let p = app.saved_popup.as_ref().unwrap();
        assert_eq!(p.hits, vec![0, 1]);
        app.input(center(p.row(index)));
        let p = app.saved_popup.as_ref().unwrap();
        app.input(center(p.button(1, 2)));
        let p = app.saved_popup.as_ref().unwrap();
        app.input(center(p.style_action()));
        let p = app.saved_popup.as_ref().unwrap();
        app.input(center(p.style_cell(3)));
        if index == 1 {
            assert_eq!(app.mark_style(0), Some(HighlightStyle::Lighten));
        }
    }
    let marked = primitive_app::frame(&app, &mut canvas, false);
    for y in b.y..b.y + b.height {
        let start = (y as usize) * WIDTH as usize + b.x as usize;
        assert_eq!(
            &marked[start..start + b.width as usize],
            &original[start..start + b.width as usize]
        );
    }
    app.open_highlight_styles(None);
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(2)));
    app.input(center(app.saved_popup.as_ref().unwrap().button(1, 2)));
    assert_eq!(app.controls.highlight_style, HighlightStyle::Strikethrough);
    app.open_highlight_styles(None);
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(1)));
    app.input(center(app.saved_popup.as_ref().unwrap().button(0, 2)));
    assert_eq!(
        app.effective_highlight_style(),
        HighlightStyle::Strikethrough
    );
    assert!(app.library.state(id).unwrap().highlight_style.is_none());
    app.save().unwrap();
    let reopened = Reader::open(&_scratch.root, &_scratch.state, vec![font]).unwrap();
    assert_eq!(
        reopened.controls.highlight_style,
        HighlightStyle::Strikethrough
    );
    let styled = &reopened.library.state(id).unwrap().highlights;
    for (mark, (selection, note)) in styled
        .iter()
        .zip([(&first, "First note"), (&second, "Second note")])
    {
        assert_eq!(
            (&mark.selection, mark.note.as_deref(), mark.style),
            (selection, Some(note), HighlightStyle::Invert)
        );
    }
}

#[test]
fn highlight_styles_on_one_annotated_reading_page() {
    let (_scratch, mut app, font, _) = fixture(None, false);
    let probe = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .chapter(1)
        .unwrap();
    let anchor = probe.fragment("door").unwrap();
    let block = probe
        .source
        .blocks
        .iter()
        .find(|b| b.key == anchor.block)
        .unwrap();
    let words: Vec<_> = crate::selection::word_ranges(&block.text).collect();
    let ranges = [(0, 3), (5, 7), (8, 11), (13, 16)];
    let marks: Vec<_> = ranges
        .into_iter()
        .map(|(lo, hi)| {
            probe
                .select_range(
                    &probe.anchor(block.key, words[lo].start),
                    &probe.anchor(block.key, words[hi].end),
                )
                .unwrap()
        })
        .collect();
    app.restore_position(&ReadingPosition {
        chapter: 1,
        page: 0,
        anchor: Some(anchor),
    })
    .unwrap();
    for (mark, style) in marks.iter().zip(HighlightStyle::ALL) {
        let id = app.open_book_id().unwrap();
        app.library.state_mut(id).unwrap().highlight_style = Some(style);
        app.selection = Some(mark.clone());
        app.save_highlight(None).unwrap();
    }
    app.notice = None;
    let id = app.open_book_id().unwrap();
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .highlights
            .iter()
            .map(|m| m.style)
            .collect::<Vec<_>>(),
        HighlightStyle::ALL
    );
    crate::app::tests::capture_test_screen(&app, &[font], "READER_HIGHLIGHT_PAGE_SCREEN", false);
}
