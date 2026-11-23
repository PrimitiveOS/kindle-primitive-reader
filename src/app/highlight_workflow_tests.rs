use super::tests::{center, fixture};
use super::*;
use crate::highlight_style::{Appearance, HighlightColor, HighlightStyle, Paint};
use primitive_app::App;
use serde_json::Value;

fn seed(app: &mut Reader) {
    if app.fixed.is_some() {
        let mut selections = Vec::new();
        'pages: for page in 0..app.fixed.as_ref().unwrap().reading.count().min(20) {
            let text = app.fixed.as_ref().unwrap().reading.page_text(page).unwrap();
            for word in &text.words {
                let point = hayro::kurbo::Point::new(
                    (word.bounds.x0 + word.bounds.x1) as f64 / 2.,
                    (word.bounds.y0 + word.bounds.y1) as f64 / 2.,
                );
                selections.push((
                    page,
                    crate::fixed_session::PdfSelection::at(page, point, text.clone()).unwrap(),
                ));
                if selections.len() == 2 {
                    break 'pages;
                }
            }
        }
        assert_eq!(selections.len(), 2, "select two actual PDF text words");
        for (page, selection) in selections {
            app.jump_fixed(page, None).unwrap();
            app.pdf_selection = Some(selection);
            app.save_pdf_highlight(Some("Preserve the original note.".into()))
                .unwrap();
        }
    } else {
        let reading = &mut app.active.as_mut().unwrap().reading;
        let (chapter, probe) = (0..reading.probe.chapter_count())
            .find_map(|chapter| {
                let probe = reading.config.chapter(chapter).ok()?;
                probe
                    .source
                    .blocks
                    .iter()
                    .any(|block| block.text.split_whitespace().count() >= 3)
                    .then_some((chapter, probe))
            })
            .expect("select actual publication text");
        let block = probe
            .source
            .blocks
            .iter()
            .find(|block| block.text.split_whitespace().count() >= 3)
            .unwrap();
        let ranges: Vec<_> = block
            .text
            .split_whitespace()
            .take(2)
            .map(|word| {
                let first = word.as_ptr() as usize - block.text.as_ptr() as usize;
                probe
                    .select_range(
                        &probe.anchor(block.key, first),
                        &probe.anchor(block.key, first + word.len()),
                    )
                    .unwrap()
            })
            .collect();
        reading.jump_to_anchor(chapter, &ranges[0].start).unwrap();
        for range in ranges {
            app.selection = Some(range);
            app.save_highlight(Some("Preserve the original note.".into()))
                .unwrap();
        }
    }
    let id = app.open_book_id().unwrap();
    let state = app.library.state_mut(id).unwrap();
    for (i, mark) in state.highlights.iter_mut().enumerate() {
        mark.style = if i == 0 {
            HighlightStyle::Lighten
        } else {
            HighlightStyle::Underline
        };
        mark.color = if i == 0 {
            HighlightColor::Red
        } else {
            HighlightColor::Blue
        };
        mark.created_at = Some(1_740_777_900 + i as i64);
    }
    for (i, mark) in state.pdf_highlights.iter_mut().enumerate() {
        mark.style = if i == 0 {
            HighlightStyle::Lighten
        } else {
            HighlightStyle::Underline
        };
        mark.color = if i == 0 {
            HighlightColor::Red
        } else {
            HighlightColor::Blue
        };
        mark.created_at = Some(1_740_777_900 + i as i64);
    }
    app.mark_book_changed();
    app.save().unwrap();
}
fn marks(app: &Reader) -> Vec<Value> {
    let state = app.library.state(app.open_book_id().unwrap()).unwrap();
    state
        .highlights
        .iter()
        .map(|mark| serde_json::to_value(mark).unwrap())
        .chain(
            state
                .pdf_highlights
                .iter()
                .map(|mark| serde_json::to_value(mark).unwrap()),
        )
        .collect()
}
fn original_fields(mut marks: Vec<Value>) -> Vec<Value> {
    for mark in &mut marks {
        let object = mark.as_object_mut().unwrap();
        object.remove("style");
        object.remove("color");
    }
    marks
}
fn begin_all(app: &mut Reader) {
    app.open_highlight_styles(None);
    let entry = app.saved_popup.as_ref().unwrap().workflow_entry(true);
    assert_eq!(app.input(center(entry)), Change::Partial);
    assert!(matches!(
        app.saved_popup.as_ref().unwrap().view,
        View::ApplyAll { .. }
    ));
}
fn set_current(app: &mut Reader) {
    app.save_highlight_style(None, Some(HighlightStyle::Strikethrough));
    app.save_highlight_color(None, Some(HighlightColor::Purple));
}
fn finish_all(app: &mut Reader) {
    let apply = app.saved_popup.as_ref().unwrap().button(1, 2);
    assert_eq!(app.input(center(apply)), Change::Partial);
}

