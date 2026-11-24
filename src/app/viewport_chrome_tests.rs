use super::tests::{Fixture, capture_test_screen};
use super::*;
use crate::reading_controls::{FooterTrackPosition, ProgressScope, SkimPosition};
use crate::session::ReadingViewport;

fn tap(rect: Rect) -> Input {
    Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
}

fn open_app(fixture: &Fixture) -> Reader {
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

fn capture(app: &Reader, key: &str) {
    let Some(path) = std::env::var_os(key) else {
        return;
    };
    let (width, height) = app.reading_viewport().dimensions();
    let fonts: Vec<_> = app
        .fonts
        .iter()
        .map(|font| font.display().to_string())
        .collect();
    let mut canvas = Canvas::new_viewport(width, height, WIDTH, &fonts).unwrap();
    let page = primitive_app::frame(app, &mut canvas, false);
    primitive_app::write_png(Path::new(&path), width, height, &page).unwrap();
}

#[test]
fn reading_footer_and_skim_follow_the_prepared_reflow_viewport() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let mut app = open_app(&fixture);
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(0, 2)
        .unwrap();
    app.controls.footer_scope = ProgressScope::Book;
    app.controls.footer_track_position = FooterTrackPosition::Below;
    let original = app.current_position().unwrap();
    capture_test_screen(&app, &app.fonts, "READER_VIEWPORT_CHROME_PORTRAIT", false);

    let viewport = ReadingViewport::LANDSCAPE;
    let reading = &mut app.active.as_mut().unwrap().reading;
    *reading = reading.prepare_viewport(viewport).unwrap();
    assert_eq!(app.reading_viewport(), viewport);
    let (width, height) = viewport.dimensions();
    let exit = viewport.done();
    assert!(exit.right() as u32 <= width && exit.bottom() as u32 <= height);
    let track = app.footer_track_rect(180).unwrap();
    assert_eq!(track.y, height as i32 - 12);
    assert!(track.right() < exit.x);
    assert_eq!(app.page_paint().visible, 0..viewport.page_height());
    capture(&app, "READER_VIEWPORT_CHROME_LANDSCAPE");

    let lower_swipe = Input::Swipe {
        dx: 0.0,
        dy: -145.0,
        start_y: height as f32 - 22.0,
    };
    assert_eq!(
        app.reading_edge_swipe(lower_swipe, width as f32 / 2.0),
        Some(Change::Partial)
    );
    let toolbar_exit = app.reading_button("Exit");
    assert!(toolbar_exit.x > 1264 && toolbar_exit.bottom() as u32 <= height);
    capture(&app, "READER_VIEWPORT_CHROME_BARS");
    app.show(Screen::Book);

    app.controls.skim_position = SkimPosition::Bottom;
    app.activate(Action::Skim).unwrap();
    let skim = app.skim_layout();
    assert!(skim.panel.bottom() <= viewport.page_height() as i32);
    assert!(skim.track.right() > 1264);
    capture(&app, "READER_VIEWPORT_CHROME_SKIM");
    assert_eq!(app.input(tap(skim.cancel)), Change::Partial);
    assert_eq!(app.screen, Screen::Book);

    let reading = &mut app.active.as_mut().unwrap().reading;
    *reading = reading.prepare_viewport(ReadingViewport::PORTRAIT).unwrap();
    assert_eq!(app.current_position().as_ref(), Some(&original));
    assert_eq!(app.reading_viewport().done(), DONE);
}

#[test]
fn fixed_chrome_and_footer_use_the_resized_zoom_and_exit_hit_areas() {
    let fixture = Fixture::new();
    let pdf = fixture.root.join("Tilted Scan.pdf");
    fs::copy(crate::test_fixture("tilted-scan.pdf"), &pdf).unwrap();
    let mut app = open_app(&fixture);
    app.open_path(&pdf).unwrap();
    let reading = &mut app.fixed.as_mut().unwrap().reading;
    let viewport = ReadingViewport::LANDSCAPE;
    let candidate = reading.prepare_viewport(viewport).unwrap();
    reading.commit_viewport(candidate).unwrap();
    let zoom = reading.zoom_buttons().0;
    assert_eq!(zoom.y, viewport.done().y);
    assert!(zoom.x > 1264 - 20);
    let track = app.footer_track_rect(168).unwrap();
    assert!(track.right() < zoom.x);
    capture(&app, "READER_VIEWPORT_CHROME_FIXED");
    assert_eq!(app.input(tap(viewport.done())), Change::Exit);
}

