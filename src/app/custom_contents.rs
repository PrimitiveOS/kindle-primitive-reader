//! Build and navigate a book-specific alternative to the publisher's contents.
use super::*;
use crate::library::custom_contents::{CustomContents, TITLE_LIMIT};

const REMOVE: Rect = Rect::new(LIST_X, LIST_Y + 234, 350, 104);
const CONFIRM: Rect = Rect::new(192, 545, 880, 392);
const CANCEL: Rect = Rect::new(232, 779, 352, 110);
const DELETE: Rect = Rect::new(640, 779, 388, 110);

fn fixed_page_position(page: usize) -> ReadingPosition {
    ReadingPosition {
        chapter: 0,
        page,
        anchor: None,
    }
}

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
    overview: Option<PageOverview>,
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

    pub(super) fn edit_fixed_thumbnail_chapter(&mut self, page: usize) -> Result<()> {
        if self
            .fixed
            .as_ref()
            .is_none_or(|book| page >= book.reading.count())
        {
            return Err("That page is unavailable".into());
        }
        self.edit_custom(
            fixed_page_position(page),
            String::new(),
            Screen::PageOverview,
        )
    }

    pub(super) fn has_fixed_thumbnail_chapter(&self, page: usize) -> bool {
        self.chapter_state().is_some_and(|state| {
            state
                .custom_contents
                .find(&fixed_page_position(page))
                .is_some()
        })
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
            if chapters.enabled && self.fixed.is_some() {
                items.push(
                    Item::new("Page browser", Action::Overview)
                        .detail("Hold a page to add or edit a chapter"),
                );
            }
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
                fixed_page_position(selection.page()),
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
        let overview = if back == Screen::PageOverview {
            self.overview.take()
        } else {
            None
        };
        self.show(Screen::CustomTitle);
        self.chapter_draft = Some(Draft {
            position,
            back,
            editing,
            confirm_remove: false,
            overview,
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
        self.return_from_chapter_editor(screen);
        Ok(())
    }

    fn return_from_chapter_editor(&mut self, screen: Screen) {
        let overview = self
            .chapter_draft
            .as_mut()
            .and_then(|draft| draft.overview.take());
        self.show(screen);
        if let Some(overview) = overview {
            self.overview = Some(overview);
        }
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
        let page = draft
            .overview
            .as_ref()
            .map(|_| format!("Starts on page {}", draft.position.page + 1));
        if let Some(notice) = self.notice.as_deref().or(page.as_deref()) {
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
            self.return_from_chapter_editor(back);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::overview_bookmarks::{PageAction, tap_page_action};
    use crate::app::tests::{Fixture, capture_test_screen};

    fn open_document(fixture: &Fixture) -> (Reader, PathBuf, Vec<u8>) {
        let path = fixture.root.join("Survey sheets.pdf");
        let source = crate::fixed_session::tests::striped_pdf_bytes(8);
        fs::write(&path, &source).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reader.open_path(&path).unwrap();
        (reader, path, source)
    }

    fn tap(rect: Rect) -> Input {
        let (x, y) = center(rect);
        Input::Tap(x, y)
    }

    fn center(rect: Rect) -> (f32, f32) {
        ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn hold_thumbnail(reader: &mut Reader, tile: usize) {
        let (x, y) = center(overview_tile(tile));
        assert_eq!(reader.input(Input::LongTap(x, y)), Change::Partial);
    }

    fn enter_title(reader: &mut Reader, text: &str) {
        assert_eq!(reader.screen, Screen::CustomTitle);
        reader.editor.text = text.chars().collect();
        reader.editor.caret = reader.editor.text.len();
        let (_, go) = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(reader.editor.layer, "Save")
            .into_iter()
            .find(|(key, _)| key.key == primitive_ui::Key::Go)
            .unwrap();
        assert_eq!(reader.input(tap(go)), Change::Partial);
    }

    #[test]
    fn fixed_thumbnail_chapter_create_edit_cancel_remove_and_reopen() {
        let fixture = Fixture::new();
        let (mut reader, path, source) = open_document(&fixture);
        let position = reader.current_position().unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        reader.custom_action(CustomAction::Toggle).unwrap();
        reader.show(Screen::CustomContents);
        let page_browser = reader
            .custom_items()
            .into_iter()
            .find(|item| item.title == "Page browser")
            .unwrap();
        reader.activate(page_browser.action).unwrap();
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert_eq!(reader.overview.as_ref().unwrap().first, 4);
        let font_paths: Vec<_> = reader
            .fonts
            .iter()
            .map(|font| font.to_string_lossy().into_owned())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let preview = primitive_app::frame(&reader, &mut canvas, false);
        hold_thumbnail(&mut reader, 1);
        assert!(!reader.has_fixed_thumbnail_chapter(5));
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_FIXED_THUMBNAIL_CHAPTER_ACTIONS",
            false,
        );
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(primitive_app::frame(&reader, &mut canvas, false), preview);
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::Chapter);
        assert_eq!(reader.chapter_draft.as_ref().unwrap().position.page, 5);
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(reader.screen, Screen::PageOverview);
        assert_eq!(primitive_app::frame(&reader, &mut canvas, false), preview);
        assert!(
            reader
                .chapter_state()
                .unwrap()
                .custom_contents
                .entries()
                .is_empty()
        );

        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::Chapter);
        enter_title(&mut reader, "Survey site");
        assert_eq!(reader.screen, Screen::PageOverview);
        assert_eq!(primitive_app::frame(&reader, &mut canvas, false), preview);
        hold_thumbnail(&mut reader, 1);
        assert!(reader.has_fixed_thumbnail_chapter(5));
        tap_page_action(&mut reader, PageAction::Chapter);
        assert_eq!(reader.editor.string(), "Survey site");
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_FIXED_THUMBNAIL_CHAPTER_EDIT",
            false,
        );
        enter_title(&mut reader, "Survey site revised");
        assert_eq!(reader.current_position(), Some(position.clone()));
        assert!(reader.page_is_bookmarked());
        assert!(reader.back.is_empty());
        assert_eq!(reader.overview.as_ref().unwrap().first, 4);
        reader.save().unwrap();

        let mut reopened =
            Reader::open(&fixture.root, &fixture.state, reader.fonts.clone()).unwrap();
        reopened.open_path(&path).unwrap();
        assert_eq!(reopened.current_position(), Some(position.clone()));
        assert!(reopened.page_is_bookmarked());
        assert!(reopened.custom_contents_enabled());
        reopened.show(Screen::FixedContents);
        let entry = reopened
            .items()
            .into_iter()
            .find(|item| item.title == "Survey site revised")
            .unwrap();
        reopened.activate(entry.action).unwrap();
        assert_eq!(reopened.current_position().unwrap().page, 5);
        reopened.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(reopened.current_position(), Some(position.clone()));
        reopened.activate(Action::Overview).unwrap();
        assert_eq!(reopened.input(Input::PageForward), Change::Partial);
        hold_thumbnail(&mut reopened, 1);
        tap_page_action(&mut reopened, PageAction::Chapter);
        assert_eq!(reopened.input(tap(REMOVE)), Change::Partial);
        assert_eq!(reopened.input(tap(CANCEL)), Change::Partial);
        assert_eq!(reopened.screen, Screen::CustomTitle);
        assert_eq!(
            reopened
                .chapter_state()
                .unwrap()
                .custom_contents
                .entries()
                .len(),
            1
        );
        assert_eq!(reopened.input(tap(REMOVE)), Change::Partial);
        assert_eq!(reopened.input(tap(DELETE)), Change::Partial);
        assert_eq!(reopened.screen, Screen::PageOverview);
        assert_eq!(reopened.overview.as_ref().unwrap().first, 4);
        assert!(
            reopened
                .chapter_state()
                .unwrap()
                .custom_contents
                .entries()
                .is_empty()
        );
        assert_eq!(reopened.current_position(), Some(position));
        assert!(reopened.page_is_bookmarked());
        assert_eq!(fs::read(&path).unwrap(), source);
    }

    #[test]
    fn fixed_thumbnail_chapter_preserves_access_to_hidden_flow_editing() {
        use super::super::hidden_flows::{FlowAction, FlowPopup};
        use primitive_ui::{Sheet, SheetAction};

        let fixture = Fixture::new();
        let (mut reader, _, _) = open_document(&fixture);
        let origin = reader.current_position();
        reader.custom_action(CustomAction::Toggle).unwrap();
        reader.activate(Action::Flow(FlowAction::Toggle)).unwrap();
        reader.activate(Action::Overview).unwrap();
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::HiddenFlow);
        assert!(matches!(reader.flow_popup, Some(FlowPopup::Mark { .. })));
        let sheet = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "");
        assert_eq!(
            reader.input(tap(sheet.button(SheetAction::Confirm))),
            Change::Partial
        );
        assert_eq!(reader.screen, Screen::PageOverview);
        assert_eq!(
            reader.chapter_state().unwrap().hidden_flows.points()[0]
                .position
                .page,
            1
        );
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::Cancel);
        assert!(reader.flow_popup.is_none());
        assert!(reader.chapter_state().unwrap().bookmarks.is_empty());
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::Bookmark);
        assert_eq!(
            reader.chapter_state().unwrap().bookmarks[0].position.page,
            1
        );
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::Chapter);
        enter_title(&mut reader, "Supplement");
        assert_eq!(reader.current_position(), origin);
        reader.show(Screen::FixedContents);
        let supplement = reader
            .items()
            .into_iter()
            .find(|item| item.title == "Supplement")
            .unwrap();
        reader.activate(supplement.action).unwrap();
        assert_eq!(reader.current_position().unwrap().page, 1);
        assert!(reader.page_is_bookmarked());
        assert!(
            reader
                .fixed
                .as_ref()
                .unwrap()
                .reading
                .hidden_flow_map()
                .unwrap()
                .flow(1)
                > 0
        );
        reader.custom_action(CustomAction::Toggle).unwrap();
        reader.activate(Action::Overview).unwrap();
        hold_thumbnail(&mut reader, 1);
        tap_page_action(&mut reader, PageAction::HiddenFlow);
        assert!(matches!(reader.flow_popup, Some(FlowPopup::Mark { .. })));
    }
}
