use super::*;
use crate::app::{Change, Input, ReadingOrientation, tests::Fixture};
use primitive_app::{
    App, HEIGHT,
    viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest},
};

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

#[test]
fn typeface_viewport_preserves_font_focus_and_the_exact_epub_source() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let fonts = vec![
        crate::test_fixture("fonts/NotoSans-Regular.ttf"),
        crate::test_fixture("fonts/NotoSans-Bold.ttf"),
    ];
    let font_paths: Vec<_> = fonts
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    app.follow_reflow_page(0, 2).unwrap();
    let source = app.current_position().unwrap().anchor;
    assert!(source.is_some());
    let id = app.open_book_id().unwrap();
    app.open_reading_edge(false, WIDTH as f32 / 2.0);
    let type_panel = app.reading_button("Type");
    assert_eq!(app.input(tap(type_panel)), Change::Partial);
    let chooser = app.reading_button("Reading font");
    assert_eq!(app.input(tap(chooser)), Change::Partial);
    assert_eq!(app.screen, Screen::ReadingFonts);
    let selected_file = "Z Selected.ttf";
    fs::copy(
        crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        app.fonts_directory.join("A Display.ttf"),
    )
    .unwrap();
    fs::copy(&fonts[0], app.fonts_directory.join("B Sans.ttf")).unwrap();
    fs::write(app.fonts_directory.join("C Unreadable.ttf"), b"bad font").unwrap();
    fs::copy(
        crate::test_fixture("fonts/Silkscreen-Regular.ttf"),
        app.fonts_directory.join(selected_file),
    )
    .unwrap();
    fs::copy(
        crate::test_fixture("fonts/NotoSansArabic-Regular.ttf"),
        app.fonts_directory.join("D Arabic.ttf"),
    )
    .unwrap();
    for i in 1..=4 {
        fs::copy(
            &fonts[1],
            app.fonts_directory.join(format!("F Variant {i:02}.ttf")),
        )
        .unwrap();
    }
    app.activate(Action::ShowFonts).unwrap();
    let wide = Viewport {
        generation: 1,
        turn: LogicalTurn::Clockwise,
        logical_size: (HEIGHT, WIDTH),
    };
    let mut canvas = Canvas::new_viewport(HEIGHT, WIDTH, WIDTH, &font_paths).unwrap();
    app.controls.sticky_orientation = Some(ReadingOrientation::LandscapeRight);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(wide.turn))
    );
    app.stage_viewport(wide).unwrap();
    canvas.begin();
    app.draw_staged_viewport(&mut canvas).unwrap();
    let staged = canvas.finish(false);
    app.finish_viewport(wide, ViewportDecision::Commit);
    assert_eq!(app.current_position().unwrap().anchor, source);
    assert_eq!(primitive_app::frame(&app, &mut canvas, false), staged);
    let navigation = app.navigation_layout();
    let rows = app.list_geometry();
    assert!(navigation.back().x > WIDTH as i32);
    assert!(rows.row(1).right() > WIDTH as i32);
    if let Some(path) = std::env::var_os("READER_TYPEFACE_LANDSCAPE_CAPTURE") {
        primitive_app::write_png(std::path::Path::new(&path), HEIGHT, WIDTH, &staged).unwrap();
    }
    let (name, specimen) = app
        .typeface_preview("A Display.ttf", &font_paths, false, false)
        .unwrap();
    assert!(name.contains("Silkscreen"));
    assert!(specimen.pixels.iter().any(|&ink| ink < 64));
    let (_, sans) = app
        .typeface_preview("B Sans.ttf", &font_paths, false, false)
        .unwrap();
    assert_ne!(specimen.pixels, sans.pixels);
    assert!(
        app.typeface_preview("C Unreadable.ttf", &font_paths, false, false)
            .is_none()
    );

    let (arabic_name, arabic) = app
        .typeface_preview("D Arabic.ttf", &font_paths, false, false)
        .unwrap();
    assert!(arabic_name.contains("Arabic"));
    assert!(arabic.pixels.iter().any(|&ink| ink < 64));
    assert_ne!(arabic.pixels, sans.pixels);
    let (_, dark) = app
        .typeface_preview("D Arabic.ttf", &font_paths, false, true)
        .unwrap();
    assert!(
        arabic
            .pixels
            .iter()
            .zip(dark.pixels)
            .all(|(&light, dark)| light == 255 - dark)
    );

    assert_eq!(app.input(tap(navigation.back())), Change::Partial);
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.reading_panel, Some(BarPanel::Text));
    let chooser = app.reading_button("Reading font");
    assert_eq!(app.input(tap(chooser)), Change::Partial);
    assert_eq!(app.input(tap(navigation.next())), Change::Partial);
    let index = app
        .list_window()
        .1
        .iter()
        .position(|item| item.title == selected_file)
        .unwrap();
    let selected = app.list_geometry().row(index);
    assert_eq!(
        app.input(Input::Tap(1510.0, (selected.y + selected.h / 2) as f32)),
        Change::Partial
    );
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.reading_viewport().dimensions(), (HEIGHT, WIDTH));
    assert_eq!(app.current_position().unwrap().anchor, source);
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .as_deref(),
        Some(selected_file)
    );
    assert_eq!(
        app.active.as_ref().unwrap().reading.config.fonts[0],
        app.fonts_directory.join(selected_file)
    );
    app.save().unwrap();

    app.show(Screen::Typography);
    assert_eq!(
        app.viewport_request(),
        Some(ViewportRequest::Fixed(wide.turn))
    );
    app.activate(Action::ShowFonts).unwrap();
    let ordinal = app
        .available_fonts
        .iter()
        .position(|file| file == selected_file)
        .unwrap()
        + 1;
    assert_eq!(app.list_page, ordinal / app.list_geometry().rows);
    assert_eq!(app.list_page, 1);
    let portrait = Viewport {
        generation: 2,
        turn: LogicalTurn::Upright,
        logical_size: (WIDTH, HEIGHT),
    };
    let mut canvas = Canvas::new_viewport(WIDTH, HEIGHT, WIDTH, &font_paths).unwrap();
    app.stage_viewport(portrait).unwrap();
    canvas.begin();
    app.draw_staged_viewport(&mut canvas).unwrap();
    canvas.finish(false);
    app.finish_viewport(portrait, ViewportDecision::Commit);
    assert_eq!(app.list_page, 0);
    assert!(
        app.list_window()
            .1
            .iter()
            .any(|item| item.title == selected_file)
    );
    let back = app.navigation_layout().back();
    assert_eq!(app.input(tap(back)), Change::Partial);
    assert_eq!(app.screen, Screen::Typography);
    assert_eq!(app.current_position().unwrap().anchor, source);

    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(reopened.current_position().unwrap().anchor, source);
    assert_eq!(
        reopened
            .library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .as_deref(),
        Some(selected_file)
    );
}

