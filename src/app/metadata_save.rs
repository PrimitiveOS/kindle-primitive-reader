//! Schedule book metadata writes after reading input and never during a page paint.
use super::{Action, Item, Reader, ReadingControls};
use crate::reading_controls::MetadataSave;
use std::time::{Duration, Instant};

const AFTER_CHANGE: Duration = Duration::from_millis(800);
const AFTER_PAGE: Duration = Duration::from_millis(80);

pub(super) struct Checkpoint {
    last_save: Instant,
    due: Option<Instant>,
}

impl Checkpoint {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            last_save: now,
            due: None,
        }
    }

    pub(super) fn record(&mut self, policy: MetadataSave, page_changed: bool, now: Instant) {
        if policy == MetadataSave::AfterChange {
            self.due = Some(now + AFTER_CHANGE);
        } else if page_changed
            && self.due.is_none()
            && policy
                .interval()
                .is_some_and(|interval| now.duration_since(self.last_save) >= interval)
        {
            self.due = Some(now + AFTER_PAGE);
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.due
    }

    pub(super) fn cancel(&mut self) {
        self.due = None;
    }
}

impl Reader {
    pub(super) fn metadata_save_items(&self) -> Vec<Item> {
        let controls = self.controls;
        let language = controls.interface_language;
        MetadataSave::ALL
            .into_iter()
            .map(|metadata_save| {
                let row = Item::new(
                    language.metadata_settings_text(metadata_save.label()),
                    Action::SetReadingControls(ReadingControls {
                        metadata_save,
                        ..controls
                    }),
                );
                if metadata_save == controls.metadata_save {
                    row.detail(language.metadata_settings_text("Selected"))
                } else {
                    row
                }
            })
            .collect()
    }

    pub(super) fn mark_book_changed(&mut self) {
        if let Some(search) = &mut self.page_bookmark_search {
            search.invalidate();
        }
        self.dirty = true;
        self.metadata_checkpoint
            .record(self.controls.metadata_save, false, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        DONE, Screen,
        tests::{Fixture, capture_test_screen, tap_contents_item},
    };
    use crate::library::Library;
    use primitive_app::{App, Change, Lifecycle};
    use primitive_ui::Input;
    use std::path::{Path, PathBuf};

    #[test]
    fn periodic_checkpoints_wait_for_a_page_change_after_the_selected_interval() {
        let start = Instant::now();
        for policy in [
            MetadataSave::FiveMinutes,
            MetadataSave::FifteenMinutes,
            MetadataSave::ThirtyMinutes,
            MetadataSave::OneHour,
        ] {
            let mut checkpoint = Checkpoint::new(start);
            let threshold = start + policy.interval().unwrap();
            checkpoint.record(policy, true, threshold - Duration::from_millis(1));
            assert_eq!(checkpoint.deadline(), None);
            checkpoint.record(policy, false, threshold);
            assert_eq!(checkpoint.deadline(), None);
            checkpoint.record(policy, true, threshold);
            assert_eq!(checkpoint.deadline(), Some(threshold + AFTER_PAGE));
            checkpoint.record(policy, true, threshold + Duration::from_millis(1));
            assert_eq!(checkpoint.deadline(), Some(threshold + AFTER_PAGE));
            checkpoint.cancel();
            assert_eq!(checkpoint.deadline(), None);
        }
        let mut checkpoint = Checkpoint::new(start);
        checkpoint.record(
            MetadataSave::CloseOrSleep,
            true,
            start + Duration::from_secs(3601),
        );
        assert_eq!(checkpoint.deadline(), None);
        checkpoint.record(MetadataSave::AfterChange, true, start);
        let another = start + Duration::from_millis(1);
        checkpoint.record(MetadataSave::AfterChange, false, another);
        assert_eq!(checkpoint.deadline(), Some(another + AFTER_CHANGE));
    }

    fn setup(fixture: &Fixture) -> Reader {
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&fixture.book).unwrap();
        app
    }

