use super::*;
use crate::app::{position, tests::Fixture};
use primitive_app::App;

fn source_point(app: &mut Reader, label: &str) -> (f32, f32) {
    let reading = &mut app.active.as_mut().unwrap().reading;
    let first = reading.probe.matches(label).into_iter().next().unwrap();
    reading.jump_to_anchor(0, &first).unwrap();
    let mut end = first.clone();
    end.offset += label.len();
    let selected = reading.probe.select_range(&first, &end).unwrap();
    let rect = reading.highlight_rects(&selected)[0];
    (
        (rect.x + rect.width / 2) as f32,
        (rect.y + rect.height / 2) as f32,
    )
}
fn screenshot(app: &Reader, case: &str, fonts: &[std::path::PathBuf]) {
    let Some(folder) = std::env::var_os("READER_FOOTNOTE_ROOT_SCREENS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let paths = fonts
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
    for dark in [false, true] {
        let pixels = primitive_app::frame(app, &mut canvas, dark);
        primitive_app::write_png(
            &folder.join(format!(
                "footnote-{case}-{}.png",
                if dark { "dark" } else { "light" }
            )),
            WIDTH,
            HEIGHT,
            &pixels,
        )
        .unwrap();
    }
}

#[test]
fn native_oracle_popup_roots_render_inline_styles_and_preserve_source_page_and_exit() {
    let fixture = Fixture::new();
    std::fs::copy(
        crate::test_fixture("footnote-export-roots.epub"),
        &fixture.book,
    )
    .unwrap();
    let original = std::fs::read(&fixture.book).unwrap();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.controls.footnote_popups = true;
    for (number, case, word, visible) in [
        ("1", "A", "INLINE INNER NOTE A", true),
        ("2", "B", "INLINE ASIDE NOTE B", false),
        ("3", "C", "LINKED INNER NOTE C", true),
        ("4", "D", "LINKED ASIDE NOTE D", true),
        ("5", "E", "VISIBLE INLINE TARGET E", true),
        ("6", "F", "HIDDEN BLOCK TARGET F", true),
        ("7", "G", "HIDDEN INLINE TARGET G", false),
        ("8", "H", "VISIBLE DIV TARGET H", true),
    ] {
        let point = source_point(&mut app, number);
        let reading = &mut app.active.as_mut().unwrap().reading;
        let before = position(reading);
        let frame = reading.probe.capture(reading.page).unwrap();
        let history = app.back.len();
        app.input(Input::Tap(point.0, point.1));
        assert_eq!(app.screen, Screen::Footnote, "{case}");
        let popup = app.footnote.as_ref().unwrap();
        assert_eq!(!popup.preview.matches(word).is_empty(), visible, "{case}");
        if case == "B" {
            assert!(
                popup.frame.pixels.iter().all(|p| *p == 255),
                "inline display:none must remain effective"
            );
        }
        if matches!(case, "E" | "G" | "H") {
            assert!(
                !popup.preview.matches("Visible before").is_empty(),
                "{case}"
            );
        }
        if case == "F" {
            assert!(popup.preview.matches("Hidden before").is_empty());
        }
        if case != "B" {
            assert!(popup.frame.pixels.iter().any(|p| *p < 200), "{case}");
        }
        screenshot(&app, case, &fonts);
        let close = popup.close_button();
        app.input(Input::Tap(
            (close.x + close.w / 2) as f32,
            (close.y + close.h / 2) as f32,
        ));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            position(&app.active.as_ref().unwrap().reading),
            before,
            "{case}"
        );
        assert_eq!(app.back.len(), history, "{case}");
        let reading = &mut app.active.as_mut().unwrap().reading;
        assert_eq!(
            reading.probe.capture(reading.page).unwrap(),
            frame,
            "{case}"
        );
    }
    let saved = position(&app.active.as_ref().unwrap().reading);
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(position(&reopened.active.as_ref().unwrap().reading), saved);
    assert_eq!(std::fs::read(&fixture.book).unwrap(), original);
}
