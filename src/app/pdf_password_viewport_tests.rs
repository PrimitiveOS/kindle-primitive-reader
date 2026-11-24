use super::{Action, FolderDirection, Reader, Screen, UNLOCK_TOUCH, WIDTH, tests::Fixture};
use crate::reading_controls::ReadingOrientation;
use primitive_app::{
    App, Change,
    viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest},
};
use primitive_ui::{Canvas, Input, Key, Rect};
use std::{fs, path::Path};

fn reader(fixture: &Fixture) -> Reader {
    Reader::open(
        &fixture.root,
        &fixture.state,
        vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ],
    )
    .unwrap()
}

fn canvas(reader: &Reader, (width, height): (u32, u32)) -> Canvas {
    let fonts: Vec<_> = reader
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    Canvas::new_viewport(width, height, WIDTH, &fonts).unwrap()
}

fn frame(reader: &Reader) -> Vec<u8> {
    let mut canvas = canvas(reader, reader.reading_viewport().dimensions());
    primitive_app::frame(reader, &mut canvas, false)
}

fn stage(reader: &mut Reader, target: Viewport, decision: ViewportDecision) -> Vec<u8> {
    let old_dimensions = reader.reading_viewport().dimensions();
    let mut canvas = canvas(reader, target.logical_size);
    reader.stage_viewport(target).unwrap();
    canvas.begin();
    reader.draw_staged_viewport(&mut canvas).unwrap();
    let pixels = canvas.finish(false);
    assert_eq!(reader.reading_viewport().dimensions(), old_dimensions);
    reader.finish_viewport(target, decision);
    pixels
}

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn press(reader: &mut Reader, key: Key) {
    let layout = reader.navigation_layout().keyboard();
    let (_, rect) = layout
        .keyboard_keys_with(reader.editor.layer, "Open")
        .into_iter()
        .find(|(target, _)| target.key == key)
        .unwrap();
    let input = tap(rect);
    assert!(reader.press_point(input).is_some());
    assert_eq!(reader.input(input), Change::Partial);
}

