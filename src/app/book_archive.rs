//! Choose an archive inside Documents and confirm moving the currently open edition.
use super::*;
use crate::library::BrowseKind;
use crate::library::archive::{DEFAULT_FOLDER, MovePlan};
use primitive_ui::{Sheet, SheetAction};

#[cfg(test)]
mod tests;

const CONFIG_LIMIT: u64 = 16 * 1024;
const VIEW: Rect = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);

pub(super) struct Archive {
    preference: PathBuf,
    folder: PathBuf,
    picker: Option<FolderPicker>,
    plan: Option<MovePlan>,
}

pub(super) struct FolderPicker {
    pub(super) directory: PathBuf,
    subdirectories: Vec<PathBuf>,
}

#[derive(Clone)]
pub(super) enum Command {
    Open,
    Move,
    ChooseFolder,
    Browse(PathBuf),
    UseFolder,
}

impl Archive {
    pub(super) fn load(preference: PathBuf) -> Self {
        let folder = with_storage(|| {
            let mut data = Vec::new();
            fs::File::open(&preference)?
                .take(CONFIG_LIMIT + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 > CONFIG_LIMIT {
                return Err(io::Error::other("The archive folder setting is too large."));
            }
            serde_json::from_slice(&data).map_err(io::Error::other)
        })
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_FOLDER));
        Self {
            preference,
            folder,
            picker: None,
            plan: None,
        }
    }

    pub(super) fn close_surfaces(&mut self, screen: Screen) {
        if screen != Screen::BookArchive {
            self.plan = None;
        }
        if screen != Screen::ArchiveFolder {
            self.picker = None;
        }
    }
}

pub(super) fn folder_title(folder: &Path) -> String {
    let mut title = "Documents".to_owned();
    for part in folder {
        title.push_str(" / ");
        title.push_str(&crate::library::display_filename(&part.to_string_lossy()));
    }
    title
}

impl FolderPicker {
    pub(super) fn load(library: &Library, directory: PathBuf) -> io::Result<Self> {
        let listing = with_storage(|| library.browse(&directory))?;
        let subdirectories = listing
            .entries
            .into_iter()
            .filter(|entry| matches!(entry.kind, BrowseKind::Folder))
            .map(|entry| entry.path)
            .collect();
        Ok(Self {
            directory,
            subdirectories,
        })
    }

    pub(super) fn items(
        &self,
        language: super::interface_language::InterfaceLanguage,
        use_folder: Action,
        browse: impl Fn(PathBuf) -> Action,
    ) -> Vec<Item> {
        let mut rows = vec![
            Item::new(language.book_archive_text("Use this folder"), use_folder)
                .detail(folder_title(&self.directory)),
        ];
        if let Some(parent) = self.directory.parent() {
            rows.push(Item::new(
                language.book_archive_text("Up"),
                browse(parent.to_owned()),
            ));
        }
        rows.extend(self.subdirectories.iter().map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            Item::new(
                crate::library::display_filename(&name),
                browse(path.clone()),
            )
        }));
        rows
    }
}

impl Reader {
    pub(super) fn archive_items(&self) -> Vec<Item> {
        let language = self.controls.interface_language;
        if self.screen == Screen::BookArchive {
            return vec![
                Item::new(
                    language.book_archive_text("Move current book"),
                    Action::BookArchive(Command::Move),
                )
                .detail(folder_title(&self.archive.folder)),
                Item::new(
                    language.book_archive_text("Archive folder"),
                    Action::BookArchive(Command::ChooseFolder),
                )
                .detail(language.book_archive_text("Choose a folder in Documents")),
            ];
        }
        let Some(picker) = &self.archive.picker else {
            return Vec::new();
        };
        picker.items(language, Action::BookArchive(Command::UseFolder), |path| {
            Action::BookArchive(Command::Browse(path))
        })
    }

