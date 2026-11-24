mod files;

use super::{
    super::ActiveFixed, Action, Command, Date, Item, Page, Reader, Result, Screen, Zone, short_time,
};
use crate::library::{DocumentFormat, Library, display_filename};
use crate::reading_stats::history::sqlite_import::{self, Preview, Records};
use primitive_app::with_storage;
use ring::digest::{Digest, SHA256, digest};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const DATABASE: &str = "statistics.sqlite3";
const OPEN_BOOK: &str = "Open a PDF or DjVu book and its reading statistics first";

#[derive(Clone, Copy, PartialEq, Eq)]
struct BookIdentity {
    id: u64,
    checksum: [u8; 16],
    pages: usize,
}

pub(super) struct Pending {
    book: BookIdentity,
    path: PathBuf,
    checksum: Digest,
    records: Records,
    preview: Preview,
}

pub(super) struct Picker {
    directory: PathBuf,
    listing: files::Listing,
    parents: Vec<usize>,
    return_ordinal: usize,
}

impl Reader {
    pub(super) fn koreader_statistics_book(&self) -> Option<&ActiveFixed> {
        selected_book(self.fixed.as_ref(), self.history_view.book)
    }

    pub(super) fn begin_koreader_statistics_import(&mut self) -> Result<()> {
        self.koreader_statistics_book().ok_or(OPEN_BOOK)?;
        self.history_view.koreader_import = None;
        self.history_view.koreader_databases = None;
        self.history_view.import_summary_page = self.list_page;
        self.show(Screen::ReadingHistory(Page::ImportKoReader));
        Ok(())
    }

    pub(super) fn preview_koreader_statistics_import(&mut self) -> Result<()> {
        if self.screen != Screen::ReadingHistory(Page::ImportKoReader) {
            return Err("Open the statistics import first".into());
        }
        let path = self
            .history_view
            .koreader_import
            .take()
            .map_or_else(|| PathBuf::from(DATABASE), |pending| pending.path);
        let pending = self.preview_koreader_database(path)?;
        self.history_view.koreader_import = Some(pending);
        self.notice = None;
        Ok(())
    }

    fn preview_koreader_database(&self, path: PathBuf) -> Result<Pending> {
        let (book, bytes) = with_storage(|| {
            Ok((
                book_identity(&self.library, self.koreader_statistics_book())?,
                read_database(&self.library, &path)?,
            ))
        })?;
        let checksum = digest(&SHA256, &bytes);
        let records = sqlite_import::read(bytes, book.checksum, book.pages)?;
        let preview = self
            .reading_history
            .preview_koreader_import(book.id, &records);
        Ok(Pending {
            book,
            path,
            checksum,
            records,
            preview,
        })
    }

    pub(super) fn browse_koreader_databases(&mut self) -> Result<()> {
        if self.screen != Screen::ReadingHistory(Page::ImportKoReader) {
            return Err("Open the statistics import first".into());
        }
        let directory = PathBuf::new();
        let listing = with_storage(|| files::list(&self.library, &directory))?;
        self.history_view.koreader_databases = Some(Picker {
            directory,
            listing,
            parents: Vec::new(),
            return_ordinal: self.reading_list_ordinal(),
        });
        self.show(Screen::ReadingHistory(Page::ChooseKoReaderDatabase));
        Ok(())
    }

    pub(super) fn pick_koreader_entry(&mut self, index: usize) -> Result<()> {
        if self.screen != Screen::ReadingHistory(Page::ChooseKoReaderDatabase) {
            return Err("Open the statistics file picker first".into());
        }
        let entry = self
            .history_view
            .koreader_databases
            .as_ref()
            .and_then(|picker| picker.listing.entries.get(index))
            .ok_or("Choose an available entry")?;
        let path = entry.path.clone();
        if entry.folder {
            let listing = with_storage(|| files::list(&self.library, &path))?;
            let ordinal = self.reading_list_ordinal();
            let picker = self.history_view.koreader_databases.as_mut().unwrap();
            picker.parents.push(ordinal);
            picker.directory = path;
            picker.listing = listing;
            self.show(Screen::ReadingHistory(Page::ChooseKoReaderDatabase));
        } else {
            let pending = self.preview_koreader_database(path)?;
            self.history_view.koreader_import = Some(pending);
            self.history_view.koreader_databases = None;
            self.show(Screen::ReadingHistory(Page::ImportKoReader));
        }
        Ok(())
    }

