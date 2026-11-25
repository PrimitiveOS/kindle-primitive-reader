//! Saved shortcuts into the Documents browser or an individual book.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use primitive_app::{Change, with_storage};
use primitive_ui::{Canvas, Input, Rect, Sheet, SheetAction};
use serde::{Deserialize, Serialize};

use super::gestures::Binding;
use super::{Action, Item, Reader, Screen, read_import_file};
use crate::Result;
use crate::library::{BrowseKind, BrowseList, Library, safe_path};

const MAX_SHORTCUTS: usize = 128;
const MAX_PATH_BYTES: usize = 1024;
const MAX_BYTES: u64 = 512 * 1024;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    List,
    Choose,
    FolderAction,
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::List => "Book shortcuts",
            Self::Choose => "Choose a book or folder",
            Self::FolderAction => "Folder shortcut action",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FolderAction {
    #[default]
    Browser,
    LastBook,
}

impl FolderAction {
    fn title(self) -> &'static str {
        match self {
            Self::Browser => "Browse folder",
            Self::LastBook => "Open last-read book",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Book,
    Folder,
}

#[derive(Clone, Deserialize, Serialize)]
struct Shortcut {
    id: u64,
    kind: Kind,
    path: PathBuf,
}

impl Shortcut {
    fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Documents")
    }

