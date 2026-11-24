use super::*;
use crate::app::tests::Fixture;
use primitive_app::App;

pub(super) fn center(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}
pub(super) fn fixture(variable: Option<&str>, pdf: bool) -> (Fixture, Reader, PathBuf, bool) {
    let fixture = Fixture::new();
    let external = variable.and_then(std::env::var_os).map(PathBuf::from);
    let path = external.clone().unwrap_or_else(|| {
        if pdf {
            let p = fixture.root.join("Estuary.pdf");
            std::fs::write(&p, crate::fixed::tests::document()).unwrap();
            p
        } else {
            fixture.book.clone()
        }
    });
    let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
    let mut reader =
        Reader::open(path.parent().unwrap(), &fixture.state, vec![font.clone()]).unwrap();
    reader.open_path(&path).unwrap();
    (fixture, reader, font, external.is_some())
}

fn capture(reader: &Reader, canvas: &mut Canvas, env: &str, external: bool) {
    if !external && let Some(path) = std::env::var_os(env) {
        let frame = primitive_app::frame(reader, canvas, false);
        primitive_app::write_png(std::path::Path::new(&path), WIDTH, HEIGHT, &frame).unwrap();
    }
}

#[test]
fn tap_saved_epub_highlights_chooses_overlaps_and_reads_and_edits_the_note_in_place() {
    let (_fixture, mut app, font, external) = fixture(Some("READER_TEST_TAP_EPUB"), false);
    let chapter = if external {
        std::env::var("READER_TEST_TAP_CHAPTER")
            .unwrap_or_else(|_| "1".into())
            .parse::<usize>()
            .unwrap()
    } else {
        1
    };
    let probe = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .chapter(chapter)
        .unwrap();
    let (block, words) = if external {
        probe
            .source
            .blocks
            .iter()
            .find_map(|block| {
                let words = crate::selection::word_ranges(&block.text).collect::<Vec<_>>();
                let chosen = words.windows(3).find(|words| {
                    words
                        .iter()
                        .all(|word| block.text[word.clone()].chars().any(char::is_alphanumeric))
                        && block.links.iter().all(|link| {
                            link.bytes.end <= words[0].start || words[2].end <= link.bytes.start
                        })
                })?;
                Some((block, chosen.to_vec()))
            })
            .expect("The selected chapter needs three consecutive unlinked words")
    } else {
        let anchor = probe.fragment("door").unwrap();
        let block = probe
            .source
            .blocks
            .iter()
            .find(|b| b.key == anchor.block)
            .unwrap();
        (
            block,
            crate::selection::word_ranges(&block.text).take(7).collect(),
        )
    };
    let span = |from: usize, to: usize| {
        probe
            .select_range(
                &probe.anchor(block.key, words[from].start),
                &probe.anchor(block.key, words[to].end),
            )
            .unwrap()
    };
    let middle = if external { 1 } else { 2 };
    let overlap = span(middle, middle);
    let long_note = [
            "The open door appears twice in this chapter. Both times Linh leaves the notebook beside it. On my next reading, I want to follow where she puts her hands as the light changes.",
            "The harbor is easier to hear after dusk. In the first chapter she measures one step against the tide. Here, beside the same water, she waits long enough to hear the boats coming home.",
            "For the reading group, compare the lantern beside the door with the lamps across the water. Does either light change what Linh can see, or only what she expects to find?"
        ].join("\n\n");
    app.selection = Some(span(0, if external { 1 } else { 6 }));
    app.save_highlight(Some(long_note.clone())).unwrap();
    app.selection = Some(span(middle, if external { 2 } else { 6 }));
    app.save_highlight(None).unwrap();
    app.restore_position(&ReadingPosition {
        chapter,
        page: 0,
        anchor: Some(overlap.start.clone()),
    })
    .unwrap();
    let raw = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&overlap)[0];
    let hit = Input::Tap(
        (raw.x + raw.width / 2) as f32,
        (raw.y + raw.height / 2) as f32,
    );
    let origin = app.current_history_location().unwrap();
    let history = app.back.clone();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let frame = primitive_app::frame(&app, &mut canvas, false);
    app.input(hit);
    let p = app.saved_popup.as_ref().unwrap();
    assert!(p.view == View::Choices && p.hits == [0, 1]);
    capture(&app, &mut canvas, "READER_TAP_OVERLAP_SCREENSHOT", external);
    let row = center(app.saved_layout().row(0));
    app.input(row);
    assert!(
        app.saved_popup
            .as_ref()
            .is_some_and(|p| p.view == View::Note(0))
    );
    let _ = primitive_app::frame(&app, &mut canvas, false);
    assert!(app.saved_popup.as_ref().unwrap().pages.get() > 1);
    capture(&app, &mut canvas, "READER_TAP_NOTE_SCREENSHOT", external);
    app.input(Input::PageForward);
    assert_eq!(app.saved_popup.as_ref().unwrap().page, 1);
    app.input(Input::PageBack);
    assert_eq!(app.saved_popup.as_ref().unwrap().page, 0);
    let edit = center(app.saved_layout().button(1, 4));
    app.input(edit);
    assert!(app.screen == Screen::EditNote && app.editor.string() == long_note.trim());
    app.input(Input::PageBack);
    assert!(app.saved_popup_over_book());
    let close = center(app.saved_layout().close());
    app.input(close);
    assert!(
        app.saved_popup.is_none()
            && app.back == history
            && app.current_history_location().as_ref() == Some(&origin)
    );
    assert!(primitive_app::frame(&app, &mut canvas, false) == frame);
    app.input(hit);
    let row = center(app.saved_layout().row(1));
    app.input(row);
    let p = app.saved_popup.as_ref().unwrap();
    assert!(p.view == View::Actions(1));
    let extend = center(app.saved_layout().manage_button(true));
    app.input(extend);
    assert!(app.range_edit.is_some());
    app.input(Input::Tap(125.0, HEIGHT as f32 - 43.0));
    assert!(
        app.saved_popup_over_book() && app.saved_popup.as_ref().unwrap().view == View::Actions(1)
    );
    assert!(app.current_history_location().as_ref() == Some(&origin));
    let remove = center(app.saved_layout().button(1, 2));
    app.input(remove);
    assert!(app.saved_popup.as_ref().unwrap().view == View::RemoveMark(1));
    let confirm = center(app.saved_layout().button(1, 2));
    app.input(confirm);
    assert!(app.saved_popup.is_none() && app.screen == Screen::Book);
    app.input(hit);
    assert!(app.saved_popup.as_ref().unwrap().view == View::Note(0));
    app.input(Input::Tap(18.0, 580.0));
    assert!(app.saved_popup.is_none());
    app.input(Input::Tap(30.0, 890.0));
    app.tick(Instant::now() + Duration::from_secs(2));
    assert!(app.saved_popup.is_none());
    assert!(app.current_history_location().as_ref() != Some(&origin));
}

