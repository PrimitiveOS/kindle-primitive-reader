//! Edit the anchored extent of a saved highlight while leaving its note in place.
use super::{
    CAPTION, Canvas, Change, DONE, HEIGHT, HistoryLocation, Input, PAGE_HEIGHT, Reader, Rect,
    Result, Screen, TextSelection, WIDTH,
};

const CANCEL: Rect = Rect::new(24, PAGE_HEIGHT as i32 + 5, 192, 60);
const APPLY: Rect = Rect::new(WIDTH as i32 - 375, PAGE_HEIGHT as i32 + 5, 200, 60);

use crate::fixed_session::{FixedJump, WordAtTouch};
use crate::library::PdfHighlight;

pub(super) enum Draft {
    Text {
        original: TextSelection,
        current: TextSelection,
    },
    Fixed {
        original: PdfHighlight,
        current: PdfHighlight,
    },
}

impl Draft {
    fn changed(&self) -> bool {
        match self {
            Self::Text { original, current } => original != current,
            Self::Fixed { original, current } => original != current,
        }
    }
}

pub(super) struct RangeEdit {
    pub(super) index: usize,
    pub(super) draft: Draft,
    origin: HistoryLocation,
}

impl Reader {
    pub(super) fn begin_extending_highlight(&mut self) -> Result<()> {
        let index = self.highlight_index;
        let origin = self
            .current_history_location()
            .ok_or("The reading position is unavailable")?;
        let draft = if let Some(active) = &self.active {
            let original = self
                .library
                .state(active.id)
                .and_then(|book| book.highlights.get(index))
                .ok_or("That highlight is unavailable")?
                .selection
                .clone();
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
            Draft::Text {
                current: original.clone(),
                original,
            }
        } else if let Some(fixed) = &self.fixed {
            let original = self
                .library
                .state(fixed.id)
                .and_then(|book| book.pdf_highlights.get(index))
                .ok_or("That highlight is unavailable")?
                .clone();
            self.jump_fixed(original.page, FixedJump::regions(&original.bounds))?;
            self.show(Screen::Fixed);
            Draft::Fixed {
                current: original.clone(),
                original,
            }
        } else {
            return Err("Open a saved highlight first".into());
        };
        self.range_edit = Some(RangeEdit {
            index,
            draft,
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
            let Some(edit) = self.range_edit.as_mut() else {
                return Change::None;
            };
            let update = match &mut edit.draft {
                Draft::Text { original, current } => {
                    let Some(reading) = self.active.as_ref().map(|a| &a.reading) else {
                        return Change::None;
                    };
                    let Some(word) = reading.word_at(x, y) else {
                        return Change::None;
                    };
                    reading
                        .extend_saved_quote(original, &word)
                        .map(|draft| *current = draft)
                }
                Draft::Fixed { original, current } => {
                    let Some(reading) = self.fixed.as_ref().map(|a| &a.reading) else {
                        return Change::None;
                    };
                    match reading.select_word(x, y) {
                        Ok(WordAtTouch::Ready(Some(word))) => reading
                            .extend_saved_mark(original, &word)
                            .map(|draft| *current = draft),
                        Ok(WordAtTouch::Ready(None)) => return Change::None,
                        Ok(WordAtTouch::Recognize { page, .. }) => {
                            Err(format!("Run OCR on page {} first", page + 1).into())
                        }
                        Err(error) => Err(error),
                    }
                }
            };
            return match update {
                Ok(()) => {
                    self.notice = None;
                    Change::Partial
                }
                Err(error) => self.report(error),
            };
        }
        if let Some(active) = &mut self.active {
            use primitive_app::App;
            active.reading.input(input)
        } else if let Some(fixed) = &mut self.fixed {
            match fixed.reading.input(input) {
                Ok(true) => Change::Partial,
                Ok(false) => Change::None,
                Err(error) => self.report(error),
            }
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
            if !edit.draft.changed() {
                return Err("Choose a word outside the saved highlight".into());
            }
            let book = self.open_book_id().ok_or("The book is unavailable")?;
            let state = self
                .library
                .state_mut(book)
                .ok_or("The library entry is unavailable")?;
            let screen = match &edit.draft {
                Draft::Text { original, current } => {
                    let mark = state
                        .highlights
                        .get_mut(edit.index)
                        .filter(|mark| &mark.selection == original)
                        .ok_or("The original highlight changed")?;
                    mark.selection = current.clone();
                    Screen::Book
                }
                Draft::Fixed { original, current } => {
                    let mark = state
                        .pdf_highlights
                        .get_mut(edit.index)
                        .filter(|mark| &**mark == original)
                        .ok_or("The original highlight changed")?;
                    *mark = current.clone();
                    Screen::Fixed
                }
            };
            self.range_edit = None;
            self.mark_dirty();
            if self.current_history_location().as_ref() != Some(&origin) {
                self.push_history(origin);
            }
            self.saved_popup = None;
            self.show(screen);
            self.notice = Some("Highlight extended".into());
        } else {
            self.go_to_location(&origin)?;
            self.range_edit = None;
            if self.saved_popup.is_some() {
                self.return_to_saved_mark();
            } else {
                self.show(Screen::Annotation);
            }
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
        canvas.button(APPLY, "Apply", edit.draft.changed(), false);
    }
}