fn adopt_landscape(app: &mut Reader) -> primitive_app::viewport::Viewport {
    use crate::app::reading_viewport_stage::draw_test_candidate;
    use primitive_app::viewport::{LogicalTurn, ViewportDecision};

    let upright = draw_test_candidate(app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    app.set_reading_orientation(ReadingOrientation::LandscapeLeft)
        .unwrap();
    let accepted = draw_test_candidate(app, 2, LogicalTurn::Counterclockwise, "", false);
    app.finish_viewport(accepted, ViewportDecision::Commit);
    accepted
}

fn open_wide_chapter(fixture: &Fixture) -> Reader {
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let mut app = open_app(fixture);
    app.open_path(&fixture.book).unwrap();
    app.active
        .as_mut()
        .unwrap()
        .reading
        .go_to_page(0, 2)
        .unwrap();
    app
}

fn assert_wide_body_and_footer(app: &Reader) {
    use crate::app::selection_viewport_tests::{canvas, press};

    let viewport = ReadingViewport::LANDSCAPE;
    let (width, _) = viewport.dimensions();
    assert_eq!(app.reading_viewport(), viewport);
    let mut frame = canvas(app, viewport);
    let pixels = primitive_app::frame(app, &mut frame, false);
    let ink_in = |area: Rect| {
        pixels
            .chunks_exact(width as usize)
            .skip(area.y as usize)
            .take(area.h as usize)
            .map(|row| {
                row[area.x as usize..area.right() as usize]
                    .iter()
                    .filter(|&&ink| ink < 96)
                    .count()
            })
            .sum::<usize>()
    };
    if let Some(book) = &app.active {
        let reading = &book.reading;
        let expected = if app.controls.overlap_footer {
            viewport.dimensions().1
        } else {
            viewport.page_height()
        };
        assert_eq!(
            (
                reading.config.settings.width,
                reading.config.settings.height
            ),
            (width, expected)
        );
        assert_eq!(
            (reading.probe.settings.width, reading.probe.settings.height),
            (width, expected)
        );
        assert!(
            ink_in(Rect::new(WIDTH as i32, 140, 360, 960)) > 150,
            "the rendered body stops before the landscape right edge"
        );
    }
    let caption = Rect::new(42, viewport.page_height() as i32 + 8, 950, 47);
    assert!(
        ink_in(caption) > 100,
        "the landscape page caption is missing"
    );
    press(app, &mut frame, viewport.done(), false);
}

#[test]
fn applying_a_reflow_profile_preserves_the_landscape_body_footer_and_source() {
    use crate::app::tests::tap_contents_item;
    use crate::reading_profiles::{Field, ValueField};

    let fixture = Fixture::new();
    let mut app = open_wide_chapter(&fixture);
    let accepted = adopt_landscape(&mut app);
    let before = app.current_position().unwrap();
    assert_wide_body_and_footer(&app);
    capture(&app, "READER_PROFILE_LANDSCAPE_BEFORE");

    app.save_reading_profile("Wide layout").unwrap();
    let mut profiles = app.profiles.clone();
    profiles[0].set_fields(vec![Field::LineSpacing]).unwrap();
    profiles[0]
        .set_value(ValueField::LineSpacing, 122.0)
        .unwrap();
    reading_profiles::save(&app.profiles_path, &app.profiles, &profiles).unwrap();
    app.profiles = profiles;
    app.show(Screen::ReadingProfile(0));
    tap_contents_item(&mut app, "Apply to this book");

    assert_eq!(app.screen, Screen::Book);
    assert!(!app.viewport_needs_stage(accepted));
    assert_eq!(app.current_position().unwrap().anchor, before.anchor);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .effective_line_spacing_percent(),
        122
    );
    assert_wide_body_and_footer(&app);
    capture(&app, "READER_PROFILE_LANDSCAPE_AFTER");
    assert_wide_body_and_footer(&app);
    assert_eq!(
        app.input(tap(ReadingViewport::LANDSCAPE.done())),
        Change::Exit
    );
}