#[test]
fn bulk_restyle_requires_consent_and_preserves_every_saved_passage_note_date_and_page() {
    for pdf in [false, true] {
        let (_fixture, mut app, font, _) = fixture(None, pdf);
        seed(&mut app);
        let before = marks(&app);
        let position = app.current_history_location();
        let history = app.back.clone();
        set_current(&mut app);
        begin_all(&mut app);
        assert_eq!(
            marks(&app),
            before,
            "opening a confirmation cannot change marks"
        );
        let cancel = app.saved_popup.as_ref().unwrap().button(0, 2);
        app.input(center(cancel));
        assert_eq!(marks(&app), before);
        begin_all(&mut app);
        app.input(Input::PageBack);
        assert_eq!(marks(&app), before);
        begin_all(&mut app);
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
        let pixels = primitive_app::frame(&app, &mut canvas, false);
        for (button, tone) in [(0, 255), (1, 0)] {
            let rect = app.saved_popup.as_ref().unwrap().button(button, 2);
            assert_eq!(
                pixels[(rect.y + 30) as usize * WIDTH as usize + (rect.x + 30) as usize],
                tone,
                "the safe Cancel remains outlined and the Apply action is filled"
            );
        }
        crate::app::tests::capture_test_screen(
            &app,
            &[font.clone()],
            if pdf {
                "READER_BULK_PDF_SCREEN"
            } else {
                "READER_BULK_EPUB_SCREEN"
            },
            false,
        );
        app.controls.night_pages = true;
        crate::app::tests::capture_test_screen(
            &app,
            &[font.clone()],
            if pdf {
                "READER_BULK_PDF_DARK_SCREEN"
            } else {
                "READER_BULK_EPUB_DARK_SCREEN"
            },
            true,
        );
        app.controls.night_pages = false;
        finish_all(&mut app);
        assert!(app.saved_popup.is_none());
        let after = marks(&app);
        assert_eq!(after.len(), 2);
        assert!(
            after
                .iter()
                .all(|mark| mark["style"] == "strikeout" && mark["color"] == "purple")
        );
        assert_eq!(original_fields(after), original_fields(before));
        assert_eq!(app.current_history_location(), position);
        assert_eq!(app.back, history);
    }
}

#[test]
fn bulk_restyle_failure_restores_all_marks_and_can_retry_without_partial_changes() {
    for pdf in [false, true] {
        let (fixture, mut app, _font, _) = fixture(None, pdf);
        seed(&mut app);
        set_current(&mut app);
        app.save().unwrap();
        let before = app.library.bytes().unwrap();
        let stored = std::fs::read(&fixture.state).unwrap();
        let rollback = fixture.root.join("library-before-bulk.json");
        std::fs::rename(&fixture.state, &rollback).unwrap();
        std::fs::create_dir(&fixture.state).unwrap();
        begin_all(&mut app);
        finish_all(&mut app);
        assert!(matches!(
            app.saved_popup.as_ref().unwrap().view,
            View::ApplyAll { .. }
        ));
        assert!(app.notice.is_some());
        assert_eq!(app.library.bytes().unwrap(), before);
        assert_eq!(std::fs::read(&rollback).unwrap(), stored);
        std::fs::remove_dir(&fixture.state).unwrap();
        std::fs::rename(&rollback, &fixture.state).unwrap();
        finish_all(&mut app);
        assert!(app.saved_popup.is_none());
        assert!(marks(&app).iter().all(|mark| mark["color"] == "purple"));
        let saved = crate::library::Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(saved.bytes().unwrap(), app.library.bytes().unwrap());
    }
}

