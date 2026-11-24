use super::*;
use crate::app::saved_highlight_popup::tests::{center, fixture};
use crate::highlight_style::{HighlightColor, NoteMarker};
use primitive_app::App;

fn choose_color(app: &mut Reader, index: Option<usize>, color: HighlightColor) {
    app.open_highlight_styles(index);
    let entry = app.saved_layout().color_entry(index, false);
    assert_eq!(app.input(center(entry)), Change::Partial);
    if index.is_none()
        && color == HighlightColor::Red
        && app
            .active
            .as_ref()
            .is_some_and(|active| active.reading.probe.fragment("door").is_some())
    {
        if let Some(directory) = std::env::var_os("READER_HIGHLIGHT_COLOR_SCREENSHOTS") {
            let fonts: Vec<_> = app.fonts.iter().map(|p| p.display().to_string()).collect();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
            let pixels = primitive_app::frame(app, &mut canvas, false);
            primitive_app::write_png(
                &PathBuf::from(directory).join("color-picker.png"),
                WIDTH,
                HEIGHT,
                &pixels,
            )
            .unwrap();
        }
    }
    let cell = app.saved_layout().color_cell(
        HighlightColor::ALL
            .iter()
            .position(|&c| c == color)
            .unwrap(),
    );
    assert_eq!(app.input(center(cell)), Change::Partial);
}
fn note(app: &mut Reader, marker: NoteMarker) {
    app.open_highlight_styles(None);
    let entry = app.saved_layout().color_entry(None, true);
    app.input(center(entry));
    let cell = app
        .saved_layout()
        .style_cell(NoteMarker::ALL.iter().position(|&m| m == marker).unwrap());
    assert_eq!(app.input(center(cell)), Change::Partial);
    if marker == NoteMarker::Sideline
        && let Some(directory) = std::env::var_os("READER_HIGHLIGHT_COLOR_SCREENSHOTS")
    {
        let fonts: Vec<_> = app.fonts.iter().map(|p| p.display().to_string()).collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &fonts).unwrap();
        let pixels = primitive_app::frame(app, &mut canvas, false);
        primitive_app::write_png(
            &PathBuf::from(directory).join("note-picker.png"),
            WIDTH,
            HEIGHT,
            &pixels,
        )
        .unwrap();
    }
    let close = app.saved_layout().close();
    app.input(center(close));
}
fn save_epub_note(app: &mut Reader) -> crate::selection::TextSelection {
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(1, Some("door"))
        .unwrap();
    let reading = &app.active.as_ref().unwrap().reading;
    let first = reading.probe.fragment("door").unwrap();
    let block = reading
        .probe
        .source
        .blocks
        .iter()
        .find(|b| b.key == first.block)
        .unwrap();
    let last = reading.probe.anchor(block.key, block.text.len());
    let mark = reading.probe.select_range(&first, &last).unwrap();
    app.selection = Some(mark.clone());
    app.save_highlight(Some("Check the lantern at dusk.".into()))
        .unwrap();
    mark
}