#[test]
fn tap_saved_pdf_highlights_matches_zoomed_page_geometry_and_preserves_the_viewport() {
    let (_fixture, mut app, font, external) = fixture(Some("READER_TEST_TAP_PDF"), true);
    let page = if external {
        std::env::var("READER_TEST_TAP_PAGE")
            .unwrap_or_else(|_| "1".into())
            .parse::<usize>()
            .unwrap()
    } else {
        1
    };
    let fixed = app.fixed.as_mut().unwrap();
    let id = fixed.id;
    let text = fixed.reading.page_text(page).unwrap();
    let word = text
        .words
        .iter()
        .find(|w| w.bounds.x1 - w.bounds.x0 > 0.007 && w.bounds.y1 - w.bounds.y0 > 0.004)
        .unwrap();
    let bounds = word.bounds;
    let quote = &text.text[word.bytes.clone()];
    let state = app.library.remember(&fixed.entry, id, &fixed.entry.title);
    for note in [Some("Sounding at the jetty".into()), None] {
        state
            .add_pdf_highlight(PdfHighlight {
                chapter_title: None,
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                page,
                bounds: vec![bounds],
                continuation: vec![],
                text: quote.into(),
                note,
            })
            .unwrap();
    }
    let mut view = fixed.reading.preferences();
    view.zoom = 4;
    fixed.reading.restore_view(view).unwrap();
    app.jump_fixed(page, FixedJump::regions(&[bounds])).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    assert_eq!(fixed.reading.preferences().zoom, 4);
    let p = fixed
        .reading
        .visible_rectangle(page, bounds)
        .unwrap()
        .center();
    let hit = Input::Tap(p.x as f32, p.y as f32);
    let origin = app.current_history_location().unwrap();
    let history = app.back.clone();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let frame = primitive_app::frame(&app, &mut canvas, false);
    app.input(hit);
    assert!(
        app.saved_popup
            .as_ref()
            .is_some_and(|p| p.view == View::Choices && p.hits.len() == 2)
    );
    let row = center(app.saved_layout().row(0));
    app.input(row);
    capture(
        &app,
        &mut canvas,
        "READER_TAP_ZOOMED_PDF_SCREENSHOT",
        external,
    );
    assert!(
        app.saved_popup
            .as_ref()
            .is_some_and(|p| p.view == View::Note(0))
    );
    let close = center(app.saved_layout().close());
    app.input(close);
    assert!(app.current_history_location().as_ref() == Some(&origin) && app.back == history);
    assert!(primitive_app::frame(&app, &mut canvas, false) == frame);
    // The gesture outside the selected word still goes to the document.
    let left = (22.0_f32, 820.0_f32);
    let bounds = app
        .fixed
        .as_ref()
        .unwrap()
        .reading
        .visible_rectangle(page, bounds)
        .unwrap();
    let y = if bounds.contains(hayro::kurbo::Point::new(left.0 as f64, left.1 as f64)) {
        1100.0
    } else {
        left.1
    };
    let before_page = app.fixed.as_ref().unwrap().reading.page();
    let before_view = app.fixed.as_ref().unwrap().reading.preferences();
    app.input(Input::Tap(left.0, y));
    app.tick(Instant::now() + Duration::from_secs(2));
    assert!(app.saved_popup.is_none());
    let reading = &app.fixed.as_ref().unwrap().reading;
    assert_eq!(reading.page(), before_page);
    let after_view = reading.preferences();
    assert!(
        after_view.offset_x != before_view.offset_x || after_view.offset_y != before_view.offset_y
    );
}