#[test]
fn locked_pdf_viewport_preserves_the_library_and_authenticates_in_each_landscape_turn() {
    let fixture = Fixture::new();
    let mut reader = reader(&fixture);
    let protected = fixture.root.join("Protected sample.pdf");
    let bytes = include_bytes!("../../tests/fixtures/pdf-password/r6-long.pdf");
    fs::write(&protected, bytes).unwrap();
    let fixture_word = format!("estuary{}river", "n".repeat(88));
    for (index, (turn, preference)) in [
        (LogicalTurn::Clockwise, ReadingOrientation::LandscapeRight),
        (
            LogicalTurn::Counterclockwise,
            ReadingOrientation::LandscapeLeft,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        reader.controls.sticky_orientation = Some(preference);
        let target = Viewport {
            generation: index as u64 * 10 + 1,
            turn,
            logical_size: (1680, 1264),
        };
        assert_eq!(
            reader.viewport_request(),
            Some(ViewportRequest::Fixed(turn))
        );
        stage(&mut reader, target, ViewportDecision::Commit);
        reader.open_path(&protected).unwrap();
        assert_eq!(reader.screen, Screen::PdfPassword);
        assert_eq!(reader.open_book_id(), None);
        assert!(!reader.viewport_needs_stage(target));
        // The prompt keeps the accepted turn even before a destination has an orientation policy.
        reader.controls.sticky_orientation = None;
        assert_eq!(
            reader.viewport_request(),
            Some(ViewportRequest::Fixed(turn))
        );
        reader.controls.sticky_orientation = Some(preference);
        let back = reader.navigation_layout().back();
        assert!(back.x > WIDTH as i32);
        for ch in "wrong".chars() {
            press(&mut reader, Key::Char(ch));
        }
        press(&mut reader, Key::Go);
        assert_eq!(reader.screen, Screen::PdfPassword);
        assert_eq!(reader.open_book_id(), None);
        assert!(reader.editor.string().is_empty());
        assert!(
            reader
                .notice
                .as_deref()
                .unwrap()
                .contains("Password rejected")
        );
        let retry_frame = frame(&reader);
        stage(
            &mut reader,
            Viewport {
                generation: target.generation + 1,
                turn: LogicalTurn::Upright,
                logical_size: (1264, 1680),
            },
            ViewportDecision::Abort,
        );
        assert_eq!(reader.reading_viewport().dimensions(), (1680, 1264));
        assert_eq!(frame(&reader), retry_frame);
        assert_eq!(reader.input(tap(back)), Change::Partial);
        assert_eq!(reader.screen, Screen::Library);
        assert!(reader.locked.is_none());
        assert!(!reader.viewport_needs_stage(target));
        reader.open_path(&protected).unwrap();
        for (i, ch) in fixture_word.chars().enumerate() {
            press(&mut reader, Key::Char(ch));
            if index == 0
                && i == 6
                && let Some(path) = std::env::var_os("READER_LOCKED_PDF_VIEWPORT_CAPTURE")
            {
                let pixels = frame(&reader);
                primitive_app::write_png(Path::new(&path), 1680, 1264, &pixels).unwrap();
            }
        }
        press(&mut reader, Key::Go);
        assert_eq!(reader.screen, Screen::Fixed);
        assert_eq!(
            reader.viewport_request(),
            Some(ViewportRequest::Fixed(turn))
        );
        assert!(!reader.viewport_needs_stage(target));
        let fixed = &reader.fixed.as_ref().unwrap().reading;
        assert!(fixed.is_encrypted_pdf());
        assert_eq!(fixed.viewport().dimensions(), (1680, 1264));
        assert_eq!(reader.current_position().unwrap().page, index);
        assert!(reader.editor.string().is_empty());
        assert!(reader.locked.is_none());
        reader.follow_fixed(1, None).unwrap();
        reader.activate(Action::Show(Screen::Library)).unwrap();
    }
    assert_eq!(fs::read(protected).unwrap(), bytes);
    assert!(
        !fs::read_to_string(&fixture.state)
            .unwrap()
            .contains(&fixture_word)
    );
}

#[test]
fn locked_pdf_viewport_cancel_restores_the_previous_book_and_its_locked_footer() {
    let fixture = Fixture::new();
    let mut reader = reader(&fixture);
    let shelf = fixture.root.join("Samples");
    fs::create_dir(&shelf).unwrap();
    let previous = shelf.join("Volume 1.pdf");
    let protected = shelf.join("Volume 2.pdf");
    fs::copy(crate::test_fixture("tilted-scan.pdf"), &previous).unwrap();
    fs::copy(crate::test_fixture("password-required.pdf"), &protected).unwrap();
    reader.open_path(&previous).unwrap();
    let viewport = Viewport {
        generation: 1,
        turn: LogicalTurn::Clockwise,
        logical_size: (1680, 1264),
    };
    reader.controls.sticky_orientation = Some(ReadingOrientation::LandscapeRight);
    stage(&mut reader, viewport, ViewportDecision::Commit);
    let opening = reader.current_history_location();
    let before = reader.open_book_id();
    reader.activate(Action::LockTouch).unwrap();
    reader
        .activate(Action::OpenFolderFile(FolderDirection::Next))
        .unwrap();
    assert_eq!(reader.screen, Screen::PdfPassword);
    assert!(reader.touch_locked);
    press(&mut reader, Key::Char('x'));
    press(&mut reader, Key::Go);
    assert_eq!(reader.screen, Screen::PdfPassword);
    assert_eq!(reader.open_book_id(), before);
    assert_eq!(reader.input(Input::PageBack), Change::Partial);
    assert_eq!(reader.screen, Screen::Fixed);
    assert_eq!(reader.current_history_location(), opening);
    assert!(reader.touch_locked);
    assert!(!reader.viewport_needs_stage(viewport));
    assert_eq!(reader.input(tap(UNLOCK_TOUCH)), Change::None);
    let unlock = reader.reading_viewport().bottom_right(UNLOCK_TOUCH);
    reader.end_dialog = Some(super::end_of_book::Dialog::Actions);
    assert_eq!(reader.input(tap(unlock)), Change::Partial);
    assert!(!reader.touch_locked);
    assert_eq!(reader.current_history_location(), opening);
    assert_eq!(reader.input(Input::PageBack), Change::Partial);
    reader.activate(Action::LockTouch).unwrap();
    assert_eq!(
        reader.input(tap(reader.reading_viewport().done())),
        Change::Exit
    );
}
