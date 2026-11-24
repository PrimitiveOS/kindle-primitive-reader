use super::*;
use crate::app::reading_viewport_stage::draw_test_candidate;
use crate::app::selection_viewport_tests::{canvas, key, press, tap};
use crate::app::tests::{Fixture, tap_contents_item};
use crate::app::{Action, App, BarPanel, Change, Input, ReadingOrientation};
use crate::footnotes::font_size::FontSize;
use crate::session::ReadingViewport;
use primitive_app::viewport::{LogicalTurn, ViewportDecision, ViewportRequest};
use primitive_ui::{Key, Layer};

fn reader(fixture: &Fixture) -> Reader {
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    Reader::open(&fixture.root, &fixture.state, fonts).unwrap()
}

fn adopt_landscape(app: &mut Reader, orientation: ReadingOrientation, turn: LogicalTurn) {
    let upright = draw_test_candidate(app, 1, LogicalTurn::Upright, "", false);
    app.finish_viewport(upright, ViewportDecision::Commit);
    app.set_reading_orientation(orientation).unwrap();
    let rotated = draw_test_candidate(app, 2, turn, "", false);
    app.finish_viewport(rotated, ViewportDecision::Commit);
    assert_landscape(app, turn);
}

fn assert_landscape(app: &Reader, turn: LogicalTurn) {
    assert_eq!(app.viewport_request(), Some(ViewportRequest::Fixed(turn)));
    assert_eq!(app.reading_viewport(), ReadingViewport::LANDSCAPE);
}

fn press_key(app: &mut Reader, key_name: Key) {
    let rect = key(app, key_name, "Set");
    tap(app, rect);
}

fn tap_back(app: &mut Reader) {
    let back = app.navigation_layout().back();
    tap(app, back);
}

fn write(app: &mut Reader, value: &str) {
    while !app.editor.text.is_empty() {
        press_key(app, Key::Delete);
    }
    for character in value.chars() {
        let layer = if character == '-' {
            Layer::Symbols
        } else {
            Layer::Lower
        };
        if app.editor.layer != layer {
            press_key(app, Key::Layer(layer));
        }
        press_key(app, Key::Char(character));
    }
    assert_eq!(app.editor.string(), value);
}

fn long_tap(app: &mut Reader, rect: Rect) {
    assert_eq!(
        app.input(Input::LongTap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32
        )),
        Change::Partial
    );
}

#[test]
fn word_spacing_keeps_both_drafts_through_viewport_stage_and_commits_from_the_keyboard() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let turn = LogicalTurn::Clockwise;
    adopt_landscape(&mut app, ReadingOrientation::LandscapeRight, turn);
    let before = app.current_position().unwrap();
    let spacing_before = app
        .active
        .as_ref()
        .unwrap()
        .reading
        .config
        .settings
        .percent_word_spacing;
    app.open_reading_edge(false, 840.);
    app.reading_panel = Some(BarPanel::TextSpacing);
    let more = app.reading_button("More");
    tap(&mut app, more);
    assert!(matches!(
        app.screen,
        Screen::NumberingValue(NumberField::WordSpacing(_))
    ));
    assert_landscape(&app, turn);

    let layout = app.numeric_layout();
    let [scaling, reduction] = layout.choices([Rect::default(); 2]);
    let [minus, default, plus] = layout.steps([Rect::default(); 3], true);
    let keyboard_y = app.navigation_layout().keyboard().keyboard_area().y;
    let detail = layout.detail(Rect::default(), true);
    assert!(layout.field().right() < scaling.x);
    assert!(layout.field().bottom() < keyboard_y);
    assert!(layout.notice().bottom() < keyboard_y);
    assert!(scaling.bottom() < minus.y && reduction.bottom().max(plus.bottom()) < keyboard_y);
    assert!(minus.bottom() < detail.y && detail.bottom() < keyboard_y);
    let mut frame = canvas(&app, ReadingViewport::LANDSCAPE);
    for rect in [
        scaling,
        reduction,
        minus,
        default,
        plus,
        app.navigation_layout().back(),
    ] {
        press(&app, &mut frame, rect, false);
    }

    write(&mut app, "123");
    tap(&mut app, reduction);
    write(&mut app, "67");
    let screen = app.screen;
    let caret = app.editor.caret;
    assert_eq!(app.input(Input::PageForward), Change::None);
    let aborted = draw_test_candidate(&mut app, 3, LogicalTurn::Upright, "", false);
    app.finish_viewport(aborted, ViewportDecision::Abort);
    assert_eq!(app.screen, screen);
    assert_eq!(app.editor.string(), "67");
    assert_eq!(app.editor.caret, caret);
    assert_landscape(&app, turn);
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .percent_word_spacing,
        spacing_before
    );

    let shown = draw_test_candidate(&mut app, 4, turn, "landscape-numeric-editor", false);
    app.finish_viewport(shown, ViewportDecision::Commit);
    assert_eq!(app.editor.string(), "67");
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::BookTools);
    assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
    assert_landscape(&app, turn);
    assert_eq!(app.current_position().unwrap().anchor, before.anchor);
    let applied = Some(crate::WordSpacePercent {
        scaling: 123,
        minimum: 67,
    });
    assert_eq!(
        app.active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .percent_word_spacing,
        applied
    );
    app.save().unwrap();
    let mut reopened = reader(&fixture);
    reopened.open_path(&fixture.book).unwrap();
    assert_eq!(
        reopened
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .percent_word_spacing,
        applied
    );
}

