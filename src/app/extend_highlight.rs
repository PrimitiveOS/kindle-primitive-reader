//! Edit the anchored extent of a saved highlight while leaving its note in place.
use super::{
    CAPTION, Canvas, Change, DONE, HEIGHT, HistoryLocation, Input, PAGE_HEIGHT, Reader, Rect,
    Result, Screen, TextSelection, WIDTH,
};

const CANCEL: Rect = Rect::new(24, PAGE_HEIGHT as i32 + 5, 192, 60);
const APPLY: Rect = Rect::new(WIDTH as i32 - 375, PAGE_HEIGHT as i32 + 5, 200, 60);

pub(super) struct RangeEdit {
    pub(super) index: usize,
    pub(super) original: TextSelection,
    pub(super) draft: TextSelection,
    origin: HistoryLocation,
}

impl Reader {
    pub(super) fn begin_extending_highlight(&mut self) -> Result<()> {
        let active = self.active.as_ref().ok_or("Open a text highlight first")?;
        let index = self.highlight_index;
        let original = self
            .library
            .state(active.id)
            .and_then(|book| book.highlights.get(index))
            .ok_or("That highlight is unavailable")?
            .selection
            .clone();
        let origin = self
            .current_history_location()
            .ok_or("The reading position is unavailable")?;
        let chapter = active
            .reading
            .probe
            .chapter_index(&original.start.chapter)
            .ok_or("The saved chapter is unavailable")?;
        self.restore_position(&super::ReadingPosition {
            chapter,
            page: 0,
            anchor: Some(original.start.clone()),
        })?;
        self.show(Screen::Book);
        self.range_edit = Some(RangeEdit {
            index,
            draft: original.clone(),
            original,
            origin,
        });
        self.sync_auto_turn(std::time::Instant::now());
        Ok(())
    }

    pub(super) fn range_input(&mut self, input: Input) -> Change {
        let finish = match input {
            Input::Tap(x, y) if CANCEL.contains(x, y) => Some(false),
            Input::Tap(x, y) if APPLY.contains(x, y) => Some(true),
            Input::Tap(x, y) if DONE.contains(x, y) => {
                if let Err(error) = self.finish_range(false) {
                    return self.report(error);
                }
                return self.exit();
            }
            _ => None,
        };
        if let Some(save) = finish {
            return match self.finish_range(save) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
        }
        if let Input::Tap(x, y) | Input::LongTap(x, y) = input {
            let Some(active) = &self.active else {
                return Change::None;
            };
            let Some(word) = active.reading.word_at(x, y) else {
                return Change::None;
            };
            let Some(edit) = self.range_edit.as_mut() else {
                return Change::None;
            };
            return match active.reading.extend_saved_quote(&edit.original, &word) {
                Ok(draft) => {
                    edit.draft = draft;
                    self.notice = None;
                    Change::Partial
                }
                Err(error) => self.report(error),
            };
        }
        if let Some(active) = &mut self.active {
            use primitive_app::App;
            active.reading.input(input)
        } else {
            Change::None
        }
    }

    fn finish_range(&mut self, save: bool) -> Result<()> {
        let edit = self
            .range_edit
            .as_ref()
            .ok_or("No selection is being edited")?;
        let origin = edit.origin.clone();
        if save {
            if edit.draft == edit.original {
                return Err("Choose a word outside the saved highlight".into());
            }
            let active = self.active.as_ref().ok_or("The book is unavailable")?;
            let mark = self
                .library
                .state_mut(active.id)
                .and_then(|book| book.highlights.get_mut(edit.index))
                .filter(|mark| mark.selection == edit.original)
                .ok_or("The original highlight changed")?;
            mark.selection = edit.draft.clone();
            self.range_edit = None;
            self.mark_dirty();
            if self.current_history_location().as_ref() != Some(&origin) {
                self.push_history(origin);
            }
            self.show(Screen::Book);
            self.notice = Some("Highlight extended".into());
        } else {
            self.go_to_location(&origin)?;
            self.range_edit = None;
            self.show(Screen::Annotation);
        }
        Ok(())
    }

    pub(super) fn draw_range_tools(&self, canvas: &mut Canvas) {
        let Some(edit) = &self.range_edit else { return };
        canvas.fill(
            Rect::new(
                0,
                PAGE_HEIGHT as i32,
                WIDTH as i32 - 155,
                HEIGHT as i32 - PAGE_HEIGHT as i32,
            ),
            255,
        );
        canvas.button(CANCEL, "Cancel", false, false);
        canvas.text(
            Rect::new(238, PAGE_HEIGHT as i32 + 4, 600, 64),
            self.notice
                .as_deref()
                .unwrap_or("Tap a word to extend the highlight"),
            CAPTION,
        );
        canvas.button(APPLY, "Apply", edit.draft != edit.original, false);
    }
}
