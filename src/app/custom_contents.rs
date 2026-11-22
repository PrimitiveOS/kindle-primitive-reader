//! Build and navigate a book-specific alternative to the publisher's contents.
use super::*;
use crate::library::custom_contents::{CustomContents, TITLE_LIMIT};

const REMOVE: Rect = Rect::new(LIST_X, LIST_Y + 234, 350, 104);
const CONFIRM: Rect = Rect::new(192, 545, 880, 392);
const CANCEL: Rect = Rect::new(232, 779, 352, 110);
const DELETE: Rect = Rect::new(640, 779, 388, 110);

#[derive(Clone, Copy)]
pub(super) enum CustomAction {
    Here,
    Bookmark(usize),
    Edit(usize),
    Open(usize),
    Toggle,
}

pub(super) struct Draft {
    position: ReadingPosition,
    back: Screen,
    editing: bool,
    confirm_remove: bool,
}

impl Reader {
    fn chapter_state(&self) -> Option<&BookState> {
        self.library.state(self.open_book_id()?)
    }

    fn chapters_mut(&mut self) -> Result<&mut CustomContents> {
        let id = self.open_book_id().ok_or("Open a book first")?;
        Ok(&mut self
            .library
            .state_mut(id)
            .ok_or("This book is unavailable")?
            .custom_contents)
    }

    pub(super) fn custom_contents_enabled(&self) -> bool {
        self.chapter_state()
            .is_some_and(|state| state.custom_contents.enabled)
    }

    pub(super) fn custom_items(&self) -> Vec<Item> {
        let Some(state) = self.chapter_state() else {
            return Vec::new();
        };
        let chapters = &state.custom_contents;
        if self.screen == Screen::CustomBookmarks {
            return state
                .bookmarks
                .iter()
                .enumerate()
                .map(|(i, mark)| Item::new(&mark.label, Action::Custom(CustomAction::Bookmark(i))))
                .collect();
        }
        let mut items = Vec::new();
        if self.screen == Screen::CustomContents {
            items.push(Item::new(
                "Add current location",
                Action::Custom(CustomAction::Here),
            ));
            if !state.bookmarks.is_empty() {
                items.push(Item::new(
                    "Add from bookmark",
                    Action::Show(Screen::CustomBookmarks),
                ));
            }
            if !chapters.entries().is_empty() {
                items.push(Item::new(
                    "Edit chapters",
                    Action::Show(Screen::CustomEdits),
                ));
            }
            items.push(Item::new(
                if chapters.enabled {
                    "Use publisher contents"
                } else {
                    "Use custom contents"
                },
                Action::Custom(CustomAction::Toggle),
            ));
        } else if self.screen != Screen::CustomEdits {
            items.push(Item::new(
                "Manage custom contents",
                Action::Show(Screen::CustomContents),
            ));
        }
        for (i, entry) in chapters.entries().iter().enumerate() {
            let target = if self.screen == Screen::CustomEdits {
                CustomAction::Edit(i)
            } else {
                CustomAction::Open(i)
            };
            let detail = if self.fixed.is_some() {
                format!("Page {}", entry.position.page.saturating_add(1))
            } else {
                format!("Chapter {}", entry.position.chapter.saturating_add(1))
            };
            items.push(Item::new(&entry.label, Action::Custom(target)).detail(detail));
        }
        items
    }

    fn custom_entry(&self, index: usize) -> Result<&Bookmark> {
        self.chapter_state()
            .and_then(|state| state.custom_contents.entries().get(index))
            .ok_or_else(|| "That chapter is unavailable".into())
    }

    pub(super) fn custom_action(&mut self, action: CustomAction) -> Result<()> {
        match action {
            CustomAction::Toggle => {
                let chapters = self.chapters_mut()?;
                chapters.enabled = !chapters.enabled;
                self.mark_book_changed();
                self.show(self.contents_screen());
                Ok(())
            }
            CustomAction::Here => {
                let position = self.current_position().ok_or("Open a book first")?;
                self.edit_custom(position, String::new(), Screen::CustomContents)
            }
            CustomAction::Edit(index) => {
                let entry = self.custom_entry(index)?.clone();
                self.edit_custom(entry.position, entry.label, Screen::CustomEdits)
            }
            CustomAction::Bookmark(index) => {
                let entry = self
                    .chapter_state()
                    .and_then(|state| state.bookmarks.get(index))
                    .ok_or("That bookmark is unavailable")?
                    .clone();
                self.edit_custom(entry.position, entry.label, Screen::CustomBookmarks)
            }
            CustomAction::Open(index) => {
                let position = self.custom_entry(index)?.position.clone();
                if let Some(active) = &mut self.active {
                    let anchor = position
                        .anchor
                        .as_ref()
                        .ok_or("This chapter has no text anchor")?;
                    let previous = super::position(&active.reading);
                    active.reading.jump_to_anchor(position.chapter, anchor)?;
                    self.finish_reflow_jump(previous);
                    Ok(())
                } else {
                    if position.chapter != 0
                        || position.anchor.is_some()
                        || self
                            .fixed
                            .as_ref()
                            .is_none_or(|book| position.page >= book.reading.count())
                    {
                        return Err("That chapter belongs to a different document layout".into());
                    }
                    self.follow_fixed(position.page, None)
                }
            }
        }
    }