#[test]
fn taps_on_saved_text_links_follow_the_link_before_opening_the_highlight() {
    let (_fixture, mut app, _, _) = fixture(None, false);
    let active = app.active.as_ref().unwrap();
    let block = active
        .reading
        .probe
        .source
        .blocks
        .iter()
        .find(|b| b.links.iter().any(|l| l.href.ends_with("#tide-note")))
        .unwrap();
    let link = block
        .links
        .iter()
        .find(|l| l.href.ends_with("#tide-note"))
        .unwrap();
    let probe = &active.reading.probe;
    let selection = probe
        .select_range(
            &probe.anchor(block.key, link.bytes.start),
            &probe.anchor(block.key, link.bytes.end),
        )
        .unwrap();
    app.selection = Some(selection.clone());
    app.save_highlight(None).unwrap();
    app.restore_position(&ReadingPosition {
        chapter: 0,
        page: 0,
        anchor: Some(selection.start.clone()),
    })
    .unwrap();
    let rect = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&selection)[0];
    let input = Input::Tap(
        (rect.x + rect.width / 2) as f32,
        (rect.y + rect.height / 2) as f32,
    );
    let Input::Tap(x, y) = input else {
        unreachable!()
    };
    assert!(app.active.as_ref().unwrap().reading.link_at(x, y).is_some());
    let origin = app.current_history_location().unwrap();
    app.input(input);
    assert!(app.saved_popup.is_none());
    assert!(
        app.footnote.is_some()
            || (app.current_history_location().as_ref() != Some(&origin)
                && app.back.contains(&origin))
    );
}