    pub(super) fn archive_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.current_document().ok_or("Open a book first")?;
                self.show(Screen::BookArchive);
            }
            Command::Move => {
                let (book, id) = self.current_document().ok_or("Open a book first")?;
                self.archive.plan = Some(with_storage(|| {
                    self.library.archive_plan(book, id, &self.archive.folder)
                })?);
            }
            Command::ChooseFolder => {
                let configured = self.archive.folder.clone();
                let start = with_storage(|| self.library.check_archive_folder(&configured))
                    .map_or_else(|_| PathBuf::new(), |()| configured);
                self.browse_archive_folders(start)?;
            }
            Command::Browse(path) => self.browse_archive_folders(path)?,
            Command::UseFolder => {
                let picker = self
                    .archive
                    .picker
                    .as_ref()
                    .ok_or("Choose an archive folder")?;
                let folder = picker.directory.clone();
                with_storage(|| {
                    self.library.check_archive_folder(&folder)?;
                    let bytes = serde_json::to_vec(&folder).map_err(io::Error::other)?;
                    if bytes.len() as u64 > CONFIG_LIMIT {
                        return Err(io::Error::other("Choose a folder with a shorter path."));
                    }
                    primitive_app::save(&self.archive.preference, &bytes)
                })?;
                self.archive.folder = folder;
                self.show(Screen::BookArchive);
            }
        }
        Ok(())
    }

    fn browse_archive_folders(&mut self, directory: PathBuf) -> io::Result<()> {
        let picker = FolderPicker::load(&self.library, directory)?;
        self.show(Screen::ArchiveFolder);
        self.archive.picker = Some(picker);
        Ok(())
    }

    fn confirm_archive_move(&mut self, plan: MovePlan) -> Result<()> {
        let (book, id) = self.current_document().ok_or("Open a book first")?;
        if id != plan.id || book.path != plan.source.path {
            return Err("The open book changed. Open Move to archive again.".into());
        }
        self.save()?;
        let result = with_storage(|| self.library.archive_book(&plan))?;
        let warning = self.metadata_archive_close_warning();
        self.close_book_to_library();
        let folder = plan.destination.parent().unwrap_or_else(|| Path::new(""));
        if self.browse_to(folder.to_owned()).is_ok() {
            self.reveal_book_row(&plan.destination);
        }
        self.notice = warning.or_else(|| {
            Some(if result.source_kept {
                "Archive copy saved. Source file kept.".into()
            } else {
                "Book moved to archive".into()
            })
        });
        Ok(())
    }

    pub(super) fn draw_archive_confirmation(&self, canvas: &mut Canvas) {
        if let Some(plan) = &self.archive.plan {
            let language = self.controls.interface_language;
            let filename = plan
                .source
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let source = folder_title(plan.source.path.parent().unwrap_or_else(|| Path::new("")));
            let destination =
                folder_title(plan.destination.parent().unwrap_or_else(|| Path::new("")));
            let filename = crate::library::display_filename(&filename);
            let sheet = Sheet::centered(
                VIEW,
                language.book_archive_text("Move this book?"),
                "",
                language.book_archive_text("Move book"),
            );
            sheet.draw(canvas, None);
            let cancel = sheet.button(SheetAction::Cancel);
            if !language.is_english() {
                canvas.button(cancel, language.book_archive_text("Cancel"), false, false);
            }
            let width = sheet.button(SheetAction::Confirm).right() - cancel.x;
            let height = canvas.px(36);
            let rows = [
                filename,
                format!("{} {source}", language.book_archive_text("From")),
                format!("{} {destination}", language.book_archive_text("To")),
            ];
            let top = cancel.y - 3 * height - canvas.px(12);
            for (index, label) in rows.iter().enumerate() {
                canvas.text(
                    Rect::new(cancel.x, top + index as i32 * height, width, height),
                    label,
                    CAPTION,
                );
            }
        }
    }

    pub(super) fn archive_confirmation_input(&mut self, input: Input) -> Option<Change> {
        self.archive.plan.as_ref()?;
        let action = match input {
            Input::PageBack => Some(SheetAction::Cancel),
            Input::Tap(x, y) => Sheet::centered(VIEW, "", "", "").hit(x, y),
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        let plan = self.archive.plan.take().unwrap();
        Some(match action {
            SheetAction::Cancel => Change::Partial,
            SheetAction::Confirm => match self.confirm_archive_move(plan) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
        })
    }
}