#[test]
fn selection_color_is_global_uses_the_book_color_and_stays_disabled_for_invert() {
    let (fixture, mut app, font, _) = fixture(None, false);
    app.save_highlight_color(None, Some(HighlightColor::Blue));
    app.open_highlight_styles(None);
    let toggle = app.saved_popup.as_ref().unwrap().workflow_entry(false);
    app.input(center(toggle));
    assert!(app.controls.selection_uses_highlight_color);
    assert_eq!(app.page_paint().selection_color, Some(HighlightColor::Blue));
    let controls = ReadingControls::load(&app.controls_path, true);
    assert!(controls.selection_uses_highlight_color);
    app.save_highlight_style(None, Some(HighlightStyle::Invert));
    assert_eq!(app.page_paint().selection_color, None);
    app.input(center(toggle));
    assert!(
        app.controls.selection_uses_highlight_color,
        "disabled input must preserve the global preference"
    );
    app.save_highlight_style(None, Some(HighlightStyle::Lighten));
    app.close_saved_popup();
    app.save().unwrap();
    let mut reopened = Reader::open(&fixture.root, &fixture.state, vec![font]).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened.page_paint().selection_color,
        Some(HighlightColor::Blue)
    );
    assert!(
        marks(&reopened).is_empty(),
        "temporary-selection preferences cannot manufacture saved marks"
    );
}

#[test]
fn named_temporary_color_uses_the_distinct_cre_and_fixed_painters_in_both_palettes() {
    let tones = [89u32, 131, 204, 110, 191, 187, 114, 123];
    for (i, color) in HighlightColor::ALL[..8].iter().copied().enumerate() {
        for night in [false, true] {
            let (r, g, b) = color.rgb().unwrap();
            let tone = if night {
                ((255 - r) + 2 * (255 - g) + (255 - b)) / 4
            } else {
                tones[i]
            };
            for opacity in [0, 20, 60, 100] {
                let appearance = Appearance {
                    selection_opacity: opacity,
                    line_height: 50,
                    ..Default::default()
                };
                let mut row: Vec<u8> = (0..=255).collect();
                Paint::SelectionColor {
                    color,
                    night,
                    reflow: true,
                }
                .draw_row(&mut row, appearance);
                for (source, &actual) in row.iter().enumerate() {
                    assert_eq!(actual, ((source as u32 * tone + 127) / 255) as u8);
                }
                let mut fixed: Vec<u8> = (0..=255).collect();
                let mut reference = fixed.clone();
                Paint::SelectionColor {
                    color,
                    night,
                    reflow: false,
                }
                .draw_row(&mut fixed, appearance);
                Paint::Annotation {
                    style: HighlightStyle::Lighten,
                    color,
                    note: false,
                    first: false,
                    night,
                }
                .draw_row(
                    &mut reference,
                    Appearance {
                        highlight_opacity: opacity,
                        ..appearance
                    },
                );
                assert_eq!(fixed, reference);
                assert_eq!(
                    Paint::SelectionColor {
                        color,
                        night,
                        reflow: true
                    }
                    .fill_band(0, 90, 50),
                    None
                );
                assert_eq!(
                    Paint::SelectionColor {
                        color,
                        night,
                        reflow: false
                    }
                    .fill_band(0, 90, 50),
                    Some(23..68)
                );
            }
        }
    }
}

#[test]
#[ignore = "requires READER_SELECTION_COLOR_ORACLE from compare-selection-colors.py"]
fn temporary_selection_tones_match_the_actual_pinned_cre_grayscale_function() {
    let bytes = std::fs::read(std::env::var_os("READER_SELECTION_COLOR_ORACLE").unwrap()).unwrap();
    assert_eq!(bytes.len(), 16);
    for (i, color) in HighlightColor::ALL[..8].iter().copied().enumerate() {
        for night in [false, true] {
            let mut paper = [255u8];
            Paint::SelectionColor {
                color,
                night,
                reflow: true,
            }
            .draw_row(&mut paper, Appearance::default());
            assert_eq!(paper[0], bytes[i * 2 + usize::from(night)]);
        }
    }
}