    #[test]
    fn close_and_sleep_save_the_position_and_bookmark_with_periodic_saving_disabled() {
        let fixture = Fixture::new();
        let mut app = setup(&fixture);
        assert_eq!(app.controls.metadata_save, MetadataSave::FifteenMinutes);
        app.save().unwrap();
        let id = app.open_book_id().unwrap();

        app.activate(Action::Show(Screen::ReadingControls)).unwrap();
        tap_contents_item(&mut app, "Save reading data");
        assert_eq!(app.screen, Screen::MetadataSaveSettings);
        capture_test_screen(&app, &app.fonts, "READER_TEST_METADATA_SCREEN", false);
        tap_contents_item(&mut app, "On close or sleep");
        assert_eq!(
            ReadingControls::load(&app.controls_path, true).metadata_save,
            MetadataSave::CloseOrSleep
        );
        let reading = &app.active.as_ref().unwrap().reading;
        let start = reading.probe.fragment("tide-note").unwrap();
        let mut end = start.clone();
        end.offset += "The first step".len();
        app.selection = reading.probe.select_range(&start, &end);
        let before_marks = crate::mark_time::now().unwrap();
        app.save_highlight(Some("Check the tide chart".into()))
            .unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        app.activate(Action::AddBookmark).unwrap();

        let mark_window = before_marks..=crate::mark_time::now().unwrap();
        let previous = Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(
            previous
                .state(id)
                .unwrap()
                .position
                .as_ref()
                .unwrap()
                .chapter,
            0
        );
        assert!(previous.state(id).unwrap().bookmarks.is_empty());
        assert!(previous.state(id).unwrap().highlights.is_empty());
        assert_eq!(app.metadata_checkpoint.deadline(), None);

        let mut lifecycle = Lifecycle::default();
        lifecycle.input(&mut app, Input::Sleep, |error| panic!("{error}"));
        let saved = Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(
            saved.state(id).unwrap().position.as_ref().unwrap().chapter,
            1
        );
        assert_eq!(saved.state(id).unwrap().bookmarks.len(), 1);
        assert_eq!(
            saved.state(id).unwrap().highlights[0].note.as_deref(),
            Some("Check the tide chart")
        );
        let marks = saved.state(id).unwrap();
        assert!(mark_window.contains(&marks.bookmarks[0].created_at.unwrap()));
        assert!(mark_window.contains(&marks.highlights[0].created_at.unwrap()));
        lifecycle.input(&mut app, Input::Wake, |error| panic!("{error}"));
        assert_eq!(app.current_position().unwrap().chapter, 1);
        app.follow_reflow_page(0, 0).unwrap();
        assert_eq!(app.metadata_checkpoint.deadline(), None);
        let exit = Input::Tap((DONE.x + DONE.w / 2) as f32, (DONE.y + DONE.h / 2) as f32);
        assert_eq!(
            lifecycle.input(&mut app, exit, |error| panic!("{error}")),
            Change::Exit
        );
        let saved = Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(
            saved.state(id).unwrap().position.as_ref().unwrap().chapter,
            0
        );
        assert_eq!(saved.state(id).unwrap().bookmarks.len(), 1);
    }

    #[test]
    fn switching_documents_commits_dirty_metadata_for_the_previous_book() {
        let fixture = Fixture::new();
        let another = fixture.root.join("Next book.txt");
        std::fs::write(&another, "A different document").unwrap();
        let mut app = setup(&fixture);
        app.controls.metadata_save = MetadataSave::CloseOrSleep;
        let id = app.open_book_id().unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        app.activate(Action::AddBookmark).unwrap();

        app.open_path(&another).unwrap();
        let saved = Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(
            saved.state(id).unwrap().position.as_ref().unwrap().chapter,
            1
        );
        assert_eq!(saved.state(id).unwrap().bookmarks.len(), 1);
        assert!(saved.state(app.open_book_id().unwrap()).is_none());
    }

    #[test]
    fn manual_save_from_tools_keeps_the_document_at_its_current_location() {
        let fixture = Fixture::new();
        let mut app = setup(&fixture);
        app.controls.metadata_save = MetadataSave::CloseOrSleep;
        app.save().unwrap();
        app.follow_reflow_page(1, 0).unwrap();
        app.activate(Action::AddBookmark).unwrap();
        let before = app.current_position().unwrap();
        let id = app.open_book_id().unwrap();
        let old = Library::open(&fixture.root, &fixture.state).unwrap();
        assert!(old.state(id).unwrap().bookmarks.is_empty());
        assert_eq!(app.open_reading_edge(true, 630.0), Change::Partial);
        let tap = |rect: primitive_ui::Rect| {
            Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
        };
        if app.top_group != Some(crate::app::TopGroup::Tools) {
            app.input(tap(app.reading_button("Tools")));
        }
        assert_eq!(
            app.input(tap(app.reading_button("Save reading data"))),
            Change::Partial
        );
        assert_eq!(app.notice.as_deref(), Some("Reading data saved"));
        assert_eq!(app.current_position(), Some(before.clone()));
        let saved = Library::open(&fixture.root, &fixture.state).unwrap();
        assert_eq!(saved.state(id).unwrap().position, Some(before));
        assert_eq!(saved.state(id).unwrap().bookmarks.len(), 1);
    }