#[test]
fn multiple_boxes_hit_once_and_a_note_can_be_removed_without_removing_the_passage() {
    let (_fixture, mut app, font, _) = fixture(None, true);
    let active = app.fixed.as_mut().unwrap();
    let id = active.id;
    let layer = active.reading.page_text(1).unwrap();
    let word = &layer.words[0];
    let state = app.library.remember(&active.entry, id, &active.entry.title);
    for index in 0..6 {
        state
            .add_pdf_highlight(PdfHighlight {
                chapter_title: None,
                color: Default::default(),
                created_at: None,
                style: Default::default(),
                page: 1,
                bounds: vec![word.bounds, word.bounds],
                text: layer.text[word.bytes.clone()].into(),
                note: Some(format!("Observation {index}")),
                continuation: vec![],
            })
            .unwrap();
    }
    app.jump_fixed(1, FixedJump::regions(&[word.bounds]))
        .unwrap();
    let pos = app
        .fixed
        .as_ref()
        .unwrap()
        .reading
        .visible_rectangle(1, word.bounds)
        .unwrap()
        .center();
    app.input(Input::Tap(pos.x as f32, pos.y as f32));
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let _ = primitive_app::frame(&app, &mut canvas, false);
    assert_eq!(app.saved_popup.as_ref().unwrap().hits.len(), 6);
    app.input(Input::PageForward);
    let row = center(app.saved_layout().row(0));
    app.input(row);
    let p = app.saved_popup.as_ref().unwrap();
    assert!(p.view == View::Note(4));
    let action = center(app.saved_layout().button(1, 2));
    app.input(action);
    let remove = center(app.saved_layout().button(0, 2));
    app.input(remove);
    let confirm = center(app.saved_layout().button(1, 2));
    app.input(confirm);
    assert!(app.saved_popup.as_ref().unwrap().view == View::Actions(4));
    let marks = &app.library.state(id).unwrap().pdf_highlights;
    assert!(
        marks.len() == 6
            && marks[4].note.is_none()
            && marks[3].note.is_some()
            && marks[5].note.is_some()
    );
}

#[test]
fn an_annotated_pdf_link_in_a_custom_gesture_corner_still_follows_the_destination() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Cove map.pdf");
    let content = "BT /F1 12 Tf 4 480 Td (Cove) Tj ET\n";
    let stream = format!(
        "<< /Length {} >>\nstream\n{content}endstream",
        content.len()
    );
    let original = crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 380 500] /Annots [5 0 R] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 380 500] >>",
        "<< /Type /Annot /Subtype /Link /Rect [0 445 55 500] /Dest [4 0 R /Fit] >>",
        &stream,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]);
    std::fs::write(&path, original).unwrap();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.open_path(&path).unwrap();
    let fixed = app.fixed.as_ref().unwrap();
    let text = fixed.reading.page_text(0).unwrap();
    let word = &text.words[0];
    let point = fixed
        .reading
        .visible_rectangle(0, word.bounds)
        .unwrap()
        .center();
    assert!(
        fixed
            .reading
            .link_at(point.x as f32, point.y as f32)
            .is_some()
    );
    app.library
        .remember(&fixed.entry, fixed.id, &fixed.entry.title)
        .add_pdf_highlight(PdfHighlight {
            chapter_title: None,
            color: Default::default(),
            created_at: None,
            style: Default::default(),
            page: 0,
            bounds: vec![word.bounds],
            text: text.text[word.bytes.clone()].into(),
            note: None,
            continuation: vec![],
        })
        .unwrap();
    app.gestures
        .set(Gesture::TapTopLeft, Binding::Contents)
        .unwrap();
    let hit = Input::Tap(point.x as f32, point.y as f32);
    assert!(app.assigned_gesture(hit, None).is_some());
    assert_eq!(app.screen, Screen::FixedContents);
    app.show(Screen::Fixed);
    app.input(hit);
    assert!(app.screen == Screen::Fixed && app.saved_popup.is_none());
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 1);
    app.activate(Action::ReturnToLocation).unwrap();
    assert_eq!(app.fixed.as_ref().unwrap().reading.page(), 0);
}

