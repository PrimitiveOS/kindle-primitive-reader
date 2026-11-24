//! Edit the two reading-statistics limits before saving them together.
use super::{
    CAPTION, Canvas, Change, Command, Input, Options, Page, Reader, Rect, Screen, TextStyle,
};
use crate::app::step_buttons::StepButtons;
use crate::reading_stats::history::{MAX_DURATION_RANGE, MIN_DURATION_RANGE};
use primitive_ui::{Editor, Key, Press};
use std::ops::RangeInclusive;

const SHEET: Rect = Rect::new(86, 490, 1092, 700);
const TITLE: Rect = Rect::new(130, 524, 1004, 75);
const DESCRIPTION: Rect = Rect::new(130, 613, 1004, 100);
const DEFAULT: Rect = Rect::new(130, 904, 1004, 89);
const NOTICE: Rect = Rect::new(130, 1002, 1004, 52);
const CANCEL: Rect = Rect::new(130, 1065, 472, 86);
const APPLY: Rect = Rect::new(662, 1065, 472, 86);

const ENTRY: Rect = Rect::new(86, 392, 1092, 896);
const ENTRY_TITLE: Rect = Rect::new(130, 424, 1004, 72);
const ENTRY_FIELD: Rect = Rect::new(130, 510, 1004, 100);
const ENTRY_HINT: Rect = Rect::new(130, 620, 1004, 54);
const ENTRY_CANCEL: Rect = Rect::new(130, 1160, 472, 86);
const ENTRY_USE: Rect = Rect::new(662, 1160, 472, 86);

fn digit_buttons() -> impl Iterator<Item = (Rect, &'static str, Key)> {
    ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            let index = index as i32;
            let rect = Rect::new(130 + index % 3 * 342, 686 + index / 3 * 112, 320, 94);
            (rect, label, Key::Char(char::from(label.as_bytes()[0])))
        })
        .chain([
            (Rect::new(130, 1022, 662, 94), "0", Key::Char('0')),
            (Rect::new(814, 1022, 320, 94), "Delete", Key::Delete),
        ])
}

#[derive(Clone, Copy)]
enum Limit {
    Min,
    Max,
}

impl Limit {
    fn range(self) -> RangeInclusive<u32> {
        match self {
            Self::Min => MIN_DURATION_RANGE,
            Self::Max => MAX_DURATION_RANGE,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Min => "Minimum",
            Self::Max => "Maximum",
        }
    }

    fn value(self, options: Options) -> u32 {
        match self {
            Self::Min => options.min_seconds,
            Self::Max => options.max_seconds,
        }
    }

    fn set(self, options: &mut Options, value: u32) {
        match self {
            Self::Min => options.min_seconds = value,
            Self::Max => options.max_seconds = value,
        }
    }

    fn left(self) -> i32 {
        match self {
            Self::Min => 130,
            Self::Max => 662,
        }
    }

    fn value_button(self) -> Rect {
        Rect::new(self.left() + 130, 782, 212, 94)
    }

    fn steps(self) -> StepButtons {
        let left = self.left();
        StepButtons::new(
            Rect::new(left, 782, 110, 94),
            Rect::new(left + 362, 782, 110, 94),
        )
    }

    fn step(self, options: &mut Options, input: Input) -> bool {
        let (tap_step, hold_units) = match self {
            Self::Min => (1, 10),
            Self::Max => (10, 6),
        };
        let Some(delta) = self.steps().delta(input, hold_units) else {
            return false;
        };
        let range = self.range();
        let next = self
            .value(*options)
            .saturating_add_signed(i32::from(delta) * tap_step)
            .clamp(*range.start(), *range.end());
        self.set(options, next);
        true
    }
}

const LIMITS: [Limit; 2] = [Limit::Min, Limit::Max];

struct Entry {
    limit: Limit,
    editor: Editor,
    invalid: bool,
}

pub(super) struct Draft {
    options: Options,
    entry: Option<Entry>,
}

impl Reader {
    pub(super) fn open_duration_limits(&mut self) {
        self.notice = None;
        self.history_view.duration_limits_draft = Some(Draft {
            options: self.reading_history.options(),
            entry: None,
        });
    }

