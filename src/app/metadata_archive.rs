//! Configure and use the book metadata archive independently of moving book files.
use super::book_archive::{FolderPicker, folder_title};
use super::*;
use crate::library::metadata_archive::{MoveOutcome, SavedOn, Snapshot};
use primitive_ui::{Sheet, SheetAction};

#[cfg(test)]
mod tests;

const MAX_PREFERENCE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Settings,
    Folder,
    Books,
    Entry(u64),
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Settings => "Book metadata archive",
            Self::Folder => "Archive folder",
            Self::Books => "Archived books",
            Self::Entry(_) => "Archived book",
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Preference {
    folder: PathBuf,
    on_close: bool,
    on_delete: bool,
}

pub(super) struct State {
    path: PathBuf,
    preference: Option<Preference>,
    origin: Screen,
    picker: Option<FolderPicker>,
    books: Vec<Snapshot>,
    confirmation: Option<Confirmation>,
    consumption: Option<Consumption>,
}

struct Consumption {
    id: u64,
    folder: PathBuf,
    revision: String,
}

enum Confirmation {
    Restore {
        entry: LibraryBook,
        snapshot: Box<Snapshot>,
        folder: PathBuf,
    },
    Remove(u64),
    Move(PathBuf),
    Info {
        title: String,
        text: String,
    },
}

pub(super) enum Command {
    Open,
    About,
    ChooseFolder,
    Browse(PathBuf),
    UseFolder,
    Toggle(SavedOn),
    Books,
    Entry(u64),
    Remove(u64),
    ReadMark(String, String),
}

impl State {
    pub(super) fn load(path: PathBuf) -> Self {
        let preference = with_storage(|| {
            let mut data = Vec::new();
            fs::File::open(&path)?
                .take(MAX_PREFERENCE_BYTES + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 > MAX_PREFERENCE_BYTES {
                return Err(io::Error::other("The archive setting is too large."));
            }
            serde_json::from_slice(&data).map_err(io::Error::other)
        })
        .ok();
        Self {
            path,
            preference,
            origin: Screen::ReadingControls,
            picker: None,
            books: Vec::new(),
            confirmation: None,
            consumption: None,
        }
    }

    fn set(&mut self, preference: Preference) -> io::Result<()> {
        let data = serde_json::to_vec(&preference).map_err(io::Error::other)?;
        if data.len() as u64 > MAX_PREFERENCE_BYTES {
            return Err(io::Error::other("Choose a folder with a shorter path."));
        }
        primitive_app::save(&self.path, &data)?;
        self.preference = Some(preference);
        Ok(())
    }

    pub(super) fn close_surfaces(&mut self, screen: Screen) {
        if screen != Screen::MetadataArchive(Page::Folder) {
            self.picker = None;
        }
        if !matches!(
            screen,
            Screen::MetadataArchive(Page::Books | Page::Entry(_))
        ) {
            self.books.clear();
        }
        self.confirmation = None;
    }

    pub(super) fn abandon_archive_consumption(&mut self) {
        self.consumption = None;
    }

    pub(super) fn enabled(&self, reason: SavedOn) -> bool {
        self.preference.as_ref().is_some_and(|pref| match reason {
            SavedOn::Close => pref.on_close,
            SavedOn::Delete => pref.on_delete,
        })
    }

    pub(super) fn waiting_to_restore(&self) -> bool {
        matches!(self.confirmation, Some(Confirmation::Restore { .. }))
    }
}

impl Reader {
    pub(super) fn metadata_archive_item(&self) -> Item {
        Item::new(
            "Book metadata archive",
            Action::MetadataArchive(Command::Open),
        )
        .detail(self.metadata_archive.preference.as_ref().map_or_else(
            || "Choose a folder to save reading data copies".into(),
            |preference| folder_title(&preference.folder),
        ))
    }

