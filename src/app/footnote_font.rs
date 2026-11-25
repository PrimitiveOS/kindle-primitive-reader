//! Edit the saved footnote size with the shared numeric input and step buttons.
use super::{step_buttons::StepButtons, *};
use crate::footnotes::font_size::FontSize;

const RELATIVE: Rect = Rect::new(48, LIST_Y + 172, 568, 104);
const FIXED: Rect = Rect::new(648, LIST_Y + 172, 568, 104);
const FOOTNOTE_STEPS: [Rect; 3] = [
    Rect::new(48, LIST_Y + 300, 352, 110),
    Rect::new(420, LIST_Y + 316, 424, 62),
    Rect::new(864, LIST_Y + 300, 352, 110),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Relative,
    Fixed,
}

impl Mode {
    pub(super) fn current(size: FontSize) -> Self {
        match size {
            FontSize::Relative(_) => Self::Relative,
            FontSize::Fixed(_) => Self::Fixed,
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Self::Relative => "Offset from -10 to +5 pixels",
            Self::Fixed => "Fixed size from 12 to 255",
        }
    }

    fn range(self) -> std::ops::RangeInclusive<i16> {
        match self {
            Self::Relative => FontSize::RELATIVE_RANGE,
            Self::Fixed => FontSize::FIXED_RANGE,
        }
    }
}

impl Reader {
    pub(super) fn prepare_footnote_font_editor(&mut self) {
        let Screen::FootnoteFontSize(mode) = self.screen else {
            return;
        };
        let value = match (mode, self.controls.footnote_font_size) {
            (Mode::Relative, FontSize::Relative(value)) | (Mode::Fixed, FontSize::Fixed(value)) => {
                value
            }
            (Mode::Relative, _) => -2,
            (Mode::Fixed, _) => self.active.as_ref().map_or(12, |book| {
                book.reading
                    .config
                    .settings
                    .font_size
                    .round()
                    .clamp(12.0, 255.0) as i16
            }),
        };
        self.editor = Editor::new(4);
        self.editor.layer = primitive_ui::Layer::Lower;
        self.set_footnote_font_text(value);
    }

    fn set_footnote_font_text(&mut self, value: i16) {
        self.editor.text = value.to_string().chars().collect();
        self.editor.caret = self.editor.text.len();
    }

    fn footnote_font_size_value(&self, mode: Mode) -> Result<i16> {
        self.editor
            .string()
            .trim()
            .parse::<i16>()
            .ok()
            .filter(|value| mode.range().contains(value))
            .ok_or_else(|| {
                self.controls
                    .interface_language
                    .footnote_font_text(match mode {
                        Mode::Relative => "Enter a whole-number offset from -10 to +5",
                        Mode::Fixed => "Enter a whole number from 12 to 255",
                    })
                    .into()
            })
    }

    pub(super) fn footnote_font_input(&mut self, input: Input) -> Option<Change> {
        let Screen::FootnoteFontSize(mode) = self.screen else {
            return None;
        };
        let layout = self.numeric_layout();
        let [relative, fixed] = layout.choices([RELATIVE, FIXED]);
        let selected = match input {
            Input::Tap(x, y) if relative.contains(x, y) => Some(Mode::Relative),
            Input::Tap(x, y) if fixed.contains(x, y) => Some(Mode::Fixed),
            _ => None,
        };
        if let Some(selected) = selected {
            if selected == mode {
                return Some(Change::None);
            }
            self.show(Screen::FootnoteFontSize(selected));
            return Some(Change::Partial);
        }
        let [decrease, _, increase] = layout.steps(FOOTNOTE_STEPS, true);
        let step = if mode == Mode::Fixed { 10 } else { 3 };
        let delta = StepButtons::new(decrease, increase).delta(input, step)?;
        Some(match self.footnote_font_size_value(mode) {
            Ok(value) => {
                let range = mode.range();
                self.set_footnote_font_text((value + delta).clamp(*range.start(), *range.end()));
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    pub(super) fn apply_footnote_font_input(&mut self, mode: Mode) -> Result<()> {
        let value = self.footnote_font_size_value(mode)?;
        let footnote_font_size = match mode {
            Mode::Relative => FontSize::Relative(value),
            Mode::Fixed => FontSize::Fixed(value),
        };
        self.activate(Action::SetReadingControls(ReadingControls {
            footnote_font_size,
            ..self.controls
        }))?;
        self.show(Screen::LinkSettings);
        Ok(())
    }

    pub(super) fn draw_footnote_font_editor(&self, canvas: &mut Canvas, mode: Mode) {
        let layout = self.numeric_layout();
        let [relative, fixed] = layout.choices([RELATIVE, FIXED]);
        let [decrease, caption, increase] = layout.steps(FOOTNOTE_STEPS, true);
        let language = self.controls.interface_language;
        canvas.button(
            relative,
            language.footnote_font_text("Relative"),
            mode == Mode::Relative,
            false,
        );
        canvas.button(
            fixed,
            language.footnote_font_text("Fixed"),
            mode == Mode::Fixed,
            false,
        );
        StepButtons::new(decrease, increase).draw(canvas);
        if !layout.is_landscape() {
            canvas.text(
                caption,
                language.footnote_font_text(mode.hint()),
                CAPTION.center(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen, tap_contents_item};

    fn tap(app: &mut Reader, rect: Rect) -> Change {
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    fn set(app: &mut Reader, value: &str) {
        app.editor.text = value.chars().collect();
        app.editor.caret = app.editor.text.len();
        let button = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap()
            .1;
        assert_eq!(tap(app, button), Change::Partial);
    }

    #[test]
    fn footnote_font_editor_saves_valid_modes_and_keeps_the_prior_value_until_set() {
        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let open = || Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let mut app = open();
        let before = app.controls;
        app.open_path(&fixture.book).unwrap();
        app.show(Screen::LinkSettings);
        tap_contents_item(&mut app, "Footnote popup font size");
        assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Relative));
        assert_eq!(app.editor.string(), "-2");
        capture_test_screen(
            &app,
            &fonts,
            "READER_FOOTNOTE_FONT_SIZE_SETTINGS_FRAME",
            false,
        );
        set(&mut app, "-11");
        assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Relative));
        assert_eq!(app.controls, before);
        set(&mut app, "-10");
        assert_eq!(app.screen, Screen::LinkSettings);
        let mut expected = before;
        expected.footnote_font_size = FontSize::Relative(-10);
        assert_eq!(app.controls, expected);
        tap_contents_item(&mut app, "Footnote popup font size");
        assert_eq!(tap(&mut app, FIXED), Change::Partial);
        assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Fixed));
        let document = app
            .active
            .as_ref()
            .unwrap()
            .reading
            .config
            .settings
            .font_size;
        assert_eq!(
            app.editor.string(),
            (document.round().clamp(12.0, 255.0) as i16).to_string()
        );
        set(&mut app, "256");
        assert_eq!(app.screen, Screen::FootnoteFontSize(Mode::Fixed));
        assert_eq!(app.controls, expected);
        set(&mut app, "255");
        expected.footnote_font_size = FontSize::Fixed(255);
        assert_eq!(app.controls, expected);
        tap_contents_item(&mut app, "Footnote popup font size");
        assert_eq!(app.editor.string(), "255");
        assert_eq!(tap(&mut app, RELATIVE), Change::Partial);
        assert_eq!(app.editor.string(), "-2");
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.screen, Screen::LinkSettings);
        assert_eq!(app.controls, expected);
        drop(app);
        assert_eq!(open().controls, expected);
    }
}