fn rendered_selection(app: &mut Reader, font: &Path, capture: bool) {
    // Keep the action panel beside the selected range so this pixel test sees
    // the selection painter rather than the default centered dialog surface.
    app.controls.selection_popup_position =
        crate::reading_controls::SelectionPopupPosition::Highlight;
    app.save_highlight_color(None, Some(HighlightColor::Blue));
    app.save_highlight_style(None, Some(HighlightStyle::Lighten));
    app.open_highlight_styles(None);
    let toggle = app.saved_popup.as_ref().unwrap().workflow_entry(false);
    if !app.controls.selection_uses_highlight_color {
        app.input(center(toggle));
    }
    app.close_saved_popup();
    app.notice = None;
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let before = primitive_app::frame(app, &mut canvas, false);
    let saved = marks(app);
    let (rect, tone, raw) = if let Some(fixed) = &app.fixed {
        let page = fixed.reading.page();
        let text = fixed.reading.page_text(page).unwrap();
        let word = text.words.first().unwrap();
        let bounds = word.bounds;
        let point = hayro::kurbo::Point::new(
            (bounds.x0 + bounds.x1) as f64 / 2.,
            (bounds.y0 + bounds.y1) as f64 / 2.,
        );
        let rect = fixed.reading.visible_rectangle(page, bounds).unwrap();
        app.pdf_selection =
            Some(crate::fixed_session::PdfSelection::at(page, point, text).unwrap());
        (
            crate::selection::HighlightRect {
                x: rect.x0.floor() as i32,
                y: rect.y0.floor() as i32,
                width: rect.width().ceil() as i32,
                height: rect.height().ceil() as i32,
            },
            88u16,
            None,
        )
    } else {
        let state = app.library.state(app.open_book_id().unwrap()).unwrap();
        let selection = state.highlights[0].selection.clone();
        let reading = &mut app.active.as_mut().unwrap().reading;
        let rect = reading.highlight_rects(&selection)[0];
        let source = reading.probe.capture(reading.page).unwrap();
        app.selection = Some(selection);
        (rect, 114u16, Some(source))
    };
    let after = primitive_app::frame(app, &mut canvas, false);
    if capture {
        crate::app::tests::capture_test_screen(
            app,
            &[font.to_owned()],
            if app.fixed.is_some() {
                "READER_NAMED_SELECTION_PDF_SCREEN"
            } else {
                "READER_NAMED_SELECTION_EPUB_SCREEN"
            },
            false,
        );
        let previous = app.controls.night_pages;
        app.controls.night_pages = true;
        crate::app::tests::capture_test_screen(
            app,
            &[font.to_owned()],
            if app.fixed.is_some() {
                "READER_NAMED_SELECTION_PDF_DARK_SCREEN"
            } else {
                "READER_NAMED_SELECTION_EPUB_DARK_SCREEN"
            },
            true,
        );
        app.controls.night_pages = previous;
    }
    let mut checked = 0;
    for y in (rect.y + 3).max(0)..(rect.y + rect.height - 3).min(PAGE_HEIGHT as i32) {
        for x in (rect.x + 3).max(0)..(rect.x + rect.width - 3).min(WIDTH as i32) {
            let i = y as usize * WIDTH as usize + x as usize;
            let expected = if let Some(raw) = &raw {
                // CRE selection precedes ReaderView's saved Red Lighten mark.
                // Keep the two independent integer roundings in that order.
                let selected = (u16::from(raw[i]) * tone + 127) / 255;
                ((selected * 106 + 127) / 255) as u8
            } else {
                ((u16::from(before[i]) * tone + 127) / 255) as u8
            };
            assert_eq!(after[i], expected);
            checked += 1;
        }
    }
    assert!(checked > 30);
    assert_eq!(marks(app), saved);
    app.selection = None;
    app.pdf_selection = None;
}

#[test]
fn normal_reflow_and_pdf_selection_frames_paint_the_chosen_named_color_without_saving_a_mark() {
    for pdf in [false, true] {
        let (_fixture, mut app, font, _) = fixture(None, pdf);
        seed(&mut app);
        rendered_selection(&mut app, &font, true);
    }
}