    #[test]
    fn existing_reading_settings_keep_their_original_write_cadence() {
        let fixture = Fixture::new();
        let path = fixture.root.join("old-controls.json");
        assert_eq!(
            ReadingControls::load(&path, false).metadata_save,
            MetadataSave::FifteenMinutes
        );
        assert_eq!(
            ReadingControls::load(&path, true).metadata_save,
            MetadataSave::AfterChange
        );
        std::fs::write(&path, br#"{"footnote_popups":true}"#).unwrap();
        let migrated = ReadingControls::load(&path, true);
        assert!(migrated.footnote_popups);
        assert_eq!(migrated.metadata_save, MetadataSave::AfterChange);
        migrated.save(&path).unwrap();
        assert_eq!(ReadingControls::load(&path, true), migrated);
    }

    #[test]
    #[ignore = "requires privately selected original EPUB and PDF documents"]
    fn original_positions_checkpoint_after_reading_input_and_survive_the_lifecycle() {
        let root = PathBuf::from(std::env::var_os("READER_TEST_METADATA_DOCUMENTS").unwrap());
        let manifest = PathBuf::from(std::env::var_os("READER_TEST_METADATA_LIST").unwrap());
        let fixture = Fixture::new();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
        let mut formats = [false; 2];
        for (index, path) in std::fs::read_to_string(manifest)
            .unwrap()
            .lines()
            .enumerate()
        {
            let path = Path::new(path);
            app.open_path(path).unwrap();
            app.save().unwrap();
            let id = app.open_book_id().unwrap();
            let at_open = app.current_position().unwrap();
            formats[usize::from(app.fixed.is_some())] = true;
            let interval = MetadataSave::FiveMinutes.interval().unwrap();
            let older = Instant::now()
                .checked_sub(interval + Duration::from_secs(1))
                .unwrap();
            app.controls.metadata_save = MetadataSave::FiveMinutes;
            app.metadata_checkpoint = Checkpoint::new(older);
            let mut lifecycle = Lifecycle::default();
            lifecycle.input(&mut app, Input::PageForward, |error| panic!("{error}"));
            let input_due = if let Some(active) = &app.active {
                active.reading.deadline()
            } else {
                app.fixed.as_ref().unwrap().reading.deadline()
            };
            if let Some(due) = input_due {
                app.tick(due);
            }
            let after_turn = app.current_position().unwrap();
            assert_ne!(after_turn, at_open);
            let write_due = app.metadata_checkpoint.deadline().unwrap();
            let disk = Library::open(&root, &fixture.state).unwrap();
            assert_eq!(disk.state(id).unwrap().position, Some(at_open));
            app.tick(write_due);
            let disk = Library::open(&root, &fixture.state).unwrap();
            assert_eq!(disk.state(id).unwrap().position, Some(after_turn.clone()));

            app.controls.metadata_save = MetadataSave::CloseOrSleep;
            app.activate(Action::AddBookmark).unwrap();
            assert_eq!(app.metadata_checkpoint.deadline(), None);
            assert!(
                Library::open(&root, &fixture.state)
                    .unwrap()
                    .state(id)
                    .unwrap()
                    .bookmarks
                    .is_empty()
            );
            lifecycle.input(&mut app, Input::Sleep, |error| panic!("{error}"));
            let disk = Library::open(&root, &fixture.state).unwrap();
            assert_eq!(disk.state(id).unwrap().bookmarks.len(), 1);
            lifecycle.input(&mut app, Input::Wake, |error| panic!("{error}"));
            assert_eq!(app.current_position(), Some(after_turn.clone()));

            let mut reopened = Reader::open(&root, &fixture.state, fonts.clone()).unwrap();
            reopened.open_path(path).unwrap();
            assert_eq!(reopened.current_position(), Some(after_turn));
            app.activate(Action::Show(Screen::Library)).unwrap();
            eprintln!(
                "Metadata case {} wrote after a page, flushed on sleep, and restored the position",
                index + 1
            );
        }
        assert_eq!(
            formats,
            [true, true],
            "exercise both reflow and fixed documents"
        );
    }
}