    pub(in crate::app) fn draw_history_duration_limits(&self, canvas: &mut Canvas) {
        let Some(draft) = self
            .history_view
            .duration_limits_draft
            .as_ref()
            .filter(|_| self.screen == Screen::ReadingHistory(Page::Options))
        else {
            return;
        };
        if let Some(entry) = &draft.entry {
            canvas.popup(ENTRY);
            canvas.text(
                ENTRY_TITLE,
                &format!("{} page duration", entry.limit.title()),
                TextStyle::new(48).bold(),
            );
            canvas.single_line_field(
                ENTRY_FIELD,
                &entry.editor,
                &entry.limit.value(draft.options).to_string(),
                false,
            );
            let range = entry.limit.range();
            let hint = if entry.invalid {
                format!(
                    "Enter a whole number from {} to {}",
                    range.start(),
                    range.end()
                )
            } else {
                format!("{} to {} seconds", range.start(), range.end())
            };
            canvas.text(ENTRY_HINT, &hint, CAPTION);
            for (rect, label, _) in digit_buttons() {
                canvas.button(rect, label, false, false);
            }
            canvas.button(ENTRY_CANCEL, "Cancel", false, false);
            canvas.button(ENTRY_USE, "Use value", true, false);
            return;
        }
        canvas.popup(SHEET);
        canvas.text(
            TITLE,
            "Read page duration limits",
            TextStyle::new(48).bold(),
        );
        canvas.text(
            DESCRIPTION,
            "Cap visits above the maximum. Otherwise, skip visits below the minimum.",
            CAPTION.lines(2),
        );
        for limit in LIMITS {
            canvas.text(
                Rect::new(limit.left(), 726, 472, 45),
                limit.title(),
                CAPTION.center(),
            );
            limit.steps().draw(canvas);
            canvas.button(
                limit.value_button(),
                &format!("{} s", limit.value(draft.options)),
                false,
                false,
            );
        }
        let defaults = Options::default();
        canvas.button(
            DEFAULT,
            &format!(
                "Defaults    {} s - {} s",
                defaults.min_seconds, defaults.max_seconds
            ),
            false,
            false,
        );
        if let Some(notice) = &self.notice {
            canvas.text(NOTICE, notice, CAPTION);
        }
        canvas.button(CANCEL, "Cancel", false, false);
        canvas.button(
            APPLY,
            "Apply",
            draft.options != self.reading_history.options(),
            false,
        );
    }

    pub(super) fn duration_limits_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let draft = self
            .history_view
            .duration_limits_draft
            .as_mut()
            .filter(|_| self.screen == Screen::ReadingHistory(Page::Options))?;
        if let Some(entry) = &mut draft.entry {
            if input == Input::PageBack
                || matches!(input, Input::Tap(x, y) if ENTRY_CANCEL.contains(x, y) || !ENTRY.contains(x, y))
            {
                draft.entry = None;
                return Some(Change::Partial);
            }
            let Input::Tap(x, y) = input else {
                return Some(Change::None);
            };
            if ENTRY_USE.contains(x, y) {
                if entry.invalid {
                    return Some(Change::None);
                }
                if let Some(value) = entry
                    .editor
                    .string()
                    .parse::<u32>()
                    .ok()
                    .filter(|value| entry.limit.range().contains(value))
                {
                    entry.limit.set(&mut draft.options, value);
                    draft.entry = None;
                } else {
                    entry.invalid = true;
                }
            } else if let Some((_, _, key)) =
                digit_buttons().find(|(rect, _, _)| rect.contains(x, y))
            {
                let change = entry.editor.press(key);
                entry.invalid = matches!(key, Key::Char(_)) && change == Press::Nothing;
            } else {
                return Some(Change::None);
            }
            return Some(Change::Partial);
        }

        if input == Input::PageBack
            || matches!(input, Input::Tap(x, y) if CANCEL.contains(x, y) || !SHEET.contains(x, y))
        {
            self.history_view.duration_limits_draft = None;
        } else if matches!(input, Input::Tap(x, y) if APPLY.contains(x, y)) {
            if draft.options == self.reading_history.options() {
                return Some(Change::None);
            }
            if !draft.options.duration_limits_valid() {
                return Some(self.report("Adjust the saved limit to fit the keypad range."));
            }
            let options = draft.options;
            if let Err(error) = self.activate_reading_history(Command::Configure(options)) {
                return Some(self.report(error));
            }
            self.history_view.duration_limits_draft = None;
        } else if matches!(input, Input::Tap(x, y) if DEFAULT.contains(x, y)) {
            let defaults = Options::default();
            draft.options.min_seconds = defaults.min_seconds;
            draft.options.max_seconds = defaults.max_seconds;
        } else if !LIMITS
            .into_iter()
            .any(|limit| limit.step(&mut draft.options, input))
        {
            let Some(limit) = LIMITS.into_iter().find(
                |limit| matches!(input, Input::Tap(x, y) if limit.value_button().contains(x, y)),
            ) else {
                return Some(Change::None);
            };
            draft.entry = Some(Entry {
                limit,
                editor: Editor::new(10),
                invalid: false,
            });
        }
        self.notice = None;
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use primitive_app::App;
    use std::fs;

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn press(app: &mut Reader, rect: Rect) {
        assert_eq!(app.input(tap(rect)), Change::Partial);
    }

    fn hold(app: &mut Reader, rect: Rect) {
        let Input::Tap(x, y) = tap(rect) else {
            unreachable!()
        };
        assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
    }

    fn enter(app: &mut Reader, value: &str) {
        for digit in value.chars() {
            let (rect, _, _) = digit_buttons()
                .find(|(_, _, key)| *key == Key::Char(digit))
                .unwrap();
            press(app, rect);
        }
    }