    pub(super) fn back_from_koreader_databases(&mut self) {
        let Some(mut picker) = self.history_view.koreader_databases.take() else {
            self.show(Screen::ReadingHistory(Page::ImportKoReader));
            return;
        };
        if let Some(ordinal) = picker.parents.pop() {
            picker.directory.pop();
            match with_storage(|| files::list(&self.library, &picker.directory)) {
                Ok(listing) => {
                    picker.listing = listing;
                    self.history_view.koreader_databases = Some(picker);
                    self.show(Screen::ReadingHistory(Page::ChooseKoReaderDatabase));
                    self.restore_reading_list_ordinal(ordinal);
                    return;
                }
                Err(error) => {
                    self.show(Screen::ReadingHistory(Page::ImportKoReader));
                    self.restore_reading_list_ordinal(picker.return_ordinal);
                    self.report(error);
                    return;
                }
            }
        }
        self.show(Screen::ReadingHistory(Page::ImportKoReader));
        self.restore_reading_list_ordinal(picker.return_ordinal);
    }

    pub(super) fn koreader_database_items(&self) -> Vec<Item> {
        let Some(picker) = &self.history_view.koreader_databases else {
            return Vec::new();
        };
        let mut items = vec![Item::new(folder_title(&picker.directory), Action::Heading)];
        items.extend(
            picker
                .listing
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let name = entry
                        .path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default();
                    let mut item = Item::new(
                        display_filename(name),
                        Action::ReadingHistory(Command::PickKoReaderEntry(index)),
                    );
                    item.folder = entry.folder;
                    item
                }),
        );
        if picker.listing.entries.is_empty() {
            items.push(Item::new("No folders or SQLite databases", Action::Heading));
        }
        if picker.listing.truncated {
            items.push(Item::new(
                "Folder listing reached its limit",
                Action::Heading,
            ));
        }
        items
    }

    pub(super) fn apply_koreader_statistics_import(&mut self) -> Result<()> {
        let pending = self
            .history_view
            .koreader_import
            .as_ref()
            .filter(|_| self.screen == Screen::ReadingHistory(Page::ImportKoReader))
            .ok_or("Preview the statistics before importing")?;
        if pending.preview.new_visits == 0 {
            return Err("There are no new records to import".into());
        }
        if !pending.preview.can_store {
            return Err("Reading history cannot store these records".into());
        }
        let count = pending.preview.new_visits;
        let library = &self.library;
        let fixed = selected_book(self.fixed.as_ref(), self.history_view.book);
        let history = &mut self.reading_history;
        with_storage(|| {
            let book = book_identity(library, fixed)?;
            let bytes = read_database(library, &pending.path)?;
            if book != pending.book || digest(&SHA256, &bytes).as_ref() != pending.checksum.as_ref()
            {
                return Err(io::Error::other(
                    "The book or database changed. Preview the import again.",
                ));
            }
            drop(bytes);
            history.import_koreader_visits(book.id, &pending.records, pending.preview)
        })?;
        self.history_back();
        self.notice = Some(format!("Imported {count} KOReader records"));
        Ok(())
    }

    pub(super) fn koreader_statistics_import_items(&self) -> Vec<Item> {
        let Some(pending) = &self.history_view.koreader_import else {
            return vec![
                Item::new("Use a closed KOReader database", Action::Heading)
                    .detail("Copy a checkpointed database from KOReader settings to Documents."),
                Item::new(
                    "Preview database",
                    Action::ReadingHistory(Command::PreviewKoReaderImport),
                )
                .detail("Documents / statistics.sqlite3"),
                browse_row(),
            ];
        };
        let preview = pending.preview;
        let label = |time: Option<i64>| {
            time.and_then(|time| Zone::Local.date_with_origin(time, 0))
                .map(Date::label)
        };
        let matched = match (
            pending.records.title.as_str(),
            pending.records.authors.as_str(),
        ) {
            ("", _) => "Untitled".into(),
            (title, "" | "N/A") => title.to_owned(),
            (title, authors) => format!("{title} · {authors}"),
        };
        let mut items = vec![
            Item::new("Matched KOReader book", Action::Heading).detail(matched),
            Item::new("Database", Action::Heading).detail(folder_title(&pending.path)),
            Item::new("New records", Action::Heading).detail(format!(
                "{} · {}",
                preview.new_visits,
                short_time(preview.seconds)
            )),
        ];
        if let (Some(first), Some(last)) = (label(preview.first_start), label(preview.last_start)) {
            items.push(
                Item::new("Reading dates", Action::Heading).detail(format!("{first} to {last}")),
            );
        }
        if preview.duplicate + preview.conflicting + pending.records.other_page_counts > 0 {
            items.push(
                Item::new("Skipped records", Action::Heading).detail(format!(
                    "{} existing, {} conflicting, {} different page count",
                    preview.duplicate, preview.conflicting, pending.records.other_page_counts,
                )),
            );
        }
        if preview.new_visits > 0 {
            if preview.can_store {
                items.push(Item::new(
                    "Import records",
                    Action::ReadingHistory(Command::ApplyKoReaderImport),
                ));
            } else {
                items.push(Item::new(
                    "Reading history cannot store these records",
                    Action::Heading,
                ));
            }
        }
        items.push(Item::new(
            "Preview again",
            Action::ReadingHistory(Command::PreviewKoReaderImport),
        ));
        items.push(browse_row());
        items
    }
}