    pub(super) fn contents_screen(&self) -> Screen {
        if self.fixed.is_some() {
            Screen::FixedContents
        } else {
            Screen::Contents
        }
    }

    pub(super) fn edit_selected_contents_chapter(&mut self) -> Result<()> {
        if let (Some(active), Some(selection)) = (&self.active, &self.selection) {
            let anchor = selection.start.clone();
            let page = active
                .reading
                .probe
                .page_for(&anchor)
                .ok_or("This selection is unavailable")?;
            self.edit_custom(
                ReadingPosition {
                    chapter: active.reading.chapter,
                    page,
                    anchor: Some(anchor),
                },
                selection.text.clone(),
                Screen::Book,
            )
        } else if let Some(selection) = &self.pdf_selection {
            self.edit_custom(
                ReadingPosition {
                    chapter: 0,
                    page: selection.page(),
                    anchor: None,
                },
                selection.text().to_owned(),
                Screen::Fixed,
            )
        } else {
            Err("Select a passage first".into())
        }
    }

    fn edit_custom(
        &mut self,
        position: ReadingPosition,
        suggested: String,
        back: Screen,
    ) -> Result<()> {
        if self.active.is_some() && position.anchor.is_none() {
            return Err("Choose a page with a text anchor".into());
        }
        let chapters = self.chapters_mut()?;
        let existing = chapters
            .find(&position)
            .map(|index| &chapters.entries()[index]);
        let name = existing.map_or(suggested.as_str(), |entry| &entry.label);
        let editing = existing.is_some();
        let title = name.split_whitespace().collect::<Vec<_>>().join(" ");
        self.editor = Editor::new(TITLE_LIMIT);
        self.editor.text = title.chars().take(TITLE_LIMIT).collect();
        self.editor.caret = self.editor.text.len();
        self.show(Screen::CustomTitle);
        self.chapter_draft = Some(Draft {
            position,
            back,
            editing,
            confirm_remove: false,
        });
        Ok(())
    }

    fn finish_custom(&mut self, remove: bool) -> Result<()> {
        let draft = self
            .chapter_draft
            .as_ref()
            .ok_or("That chapter is unavailable")?;
        let position = draft.position.clone();
        let back = draft.back;
        if remove {
            self.chapters_mut()?.remove(&position)?;
        } else {
            let title = self.editor.string();
            self.chapters_mut()?.save(position, &title)?;
        }
        self.mark_book_changed();
        let screen = match back {
            Screen::CustomBookmarks => Screen::CustomContents,
            _ => back,
        };
        if matches!(back, Screen::Fixed | Screen::Book) {
            self.selection = None;
            self.pdf_selection = None;
        }
        self.show(screen);
        Ok(())
    }

    pub(super) fn draw_chapter_editor(&self, canvas: &mut Canvas) {
        let Some(draft) = &self.chapter_draft else {
            return;
        };
        header(
            canvas,
            if draft.editing {
                "Edit chapter"
            } else {
                "Add chapter"
            },
            "Cancel",
        );
        canvas.single_line_field(
            Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 130),
            &self.editor,
            "Chapter title",
            false,
        );
        canvas.keyboard(self.editor.layer, "Save", None);
        if draft.editing {
            canvas.button(REMOVE, "Remove chapter", false, false);
        }
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(LIST_X, LIST_Y + 145, WIDTH as i32 - 2 * LIST_X, 82),
                notice,
                CAPTION,
            );
        }
        if draft.confirm_remove {
            canvas.popup(CONFIRM);
            canvas.text(
                Rect::new(CONFIRM.x + 40, CONFIRM.y + 36, CONFIRM.w - 80, 90),
                "Remove this chapter?",
                primitive_ui::TextStyle::new(50).bold(),
            );
            canvas.text(
                Rect::new(CONFIRM.x + 40, CONFIRM.y + 132, CONFIRM.w - 80, 80),
                "You can add this location again from Contents.",
                BODY,
            );
            canvas.button(CANCEL, "Cancel", false, false);
            canvas.button(DELETE, "Remove", true, false);
        }
    }

    pub(super) fn chapter_editor_input(&mut self, input: Input) -> Change {
        let Some(draft) = &mut self.chapter_draft else {
            return Change::None;
        };
        if draft.confirm_remove {
            if matches!(input, Input::Tap(x,y) if DELETE.contains(x,y)) {
                return match self.finish_custom(true) {
                    Ok(()) => Change::Partial,
                    Err(e) => self.report(e),
                };
            }
            if matches!(input, Input::PageBack)
                || matches!(input, Input::Tap(x,y) if CANCEL.contains(x,y) || !CONFIRM.contains(x,y))
            {
                draft.confirm_remove = false;
            }
            return Change::Partial;
        }
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            let back = draft.back;
            self.show(back);
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if draft.editing && REMOVE.contains(x, y) {
            draft.confirm_remove = true;
            return Change::Partial;
        }
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Save", x, y) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go)
            && let Err(e) = self.finish_custom(false)
        {
            return self.report(e);
        }
        Change::Partial
    }
}