// Assemble a temporary collection from already licensed local test faces.
// TTC table offsets are file-absolute, unlike offsets in the source fonts.
fn test_collection(faces: &[Vec<u8>]) -> Vec<u8> {
    let header = 12 + 4 * faces.len();
    let mut out = vec![0; header];
    out[..4].copy_from_slice(b"ttcf");
    out[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    out[8..12].copy_from_slice(&(faces.len() as u32).to_be_bytes());
    for (index, face) in faces.iter().enumerate() {
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let offset = out.len();
        out[12 + index * 4..16 + index * 4].copy_from_slice(&(offset as u32).to_be_bytes());
        let mut relocated = face.clone();
        let tables = u16::from_be_bytes([relocated[4], relocated[5]]) as usize;
        for table in 0..tables {
            let pos = 12 + table * 16 + 8;
            let old = u32::from_be_bytes(relocated[pos..pos + 4].try_into().unwrap());
            relocated[pos..pos + 4].copy_from_slice(&(old + offset as u32).to_be_bytes());
        }
        out.extend_from_slice(&relocated);
    }
    out
}

#[test]
fn true_type_collection_can_be_previewed_selected_and_restored() {
    let fixture = Fixture::new();
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let font_paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
    let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
    app.open_path(&fixture.book).unwrap();
    let original = app.current_position().unwrap().anchor;
    app.open_font_chooser().unwrap();
    let files = [
        fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap(),
        fs::read(crate::test_fixture("fonts/NotoSans-Regular.ttf")).unwrap(),
    ];
    let data = test_collection(&files);
    assert!(FontRef::from_index(&data, 0).is_ok());
    assert!(FontRef::from_index(&data, 1).is_ok());
    let name = "Collected faces.TTC";
    fs::write(app.fonts_directory.join(name), &data).unwrap();
    let mut context = crate::font_context(&[app.fonts_directory.join(name)]).unwrap();
    let families = context
        .collection
        .generic_families(parley::GenericFamily::Serif)
        .collect::<Vec<_>>();
    assert!(families.len() >= 2);
    assert!(
        context
            .collection
            .family(families[0])
            .unwrap()
            .fonts()
            .iter()
            .any(|face| face.index() == 0)
    );
    assert!(
        context
            .collection
            .family(families[1])
            .unwrap()
            .fonts()
            .iter()
            .any(|face| face.index() == 1)
    );
    fs::write(app.fonts_directory.join("Invalid.ttc"), b"bad font").unwrap();
    app.open_font_chooser().unwrap();
    assert!(app.available_fonts.iter().any(|file| file == name));
    let (title, specimen) = app
        .typeface_preview(name, &font_paths, false, false)
        .unwrap();
    assert!(title.contains("Silkscreen"));
    assert!(specimen.pixels.iter().any(|&ink| ink < 128));
    assert!(
        app.typeface_preview("Invalid.ttc", &font_paths, false, false)
            .is_none()
    );
    crate::app::tests::capture_test_screen(&app, &fonts, "READER_TTC_FONT_CHOOSER_FRAME", false);
    app.activate(Action::SetFont(Some(name.into()))).unwrap();
    assert_eq!(app.current_position().unwrap().anchor, original);
    let id = app.open_book_id().unwrap();
    let preferences = &app.library.state(id).unwrap().preferences;
    assert_eq!(preferences.font_file.as_deref(), Some(name));
    crate::document_defaults::validate_preferences(preferences).unwrap();
    let before = app.current_position();
    assert!(
        app.activate(Action::SetFont(Some("Invalid.ttc".into())))
            .is_err()
    );
    assert_eq!(app.current_position(), before);
    assert_eq!(
        app.library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .as_deref(),
        Some(name)
    );
    app.save().unwrap();
    drop(app);
    let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened
            .library
            .state(id)
            .unwrap()
            .preferences
            .font_file
            .as_deref(),
        Some(name)
    );
    assert_eq!(
        reopened.active.as_ref().unwrap().reading.config.fonts[0],
        reopened.fonts_directory.join(name)
    );
}