    pub(super) fn metadata_archive_items(&self, page: Page) -> Vec<Item> {
        let language = self.controls.interface_language;
        let state = &self.metadata_archive;
        let command = Action::MetadataArchive;
        match page {
            Page::Settings => {
                let mut rows =
                    vec![
                        Item::new(
                            language.metadata_settings_text("About the archive"),
                            command(Command::About),
                        )
                        .detail(language.metadata_settings_text(
                            "Save a separate copy of a book’s reading data",
                        )),
                        Item::new(
                            language.metadata_settings_text("Archive folder"),
                            command(Command::ChooseFolder),
                        )
                        .detail(state.preference.as_ref().map_or_else(
                            || {
                                language
                                    .metadata_settings_text("Choose a folder in Documents")
                                    .into()
                            },
                            |preference| folder_title(&preference.folder),
                        )),
                    ];
                if let Some(preference) = &state.preference {
                    let status = |enabled| {
                        language.metadata_settings_text(if enabled { "On" } else { "Off" })
                    };
                    rows.extend([
                        Item::new(
                            language.metadata_settings_text("Save metadata on book close"),
                            command(Command::Toggle(SavedOn::Close)),
                        )
                        .detail(status(preference.on_close)),
                        Item::new(
                            language.metadata_settings_text("Save metadata before deleting"),
                            command(Command::Toggle(SavedOn::Delete)),
                        )
                        .detail(status(preference.on_delete)),
                        Item::new(
                            language.metadata_settings_text("Archived books"),
                            command(Command::Books),
                        )
                        .detail(language.metadata_settings_text(
                            "View saved positions, bookmarks and highlights",
                        )),
                    ]);
                }
                rows
            }
            Page::Folder => state.picker.as_ref().map_or_else(Vec::new, |picker| {
                picker.items(command(Command::UseFolder), |path| {
                    command(Command::Browse(path))
                })
            }),
            Page::Books => state
                .books
                .iter()
                .map(|snapshot| {
                    let book = &snapshot.book;
                    let reason = match snapshot.saved_on {
                        SavedOn::Close => language.metadata_settings_text("Saved on close"),
                        SavedOn::Delete => language.metadata_settings_text("Saved before deletion"),
                    };
                    let date = snapshot.saved_at.and_then(crate::mark_time::local_date);
                    let detail =
                        date.map_or_else(|| reason.to_owned(), |date| format!("{reason} · {date}"));
                    Item::new(&book.title, command(Command::Entry(book.id))).detail(detail)
                })
                .collect(),
            Page::Entry(id) => {
                let Some(snapshot) = state.books.iter().find(|record| record.book.id == id) else {
                    return Vec::new();
                };
                let book = &snapshot.book;
                let mut rows = vec![
                    Item::new(
                        language.metadata_settings_text("Remove saved copy"),
                        command(Command::Remove(id)),
                    )
                    .detail(
                        language.metadata_settings_text("Keep the book and its live reading data"),
                    ),
                ];
                if let Some(percent) = book.reading_percent {
                    rows[0].detail = Some(if language.is_english() {
                        format!("Archived at {percent}% · Keep current reading data")
                    } else {
                        format!("Đã lưu tại {percent}% · Giữ dữ liệu đọc hiện tại")
                    });
                }
                rows.extend(book.bookmarks.iter().map(|mark| {
                    Item::new(
                        &mark.label,
                        command(Command::ReadMark("Bookmark".into(), mark.label.clone())),
                    )
                    .detail(language.metadata_settings_text("Bookmark"))
                }));
                rows.extend(
                    book.highlights
                        .iter()
                        .map(|mark| archived_highlight(&mark.selection.text, mark.note.as_deref())),
                );
                rows.extend(
                    book.pdf_highlights
                        .iter()
                        .map(|mark| archived_highlight(&mark.text, mark.note.as_deref())),
                );
                rows
            }
        }
    }

