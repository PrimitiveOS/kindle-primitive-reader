use super::tests::{center, fixture};
use super::*;
use crate::highlight_style::HighlightStyle;

fn reading_settings(app: &mut Reader) {
    app.open_reading_edge(true, 630.);
    if app.top_group != Some(TopGroup::Settings) {
        let options = app.reading_button("Settings");
        app.input(center(options));
    }
    let picker = app.reading_button("Highlight style");
    assert_eq!(app.input(center(picker)), Change::Partial);
}

fn choose_amount(app: &mut Reader, amount: Amount) {
    reading_settings(app);
    let p = app.saved_popup.as_ref().unwrap();
    let entry = p.amount_entry(match amount {
        Amount::HighlightOpacity => 0,
        Amount::SelectionOpacity => 1,
    });
    assert_eq!(app.input(center(entry)), Change::Partial);
    assert!(matches!(app.saved_popup.as_ref().unwrap().view,View::Adjust(v) if v==amount));
}

fn press(app: &mut Reader, plus: bool, hold: bool) {
    let area = app.saved_popup.as_ref().unwrap().adjust(plus);
    let Input::Tap(x, y) = center(area) else {
        unreachable!()
    };
    app.input(if hold {
        Input::LongTap(x, y)
    } else {
        Input::Tap(x, y)
    });
}

fn dismiss(app: &mut Reader) {
    let close = app.saved_popup.as_ref().unwrap().close();
    app.input(center(close));
    assert!(app.saved_popup.is_none());
}

fn set_appearance(app: &mut Reader) {
    choose_amount(app, Amount::HighlightOpacity);
    for _ in 0..2 {
        press(app, true, false);
    }
    press(app, false, true);
    assert_eq!(app.controls.highlight_appearance.highlight_opacity, 35);
    dismiss(app);
    choose_amount(app, Amount::SelectionOpacity);
    for _ in 0..4 {
        press(app, true, false);
    }
    assert_eq!(app.controls.highlight_appearance.selection_opacity, 60);
    dismiss(app);
}

fn mixed(src: u8, pct: u8) -> u8 {
    let alpha = (f64::from(pct) / 100.0 * 255.0).floor();
    (f64::from(src) * (255.0 - alpha) / 255.0).round() as u8
}

pub(super) fn original_saved_opacity(
    app: &mut Reader,
    font: &Path,
    rect: Rect,
    fill: std::ops::Range<i32>,
    original: &[u8],
) {
    // Enable gray as the book's drawer; the saved passage itself is edited separately.
    reading_settings(app);
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(0)));
    dismiss(app);
    app.open_highlight_styles(Some(0));
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(0)));
    let id = app.open_book_id().unwrap();
    let location = app.current_history_location().unwrap();
    let before = app.library.state(id).unwrap().clone();
    set_appearance(app);
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let pixels = primitive_app::frame(app, &mut canvas, false);
    let mut count = 0;
    for y in fill.start.max(0)..fill.end.min(PAGE_HEIGHT as i32) {
        for x in (rect.x + 2).max(0)..(rect.right() - 2).min(WIDTH as i32) {
            let k = y as usize * WIDTH as usize + x as usize;
            assert_eq!(pixels[k], mixed(original[k], 35));
            count += 1;
        }
    }
    assert!(count > 200);
    assert_eq!(location, app.current_history_location().unwrap());
    let after = app.library.state(id).unwrap();
    assert_eq!(before.highlights, after.highlights);
    assert_eq!(before.pdf_highlights, after.pdf_highlights);
    println!(
        "highlight-opacity-original saved_pixels={count} existing_anchors_and_notes_preserved=1"
    );
    // Restore other tests' requested book and stored-mark style.
    reading_settings(app);
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(1)));
    dismiss(app);
    app.open_highlight_styles(Some(0));
    app.input(center(app.saved_popup.as_ref().unwrap().style_cell(3)));
    app.save().unwrap();
}

#[test]
fn highlight_opacity_changes_both_saved_and_new_marks_and_word_selection_independently() {
    let (fixture, mut app, font, _) = fixture(None, false);
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
    let words = crate::selection::word_ranges(&block.text)
        .take(8)
        .collect::<Vec<_>>();
    let select = |i: usize| {
        probe
            .select_range(
                &probe.anchor(block.key, words[i].start),
                &probe.anchor(block.key, words[i].end),
            )
            .unwrap()
    };
    let (saved, new, active) = (select(1), select(3), select(5));
    app.restore_position(&ReadingPosition {
        chapter: 1,
        page: 0,
        anchor: Some(anchor),
    })
    .unwrap();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let origin = app.current_history_location().unwrap();
    let before = primitive_app::frame(&app, &mut canvas, false);
    app.selection = Some(saved.clone());
    app.save_highlight(Some("First".into())).unwrap();
    app.notice = None;
    set_appearance(&mut app);
    app.selection = Some(new.clone());
    app.save_highlight(Some("Second".into())).unwrap();
    app.notice = None;
    let bounds = |app: &Reader, mark: &crate::selection::TextSelection| {
        app.active.as_ref().unwrap().reading.highlight_rects(mark)[0]
    };
    let rect = bounds(&app, &active);
    let hold = Input::LongTap(
        (rect.x + rect.width / 2) as f32,
        (rect.y + rect.height / 2) as f32,
    );
    app.input(hold);
    assert!(app.selection.as_ref().is_some_and(|s| s == &active));
    let pixels = primitive_app::frame(&app, &mut canvas, false);
    for (sel, pct) in [(&saved, 35), (&new, 35), (&active, 60)] {
        let rect = bounds(&app, sel);
        let row = (rect.y + rect.height / 2) as usize * WIDTH as usize;
        let start = row + rect.x as usize;
        for k in start..start + rect.width as usize {
            assert_eq!(pixels[k], mixed(before[k], pct));
        }
    }
    app.selection = None;
    choose_amount(&mut app, Amount::HighlightOpacity);
    crate::app::tests::capture_test_screen(&app, &[font.clone()], "READER_OPACITY_SCREEN", false);
    let p = app.saved_popup.as_ref().unwrap();
    app.input(center(p.button(0, 2)));
    assert_eq!(app.controls.highlight_appearance.highlight_opacity, 20);
    let p = app.saved_popup.as_ref().unwrap();
    app.input(center(p.button(1, 2)));
    assert!(matches!(
        app.saved_popup.as_ref().unwrap().view,
        View::Styles(None)
    ));
    dismiss(&mut app);
    assert_eq!(app.current_history_location().unwrap(), origin);
    app.save().unwrap();
    let id = app.open_book_id().unwrap();
    let after = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    assert_eq!(after.controls.highlight_appearance.selection_opacity, 60);
    let marks = &after.library.state(id).unwrap().highlights;
    assert_eq!(marks.len(), 2);
    for (m, (expected, note)) in marks.iter().zip([(&saved, "First"), (&new, "Second")]) {
        assert_eq!(
            (&m.selection, m.note.as_deref(), m.style),
            (expected, Some(note), HighlightStyle::Lighten)
        );
    }
}