#[test]
fn selection_viewport_pdf_notes_hit_the_painted_marker_and_restore_the_reading_page() {
    use crate::app::selection_popup::SelectionAction;
    use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap};
    use primitive_ui::Key;

    let fixture = Fixture::new();
    let file = fixture.root.join("Harbor Survey.pdf");
    let content = "BT /F1 24 Tf 52 358 Td (Harbor Survey) Tj /F1 16 Tf 0 -44 Td (The tide reached the outer jetty at 15:30.) Tj 0 -29 Td (Clear weather let us observe the moored boats.) Tj ET";
    fs::write(&file, crate::fixed::tests::pdf_objects(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 622 430] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ])).unwrap();
    let fonts = [
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
    app.open_path(&file).unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app.controls.note_marker = NoteMarker::Sidemark;
    let mut canvas = adopt(&mut app, ReadingViewport::LANDSCAPE, 2);
    let original = app.current_history_location();
    let reading = &app.fixed.as_ref().unwrap().reading;
    let text = reading.page_text(0).unwrap();
    let word = text
        .words
        .iter()
        .find(|word| &text.text[word.bytes.clone()] == "tide")
        .unwrap();
    let word_bounds = word.bounds;
    let point = reading.visible_rectangle(0, word_bounds).unwrap().center();
    assert_eq!(
        app.input(Input::LongTap(point.x as f32, point.y as f32)),
        Change::Partial
    );
    assert_eq!(app.pdf_selection.as_ref().unwrap().text(), "tide");
    let note = app.selection_button(SelectionAction::Note);
    press(&app, &mut canvas, note, false);
    tap(&mut app, note);
    for ch in "tidal mark".chars() {
        let rect = key(
            &app,
            if ch == ' ' { Key::Space } else { Key::Char(ch) },
            "Save",
        );
        tap(&mut app, rect);
    }
    let save = key(&app, Key::Go, "Save");
    assert!(save.x > WIDTH as i32);
    tap(&mut app, save);
    assert_eq!(app.screen, Screen::Fixed);

    let reading = &app.fixed.as_ref().unwrap().reading;
    let area = reading.annotation_rectangle(0, word_bounds).unwrap();
    let rect = crate::selection::HighlightRect {
        x: area.x0.floor() as i32,
        y: area.y0.floor() as i32,
        width: area.width().ceil() as i32,
        height: area.height().ceil() as i32,
    };
    let width = app.reading_viewport().dimensions().0;
    let margin = crate::note_marker::margin(width, 0, 0, NoteMarker::Sidemark, true);
    let marker =
        crate::note_marker::bounds(rect, NoteMarker::Sidemark, margin, true, width).unwrap();
    assert!(marker.x > WIDTH as i32);
    tap(
        &mut app,
        Rect::new(marker.x, marker.y, marker.width, marker.height),
    );
    assert!(app.saved_popup.as_ref().unwrap().view == View::Note(0));
    assert_eq!(app.mark_text(0), Some(("tide", Some("tidal mark"))));
    let close = app.saved_layout().close();
    assert!(close.right() > WIDTH as i32);
    app.controls.night_pages = true;
    press(&app, &mut canvas, close, true);
    capture(&app, &mut canvas, "landscape-saved-note-dark.png", true);

    let edit = app.saved_layout().button(0, 2);
    tap(&mut app, edit);
    assert_eq!(app.screen, Screen::EditNote);
    let back = app.reading_editor_back();
    tap(&mut app, back);
    assert!(app.saved_popup_over_book());
    tap(&mut app, close);
    assert!(app.saved_popup.is_none());
    assert_eq!(app.current_history_location(), original);
}