#[test]
fn palette_changes_and_global_note_markers_preserve_each_book_mark_and_source() {
    let (fixture, mut app, font, _) = fixture(None, false);
    let original = std::fs::read(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .jump(1, Some("door"))
        .unwrap();
    choose_color(&mut app, None, HighlightColor::Red);
    let close = app.saved_layout().close();
    app.input(center(close));
    let selection = save_epub_note(&mut app);
    let id = app.open_book_id().unwrap();
    let origin = app.current_history_location();
    let created = app.library.state(id).unwrap().highlights[0].created_at;
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let source = reading.probe.capture(reading.page).unwrap();
    let rect = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&selection)[0];
    for (i, color) in HighlightColor::ALL.into_iter().enumerate() {
        choose_color(&mut app, Some(0), color);
        assert!(app.saved_popup.is_none());
        let mark = &app.library.state(id).unwrap().highlights[0];
        assert_eq!(mark.color, color);
        assert_eq!(mark.selection, selection);
        assert_eq!(mark.created_at, created);
        assert_eq!(app.current_history_location(), origin);
        assert_eq!(mark.note.as_deref(), Some("Check the lantern at dusk."));
        let pixels = primitive_app::frame(&app, &mut canvas, false);
        let tone = [106u16, 156, 231, 111, 203, 176, 88, 100, 204][i];
        let mut checked = 0;
        for y in (rect.y + 4).max(0)..(rect.y + rect.height - 4).min(PAGE_HEIGHT as i32) {
            for x in (rect.x + 2).max(0)..(rect.x + rect.width - 2).min(WIDTH as i32) {
                let k = y as usize * WIDTH as usize + x as usize;
                assert_eq!(
                    pixels[k],
                    ((u16::from(source[k]) * tone + 127) / 255) as u8,
                    "{color:?} x={x} y={y} rect={rect:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 100);
    }
    choose_color(&mut app, Some(0), HighlightColor::Blue);
    for marker in NoteMarker::ALL {
        note(&mut app, marker);
        assert_eq!(app.controls.note_marker, marker);
        for dark in [false, true] {
            app.controls.night_pages = dark;
            let image = primitive_app::frame(&app, &mut canvas, dark);
            assert!(image.iter().any(|&pixel| pixel != 255));
            if let Some(directory) = std::env::var_os("READER_HIGHLIGHT_COLOR_SCREENSHOTS") {
                let path = PathBuf::from(directory).join(format!(
                    "epub-{}-{}.png",
                    marker.label().replace(' ', "-"),
                    if dark { "dark" } else { "light" }
                ));
                primitive_app::write_png(&path, WIDTH, HEIGHT, &image).unwrap();
            }
        }
    }
    app.controls.night_pages = false;
    app.save().unwrap();
    let mut restored = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    restored.open_path(&fixture.book).unwrap();
    assert_eq!(restored.controls.note_marker, NoteMarker::Sidemark);
    assert_eq!(restored.effective_highlight_color(), HighlightColor::Red);
    assert_eq!(
        restored.library.state(id).unwrap().highlights[0].color,
        HighlightColor::Blue
    );
    assert_eq!(
        restored.library.state(id).unwrap().highlights[0].selection,
        selection
    );
    assert_eq!(std::fs::read(&fixture.book).unwrap(), original);
}

#[test]
fn tapping_a_side_note_symbol_opens_its_note_without_advancing_the_page() {
    let (_fixture, mut app, _font, _) = fixture(None, false);
    let selection = save_epub_note(&mut app);
    note(&mut app, NoteMarker::Sidemark);
    let rect = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .highlight_rects(&selection)[0];
    let reading = &app.active.as_ref().unwrap().reading;
    let x = crate::note_marker::margin(
        reading.probe.settings.width,
        reading.probe.settings.right_margin(),
        0,
        NoteMarker::Sidemark,
        false,
    );
    let origin = app.current_history_location();
    assert_eq!(
        app.input(Input::Tap((x + 16) as f32, (rect.y + 16) as f32)),
        Change::Partial
    );
    assert!(matches!(
        app.saved_popup.as_ref().unwrap().view,
        View::Note(0)
    ));
    assert_eq!(app.current_history_location(), origin);
}

fn save_pdf_note(app: &mut Reader) {
    let fixed = app.fixed.as_mut().unwrap();
    let page = 1;
    let text = fixed.reading.page_text(page).unwrap();
    let word = text
        .words
        .iter()
        .find(|w| {
            text.text[w.bytes.clone()]
                .chars()
                .any(char::is_alphanumeric)
        })
        .unwrap();
    let bounds = word.bounds;
    app.jump_fixed(page, FixedJump::regions(&[bounds])).unwrap();
    app.pdf_result_regions.clear();
    let fixed = app.fixed.as_ref().unwrap();
    let at = fixed
        .reading
        .visible_rectangle(page, bounds)
        .unwrap()
        .center();
    let crate::fixed_session::WordAtTouch::Ready(Some(selection)) =
        fixed.reading.select_word(at.x as f32, at.y as f32).unwrap()
    else {
        panic!("An actual PDF text word must be selectable")
    };
    app.pdf_selection = Some(selection);
    app.save_pdf_highlight(Some("Keep this original passage.".into()))
        .unwrap();
}

#[test]
fn pdf_note_indicators_follow_rotation_zoom_and_keep_the_saved_pdf_word() {
    let (fixture, mut app, font, _) = fixture(None, true);
    save_pdf_note(&mut app);
    let id = app.open_book_id().unwrap();
    let mark = app.library.state(id).unwrap().pdf_highlights[0].clone();
    choose_color(&mut app, Some(0), HighlightColor::Green);
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    for turns in [0, 1, 3] {
        let fixed = app.fixed.as_mut().unwrap();
        let mut prefs = fixed.reading.preferences();
        prefs.turns = turns;
        prefs.zoom = 2;
        fixed.reading.restore_view(prefs).unwrap();
        app.jump_fixed(mark.page, FixedJump::regions(&mark.bounds))
            .unwrap();
        app.pdf_result_regions.clear();
        let projected = app
            .fixed
            .as_ref()
            .unwrap()
            .reading
            .annotation_rectangle(mark.page, mark.bounds[0])
            .unwrap();
        let at = app.current_history_location();
        for marker in NoteMarker::ALL {
            note(&mut app, marker);
            for dark in [false, true] {
                app.controls.night_pages = dark;
                let pixels = primitive_app::frame(&app, &mut canvas, dark);
                assert_eq!(pixels.len(), (WIDTH * HEIGHT) as usize);
                if turns == 0
                    && let Some(directory) = std::env::var_os("READER_HIGHLIGHT_COLOR_SCREENSHOTS")
                {
                    primitive_app::write_png(
                        &PathBuf::from(directory).join(format!(
                            "pdf-{}-{}.png",
                            marker.label().replace(' ', "-"),
                            if dark { "dark" } else { "light" }
                        )),
                        WIDTH,
                        HEIGHT,
                        &pixels,
                    )
                    .unwrap();
                }
            }
            assert_eq!(app.current_history_location(), at);
            if marker == NoteMarker::Sidemark
                && projected.y0 >= 0.0
                && projected.y0 + 32.0 < PAGE_HEIGHT as f64
            {
                app.controls.night_pages = false;
                let x = crate::note_marker::margin(WIDTH, 0, 0, marker, true) + 16;
                assert_eq!(
                    app.input(Input::Tap(x as f32, projected.y0 as f32 + 16.)),
                    Change::Partial
                );
                assert!(matches!(
                    app.saved_popup.as_ref().unwrap().view,
                    View::Note(0)
                ));
                let close = app.saved_layout().close();
                app.input(center(close));
            }
        }
        let saved = &app.library.state(id).unwrap().pdf_highlights[0];
        assert_eq!(saved.color, HighlightColor::Green);
        assert_eq!(saved.bounds, mark.bounds);
        assert_eq!(saved.text, mark.text);
        assert_eq!(saved.note, mark.note);
        assert_eq!(saved.created_at, mark.created_at);
    }
    app.save().unwrap();
    let restored = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    assert_eq!(
        restored.library.state(id).unwrap().pdf_highlights[0].color,
        HighlightColor::Green
    );
}

#[test]
#[ignore = "requires two authorized original publications"]
fn original_epub_and_pdf_keep_colors_notes_anchors_and_their_unchanged_source_bytes() {
    let root = PathBuf::from(
        std::env::var_os("READER_HIGHLIGHT_COLOR_ORIGINALS")
            .expect("READER_HIGHLIGHT_COLOR_ORIGINALS"),
    );
    for name in ["035.epub", "030.pdf"] {
        let path = root.join(name);
        let original = std::fs::read(&path).unwrap();
        let fixture = crate::app::tests::Fixture::new();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        if app.fixed.is_some() {
            save_pdf_note(&mut app);
        } else {
            let reading = &mut app.active.as_mut().unwrap().reading;
            let chapter = reading.probe.chapter_count() / 2;
            reading.jump(chapter, None).unwrap();
            let word = (90..1400)
                .step_by(28)
                .find_map(|y| {
                    (70..1180)
                        .step_by(28)
                        .find_map(|x| reading.word_at(x as f32, y as f32))
                })
                .unwrap();
            app.selection = Some(word);
            app.save_highlight(Some("Keep this original passage.".into()))
                .unwrap();
        }
        let id = app.open_book_id().unwrap();
        let at = app.current_history_location();
        let original_mark = serde_json::to_value(if app.fixed.is_some() {
            serde_json::to_value(&app.library.state(id).unwrap().pdf_highlights[0]).unwrap()
        } else {
            serde_json::to_value(&app.library.state(id).unwrap().highlights[0]).unwrap()
        })
        .unwrap();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        for color in HighlightColor::ALL {
            choose_color(&mut app, Some(0), color);
            assert_eq!(app.current_history_location(), at);
            assert_eq!(app.mark_color(0), Some(color));
            for marker in [
                NoteMarker::Underline,
                NoteMarker::Sideline,
                NoteMarker::Sidemark,
            ] {
                note(&mut app, marker);
                assert_eq!(app.current_history_location(), at);
                assert!(
                    primitive_app::frame(&app, &mut canvas, false)
                        .iter()
                        .any(|&p| p < 255)
                );
            }
        }
        choose_color(&mut app, Some(0), HighlightColor::Purple);
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, vec![font]).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.mark_color(0), Some(HighlightColor::Purple));
        let mut saved = if reopened.fixed.is_some() {
            serde_json::to_value(&reopened.library.state(id).unwrap().pdf_highlights[0]).unwrap()
        } else {
            serde_json::to_value(&reopened.library.state(id).unwrap().highlights[0]).unwrap()
        };
        saved.as_object_mut().unwrap().remove("color");
        let mut expected = original_mark;
        expected.as_object_mut().unwrap().remove("color");
        assert_eq!(saved, expected);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn new_book_color_defaults_do_not_recolor_the_other_previously_opened_books() {
    let (fixture, mut app, _font, _) = fixture(None, false);
    let second = fixture.root.join("Earlier.txt");
    std::fs::write(&second, "An earlier reading passage.").unwrap();
    let third = fixture.root.join("New.txt");
    std::fs::write(&third, "A new reading passage.").unwrap();
    app.open_path(&second).unwrap();
    assert_eq!(app.effective_highlight_color(), HighlightColor::Gray);
    app.open_path(&fixture.book).unwrap();
    choose_color(&mut app, None, HighlightColor::Red);
    let set = app.saved_layout().button(1, 2);
    app.input(center(set));
    assert_eq!(app.controls.highlight_color, HighlightColor::Red);
    app.open_path(&second).unwrap();
    assert_eq!(app.effective_highlight_color(), HighlightColor::Gray);
    app.open_path(&third).unwrap();
    assert_eq!(app.effective_highlight_color(), HighlightColor::Red);
    app.open_path(&second).unwrap();
    choose_color(&mut app, None, HighlightColor::Blue);
    let use_default = app.saved_layout().button(0, 2);
    app.input(center(use_default));
    assert_eq!(app.effective_highlight_color(), HighlightColor::Red);
    app.open_path(&fixture.book).unwrap();
    assert_eq!(app.effective_highlight_color(), HighlightColor::Red);
}

#[test]
fn unopened_null_color_inherits_after_reload_while_legacy_missing_gray_remains_fixed() {
    let (fixture, mut app, font, _) = fixture(None, false);
    let fresh = fixture.root.join("Unopened.txt");
    std::fs::write(&fresh, "An unopened book.").unwrap();
    let legacy = fixture.root.join("Legacy.txt");
    std::fs::write(&legacy, "An older saved record.").unwrap();
    let entry = app.library.resolve(&fresh).unwrap();
    let fresh_id = app.library.identify(&entry).unwrap();
    let fresh_state = app.library.remember(&entry, fresh_id, &entry.title);
    assert!(fresh_state.position.is_none());
    assert!(fresh_state.highlight_color.is_none());
    assert_eq!(
        serde_json::to_value(&*fresh_state)
            .unwrap()
            .get("highlight_color"),
        Some(&serde_json::Value::Null)
    );
    let entry = app.library.resolve(&legacy).unwrap();
    let legacy_id = app.library.identify(&entry).unwrap();
    let legacy_state = app.library.remember(&entry, legacy_id, &entry.title);
    legacy_state.highlight_color = Some(HighlightColor::Gray);
    assert!(legacy_state.position.is_none());
    assert!(
        serde_json::to_value(&*legacy_state)
            .unwrap()
            .get("highlight_color")
            .is_none()
    );
    let controls = ReadingControls {
        highlight_color: HighlightColor::Blue,
        ..app.controls
    };
    app.activate(Action::SetReadingControls(controls)).unwrap();
    app.save().unwrap();
    let mut reloaded = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    assert_eq!(
        reloaded.library.state(fresh_id).unwrap().highlight_color,
        None
    );
    assert_eq!(
        reloaded.library.state(legacy_id).unwrap().highlight_color,
        Some(HighlightColor::Gray)
    );
    reloaded.open_path(&fresh).unwrap();
    assert_eq!(reloaded.effective_highlight_color(), HighlightColor::Blue);
    reloaded.open_path(&legacy).unwrap();
    assert_eq!(reloaded.effective_highlight_color(), HighlightColor::Gray);
}

#[test]
fn zoomed_pdf_note_pencil_starts_at_the_first_visible_saved_line() {
    let (_fixture, mut app, font, _) = fixture(None, true);
    let upper = PdfTextRect {
        x0: 0.45,
        y0: 0.08,
        x1: 0.46,
        y1: 0.11,
    };
    let lower = PdfTextRect {
        y0: 0.82,
        y1: 0.85,
        ..upper
    };
    let fixed = app.fixed.as_mut().unwrap();
    let mut view = fixed.reading.preferences();
    view.zoom = 4;
    fixed.reading.restore_view(view).unwrap();
    app.jump_fixed(1, FixedJump::regions(&[lower])).unwrap();
    app.pdf_result_regions.clear();
    let fixed = app.fixed.as_ref().unwrap();
    assert!(fixed.reading.visible_rectangle(1, upper).is_none());
    let visible = fixed.reading.annotation_rectangle(1, lower).unwrap();
    let id = fixed.id;
    app.library
        .state_mut(id)
        .unwrap()
        .add_pdf_highlight(PdfHighlight {
            updated_at: None,
            chapter_title: None,
            color: HighlightColor::Blue,
            created_at: None,
            style: HighlightStyle::Lighten,
            page: 1,
            bounds: vec![upper, lower],
            text: "First line\nVisible line".into(),
            note: Some("This note has an offscreen first line.".into()),
            continuation: Vec::new(),
        })
        .unwrap();
    note(&mut app, NoteMarker::None);
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let plain = primitive_app::frame(&app, &mut canvas, false);
    note(&mut app, NoteMarker::Sidemark);
    let marked = primitive_app::frame(&app, &mut canvas, false);
    let x = crate::note_marker::margin(WIDTH, 0, 0, NoteMarker::Sidemark, true);
    let y = visible.y0.floor() as i32;
    let changes = (y.max(0)..(y + 32).min(PAGE_HEIGHT as i32))
        .flat_map(|y| (x..x + 32).map(move |x| (y * WIDTH as i32 + x) as usize))
        .filter(|&k| plain[k] != marked[k])
        .count();
    assert!(
        changes > 20,
        "The pencil must appear beside the first visible line"
    );
    assert_eq!(
        app.input(Input::Tap((x + 16) as f32, (y + 16) as f32)),
        Change::Partial
    );
    assert!(matches!(
        app.saved_popup.as_ref().unwrap().view,
        View::Note(0)
    ));
}
