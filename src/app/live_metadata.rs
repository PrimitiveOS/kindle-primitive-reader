//! Book metadata settings and the list of fingerprint-based reading records.
use super::*;
use crate::library::live_metadata::{HashBook, Location};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Settings,
    About,
    Hashed,
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Settings => "Book metadata location",
            Self::About => "About book metadata",
            Self::Hashed => "Fingerprint metadata",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Open,
    Select(Location),
    ListHashed,
    OpenHashed(u64),
}

#[derive(Default)]
pub(super) struct State {
    origin: Option<Screen>,
    hashed: Vec<HashBook>,
}

fn location_detail(location: Location) -> &'static str {
    match location {
        Location::BookFolder => "Move the sidecar with the book",
        Location::Profile => "Store by path in Reader’s data folder",
        Location::Hash => "Keep reading data when you move or rename a book",
    }
}

impl Reader {
    pub(super) fn live_metadata_item(&self) -> Item {
        Item::new(
            "Book metadata location",
            Action::LiveMetadata(Command::Open),
        )
        .detail(self.library.live_metadata_location().label())
    }

    pub(super) fn live_metadata_items(&self, page: Page) -> Vec<Item> {
        let selected = self.library.live_metadata_location();
        match page {
            Page::Settings => {
                let mut rows = vec![Item::new(
                    "About book metadata location",
                    Action::Show(Screen::LiveMetadata(Page::About)),
                )];
                rows.extend(Location::ALL.into_iter().map(|location| {
                    let row = Item::new(
                        location.label(),
                        Action::LiveMetadata(Command::Select(location)),
                    );
                    row.detail(if location == selected {
                        "Selected"
                    } else {
                        location_detail(location)
                    })
                }));
                rows.push(
                    if !self.live_metadata.hashed.is_empty() || selected == Location::Hash {
                        let count = self.live_metadata.hashed.len();
                        Item::new(
                            "Books with fingerprint metadata",
                            Action::LiveMetadata(Command::ListHashed),
                        )
                        .detail(if count == 1 {
                            "1 book".to_owned()
                        } else {
                            format!("{count} books")
                        })
                    } else {
                        Item::new("No books with fingerprint metadata", Action::Heading)
                    },
                );
                rows
            }
            Page::About => {
                let mut rows = vec![
                    Item::new("Reading data", Action::Heading)
                        .detail("Position, reading settings, bookmarks, highlights and notes"),
                ];
                rows.extend(Location::ALL.into_iter().map(|location| {
                    Item::new(location.label(), Action::Heading).detail(location_detail(location))
                }));
                rows.push(
                    Item::new("Changing location", Action::Heading)
                        .detail("Reader moves each sidecar when it next saves the book"),
                );
                rows
            }
            Page::Hashed => {
                if self.live_metadata.hashed.is_empty() {
                    return vec![Item::new(
                        "No books with fingerprint metadata",
                        Action::Heading,
                    )];
                }
                self.live_metadata
                    .hashed
                    .iter()
                    .map(|book| {
                        let path = book.path.to_string_lossy();
                        let detail = book.author.as_ref().map_or_else(
                            || path.to_string(),
                            |author| format!("{author} · {path}"),
                        );
                        Item::new(
                            &book.title,
                            Action::LiveMetadata(Command::OpenHashed(book.id)),
                        )
                        .detail(detail)
                    })
                    .collect()
            }
        }
    }

    pub(super) fn live_metadata_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.live_metadata.origin = Some(self.screen);
                self.live_metadata.hashed =
                    with_storage(|| self.library.hashed_metadata_books()).unwrap_or_default();
                self.show(Screen::LiveMetadata(Page::Settings));
            }
            Command::Select(location) => {
                with_storage(|| self.library.set_live_metadata_location(location))?;
                if let Some(id) = self.open_book_id() {
                    self.library.request_live_metadata(id);
                    self.dirty = true;
                    self.save_library()?;
                }
                self.live_metadata.hashed =
                    with_storage(|| self.library.hashed_metadata_books()).unwrap_or_default();
            }
            Command::ListHashed => {
                self.live_metadata.hashed = with_storage(|| self.library.hashed_metadata_books())?;
                self.show(Screen::LiveMetadata(Page::Hashed));
            }
            Command::OpenHashed(id) => {
                let book = with_storage(|| self.library.locate_hashed_metadata_book(id))?;
                self.open_book(book)?;
            }
        }
        Ok(())
    }

    pub(super) fn back_from_live_metadata(&mut self, page: Page) {
        let next = if page == Page::Settings {
            self.live_metadata
                .origin
                .take()
                .unwrap_or(Screen::ReadingControls)
        } else {
            Screen::LiveMetadata(Page::Settings)
        };
        self.show(next);
    }
}