    fn exists(&self, library: &Library) -> bool {
        match self.kind {
            Kind::Book => library.absolute_path(&self.path).is_ok(),
            Kind::Folder => library.browse_folder(&self.path).is_ok(),
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Saved {
    last_id: u64,
    folder_action: FolderAction,
    recursive: bool,
    shortcuts: Vec<Shortcut>,
}

struct Picker {
    folder: PathBuf,
    listing: BrowseList,
}

pub(super) enum Command {
    Open,
    Choose,
    Browse(PathBuf),
    Add(Kind, PathBuf),
    FolderAction(FolderAction),
    Recursive,
    Launch(u64),
}

pub(super) struct State {
    path: PathBuf,
    saved: Saved,
    picker: Option<Picker>,
    unavailable: HashSet<u64>,
    origin: Screen,
    removing: Option<u64>,
}

fn valid_target(path: &Path, kind: Kind) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    text.len() <= MAX_PATH_BYTES
        && !text.chars().any(char::is_control)
        && (safe_path(path) || kind == Kind::Folder && text.is_empty())
}

fn load_preferences(path: &Path) -> io::Result<Saved> {
    let bytes = read_import_file(path, MAX_BYTES)?;
    let saved: Saved = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    if saved.shortcuts.len() > MAX_SHORTCUTS
        || saved.shortcuts.iter().any(|entry| {
            entry.id == 0
                || entry.id > saved.last_id
                || !ids.insert(entry.id)
                || !paths.insert(entry.path.as_path())
                || !valid_target(&entry.path, entry.kind)
        })
    {
        return Err(io::Error::other(
            "The book shortcuts file contains invalid entries",
        ));
    }
    Ok(saved)
}

impl State {
    pub(super) fn load(path: PathBuf) -> Self {
        let saved = load_preferences(&path).unwrap_or_default();
        Self {
            path,
            saved,
            picker: None,
            unavailable: HashSet::new(),
            origin: Screen::OrganizeLibrary,
            removing: None,
        }
    }

    fn lookup(&self, id: u64) -> Option<&Shortcut> {
        self.saved.shortcuts.iter().find(|entry| entry.id == id)
    }

    pub(super) fn binding_label(&self, id: u64) -> String {
        self.lookup(id).map_or_else(
            || "Removed book shortcut".into(),
            |entry| format!("Open {}", entry.name()),
        )
    }

    pub(super) fn binding_items(
        &self,
        selected: Binding,
        action: impl Fn(Binding) -> Action,
    ) -> Vec<Item> {
        self.saved
            .shortcuts
            .iter()
            .map(|entry| {
                let binding = Binding::BookShortcut(entry.id);
                let item = Item::new(self.binding_label(entry.id), action(binding));
                if binding == selected {
                    item.detail("Selected")
                } else {
                    item.detail(entry.path.to_string_lossy())
                }
            })
            .collect()
    }

    pub(super) fn recursive(&self) -> bool {
        self.saved.recursive
    }

    fn commit(&mut self, next: Saved) -> crate::Result<()> {
        primitive_app::save(&self.path, &serde_json::to_vec(&next)?)?;
        self.saved = next;
        Ok(())
    }

    pub(super) fn close_surfaces(&mut self, screen: Screen) {
        if screen != Screen::BookShortcuts(Page::Choose) {
            self.picker = None;
        }
        self.removing = None;
    }

    fn open_picker(&mut self, library: &Library, folder: PathBuf) -> Result<()> {
        let listing = with_storage(|| library.browse(&folder))?;
        self.picker = Some(Picker { folder, listing });
        Ok(())
    }

    fn refresh_unavailable(&mut self, library: &Library) {
        self.unavailable = with_storage(|| {
            Ok(self
                .saved
                .shortcuts
                .iter()
                .filter_map(|entry| (!entry.exists(library)).then_some(entry.id))
                .collect())
        })
        .unwrap_or_default();
    }

    fn add(&mut self, library: &Library, kind: Kind, path: PathBuf) -> Result<()> {
        if !valid_target(&path, kind) {
            return Err("Choose a book or folder from Documents".into());
        }
        if self.saved.shortcuts.iter().any(|entry| entry.path == path) {
            return Err("This shortcut already exists".into());
        }
        if self.saved.shortcuts.len() >= MAX_SHORTCUTS {
            return Err("Remove a shortcut before adding another".into());
        }
        let id = self
            .saved
            .last_id
            .checked_add(1)
            .ok_or("Shortcut limit reached")?;
        let entry = Shortcut { id, kind, path };
        if !with_storage(|| Ok(entry.exists(library)))? {
            return Err("This book or folder is unavailable".into());
        }
        let mut next = self.saved.clone();
        next.last_id = id;
        next.shortcuts.push(entry);
        next.shortcuts
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.commit(next)
    }

    fn remove(&mut self, id: u64) -> Result<()> {
        let mut next = self.saved.clone();
        next.shortcuts.retain(|entry| entry.id != id);
        self.commit(next)?;
        self.unavailable.remove(&id);
        Ok(())
    }
}

impl Reader {
    pub(super) fn book_shortcuts_item() -> Item {
        Item::new("Book shortcuts", Action::BookShortcut(Command::Open))
    }

    pub(super) fn book_shortcut_items(&self, page: Page) -> Vec<Item> {
        let shortcuts = &self.book_shortcuts;
        let action = |title, command| Item::new(title, Action::BookShortcut(command));
        match page {
            Page::List => {
                let mut rows = vec![
                    action("New shortcut", Command::Choose),
                    Item::new(
                        "Folder action",
                        Action::Show(Screen::BookShortcuts(Page::FolderAction)),
                    )
                    .detail(shortcuts.saved.folder_action.title()),
                ];
                rows.extend(shortcuts.saved.shortcuts.iter().map(|entry| {
                    let kind = if shortcuts.unavailable.contains(&entry.id) {
                        "Unavailable"
                    } else {
                        match entry.kind {
                            Kind::Book => "Book",
                            Kind::Folder => "Folder",
                        }
                    };
                    let folder = entry
                        .path
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .map_or_else(|| "Documents".into(), Path::to_string_lossy);
                    Item::new(
                        entry.name(),
                        Action::BookShortcut(Command::Launch(entry.id)),
                    )
                    .detail(format!("{kind} · {folder}"))
                }));
                rows
            }
            Page::Choose => {
                let Some(picker) = &shortcuts.picker else {
                    return Vec::new();
                };
                let mut rows = vec![
                    Item::new(
                        super::book_archive::folder_title(&picker.folder),
                        Action::Heading,
                    ),
                    action(
                        "Add this folder",
                        Command::Add(Kind::Folder, picker.folder.clone()),
                    ),
                ];
                if let Some(parent) = picker.folder.parent() {
                    rows.push(action("Up one folder", Command::Browse(parent.to_owned())));
                }
                rows.extend(picker.listing.entries.iter().filter_map(|entry| {
                    let (command, kind) = match entry.kind {
                        BrowseKind::Folder => (Command::Browse(entry.path.clone()), "Folder"),
                        BrowseKind::Readable(_) => {
                            (Command::Add(Kind::Book, entry.path.clone()), "Book")
                        }
                        _ => return None,
                    };
                    Some(Item::new(&entry.name, Action::BookShortcut(command)).detail(kind))
                }));
                rows
            }
            Page::FolderAction => {
                let selected = shortcuts.saved.folder_action;
                let mut rows = [FolderAction::Browser, FolderAction::LastBook]
                    .into_iter()
                    .map(|folder_action| {
                        let row =
                            action(folder_action.title(), Command::FolderAction(folder_action));
                        if folder_action == selected {
                            row.detail("Selected")
                        } else {
                            row
                        }
                    })
                    .collect::<Vec<_>>();
                if selected == FolderAction::LastBook {
                    rows.push(action("Include subfolders", Command::Recursive));
                }
                rows
            }
        }
    }

    pub(super) fn book_shortcut_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.book_shortcuts.origin = match self.screen {
                    Screen::OrganizeLibrary => Screen::OrganizeLibrary,
                    _ if self.open_book_id().is_some() => self.surface_screen(),
                    _ => Screen::Library,
                };
                self.show(Screen::BookShortcuts(Page::List));
                self.book_shortcuts.refresh_unavailable(&self.library);
            }
            Command::Choose => {
                let current = self.browser.directory();
                let folder = with_storage(|| {
                    Ok(if self.library.browse_folder(current).is_ok() {
                        current.to_owned()
                    } else {
                        PathBuf::new()
                    })
                })?;
                self.book_shortcuts.open_picker(&self.library, folder)?;
                self.show(Screen::BookShortcuts(Page::Choose));
            }
            Command::Browse(folder) => {
                self.book_shortcuts.open_picker(&self.library, folder)?;
                self.show(Screen::BookShortcuts(Page::Choose));
            }
            Command::Add(kind, path) => {
                self.book_shortcuts.add(&self.library, kind, path)?;
                self.show(Screen::BookShortcuts(Page::List));
                self.notice = None;
            }
            Command::FolderAction(folder_action) => {
                let mut next = self.book_shortcuts.saved.clone();
                next.folder_action = folder_action;
                self.book_shortcuts.commit(next)?;
            }
            Command::Recursive => {
                let mut next = self.book_shortcuts.saved.clone();
                next.recursive = !next.recursive;
                self.book_shortcuts.commit(next)?;
            }
            Command::Launch(id) => self.open_book_shortcut(id)?,
        }
        Ok(())
    }

    fn open_book_shortcut(&mut self, id: u64) -> Result<()> {
        let item = self
            .book_shortcuts
            .lookup(id)
            .ok_or("This shortcut was removed")?;
        let kind = item.kind;
        let path = item.path.clone();
        if !with_storage(|| Ok(item.exists(&self.library)))? {
            self.book_shortcuts.unavailable.insert(id);
            return Err("This shortcut's book or folder is unavailable".into());
        }
        self.book_shortcuts.unavailable.remove(&id);
        if kind == Kind::Book {
            let entry = with_storage(|| self.library.resolve(&path))?;
            return self.open_book(entry);
        }
        if self.book_shortcuts.saved.folder_action == FolderAction::Browser {
            return self.open_shortcut_folder(path);
        }
        let recursive = self.book_shortcuts.recursive();
        let entry = with_storage(|| {
            Ok(self
                .library
                .recently_read()
                .filter_map(|book| {
                    let parent = book.path.parent()?;
                    if parent != path && !(recursive && parent.starts_with(&path)) {
                        return None;
                    }
                    let entry = self.library.resolve(&book.path).ok()?;
                    (self.library.identify(&entry).ok()? == book.id).then_some(entry)
                })
                .next())
        })?
        .ok_or("No previously read book is available in this folder")?;
        self.open_book(entry)
    }

    pub(super) fn back_from_book_shortcuts(&mut self, page: Page) {
        self.show(match page {
            Page::List => self.book_shortcuts.origin,
            _ => Screen::BookShortcuts(Page::List),
        });
    }

    pub(super) fn confirm_remove_book_shortcut(&mut self, id: u64) {
        self.book_shortcuts.removing = Some(id);
        self.notice = None;
    }

    fn book_shortcut_removal_sheet(&self) -> Option<Sheet<'_>> {
        let entry = self.book_shortcuts.lookup(self.book_shortcuts.removing?)?;
        let (width, height) = self.reading_viewport().dimensions();
        Some(Sheet::centered(
            Rect::new(0, 0, width as i32, height as i32),
            "Remove this shortcut?",
            entry.name(),
            "Remove",
        ))
    }

    pub(super) fn draw_remove_book_shortcut(&self, canvas: &mut Canvas) {
        if let Some(sheet) = self.book_shortcut_removal_sheet() {
            sheet.draw(canvas, None);
        }
    }

    pub(super) fn book_shortcut_removal_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        let sheet = self.book_shortcut_removal_sheet()?;
        Some(match input {
            Input::Tap(x, y) if sheet.hit(x, y).is_some() => Some((x, y)),
            _ => None,
        })
    }

    pub(super) fn book_shortcut_removal_input(&mut self, input: Input) -> Option<Change> {
        let sheet = self.book_shortcut_removal_sheet()?;
        let action = match input {
            Input::PageBack => Some(SheetAction::Cancel),
            Input::Tap(x, y) => sheet.hit(x, y),
            Input::Sleep => {
                self.book_shortcuts.removing = None;
                return None;
            }
            _ => None,
        };
        let Some(action) = action else {
            return Some(Change::None);
        };
        let id = self.book_shortcuts.removing.take()?;
        Some(match action {
            SheetAction::Cancel => Change::Partial,
            SheetAction::Confirm => match self.book_shortcuts.remove(id) {
                Ok(()) => {
                    self.list_page = 0;
                    Change::Partial
                }
                Err(error) => self.report(error),
            },
        })
    }
}