fn selected_book(fixed: Option<&ActiveFixed>, book: Option<u64>) -> Option<&ActiveFixed> {
    fixed.filter(|fixed| {
        // Reader opens DjVu directly from bytes, without a resolver for indirect indexes.
        matches!(
            fixed.entry.format,
            DocumentFormat::Pdf | DocumentFormat::Djvu
        ) && book == Some(fixed.id)
    })
}

fn book_identity(library: &Library, fixed: Option<&ActiveFixed>) -> io::Result<BookIdentity> {
    let fixed = fixed.ok_or_else(|| io::Error::other(OPEN_BOOK))?;
    if library.identify(&fixed.entry)? != fixed.id {
        return Err(io::Error::other(
            "The book changed. Reopen it before importing.",
        ));
    }
    let book = library.absolute_path(&fixed.entry.path)?;
    Ok(BookIdentity {
        id: fixed.id,
        checksum: crate::koreader_sidecar::partial_md5(&mut fs::File::open(book)?)?,
        pages: fixed.reading.count(),
    })
}

fn read_database(library: &Library, path: &Path) -> io::Result<Vec<u8>> {
    files::read(library, path).map_err(|error| {
        if error.kind() != io::ErrorKind::NotFound {
            error
        } else if path == Path::new(DATABASE) {
            io::Error::new(
                error.kind(),
                "Copy statistics.sqlite3 to Documents or browse for it",
            )
        } else {
            io::Error::new(
                error.kind(),
                "Database moved or was removed. Browse for it again.",
            )
        }
    })
}

fn folder_title(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        "Documents".into()
    } else {
        format!("Documents / {}", display_filename(&path.to_string_lossy()))
    }
}

fn browse_row() -> Item {
    Item::new(
        "Browse Documents",
        Action::ReadingHistory(Command::BrowseKoReaderDatabases),
    )
}

#[cfg(test)]
mod tests;
