use super::{
    super::{ActiveFixed, read_import_file},
    Action, Command, Date, Item, Page, Reader, Result, Screen, Zone, short_time,
};
use crate::library::{DocumentFormat, Library};
use crate::reading_stats::history::sqlite_import::{self, MAX_SOURCE_BYTES, Preview, Records};
use primitive_app::with_storage;
use ring::digest::{Digest, SHA256, digest};
use std::{fs, io, path::Path};

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
    checksum: Digest,
    records: Records,
    preview: Preview,
}

impl Reader {
    pub(super) fn koreader_statistics_book(&self) -> Option<&ActiveFixed> {
        selected_book(self.fixed.as_ref(), self.history_view.book)
    }

    pub(super) fn begin_koreader_statistics_import(&mut self) -> Result<()> {
        self.koreader_statistics_book().ok_or(OPEN_BOOK)?;
        self.history_view.koreader_import = None;
        self.history_view.import_summary_page = self.list_page;
        self.show(Screen::ReadingHistory(Page::ImportKoReader));
        Ok(())
    }

    pub(super) fn preview_koreader_statistics_import(&mut self) -> Result<()> {
        if self.screen != Screen::ReadingHistory(Page::ImportKoReader) {
            return Err("Open the statistics import first".into());
        }
        self.history_view.koreader_import = None;
        let (book, bytes) = with_storage(|| {
            Ok((
                book_identity(&self.library, self.koreader_statistics_book())?,
                read_database(self.library.documents_path())?,
            ))
        })?;
        let checksum = digest(&SHA256, &bytes);
        let records = sqlite_import::read(bytes, book.checksum, book.pages)?;
        let preview = self
            .reading_history
            .preview_koreader_import(book.id, &records);
        self.history_view.koreader_import = Some(Pending {
            book,
            checksum,
            records,
            preview,
        });
        self.notice = None;
        Ok(())
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
            let bytes = read_database(library.documents_path())?;
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
                Item::new("Copy statistics.sqlite3 to Documents", Action::Heading)
                    .detail("Use a closed, checkpointed copy from KOReader settings."),
                Item::new(
                    "Preview database",
                    Action::ReadingHistory(Command::PreviewKoReaderImport),
                ),
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

fn read_database(documents: &Path) -> io::Result<Vec<u8>> {
    let check_sidecars = || -> io::Result<()> {
        for name in ["statistics.sqlite3-wal", "statistics.sqlite3-journal"] {
            match fs::symlink_metadata(documents.join(name)) {
                Ok(info) if info.is_file() && info.len() == 0 => {}
                Ok(_) => {
                    return Err(io::Error::other(
                        "Use a closed KOReader database copy without pending journal changes",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    };
    check_sidecars()?;
    let bytes = read_import_file(&documents.join(DATABASE), MAX_SOURCE_BYTES).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::new(error.kind(), "Copy statistics.sqlite3 to Documents first")
        } else {
            error
        }
    })?;
    check_sidecars()?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;
