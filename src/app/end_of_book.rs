//! Document-boundary actions and the confirmation presented over the last page.
use super::*;
use crate::reading_controls::EndAction;
use primitive_ui::{BLACK, Icon, SECONDARY, Sheet, SheetAction, TextStyle};

const VIEW: Rect = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
const PANEL: Rect = Rect::new(
    (WIDTH as i32 - 864) / 2,
    (HEIGHT as i32 - 632) / 2,
    864,
    632,
);
const DISMISS: Rect = Rect::new(PANEL.x + 220, PANEL.y + PANEL.h - 128, PANEL.w - 440, 96);
const CHECK: Icon = Icon {
    stroke: &["M5 12 L10 17 L19 7"],
    fill: &[],
};
const DELETE: Icon = Icon {
    stroke: &["M4 6 H20 M9 6 V3 H15 V6 M6 6 L7 21 H17 L18 6 M10 10 V17 M14 10 V17"],
    fill: &[],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Dialog {
    Actions,
    Delete,
}

const BUTTONS: [EndAction; 6] = [
    EndAction::Finish,
    EndAction::Status,
    EndAction::Beginning,
    EndAction::Next,
    EndAction::Delete,
    EndAction::Library,
];

fn button(index: usize) -> Rect {
    Rect::new(
        PANEL.x + 40 + (index % 2) as i32 * 402,
        PANEL.y + 136 + (index / 2) as i32 * 118,
        382,
        100,
    )
}

impl Reader {
    /// Close the optional end prompt through the same Back action used by the reader.
    /// A verifier can then compare saved document content across a process restart.
    pub(crate) fn dismiss_end_prompt(&mut self) -> bool {
        if self.end_dialog.is_none() {
            return false;
        }
        self.input(Input::PageBack);
        true
    }

    pub(super) fn current_document(&self) -> Option<(&LibraryBook, u64)> {
        self.fixed
            .as_ref()
            .map(|book| (&book.entry, book.id))
            .or_else(|| self.active.as_ref().map(|book| (&book.entry, book.id)))
    }

    pub(super) fn end_action_rows(&self) -> Vec<Item> {
        let controls = self.controls;
        let mut rows = vec![
            Item::new(
                "Always mark as finished",
                Action::SetReadingControls(ReadingControls {
                    finish_at_end: !controls.finish_at_end,
                    ..controls
                }),
            )
            .detail(if controls.finish_at_end { "On" } else { "Off" }),
        ];
        rows.extend(EndAction::ALL.into_iter().map(|action| {
            let row = Item::new(
                action.label(),
                Action::SetReadingControls(ReadingControls {
                    end_action: action,
                    ..controls
                }),
            );
            if controls.end_action == action {
                row.detail("Selected")
            } else {
                row
            }
        }));
        rows
    }

    pub(super) fn finish_end_attempt(&mut self, attempted: bool, change: Change) -> Change {
        if !attempted {
            return change;
        }
        self.auto_turn.configure(None, Instant::now());
        if let Some(book) = &mut self.active {
            book.reading.pause_turns();
        }
        if let Some(book) = &mut self.fixed {
            book.reading.pause();
        }
        if let Some(flow) = &mut self.text_flow {
            flow.pause();
        }
        if self.controls.finish_at_end {
            self.mark_at_end(false);
        }
        let action = self.controls.end_action;
        let result = self.apply_end_action(action, false);
        self.update_stats_activity(Instant::now());
        match result {
            Ok(()) if action == EndAction::Stay => change,
            Ok(()) if change == Change::Full => change,
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }

    fn mark_at_end(&mut self, toggle: bool) {
        let Some((entry, id)) = self.current_document() else {
            return;
        };
        let entry = entry.clone();
        let state = self.library.track(&entry, id);
        if toggle && state.status() == ReadingStatus::Finished {
            state.set_status(ReadingStatus::Reading);
        } else {
            state.set_status(ReadingStatus::Finished);
        }
        self.mark_book_changed();
    }

    fn end_delete_available(&self) -> bool {
        self.current_document()
            .is_some_and(|(entry, _)| entry.can_delete_file())
    }

    fn apply_end_action(&mut self, action: EndAction, from_dialog: bool) -> Result<()> {
        self.end_dialog = None;
        match action {
            EndAction::Ask => self.end_dialog = Some(Dialog::Actions),
            EndAction::Stay => {}
            EndAction::Status | EndAction::StatusThenLibrary => {
                let (entry, _) = self.current_document().ok_or("No open book")?;
                self.organize_book(entry.clone(), self.screen)?;
                if let Some(book) = &mut self.book_details {
                    book.close_document = action == EndAction::StatusThenLibrary;
                }
            }
            EndAction::Beginning if self.fixed.is_some() => {
                let restart = match (&self.text_flow, &self.fixed) {
                    (Some(flow), Some(fixed)) => Some(flow.from_start(&fixed.reading)?),
                    _ => None,
                };
                let previous = self.jump_fixed(0, None)?;
                if self.current_history_location().as_ref() != Some(&previous) {
                    self.push_history(previous);
                }
                if let Some(flow) = restart {
                    self.show(Screen::ReadFixedText);
                    self.text_flow = Some(flow);
                }
            }
            EndAction::Beginning => self.follow_reflow_page(0, 0)?,
            EndAction::Library => self.leave_finished_book()?,
            EndAction::Next => {
                let (entry, _) = self.current_document().ok_or("No open book")?;
                let next = with_storage(|| {
                    self.library
                        .next_book_in_folder(entry, self.controls.folder_order)
                })?
                .ok_or("There is no next readable file in this folder")?;
                self.save()?;
                self.open_book(next)?;
            }
            EndAction::Finish => {
                self.mark_at_end(from_dialog);
                self.notice = Some(if self.book_finished() {
                    "Marked as finished".into()
                } else {
                    "Marked as reading".into()
                });
            }
            EndAction::Delete if self.end_delete_available() => {
                self.end_dialog = Some(Dialog::Delete)
            }
            EndAction::Delete => return Err("This book is a folder. Manage it in Files.".into()),
        }
        Ok(())
    }

    pub(super) fn leave_finished_book(&mut self) -> Result<()> {
        let path = self
            .current_document()
            .ok_or("No open book")?
            .0
            .library_path()
            .to_owned();
        self.activate(Action::Show(Screen::Library))?;
        self.browse_from_book(&path)?;
        Ok(())
    }

    fn book_finished(&self) -> bool {
        self.open_book_id()
            .and_then(|id| self.library.state(id))
            .is_some_and(|state| state.status() == ReadingStatus::Finished)
    }

    fn delete_finished_book(&mut self) -> Result<()> {
        let (entry, id) = self.current_document().ok_or("No open book")?;
        let entry = entry.clone();
        self.save()?;
        with_storage(|| self.library.delete_book_file(&entry, id))?;
        self.mark_dirty();
        self.leave_finished_book()
    }

    fn delete_sheet<'a>(name: &'a str) -> Sheet<'a> {
        Sheet::centered(VIEW, "Delete this file?", name, "Delete file")
    }

    pub(super) fn draw_end_dialog(&self, canvas: &mut Canvas) {
        let Some(dialog) = self.end_dialog else {
            return;
        };
        if dialog == Dialog::Delete {
            let name = self
                .current_document()
                .and_then(|(entry, _)| entry.path.file_name())
                .map_or_else(|| "File".into(), |name| name.to_string_lossy());
            let detail = format!("{name}\nReader keeps your saved notes and marks.");
            Self::delete_sheet(&detail).draw(canvas, None);
            return;
        }
        canvas.popup(PANEL);
        canvas.text(
            Rect::new(PANEL.x + 40, PANEL.y + 38, PANEL.w - 80, 70),
            "End of document",
            TextStyle::new(50).bold().center(),
        );
        for (index, action) in BUTTONS.into_iter().enumerate() {
            let rect = button(index);
            let enabled = action != EndAction::Delete || self.end_delete_available();
            let pressed = enabled && canvas.control_pressed(rect, false);
            let ink = if !enabled {
                SECONDARY
            } else if pressed {
                255
            } else {
                BLACK
            };
            let (icon, label) = match action {
                EndAction::Finish => (
                    CHECK,
                    if self.book_finished() {
                        "Mark as reading"
                    } else {
                        "Mark as finished"
                    },
                ),
                EndAction::Status => (
                    bar_icons::action_icon(&Action::OrganizeCurrent),
                    "Book status",
                ),
                EndAction::Beginning => (bar_icons::FIRST_PAGE, "Go to beginning"),
                EndAction::Next => (bar_icons::NEXT_PAGE, "Open next file"),
                EndAction::Delete => (DELETE, "Delete file"),
                EndAction::Library => (
                    bar_icons::action_icon(&Action::Show(Screen::Library)),
                    "Library",
                ),
                _ => unreachable!(),
            };
            canvas.panel(rect, 16, 2, if pressed { BLACK } else { 255 });
            canvas.icon(Rect::new(rect.x + 20, rect.y + 28, 44, 44), icon, ink);
            canvas.text(
                Rect::new(rect.x + 78, rect.y + 25, rect.w - 92, 56),
                label,
                BODY.ink(ink),
            );
        }
        canvas.button(DISMISS, "Close", false, false);
    }

    pub(super) fn end_dialog_input(&mut self, input: Input) -> Change {
        let Some(dialog) = self.end_dialog else {
            return Change::None;
        };
        let result = match input {
            Input::PageBack => {
                self.end_dialog = None;
                Ok(())
            }
            Input::Tap(x, y) if self.touch_locked && UNLOCK_TOUCH.contains(x, y) => {
                self.touch_locked = false;
                Ok(())
            }
            Input::Tap(x, y) if self.touch_locked && DONE.contains(x, y) => {
                return self.exit();
            }
            Input::Tap(x, y) if !self.touch_locked => match dialog {
                Dialog::Delete => match Self::delete_sheet("").hit(x, y) {
                    Some(SheetAction::Cancel) => {
                        self.end_dialog = None;
                        Ok(())
                    }
                    Some(SheetAction::Confirm) => {
                        self.end_dialog = None;
                        self.delete_finished_book()
                    }
                    None => return Change::None,
                },
                Dialog::Actions if !PANEL.contains(x, y) || DISMISS.contains(x, y) => {
                    self.end_dialog = None;
                    Ok(())
                }
                Dialog::Actions => {
                    let action = BUTTONS
                        .into_iter()
                        .enumerate()
                        .find_map(|(index, action)| button(index).contains(x, y).then_some(action));
                    match action {
                        Some(EndAction::Delete) if !self.end_delete_available() => {
                            return Change::None;
                        }
                        Some(action) => self.apply_end_action(action, true),
                        None => return Change::None,
                    }
                }
            },
            _ => return Change::None,
        };
        self.update_stats_activity(Instant::now());
        match result {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn press_keyboard(app: &mut Reader, key: primitive_ui::Key) {
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let rect = layout
            .keyboard_keys_with(app.editor.layer, "Set")
            .into_iter()
            .find(|(entry, _)| entry.key == key)
            .unwrap()
            .1;
        assert_ne!(app.input(tap(rect)), Change::None);
    }

    fn app(fixture: &Fixture) -> Reader {
        Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap()
    }

    fn page_input(app: &mut Reader, input: Input) {
        app.input(input);
        let due = if app.screen == Screen::ReadFixedText {
            app.text_flow.as_ref().and_then(TextFlow::deadline)
        } else if let Some(active) = &app.active {
            active.reading.deadline()
        } else {
            app.fixed
                .as_ref()
                .and_then(|fixed| fixed.reading.deadline())
        };
        if let Some(due) = due {
            app.tick(due);
        }
    }

    fn chapter_end(app: &mut Reader) {
        let active = app.active.as_ref().unwrap();
        let chapter = active.reading.probe.chapter_count() - 1;
        let last = active.reading.config.chapter(chapter).unwrap().page_count() - 1;
        app.follow_reflow_page(chapter, last).unwrap();
    }

    #[test]
    fn resume_checker_compares_document_pixels_after_closing_the_end_prompt() {
        let fixture = Fixture::new();
        let jpeg = fixture.root.join("Single photograph.jpg");
        image::GrayImage::from_pixel(80, 120, image::Luma([96]))
            .save(&jpeg)
            .unwrap();
        let xhtml = fixture.root.join("Short report.xhtml");
        std::fs::write(&xhtml,
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Short report</title></head><body><p>A single page with a reading position.</p></body></html>")
            .unwrap();
        for (number, path) in [jpeg, xhtml].into_iter().enumerate() {
            let mut app = app(&fixture);
            app.open_path(&path).unwrap();
            let paths = app
                .fonts
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>();
            let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
            let document = primitive_app::frame(&app, &mut canvas, false);
            page_input(&mut app, Input::PageForward);
            assert_eq!(app.end_dialog, Some(Dialog::Actions));
            assert_ne!(primitive_app::frame(&app, &mut canvas, false), document);
            assert!(app.dismiss_end_prompt());
            assert_eq!(app.end_dialog, None);
            assert_eq!(primitive_app::frame(&app, &mut canvas, false), document);
            assert!(!app.dismiss_end_prompt());
            crate::document_check::verify(
                &fixture.root,
                &path,
                &fixture.root.join(format!("resume-{number}")),
                app.fonts.clone(),
                false,
            )
            .unwrap();
        }
    }

    #[test]
    fn paged_epub_asks_after_the_last_page_and_restarts_with_a_return_location() {
        let fixture = Fixture::new();
        let mut app = app(&fixture);
        app.open_path(&fixture.book).unwrap();
        page_input(&mut app, Input::PageBack);
        assert_eq!(app.end_dialog, None);
        chapter_end(&mut app);
        let end = app.current_position().unwrap();
        assert_eq!(app.end_dialog, None);
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.end_dialog, Some(Dialog::Actions));
        capture_test_screen(&app, &app.fonts, "READER_TEST_END_SCREEN", false);
        app.input(Input::PageForward);
        assert_eq!(app.end_dialog, Some(Dialog::Actions));
        app.input(Input::Tap(12.0, 20.0));
        assert_eq!(app.end_dialog, None);
        assert_eq!(app.current_position(), Some(end.clone()));

        page_input(&mut app, Input::PageForward);
        app.input(tap(button(0)));
        assert!(app.book_finished());
        page_input(&mut app, Input::PageForward);
        app.input(tap(button(0)));
        assert!(!app.book_finished());

        page_input(&mut app, Input::PageForward);
        app.input(tap(button(2)));
        assert_eq!(app.current_position().unwrap().chapter, 0);
        assert_eq!(app.current_position().unwrap().page, 0);
        app.activate(Action::ReturnToLocation).unwrap();
        assert_eq!(app.current_position(), Some(end));
    }

    #[test]
    fn end_actions_use_natural_folder_order_and_persist_the_selected_status() {
        let fixture = Fixture::new();
        let dir = fixture.root.join("Series");
        fs::create_dir(&dir).unwrap();
        let first = dir.join("Volume 2.txt");
        let next = dir.join("Volume 10.txt");
        fs::write(&first, "A day on the coast").unwrap();
        fs::write(dir.join("Volume 3.bin"), "Skip this unsupported file").unwrap();
        fs::write(&next, "A night on the coast").unwrap();
        let mut app = app(&fixture);
        app.open_path(&first).unwrap();
        let first_id = app.open_book_id().unwrap();
        app.activate(Action::Show(Screen::ReadingControls)).unwrap();
        let actions = app
            .items()
            .into_iter()
            .find(|item| matches!(item.action, Action::Show(Screen::EndActionSettings)))
            .unwrap();
        app.activate(actions.action).unwrap();
        let item = app
            .items()
            .into_iter()
            .find(|item| item.title == "Always mark as finished")
            .unwrap();
        app.activate(item.action).unwrap();
        let item = app
            .items()
            .into_iter()
            .find(|item| item.title == EndAction::Next.label())
            .unwrap();
        app.activate(item.action).unwrap();
        app.show(Screen::Book);
        page_input(&mut app, Input::PageForward);
        assert_eq!(
            app.current_document().unwrap().0.path,
            Path::new("Series/Volume 10.txt")
        );
        assert_eq!(
            app.library.state(first_id).unwrap().status(),
            ReadingStatus::Finished
        );
        page_input(&mut app, Input::PageForward);
        assert!(app.notice.as_deref().unwrap().contains("no next"));
        app.save().unwrap();
        let recovered = Reader::open(&fixture.root, &fixture.state, app.fonts.clone()).unwrap();
        assert_eq!(recovered.controls.end_action, EndAction::Next);
        assert!(recovered.controls.finish_at_end);
        assert_eq!(
            recovered.library.state(first_id).unwrap().status(),
            ReadingStatus::Finished
        );

        app.controls.end_action = EndAction::StatusThenLibrary;
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.screen, Screen::BookDetails);
        app.input(tap(primitive_ui::back_button(WIDTH as i32)));
        assert_eq!(app.screen, Screen::Library);
        assert!(app.open_book_id().is_none());
        assert!(
            app.library_items()
                .any(|item| item.title.contains("Volume 10"))
        );
    }

    #[test]
    fn folder_order_controls_only_the_existing_next_file_end_action() {
        use crate::library::{FolderOrder, FolderSort};

        let fixture = Fixture::new();
        let dir = fixture.root.join("Series");
        fs::create_dir(&dir).unwrap();
        let first = dir.join("Volume 10.txt");
        let next = dir.join("Volume 2.txt");
        fs::write(&first, "Port lights").unwrap();
        fs::write(&next, "A calm sea").unwrap();
        let mut app = app(&fixture);
        app.activate(Action::SetFolderOrder(FolderOrder {
            sort: FolderSort::FileName,
            reverse: false,
        }))
        .unwrap();
        app.open_path(&next).unwrap();
        let next_id = app.open_book_id().unwrap();
        app.library
            .state_mut(next_id)
            .unwrap()
            .set_status(ReadingStatus::Finished);
        app.open_path(&first).unwrap();
        let first_position = app.current_position();
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.current_position(), first_position);
        assert_eq!(app.end_dialog, Some(Dialog::Actions));
        assert_eq!(app.input(tap(button(3))), Change::Partial);
        assert_eq!(app.open_book_id(), Some(next_id));
        assert_eq!(
            app.library.state(next_id).unwrap().status(),
            ReadingStatus::Finished
        );
    }

    #[test]
    fn fixed_spreads_ask_after_the_last_visible_page_only() {
        let fixture = Fixture::new();
        let comic = fixture.root.join("Harbor signals.cbz");
        fs::copy(crate::test_fixture("harbor-signals.cbz"), &comic).unwrap();
        let mut app = app(&fixture);
        app.open_path(&comic).unwrap();
        app.activate(Action::ComicSpread(true)).unwrap();
        app.controls.end_action = EndAction::Stay;
        let count = app.fixed.as_ref().unwrap().reading.count();
        app.follow_fixed(count - 1, None).unwrap();
        assert!(
            app.fixed
                .as_ref()
                .unwrap()
                .reading
                .visible_pages()
                .any(|page| page + 1 == count)
        );
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.end_dialog, None);
        app.controls.end_action = EndAction::Ask;
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.end_dialog, Some(Dialog::Actions));
        app.input(Input::PageBack);
        assert_eq!(app.end_dialog, None);
    }

    #[test]
    fn status_finish_and_library_defaults_handle_the_last_page() {
        let fixture = Fixture::new();
        let mut app = app(&fixture);
        app.open_path(&fixture.book).unwrap();
        chapter_end(&mut app);
        let end = app.current_position().unwrap();
        app.controls.end_action = EndAction::Status;
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.screen, Screen::BookDetails);
        app.input(tap(primitive_ui::back_button(WIDTH as i32)));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_position(), Some(end.clone()));

        app.controls.end_action = EndAction::Finish;
        app.activate(Action::AutoTurn(Some(Duration::from_secs(3))))
            .unwrap();
        let deadline = app.auto_turn.deadline().unwrap();
        let mut change = app.tick(deadline);
        if change == Change::None {
            let due = app.active.as_ref().unwrap().reading.deadline().unwrap();
            change = app.tick(due);
        }
        assert_eq!(change, Change::Partial);
        assert!(app.auto_turn.interval().is_none());
        assert!(app.book_finished());
        assert_eq!(app.current_position(), Some(end.clone()));

        app.controls.end_action = EndAction::Library;
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.screen, Screen::Library);
        assert!(app.open_book_id().is_none());
        app.activate(Action::Continue).unwrap();
        assert_eq!(app.current_position(), Some(end));
        assert!(app.book_finished());
    }

    #[test]
    fn custom_auto_turn_interval_is_validated_saved_and_restored_on_reopen() {
        let fixture = Fixture::new();
        let mut app = app(&fixture);
        app.open_path(&fixture.book).unwrap();
        app.show(Screen::AutoTurn);
        let custom = app
            .items()
            .into_iter()
            .find(|item| item.title == "Custom interval")
            .unwrap();
        assert!(matches!(
            custom.action,
            Action::Show(Screen::NumberingValue(NumberField::AutoTurn))
        ));
        capture_test_screen(&app, &app.fonts, "READER_AUTO_TURN_SCREENSHOT", false);

        app.activate(custom.action).unwrap();
        assert_eq!(app.screen, Screen::NumberingValue(NumberField::AutoTurn));
        assert_eq!(app.editor.string(), "30");
        for invalid in ["0", "86401", "abc"] {
            app.editor.text = invalid.chars().collect();
            app.editor.caret = app.editor.text.len();
            assert!(
                app.apply_numbering_input(NumberField::AutoTurn).is_err(),
                "{invalid}"
            );
        }

        for _ in 0..app.editor.string().chars().count() {
            press_keyboard(&mut app, primitive_ui::Key::Delete);
        }
        for character in "86400".chars() {
            press_keyboard(&mut app, primitive_ui::Key::Char(character));
        }
        press_keyboard(&mut app, primitive_ui::Key::Go);
        assert_eq!(app.screen, Screen::AutoTurn);
        assert_eq!(app.controls.auto_turn_seconds, 86_400);
        assert_eq!(app.auto_turn.interval(), Some(Duration::from_secs(86_400)));
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).auto_turn_seconds,
            86_400
        );
        assert!(app.items().iter().any(|item| {
            item.title == "Custom interval" && item.detail.as_deref() == Some("86400 seconds")
        }));

        app.activate(Action::AutoTurn(None)).unwrap();
        assert_eq!(app.auto_turn.interval(), None);
        assert_eq!(app.controls.auto_turn_seconds, 86_400);
        assert!(!app.controls.auto_turn_enabled);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).auto_turn_seconds,
            86_400
        );
        let custom = app
            .items()
            .into_iter()
            .find(|item| item.title == "Custom interval")
            .unwrap();
        app.activate(custom.action).unwrap();
        assert_eq!(app.editor.string(), "86400");
        app.activate(Action::AutoTurn(Some(Duration::from_secs(86_400))))
            .unwrap();
        assert_eq!(app.auto_turn.interval(), Some(Duration::from_secs(86_400)));

        app.show(Screen::Book);
        let due = app.auto_turn.deadline().unwrap();
        assert!(due >= Instant::now() + Duration::from_secs(74));
        let before_sleep = app.current_position();
        app.input(Input::Sleep);
        assert_eq!(app.auto_turn.deadline(), None);
        app.tick(due + Duration::from_secs(1));
        assert_eq!(app.current_position(), before_sleep);
        app.input(Input::Wake);
        assert!(app.auto_turn.deadline().unwrap() > Instant::now());

        app.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(app.auto_turn.interval(), None);
        app.activate(Action::Continue).unwrap();
        assert_eq!(app.auto_turn.interval(), Some(Duration::from_secs(86_400)));
        assert!(app.auto_turn.deadline().is_some());
        drop(app);

        let mut reopened = Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(
            reopened.auto_turn.interval(),
            Some(Duration::from_secs(86_400))
        );
        assert!(reopened.auto_turn.deadline().is_some());
    }

    #[test]
    fn auto_turn_distance_menu_saves_a_signed_adjustment() {
        let fixture = Fixture::new();
        let mut app = app(&fixture);
        app.open_path(&fixture.book).unwrap();
        app.show(Screen::AutoTurn);
        let distance = app
            .items()
            .into_iter()
            .find(|item| item.title == "Scroll distance")
            .unwrap();
        app.activate(distance.action).unwrap();
        assert_eq!(app.screen, Screen::AutoTurnDistance);
        let increase = app
            .items()
            .into_iter()
            .find(|item| item.title == "Increase by 10%")
            .unwrap();
        app.activate(increase.action).unwrap();
        assert_eq!(app.controls.auto_turn_distance_tenths(), 11);
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).auto_turn_distance_tenths(),
            11
        );
        app.input(tap(primitive_ui::back_button(WIDTH as i32)));
        assert_eq!(app.screen, Screen::AutoTurn);
        assert!(app.items().iter().any(|item| {
            item.title == "Scroll distance" && item.detail.as_deref() == Some("+110% of a page")
        }));
    }

    #[test]
    fn fixed_page_auto_turn_pan_settings_are_editable_and_persist_per_book() {
        let fixture = Fixture::new();
        let scan = fixture.root.join("Tilted scan.pdf");
        fs::copy(crate::test_fixture("tilted-scan.pdf"), &scan).unwrap();
        let mut reader = app(&fixture);
        reader.open_path(&scan).unwrap();
        reader.show(Screen::AutoTurn);
        let pan = reader
            .items()
            .into_iter()
            .find(|item| item.title == "Fixed-page pan")
            .unwrap();
        reader.activate(pan.action).unwrap();
        assert_eq!(reader.screen, Screen::AutoTurnPan);
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_FIXED_AUTOTURN_PAN_SCREENSHOT",
            false,
        );

        reader.show(Screen::NumberingValue(
            NumberField::AutoTurnOverlapHorizontal,
        ));
        reader.editor.text = "25".chars().collect();
        reader.editor.caret = reader.editor.text.len();
        reader
            .apply_numbering_input(NumberField::AutoTurnOverlapHorizontal)
            .unwrap();
        reader.show(Screen::NumberingValue(NumberField::AutoTurnOverlapVertical));
        reader.editor.text = "70".chars().collect();
        reader.editor.caret = reader.editor.text.len();
        reader
            .apply_numbering_input(NumberField::AutoTurnOverlapVertical)
            .unwrap();

        let vertical_first = reader
            .items()
            .into_iter()
            .find(|item| item.title == "Pan order")
            .unwrap();
        assert_eq!(
            vertical_first.detail.as_deref(),
            Some("Horizontal first · vertical at the edge · tap to change")
        );
        reader.activate(vertical_first.action).unwrap();
        let bottom_to_top = reader
            .items()
            .into_iter()
            .find(|item| item.title == "Vertical direction")
            .unwrap();
        assert_eq!(
            bottom_to_top.detail.as_deref(),
            Some("Top to bottom · tap to reverse")
        );
        reader.activate(bottom_to_top.action).unwrap();
        assert_eq!(
            reader
                .items()
                .into_iter()
                .find(|item| item.title == "Pan order")
                .unwrap()
                .detail
                .as_deref(),
            Some("Vertical first · horizontal at the edge · tap to change")
        );
        let preferences = reader.fixed.as_ref().unwrap().reading.preferences();
        assert_eq!(preferences.zoom_overlap_h(), 25);
        assert_eq!(preferences.zoom_overlap_v(), 70);
        assert!(preferences.zoom_direction_vertical());
        assert!(preferences.zoom_bottom_to_top());

        reader
            .activate(Action::DocumentDefaults(
                super::super::default_settings::Command::SaveFixed,
            ))
            .unwrap();
        let defaults = reader.document_defaults.templates.fixed.unwrap();
        let defaults = defaults.apply(crate::library::FixedPreferences::default());
        assert_eq!(defaults.zoom_overlap_h(), 25);
        assert_eq!(defaults.zoom_overlap_v(), 70);
        assert!(defaults.zoom_direction_vertical());
        assert!(defaults.zoom_bottom_to_top());
        reader.save().unwrap();
        drop(reader);

        let mut reopened = app(&fixture);
        reopened.open_path(&scan).unwrap();
        let preferences = reopened.fixed.as_ref().unwrap().reading.preferences();
        assert_eq!(preferences.zoom_overlap_h(), 25);
        assert_eq!(preferences.zoom_overlap_v(), 70);
        assert!(preferences.zoom_direction_vertical());
        assert!(preferences.zoom_bottom_to_top());
    }

    #[test]
    fn deletion_requires_confirmation_and_retains_marks_for_the_restored_file() {
        let fixture = Fixture::new();
        let mut app = app(&fixture);
        app.open_path(&fixture.book).unwrap();
        let id = app.open_book_id().unwrap();
        app.activate(Action::AddBookmark).unwrap();
        chapter_end(&mut app);
        page_input(&mut app, Input::PageForward);
        app.input(tap(button(4)));
        assert_eq!(app.end_dialog, Some(Dialog::Delete));
        assert!(fixture.book.exists());
        capture_test_screen(&app, &app.fonts, "READER_TEST_END_DELETE_SCREEN", false);
        let sheet = Reader::delete_sheet("");
        app.input(tap(sheet.button(SheetAction::Cancel)));
        assert!(fixture.book.exists());
        assert_eq!(app.end_dialog, None);

        app.controls.end_action = EndAction::Delete;
        page_input(&mut app, Input::PageForward);
        app.input(tap(sheet.button(SheetAction::Confirm)));
        assert!(!fixture.book.exists());
        assert_eq!(app.screen, Screen::Library);
        assert!(app.library.recently_read().all(|book| book.id != id));
        assert_eq!(app.library.state(id).unwrap().bookmarks.len(), 1);
        let mut reopened = Reader::open(&fixture.root, &fixture.state, app.fonts.clone()).unwrap();
        assert!(reopened.library.recently_read().all(|book| book.id != id));
        fs::copy(crate::test_fixture("field-notes-epub3.epub"), &fixture.book).unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.open_book_id(), Some(id));
        assert_eq!(reopened.library.state(id).unwrap().bookmarks.len(), 1);
        assert!(reopened.library.recently_read().any(|book| book.id == id));
    }

    #[test]
    fn deletion_declines_to_remove_a_replaced_document() {
        let fixture = Fixture::new();
        let path = fixture.root.join("Short story.txt");
        fs::write(&path, "Sunset").unwrap();
        let mut app = app(&fixture);
        app.open_path(&path).unwrap();
        app.controls.end_action = EndAction::Delete;
        page_input(&mut app, Input::PageForward);
        assert_eq!(app.end_dialog, Some(Dialog::Delete));
        fs::write(&path, "Dinner").unwrap();
        let sheet = Reader::delete_sheet("");
        app.input(tap(sheet.button(SheetAction::Confirm)));
        assert_eq!(fs::read_to_string(path).unwrap(), "Dinner");
        assert!(app.notice.as_deref().unwrap().contains("changed"));
        assert_eq!(app.screen, Screen::Book);
    }

    #[test]
    #[ignore = "requires the privately selected original Kindle documents"]
    fn original_documents_reach_the_boundary_in_paged_and_continuous_reading() {
        let root = PathBuf::from(std::env::var_os("READER_TEST_BOUNDARY_DOCUMENTS").unwrap());
        let list = PathBuf::from(std::env::var_os("READER_TEST_BOUNDARY_LIST").unwrap());
        let selection = fs::read_to_string(list).unwrap();
        let state = Fixture::new();
        let mut app = Reader::open(
            &root,
            &state.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap();
        let (mut text_books, mut pdfs) = (0, 0);
        for (index, path) in selection
            .lines()
            .filter(|path| !path.is_empty())
            .enumerate()
        {
            let path = Path::new(path);
            app.open_path(path)
                .unwrap_or_else(|error| panic!("Case {} failed to open: {error}", index + 1));
            app.controls.end_action = EndAction::Ask;
            app.controls.finish_at_end = false;
            if app.active.is_some() {
                text_books += 1;
                chapter_end(&mut app);
            } else if let Some(fixed) = &app.fixed {
                assert_eq!(fixed.entry.format, DocumentFormat::Pdf);
                pdfs += 1;
                app.follow_fixed(fixed.reading.count() - 1, None).unwrap();
            } else {
                panic!("Case {} has no document provider", index + 1);
            }
            let end = app.current_position().unwrap();
            assert_eq!(app.end_dialog, None);
            page_input(&mut app, Input::PageForward);
            assert_eq!(app.end_dialog, Some(Dialog::Actions));
            assert_eq!(app.current_position(), Some(end));
            app.input(Input::PageBack);
            assert_eq!(app.end_dialog, None);

            if app.active.is_some() {
                app.activate(Action::ReadingMode(true)).unwrap();
            } else {
                app.activate(Action::FixedScroll(true)).unwrap();
            }
            for _ in 0..32 {
                let before = app.current_position().unwrap();
                page_input(&mut app, Input::PageForward);
                if app.end_dialog == Some(Dialog::Actions) {
                    assert_eq!(app.current_position(), Some(before));
                    break;
                }
            }
            assert_eq!(
                app.end_dialog,
                Some(Dialog::Actions),
                "Case {} never reached its boundary",
                index + 1
            );
            // There is deliberately no destructive action on an original.
            app.input(Input::PageBack);
            app.controls.end_action = EndAction::Beginning;
            page_input(&mut app, Input::PageForward);
            let start = app.current_position().unwrap();
            assert_eq!((start.chapter, start.page), (0, 0));
            app.activate(Action::Show(Screen::Library)).unwrap();
            eprintln!(
                "document end verification case {}: paged and continuous",
                index + 1
            );
        }
        assert!(text_books >= 2 && pdfs >= 2);
    }
}