    pub(super) fn metadata_archive_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.metadata_archive.origin = if self.screen == Screen::ReadingData {
                    Screen::ReadingData
                } else {
                    Screen::ReadingControls
                };
                self.show(Screen::MetadataArchive(Page::Settings));
            }
            Command::About => self.metadata_archive.confirmation = Some(Confirmation::Info {
                title: "About the archive".into(),
                text: "The archive keeps settings, reading position, notes and highlights. Open a matching edition to recover its saved data. The original book stays where it is.".into(),
            }),
            Command::ReadMark(title, text) => {
                self.metadata_archive.confirmation = Some(Confirmation::Info { title, text });
            }
            Command::ChooseFolder => {
                let folder = self.metadata_archive.preference.as_ref()
                    .map(|preference| preference.folder.clone())
                    .filter(|folder| with_storage(|| self.library.browse_folder(folder)).is_ok())
                    .unwrap_or_default();
                self.choose_metadata_archive_folder(folder)?;
            }
            Command::Browse(folder) => self.choose_metadata_archive_folder(folder)?,
            Command::UseFolder => {
                let state = &mut self.metadata_archive;
                let folder = state.picker.as_ref().ok_or("Choose a folder")?.directory.clone();
                with_storage(|| self.library.browse_folder(&folder))?;
                match &state.preference {
                    Some(preference) if preference.folder != folder => {
                        state.confirmation = Some(Confirmation::Move(folder));
                    }
                    Some(_) => self.show(Screen::MetadataArchive(Page::Settings)),
                    None => {
                        state.set(Preference { folder, on_close: false, on_delete: true })?;
                        self.show(Screen::MetadataArchive(Page::Settings));
                    }
                }
            }
            Command::Toggle(reason) => {
                let state = &mut self.metadata_archive;
                let preference = state.preference.as_ref().ok_or("Choose an archive folder first")?;
                state.set(Preference {
                    folder: preference.folder.clone(),
                    on_close: preference.on_close ^ (reason == SavedOn::Close),
                    on_delete: preference.on_delete ^ (reason == SavedOn::Delete),
                })?;
            }
            Command::Books => self.load_metadata_archive_books()?,
            Command::Entry(id) => self.show(Screen::MetadataArchive(Page::Entry(id))),
            Command::Remove(id) => self.metadata_archive.confirmation = Some(Confirmation::Remove(id)),
        }
        Ok(())
    }

    fn choose_metadata_archive_folder(&mut self, folder: PathBuf) -> Result<()> {
        let picker = FolderPicker::load(&self.library, folder)?;
        self.show(Screen::MetadataArchive(Page::Folder));
        self.metadata_archive.picker = Some(picker);
        Ok(())
    }

    fn move_metadata_archive_to(&mut self, folder: PathBuf) -> Result<()> {
        let state = &mut self.metadata_archive;
        let current = state
            .preference
            .as_ref()
            .ok_or("Choose an archive folder first")?;
        let from = current.folder.clone();
        let on_close = current.on_close;
        let on_delete = current.on_delete;
        let preference_path = state.path.clone();
        let outcome = with_storage(|| {
            self.library
                .move_metadata_archive(&from, &folder, &preference_path, || {
                    state.set(Preference {
                        folder: folder.clone(),
                        on_close,
                        on_delete,
                    })
                })
        })?;
        self.show(Screen::MetadataArchive(Page::Settings));
        self.notice = Some(
            match outcome {
                MoveOutcome::Moved => "Metadata archive moved",
                MoveOutcome::Empty => "Archive folder updated",
                MoveOutcome::OldCopyKept => "Archive moved. The old copy is still available.",
                MoveOutcome::DateConflict => {
                    "Archive moved. Copies with ambiguous dates remain in the old folder."
                }
            }
            .into(),
        );
        Ok(())
    }

    fn load_metadata_archive_books(&mut self) -> Result<()> {
        let preference = self
            .metadata_archive
            .preference
            .as_ref()
            .ok_or("Choose an archive folder first")?;
        let books = with_storage(|| self.library.archived_books(&preference.folder))?;
        self.show(Screen::MetadataArchive(Page::Books));
        self.metadata_archive.books = books;
        if self.metadata_archive.books.is_empty() {
            self.notice = Some("No archived books".into());
        }
        Ok(())
    }

    pub(super) fn back_from_metadata_archive(&mut self, page: Page) {
        let next = match page {
            Page::Settings => self.metadata_archive.origin,
            Page::Folder | Page::Books => Screen::MetadataArchive(Page::Settings),
            Page::Entry(_) => Screen::MetadataArchive(Page::Books),
        };
        self.show(next);
    }

    pub(super) fn save_metadata_archive(&self, id: u64, reason: SavedOn) -> io::Result<()> {
        let Some(preference) = &self.metadata_archive.preference else {
            return Ok(());
        };
        if self.metadata_archive.enabled(reason) {
            with_storage(|| {
                self.library.archive_metadata(
                    &preference.folder,
                    id,
                    reason,
                    crate::mark_time::now(),
                )
            })?;
        }
        Ok(())
    }

    pub(super) fn metadata_archive_close_warning(&self) -> Option<String> {
        let id = self.open_book_id()?;
        self.save_metadata_archive(id, SavedOn::Close)
            .err()
            .map(|error| format!("Could not archive book metadata: {error}"))
    }

    pub(super) fn offer_archived_metadata(&mut self, entry: &LibraryBook, id: u64) -> bool {
        let Some(preference) = &self.metadata_archive.preference else {
            return false;
        };
        let snapshot = match with_storage(|| self.library.archived_book(&preference.folder, id)) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return false,
            Err(error) => {
                eprintln!("reader book_metadata_archive_lookup_failed={error}");
                return false;
            }
        };
        self.metadata_archive.confirmation = Some(Confirmation::Restore {
            entry: entry.clone(),
            snapshot: Box::new(snapshot),
            folder: preference.folder.clone(),
        });
        true
    }

    fn metadata_archive_info(&self) -> Option<(Rect, Rect, &str, &str)> {
        let Confirmation::Info { title, text } = self.metadata_archive.confirmation.as_ref()?
        else {
            return None;
        };
        let layout = self.navigation_layout();
        let panel = layout.centered(Rect::new(
            (WIDTH as i32 - 880) / 2,
            (HEIGHT as i32 - 580) / 2,
            880,
            580,
        ));
        let done = Rect::new(panel.right() - 250, panel.bottom() - 134, 210, 94);
        Some((panel, done, title, text))
    }

    fn metadata_archive_sheet(&self) -> Option<Sheet<'_>> {
        let layout = self.navigation_layout();
        let screen = Rect::new(0, 0, layout.width, layout.height);
        Some(match self.metadata_archive.confirmation.as_ref()? {
            Confirmation::Restore { .. } => Sheet::centered(
                screen,
                "Restore book metadata?",
                "Restore progress, settings and marks.\nStart fresh removes the saved copy.",
                "Restore",
            ),
            Confirmation::Info { .. } => return None,
            Confirmation::Remove(_) => Sheet::centered(
                screen,
                "Remove this saved copy?",
                "The book and its current reading data stay in your Library.",
                "Remove",
            ),
            Confirmation::Move(_) => Sheet::centered(
                screen,
                "Move metadata archive?",
                "Merge saved reading data into this folder.\nKeep the newest dated copy of each edition.",
                "Move",
            ),
        })
    }

    pub(super) fn draw_metadata_archive_confirmation(&self, canvas: &mut Canvas) {
        if let Some((panel, done, title, text)) = self.metadata_archive_info() {
            canvas.popup(panel);
            let body = panel.inset(canvas.px(48));
            canvas.text(
                Rect::new(body.x, body.y, body.w, canvas.px(70)),
                title,
                primitive_ui::TITLE,
            );
            let top = body.y + canvas.px(92);
            canvas.text(
                Rect::new(body.x, top, body.w, done.y - top - canvas.px(20)),
                text,
                BODY.lines(7),
            );
            canvas.button(done, "Done", true, false);
        } else if let Some(sheet) = self.metadata_archive_sheet() {
            let pressed = [SheetAction::Cancel, SheetAction::Confirm]
                .into_iter()
                .find(|action| canvas.control_pressed(sheet.button(*action), false));
            sheet.draw(canvas, pressed);
            if self.metadata_archive.waiting_to_restore() {
                let cancel = sheet.button(SheetAction::Cancel);
                canvas.button(
                    cancel,
                    "Start fresh",
                    false,
                    pressed == Some(SheetAction::Cancel),
                );
            }
        }
    }

    pub(super) fn metadata_archive_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        if let Some((_, done, _, _)) = self.metadata_archive_info() {
            return Some(match input {
                Input::Tap(x, y) if done.contains(x, y) => Some((x, y)),
                _ => None,
            });
        }
        let sheet = self.metadata_archive_sheet()?;
        Some(match input {
            Input::Tap(x, y)
                if sheet.button(SheetAction::Confirm).contains(x, y)
                    || sheet.button(SheetAction::Cancel).contains(x, y) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(super) fn metadata_archive_confirmation_input(&mut self, input: Input) -> Option<Change> {
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        if let Some((panel, done, _, _)) = self.metadata_archive_info() {
            return Some(
                if matches!(input, Input::PageBack)
                    || matches!(input, Input::Tap(x, y) if done.contains(x, y) || !panel.contains(x, y))
                {
                    self.metadata_archive.confirmation = None;
                    Change::Partial
                } else {
                    Change::None
                },
            );
        }
        if input == Input::PageBack && self.metadata_archive.waiting_to_restore() {
            self.metadata_archive.confirmation = None;
            self.closing_before_switch = None;
            return Some(Change::Partial);
        }
        let sheet = self.metadata_archive_sheet()?;
        let action = match input {
            Input::PageBack => Some(SheetAction::Cancel),
            Input::Tap(x, y)
                if self.metadata_archive.waiting_to_restore()
                    && !sheet.button(SheetAction::Cancel).contains(x, y)
                    && !sheet.button(SheetAction::Confirm).contains(x, y) =>
            {
                None
            }
            Input::Tap(x, y) => sheet.hit(x, y),
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        let confirmation = self.metadata_archive.confirmation.take().unwrap();
        Some(
            match self.apply_metadata_archive_confirmation(confirmation, action) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
        )
    }

    pub(super) fn complete_archive_consumption(&mut self) {
        let Some(consumption) = self.metadata_archive.consumption.as_ref() else {
            return;
        };
        if self.screen == Screen::PdfPassword || self.open_book_id() != Some(consumption.id) {
            return;
        }
        let consumption = self.metadata_archive.consumption.take().unwrap();
        let result = self.save_library().and_then(|()| {
            with_storage(|| {
                self.library.remove_archived_book(
                    &consumption.folder,
                    consumption.id,
                    &consumption.revision,
                )
            })
        });
        if let Err(error) = result {
            self.notice = Some(format!("Book opened · archived copy kept: {error}"));
        }
    }

    fn apply_metadata_archive_confirmation(
        &mut self,
        confirmation: Confirmation,
        action: SheetAction,
    ) -> Result<()> {
        match confirmation {
            Confirmation::Restore {
                entry,
                snapshot,
                folder,
            } => {
                let consumption = Consumption {
                    id: snapshot.book.id,
                    folder,
                    revision: snapshot.revision.clone(),
                };
                if action == SheetAction::Confirm {
                    with_storage(|| self.library.restore_archived_book(&entry, *snapshot))?;
                } else {
                    with_storage(|| self.library.identify(&entry)).and_then(|id| {
                        if id == consumption.id && self.library.state(id).is_none() {
                            Ok(())
                        } else {
                            Err(io::Error::other(
                                "The book or its live reading data changed. Open it again.",
                            ))
                        }
                    })?;
                }
                self.metadata_archive.consumption = Some(consumption);
                self.open_with_password_mode(entry, None, false)?;
            }
            Confirmation::Remove(id) if action == SheetAction::Confirm => {
                let preference = self
                    .metadata_archive
                    .preference
                    .as_ref()
                    .ok_or("Choose an archive folder first")?;
                let revision = &self
                    .metadata_archive
                    .books
                    .iter()
                    .find(|record| record.book.id == id)
                    .ok_or("Reload the archived book list")?
                    .revision;
                with_storage(|| {
                    self.library
                        .remove_archived_book(&preference.folder, id, revision)
                })?;
                self.load_metadata_archive_books()?;
                self.notice = Some("Saved copy removed".into());
            }
            Confirmation::Move(folder) if action == SheetAction::Confirm => {
                self.move_metadata_archive_to(folder)?;
            }
            Confirmation::Remove(_) | Confirmation::Move(_) | Confirmation::Info { .. } => {}
        }
        Ok(())
    }
}

fn archived_highlight(quote: &str, note: Option<&str>) -> Item {
    let (title, text) = match note {
        Some(note) => ("Note", format!("{note}\n\n{quote}")),
        None => ("Highlight", quote.to_owned()),
    };
    Item::new(
        quote,
        Action::MetadataArchive(Command::ReadMark(title.into(), text)),
    )
    .detail(note.unwrap_or("Highlight"))
}