#[test]
fn footnote_modes_steps_and_touch_or_page_back_keep_the_links_settings_in_landscape() {
    use super::super::footnote_font::Mode;

    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let turn = LogicalTurn::Counterclockwise;
    adopt_landscape(&mut app, ReadingOrientation::LandscapeLeft, turn);
    let before = app.controls.footnote_font_size;

    app.show(Screen::ReadingControls);
    assert_landscape(&app, turn);
    tap_contents_item(&mut app, "Links");
    assert_landscape(&app, turn);
    tap_contents_item(&mut app, "Footnote popup font size");
    assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Relative));
    let layout = app.numeric_layout();
    let [_, fixed] = layout.choices([Rect::default(); 2]);
    let [decrease, _, _] = layout.steps([Rect::default(); 3], true);
    tap(&mut app, fixed);
    assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Fixed));
    assert_landscape(&app, turn);
    write(&mut app, "256");
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Fixed));
    assert!(app.notice.as_deref().unwrap().contains("12 to 255"));
    assert_eq!(app.controls.footnote_font_size, before);
    assert_eq!(app.input(Input::PageBack), Change::Partial);
    assert_eq!(app.screen, Screen::LinkSettings);
    assert_landscape(&app, turn);

    tap_contents_item(&mut app, "Footnote popup font size");
    long_tap(&mut app, decrease);
    assert_eq!(app.editor.string(), "-5");
    tap_back(&mut app);
    assert_eq!(app.controls.footnote_font_size, before);
    assert_eq!(app.screen, Screen::LinkSettings);
    assert_landscape(&app, turn);

    tap_contents_item(&mut app, "Footnote popup font size");
    write(&mut app, "-7");
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::LinkSettings);
    assert_eq!(app.controls.footnote_font_size, FontSize::Relative(-7));
    assert_landscape(&app, turn);
}

#[test]
fn inline_steps_and_defaults_return_to_their_matching_bottom_panel_in_landscape() {
    let fixture = Fixture::new();
    let mut app = reader(&fixture);
    app.open_path(&fixture.book).unwrap();
    let turn = LogicalTurn::Clockwise;
    adopt_landscape(&mut app, ReadingOrientation::LandscapeRight, turn);
    let owner = app.open_book_id().unwrap();
    for (field, panel, increment) in [
        (
            NumberField::ContentDpi(owner),
            BarPanel::TextRendering,
            20.0,
        ),
        (NumberField::LogicalFont(owner), BarPanel::Text, 4.0),
        (NumberField::LineSpacing(owner), BarPanel::TextLayout, 5.0),
        (
            NumberField::WordSpacing(app.word_spacing_draft(owner)),
            BarPanel::TextSpacing,
            10.0,
        ),
    ] {
        app.activate(Action::Show(Screen::NumberingValue(field)))
            .unwrap();
        assert_landscape(&app, turn);
        let layout = app.numeric_layout();
        let has_choices = matches!(field, NumberField::WordSpacing(_));
        let [_, default, increase] = layout.steps([Rect::default(); 3], has_choices);
        tap(&mut app, default);
        let value = app.editor.string().parse::<f64>().unwrap();
        long_tap(&mut app, increase);
        assert_eq!(
            app.editor.string().parse::<f64>().unwrap(),
            value + increment
        );
        tap_back(&mut app);
        assert_eq!(app.reading_panel, Some(panel));
        assert_eq!(app.screen, Screen::BookTools);
        assert_landscape(&app, turn);
    }
}

#[test]
fn fixed_pan_exact_value_uses_landscape_settings_and_preserves_position() {
    let fixture = Fixture::new();
    let path = fixture.root.join("Fixed panorama.pdf");
    std::fs::copy(crate::test_fixture("cmyk-print-chart.pdf"), &path).unwrap();
    let mut app = reader(&fixture);
    app.open_path(&path).unwrap();
    let turn = LogicalTurn::Counterclockwise;
    adopt_landscape(&mut app, ReadingOrientation::LandscapeLeft, turn);
    let location = app.current_history_location();
    app.activate(Action::Show(Screen::AutoTurn)).unwrap();
    assert_landscape(&app, turn);
    tap_contents_item(&mut app, "Fixed-page pan");
    assert_landscape(&app, turn);
    tap_contents_item(&mut app, "Horizontal overlap");
    assert_eq!(
        app.screen,
        Screen::NumberingValue(NumberField::AutoTurnOverlapHorizontal)
    );
    assert_landscape(&app, turn);
    write(&mut app, "43");
    press_key(&mut app, Key::Go);
    assert_eq!(app.screen, Screen::AutoTurnPan);
    assert_landscape(&app, turn);
    assert_eq!(
        app.fixed
            .as_ref()
            .unwrap()
            .reading
            .preferences()
            .zoom_overlap_h(),
        43
    );
    assert_eq!(app.current_history_location(), location);
}
