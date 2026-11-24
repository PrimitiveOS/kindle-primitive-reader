//! Book information and retained descriptions in the cross-book bookmark browser.
use super::{
    Action, BrowserLayout, CAPTION, Canvas, Change, Command, Input, Item, Page, Reader, Rect,
    Result, Screen, author_title, contains, item,
};

impl Reader {
    pub(super) fn bookmark_metadata_rows(&self) -> Vec<Item> {
        let browser = &self.bookmark_browser;
        let Some(book) = browser.book.and_then(|id| self.library.state(id)) else {
            return Vec::new();
        };
        let available = self.bookmark_book_available(book.id);
        let snapshot = browser.catalog.get(book.id);
        let mut rows = vec![
            Item::new(author_title(book), Action::Heading).detail(if available {
                "Book"
            } else {
                "Archived · Source file unavailable"
            }),
        ];
        if available {
            rows.push(item("Open book", Command::ViewBook(book.id)));
        }
        if snapshot.is_some_and(|snapshot| snapshot.has_cover) && available {
            rows.push(item("View cover", Command::ViewCover));
        }
        if snapshot.is_some_and(|snapshot| snapshot.description.is_some()) {
            rows.push(item("Read description", Command::Go(Page::Description)));
        }
        rows.push(item("Books with matches", Command::Go(Page::Books)));
        rows.push(Item::new("Source file", Action::Heading).detail(
            crate::library::display_filename(&book.path.to_string_lossy()),
        ));
        let Some(snapshot) = snapshot else {
            rows.push(Item::new("Book metadata", Action::Heading).detail(
                if browser.catalog.loading() {
                    "Reading metadata"
                } else {
                    "Not available"
                },
            ));
            return rows;
        };
        rows.push(
            Item::new("Language", Action::Heading)
                .detail(snapshot.language.as_deref().unwrap_or("Unknown language")),
        );
        if snapshot.series.is_empty() {
            rows.push(Item::new("Series", Action::Heading).detail("Unknown series"));
        } else {
            rows.extend(
                snapshot
                    .series
                    .iter()
                    .map(|series| Item::new(series, Action::Heading).detail("Series")),
            );
        }
        if snapshot.keywords.is_empty() {
            rows.push(Item::new("Keywords", Action::Heading).detail("Not supplied"));
        } else {
            rows.extend(
                snapshot
                    .keywords
                    .iter()
                    .map(|keyword| Item::new(keyword, Action::Heading).detail("Keyword")),
            );
        }
        if snapshot.description.is_none() {
            rows.push(Item::new("Description", Action::Heading).detail("Not supplied"));
        }
        rows
    }

    pub(super) fn view_bookmark_cover(&mut self) -> Result<()> {
        let id = self.bookmark_browser.book.ok_or("Choose a book first")?;
        let entry = self.locate_tracked(id)?;
        let fixed_cover = self
            .bookmark_browser
            .catalog
            .get(id)
            .is_some_and(|snapshot| snapshot.fixed);
        if fixed_cover {
            let path = super::with_storage(|| self.library.absolute_path(&entry.path))?;
            let reading = crate::fixed_session::FixedReading::open(
                self.device_reading_settings().height,
                entry.format,
                &path,
                0,
                &self.fonts,
                crate::library::FixedPreferences::default(),
                None,
                None,
                None,
            )?
            .ok_or("The document has no supported cover")?
            .into_cover_view()?;
            self.organize_book(entry, Screen::BookmarkBrowser(Page::Book))?;
            self.show(Screen::Illustration);
            self.illustration = Some(reading);
            return Ok(());
        }
        self.organize_book(entry, Screen::BookmarkBrowser(Page::Book))?;
        self.view_cover()
    }

    pub(super) fn draw_bookmark_description(&self, canvas: &mut Canvas) {
        let layout = BrowserLayout(self.navigation_layout());
        let prev = layout.control(0, 214, 72);
        let next = layout.control(2, 214, 72);
        let browser = &self.bookmark_browser;
        let Some(id) = browser.book else {
            return;
        };
        let Some(book) = self.library.state(id) else {
            return;
        };
        let Some(text) = browser
            .catalog
            .get(id)
            .and_then(|snapshot| snapshot.description.as_deref())
        else {
            return;
        };
        canvas.text(layout.body(154, 80), &book.title, CAPTION.center().lines(2));
        let (text_page, count) = self.draw_bookmark_text(canvas, layout.text(), text);
        if text_page > 0 {
            let pressed =
                canvas.control_pressed(Rect::new(prev.x, prev.y - 14, prev.w, 100), false);
            canvas.button(prev, "Previous text", false, pressed);
        }
        if text_page + 1 < count {
            let pressed =
                canvas.control_pressed(Rect::new(next.x, next.y - 14, next.w, 100), false);
            canvas.button(next, "Next text", false, pressed);
        }
        canvas.text(
            layout.centered_caption(198),
            &format!("{} of {count}", text_page + 1),
            CAPTION.center(),
        );
    }

    pub(super) fn bookmark_description_input(&mut self, input: Input) -> Change {
        let layout = BrowserLayout(self.navigation_layout());
        let prev = layout.control(0, 214, 72);
        let next = layout.control(2, 214, 72);
        let direction = match input {
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Tap(x, y) if contains(prev, x, y) => -1,
            Input::Tap(x, y) if contains(next, x, y) => 1,
            Input::Swipe { dx, dy, .. } if dx.abs() > dy.abs() => {
                if dx < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Swipe { dy, .. } => {
                if dy < 0.0 {
                    1
                } else {
                    -1
                }
            }
            _ => return Change::None,
        };
        self.step_bookmark_text(direction)
    }
}
