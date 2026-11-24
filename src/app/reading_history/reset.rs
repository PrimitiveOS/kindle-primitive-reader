use super::*;
use crate::reading_stats::history::reset::SHORT_READING_MINUTES;
use std::cmp::Reverse;

impl Reader {
    pub(super) fn history_reset_return(&self, scope: Scope) -> Page {
        match scope {
            Scope::Book(id) if self.open_book_id() != Some(id) => Page::ResetBooks,
            _ => Page::Resets,
        }
    }

    pub(super) fn history_reset_items(&self, page: Page) -> Vec<Item> {
        let current = self.open_book_id();
        let open = |page| Action::ReadingHistory(Command::Show(page));
        let confirm = |scope| open(Page::ConfirmReset(scope));
        match page {
            Page::Resets => {
                let mut rows = Vec::new();
                if let Some(id) = current {
                    let (action, title) = if self.reading_history.options().enabled {
                        (confirm(Scope::Book(id)), self.history_title(id))
                    } else {
                        (Action::Heading, "Statistics recording is off".into())
                    };
                    rows.push(
                        Item::new("Reset statistics for the current book", action).detail(title),
                    );
                }
                rows.push(Item::new(
                    "Reset statistics per book",
                    open(Page::ResetBooks),
                ));
                rows.extend(SHORT_READING_MINUTES.into_iter().map(|minutes| {
                    Item::new(
                        format!("Books read for less than {minutes} min"),
                        confirm(Scope::ShortBooks {
                            minutes,
                            keep: current,
                        }),
                    )
                }));
                rows
            }
            Page::ResetBooks => {
                let mut books: Vec<_> = self
                    .reading_history
                    .reset_books()
                    .into_iter()
                    .filter(|&(id, _)| Some(id) != current)
                    .map(|(id, stats)| {
                        (
                            Reverse(stats.last_visit),
                            self.history_title(id),
                            id,
                            stats.seconds,
                        )
                    })
                    .collect();
                books.sort_unstable();
                if books.is_empty() {
                    vec![Item::new("No other books have statistics", Action::Heading)]
                } else {
                    books
                        .into_iter()
                        .map(|(_, title, id, seconds)| {
                            Item::new(title, confirm(Scope::Book(id))).detail(short_time(seconds))
                        })
                        .collect()
                }
            }
            Page::ConfirmReset(scope) => {
                let (question, count) = match scope {
                    Scope::Book(id) => (self.history_title(id), 1),
                    Scope::ShortBooks { minutes, .. } => (
                        format!("Books read for less than {minutes} min"),
                        self.reading_history.reset_selection(scope).len(),
                    ),
                };
                let detail = match count {
                    0 => "No eligible books".into(),
                    1 => "Remove recorded statistics for one book".into(),
                    count => format!("Remove recorded statistics for {count} books"),
                };
                let mut items = vec![Item::new(question, Action::Heading).detail(detail)];
                if count > 0 {
                    items.push(Item::new(
                        "Reset statistics",
                        Action::ReadingHistory(Command::ConfirmReset),
                    ));
                }
                items.push(Item::new(
                    "Cancel",
                    Action::ReadingHistory(Command::CancelReset),
                ));
                items
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn confirm_history_reset(&mut self) -> Result<()> {
        let Screen::ReadingHistory(Page::ConfirmReset(scope)) = self.screen else {
            return Ok(());
        };
        let now = Instant::now();
        self.stats.checkpoint(&self.stats_path, now, true)?;
        let removed = self.reading_history.reset(
            scope,
            &self.stats.recorded_totals(),
            now,
            crate::mark_time::now(),
        )?;
        self.pace.discard_books(&removed);
        self.history_back();
        self.notice = Some(match removed.len() {
            0 => "No statistics reset".into(),
            1 => "Statistics reset for 1 book".into(),
            count => format!("Statistics reset for {count} books"),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use primitive_app::App;
    use std::{collections::BTreeMap, fs, time::Duration};

    fn select(app: &mut Reader, label: &str) {
        let action = app
            .items()
            .into_iter()
            .find(|row| row.title == label)
            .unwrap()
            .action;
        app.activate(action).unwrap();
    }

    #[test]
    fn reset_statistics_confirms_scope_and_preserves_reading_state_and_source() {
        let fixture = Fixture::new();
        fs::copy(crate::test_fixture("many-chapters.epub"), &fixture.book).unwrap();
        let source = fs::read(&fixture.book).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let initial_position = app.current_history_location();
        app.activate(Action::Chapter(
            crate::app::chapter_navigation::Direction::Next,
        ))
        .unwrap();
        assert_ne!(app.current_history_location(), initial_position);
        app.activate(Action::AddBookmark).unwrap();
        let book = app.open_book_id().unwrap();
        let mark_count = app.library.state(book).unwrap().bookmarks.len();
        let position = app.current_history_location();
        let book_title = app.history_title(book);
        let other = book.wrapping_add(1);
        let archived = book.wrapping_add(2);
        let at = Instant::now();
        let wall = 1_769_990_000;
        fs::write(
            &app.stats_path,
            serde_json::to_vec(&serde_json::json!({
                "version": 1, "seconds_read": BTreeMap::from([(book, 39), (other, 10)])
            }))
            .unwrap(),
        )
        .unwrap();
        app.stats = crate::reading_stats::ReadingStats::load(&app.stats_path);
        app.reading_history =
            History::load(fixture.root.join("reading-history.json"), BTreeMap::new());
        app.reading_history.remember_title(book, &book_title);
        app.reading_history.remember_title(other, "Short visit");
        app.reading_history
            .remember_title(archived, "Archived notebook");
        app.reading_history
            .set_page_at(Some((book, 1)), at, Some(wall));
        app.reading_history.set_page_at(
            Some((other, 2)),
            at + Duration::from_secs(12),
            Some(wall + 12),
        );
        app.reading_history.set_page_at(
            Some((archived, 3)),
            at + Duration::from_secs(23),
            Some(wall + 23),
        );
        app.reading_history
            .set_page_at(None, at + Duration::from_secs(84), Some(wall + 84));
        app.pace.set_page(Some((book, 7, 1)), at, 0);
        app.pace.set_page(None, at + Duration::from_secs(30), 0);
        app.pace.checkpoint(at + Duration::from_secs(30)).unwrap();
        assert_eq!(app.pace.rate(book), 30.0);

        app.activate(Action::ReadingHistory(Command::Open(Some(book))))
            .unwrap();
        select(&mut app, "Statistics settings");
        select(&mut app, "Reset statistics");
        capture_test_screen(&app, &fonts, "READER_RESET_STATISTICS_MENU", false);
        select(&mut app, "Books read for less than 1 min");
        capture_test_screen(&app, &fonts, "READER_RESET_STATISTICS_CONFIRM", false);
        select(&mut app, "Cancel");
        assert_eq!(app.reading_history.summary(Some(other), None).seconds, 11);
        select(&mut app, "Books read for less than 1 min");
        select(&mut app, "Reset statistics");
        assert_eq!(app.reading_history.summary(Some(other), None).seconds, 0);
        assert_eq!(app.reading_history.summary(Some(book), None).seconds, 12);
        assert_eq!(
            app.reading_history.summary(Some(archived), None).seconds,
            61
        );
        select(&mut app, "Reset statistics per book");
        select(&mut app, "Archived notebook");
        select(&mut app, "Reset statistics");
        assert_eq!(app.reading_history.summary(Some(archived), None).seconds, 0);
        assert_eq!(app.screen, Screen::ReadingHistory(Page::ResetBooks));
        app.history_back();
        select(&mut app, "Reset statistics for the current book");
        select(&mut app, "Reset statistics");
        assert_eq!(app.reading_history.summary(Some(book), None).seconds, 0);
        assert_eq!(app.reading_history.elapsed_since_reset(book, 39), 0);
        assert_eq!(app.pace.rate(book), 60.0);

        app.activate(Action::Show(Screen::Book)).unwrap();
        assert_eq!(app.current_history_location(), position);
        assert_eq!(app.library.state(book).unwrap().bookmarks.len(), mark_count);
        app.save().unwrap();
        let mut reopened = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reopened.open_path(&fixture.book).unwrap();
        assert_eq!(reopened.pace.rate(book), 60.0);
        assert_eq!(reopened.current_history_location(), position);
        assert_eq!(
            reopened.library.state(book).unwrap().bookmarks.len(),
            mark_count
        );
        assert_eq!(
            reopened.reading_history.summary(Some(book), None).seconds,
            0
        );
        assert_eq!(fs::read(&fixture.book).unwrap(), source);
    }
}