    fn setup() -> (Fixture, Reader, [std::path::PathBuf; 2]) {
        let fixture = Fixture::new();
        let fonts = [
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        app.open_path(&fixture.book).unwrap();
        app.activate_reading_history(Command::Open(None)).unwrap();
        app.activate_reading_history(Command::Show(Page::Options))
            .unwrap();
        (fixture, app, fonts)
    }

    fn open(app: &mut Reader) {
        let row = app
            .list_window()
            .1
            .iter()
            .position(|item| item.title == "Read page duration limits")
            .unwrap();
        press(app, app.list_geometry().row(row));
    }

    fn draft(app: &Reader) -> &Draft {
        app.history_view.duration_limits_draft.as_ref().unwrap()
    }

    #[test]
    fn duration_limits_tap_hold_keypad_cancel_defaults_and_persist_atomically() {
        let (fixture, mut app, fonts) = setup();
        let position = app.current_position().unwrap();
        open(&mut app);
        assert_eq!(app.input(tap(APPLY)), Change::None);
        press(&mut app, Rect::new(492, 782, 110, 94));
        hold(&mut app, Rect::new(492, 782, 110, 94));
        press(&mut app, Rect::new(1024, 782, 110, 94));
        hold(&mut app, Rect::new(1024, 782, 110, 94));
        assert_eq!(draft(&app).options.min_seconds, 16);
        assert_eq!(draft(&app).options.max_seconds, 190);
        press(&mut app, Limit::Max.value_button());
        enter(&mut app, "72001");
        press(&mut app, ENTRY_USE);
        assert!(draft(&app).entry.as_ref().unwrap().invalid);
        assert_eq!(draft(&app).options.max_seconds, 190);
        let (delete, _, _) = digit_buttons()
            .find(|(_, _, key)| *key == Key::Delete)
            .unwrap();
        press(&mut app, delete);
        capture_test_screen(&app, &fonts, "READER_STATS_DURATION_KEYPAD_SCREEN", false);
        press(&mut app, ENTRY_USE);
        assert_eq!(draft(&app).options.max_seconds, 7200);
        let Input::Tap(x, y) = tap(Rect::new(1024, 782, 110, 94)) else {
            unreachable!()
        };
        app.input(Input::LongTap(x, y));
        assert_eq!(draft(&app).options.max_seconds, 7200);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(app.reading_history.options(), Options::default());

        open(&mut app);
        press(&mut app, Limit::Min.value_button());
        enter(&mut app, "0");
        press(&mut app, ENTRY_USE);
        press(&mut app, Limit::Max.value_button());
        enter(&mut app, "07200");
        press(&mut app, ENTRY_USE);
        capture_test_screen(&app, &fonts, "READER_STATS_DURATION_SCREEN", false);
        press(&mut app, APPLY);
        assert_eq!(app.current_position(), Some(position));
        let reopened = Reader::open(&fixture.root, &fixture.state, fonts.to_vec()).unwrap();
        assert_eq!(reopened.reading_history.options().min_seconds, 0);
        assert_eq!(reopened.reading_history.options().max_seconds, 7200);
        open(&mut app);
        press(&mut app, DEFAULT);
        press(&mut app, CANCEL);
        assert_eq!(app.reading_history.options().max_seconds, 7200);
        open(&mut app);
        press(&mut app, DEFAULT);
        assert_eq!(draft(&app).options, Options::default());
        press(&mut app, APPLY);
        assert_eq!(app.reading_history.options(), Options::default());
    }

    #[test]
    fn duration_limits_failed_save_keeps_the_draft_for_retry() {
        let (fixture, mut app, _) = setup();
        let journal = fixture.state.with_file_name("reading-history.json");
        if journal.exists() {
            fs::remove_file(&journal).unwrap();
        }
        fs::create_dir(&journal).unwrap();
        open(&mut app);
        press(&mut app, Limit::Max.value_button());
        enter(&mut app, "7200");
        press(&mut app, ENTRY_USE);
        press(&mut app, APPLY);
        assert_eq!(app.reading_history.options(), Options::default());
        assert_eq!(draft(&app).options.max_seconds, 7200);
        assert!(app.notice.is_some());
        fs::remove_dir(&journal).unwrap();
        press(&mut app, APPLY);
        assert_eq!(app.reading_history.options().max_seconds, 7200);
        assert!(app.notice.is_none());
    }

    #[test]
    fn duration_limits_edit_keeps_an_untouched_legacy_limit() {
        let (_fixture, mut app, _) = setup();
        app.activate_reading_history(Command::Configure(Options {
            min_seconds: 180,
            max_seconds: 1800,
            ..Options::default()
        }))
        .unwrap();
        open(&mut app);
        press(&mut app, Limit::Max.value_button());
        enter(&mut app, "300");
        press(&mut app, ENTRY_USE);
        press(&mut app, APPLY);
        assert_eq!(app.reading_history.options().min_seconds, 180);
        assert_eq!(app.reading_history.options().max_seconds, 300);
    }
}