#[test]
fn native_named_reflow_selection_is_below_saved_invert_marks_as_in_stock_readerview() {
    let (_fixture, mut app, font, _) = fixture(None, false);
    seed(&mut app);
    let id = app.open_book_id().unwrap();
    app.library.state_mut(id).unwrap().highlights[0].style = HighlightStyle::Invert;
    app.save_highlight_color(None, Some(HighlightColor::Blue));
    app.open_highlight_styles(None);
    let toggle = app.saved_popup.as_ref().unwrap().workflow_entry(false);
    app.input(center(toggle));
    app.close_saved_popup();
    let selection = app.library.state(id).unwrap().highlights[0]
        .selection
        .clone();
    let reading = &mut app.active.as_mut().unwrap().reading;
    let rect = reading.highlight_rects(&selection)[0];
    let source = reading.probe.capture(reading.page).unwrap();
    app.selection = Some(selection);
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &[font.display().to_string()]).unwrap();
    let pixels = primitive_app::frame(&app, &mut canvas, false);
    let mut checked = 0;
    for y in (rect.y + 3).max(0)..(rect.y + rect.height - 3).min(PAGE_HEIGHT as i32) {
        for x in (rect.x + 3).max(0)..(rect.x + rect.width - 3).min(WIDTH as i32) {
            let i = y as usize * WIDTH as usize + x as usize;
            assert_eq!(
                pixels[i],
                255 - ((u16::from(source[i]) * 114 + 127) / 255) as u8
            );
            checked += 1;
        }
    }
    assert!(checked > 30);
}

#[test]
#[ignore = "requires unchanged authorized READER_HIGHLIGHT_WORKFLOW_ORIGINALS/035.epub and030.pdf"]
fn original_epub_and_pdf_bulk_edits_preserve_source_bytes_and_survive_reopen() {
    let root = PathBuf::from(std::env::var_os("READER_HIGHLIGHT_WORKFLOW_ORIGINALS").unwrap());
    for name in ["035.epub", "030.pdf"] {
        let fixture = crate::app::tests::Fixture::new();
        let path = root.join(name);
        let source = std::fs::read(&path).unwrap();
        let font = crate::test_fixture("fonts/NotoSans-Regular.ttf");
        let mut app = Reader::open(&root, &fixture.state, vec![font.clone()]).unwrap();
        app.open_path(&path).unwrap();
        seed(&mut app);
        let before = original_fields(marks(&app));
        let position = app.current_history_location();
        rendered_selection(&mut app, &font, false);
        set_current(&mut app);
        begin_all(&mut app);
        finish_all(&mut app);
        assert_eq!(original_fields(marks(&app)), before);
        assert_eq!(app.current_history_location(), position);
        use crate::app::bookmark_browser::{Command as BrowserCommand, Source};
        app.activate_bookmark_browser(BrowserCommand::Use(Source::History))
            .unwrap();
        app.activate_bookmark_browser(BrowserCommand::SetColor(Some(HighlightColor::Purple)))
            .unwrap();
        assert_eq!(
            app.list_window().0,
            3,
            "both original saved passages and their book heading must match"
        );
        app.activate_bookmark_browser(BrowserCommand::SetColor(Some(HighlightColor::Blue)))
            .unwrap();
        assert_eq!(app.list_window().0, 0);
        app.activate_bookmark_browser(BrowserCommand::SetColor(None))
            .unwrap();
        assert_eq!(app.list_window().0, 3);
        use crate::app::library_export::{Command as ExportCommand, Format};
        app.activate_library_export(ExportCommand::Open).unwrap();
        app.activate_library_export(ExportCommand::History).unwrap();
        app.activate_library_export(ExportCommand::Color(Some(HighlightColor::Purple)))
            .unwrap();
        app.activate_library_export(ExportCommand::Style(Some(HighlightStyle::Strikethrough)))
            .unwrap();
        app.activate_library_export(ExportCommand::Start(Format::Json))
            .unwrap();
        for _ in 0..1000 {
            let Some(deadline) = app.export_deadline() else {
                break;
            };
            app.tick(deadline);
        }
        assert!(app.export_deadline().is_none());
        let exported: Value = serde_json::from_slice(
            &std::fs::read(app.exports.join("library-highlights.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(exported["entries"].as_array().unwrap().len(), 2);
        assert!(
            exported["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["color"] == "purple" && entry["time"].is_number())
        );
        assert_eq!(app.current_history_location(), position);
        app.save().unwrap();
        let mut reopened = Reader::open(&root, &fixture.state, vec![font]).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(original_fields(marks(&reopened)), before);
        assert!(
            marks(&reopened)
                .iter()
                .all(|mark| mark["color"] == "purple" && mark["style"] == "strikeout")
        );
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }
}