#[test]
fn resetting_active_reflow_defaults_retains_accepted_landscape_pixels_and_footer() {
    let fixture = Fixture::new();
    let mut app = open_wide_chapter(&fixture);
    let accepted = adopt_landscape(&mut app);
    let before = app.current_position().unwrap();
    let mut template = app.current_text_preferences().unwrap();
    template.line_spacing_percent = Some(128);
    let mut templates = app.document_defaults.templates.clone();
    templates.reflow = Some(template);
    app.document_defaults.save(templates).unwrap();
    app.activate_document_defaults(default_settings::Command::ResetReflow)
        .unwrap();
    assert_eq!(app.screen, Screen::DocumentDefaults);
    app.show(Screen::Book);
    assert!(!app.viewport_needs_stage(accepted));
    assert_eq!(app.current_position().unwrap().anchor, before.anchor);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .effective_line_spacing_percent(),
        128
    );
    assert_wide_body_and_footer(&app);
}

#[test]
fn toggling_overlap_in_landscape_keeps_epub_and_pdf_body_and_footer_visible() {
    let fixture = Fixture::new();
    fs::copy(crate::test_fixture("long-chapter.epub"), &fixture.book).unwrap();
    let pdf = fixture.root.join("Harbor scans.pdf");
    fs::write(
        &pdf,
        crate::fixed_session::tests::striped_pdf_sized_bytes(3, 612, 820),
    )
    .unwrap();

    for path in [&fixture.book, &pdf] {
        let mut app = open_app(&fixture);
        app.open_path(path).unwrap();
        let page = if let Some(book) = &mut app.active {
            book.reading.go_to_page(0, 2).unwrap();
            None
        } else {
            let reading = &mut app.fixed.as_mut().unwrap().reading;
            reading.jump(1).unwrap();
            Some(1)
        };
        let accepted = adopt_landscape(&mut app);
        let before = app.current_position().unwrap();
        assert_wide_body_and_footer(&app);
        for overlap_footer in [true, false] {
            app.activate(Action::SetReadingControls(ReadingControls {
                overlap_footer,
                ..app.controls
            }))
            .unwrap();
            assert!(!app.viewport_needs_stage(accepted));
            let after = app.current_position().unwrap();
            assert_eq!(
                (after.chapter, after.anchor),
                (before.chapter, before.anchor.clone())
            );
            if let Some(expected) = page {
                assert_eq!(after.page, expected);
            }
            assert_wide_body_and_footer(&app);
        }
        assert_eq!(
            app.input(tap(ReadingViewport::LANDSCAPE.done())),
            Change::Exit
        );
    }
}

#[test]
fn font_fit_recovery_preserves_the_book_anchor_through_the_negotiated_landscape_redraw() {
    use crate::app::reading_viewport_stage::draw_test_candidate;
    use primitive_app::viewport::ViewportDecision;
    use primitive_ui::{Sheet, SheetAction};

    let fixture = Fixture::new();
    let mut app = open_wide_chapter(&fixture);
    let accepted = adopt_landscape(&mut app);
    let id = app.open_book_id().unwrap();
    let before = app.current_position().unwrap();
    app.library.state_mut(id).unwrap().preferences = BookPreferences {
        logical_font_size: Some(255.),
        line_height: Some(3.),
        margin_top: Some(350),
        margin_bottom: Some(350),
        ..Default::default()
    };
    app.library.state_mut(id).unwrap().position = Some(before.clone());
    app.library.persist().unwrap();
    app.open_path(&fixture.book).unwrap();
    assert!(app.logical_recovery.is_some());
    let confirm = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "")
        .button(SheetAction::Confirm);
    assert_eq!(app.input(tap(confirm)), Change::Partial);
    assert!(app.logical_recovery.is_none());
    let recovered = &app.active.as_ref().unwrap().reading.config;
    assert_eq!(recovered.viewport, ReadingViewport::PORTRAIT);
    assert_eq!(
        (recovered.settings.width, recovered.settings.height),
        (WIDTH, PAGE_HEIGHT)
    );
    assert!(app.viewport_needs_stage(accepted));

    let redrawn = draw_test_candidate(&mut app, 3, accepted.turn, "", false);
    app.finish_viewport(redrawn, ViewportDecision::Commit);
    assert_eq!(app.current_position().unwrap().anchor, before.anchor);
    assert_wide_body_and_footer(&app);
}
