use std::io;
use std::path::{Path, PathBuf};

use primitive_ui::{BLACK, HAIRLINE, Icon, PAPER, SECONDARY, back_button, chevron, header};

use super::{
    Action, BODY, CAPTION, Canvas, Change, HEIGHT, Input, Item, LIST_X, NEXT, PREV, ROW_HEIGHT,
    Reader, Rect, Result, Screen, WIDTH, library_book_item, with_storage,
};
use crate::library::{BrowseEntry, BrowseKind, BrowseList, FolderDirection};

pub(super) const LIBRARY_LIST_Y: i32 = 306;
const FILE_ROWS: usize = 10;
const UP: Rect = Rect::new(LIST_X, 150, 148, 54);
const TOOLBAR: [(&str, Screen); 4] = [
    ("Find", Screen::LibraryFind),
    ("Organize", Screen::OrganizeLibrary),
    ("Controls", Screen::ReadingControls),
    ("Reading data", Screen::ReadingData),
];
const FOLDER: Icon = Icon {
    stroke: &["M2 6 H9 L11 8 H22 V20 H2 Z"],
    fill: &[],
};
const FILE: Icon = Icon {
    stroke: &["M5 2 H14 L19 7 V22 H5 Z M14 2 V7 H19 M8 12 H16 M8 16 H16"],
    fill: &[],
};

pub(super) struct FileBrowser {
    directory: PathBuf,
    listing: BrowseList,
    page: usize,
}

impl FileBrowser {
    pub(super) fn new(listing: BrowseList) -> Self {
        Self {
            directory: PathBuf::new(),
            listing,
            page: 0,
        }
    }

    pub(super) fn first_page(&mut self) {
        self.page = 0;
    }
}

impl Reader {
    fn read_browser(&self, directory: PathBuf) -> io::Result<FileBrowser> {
        let listing = with_storage(|| self.library.browse(&directory))?;
        Ok(FileBrowser {
            directory,
            listing,
            page: 0,
        })
    }

    pub(super) fn browse_to(&mut self, directory: PathBuf) -> io::Result<()> {
        self.browser = self.read_browser(directory)?;
        self.show(Screen::Library);
        Ok(())
    }

    fn read_book_folder(&self, path: &Path) -> io::Result<FileBrowser> {
        let directory = path.parent().unwrap_or_else(|| Path::new("")).to_owned();
        self.read_browser(directory)
    }

    pub(super) fn current_book_folder(&mut self) -> Result<()> {
        let path = self
            .current_document()
            .ok_or("No open book")?
            .0
            .library_path()
            .to_owned();
        let browser = self.read_book_folder(&path)?;
        self.activate(Action::Show(Screen::Library))?;
        self.browser = browser;
        self.reveal_book_row(&path);
        Ok(())
    }

    pub(super) fn open_folder_file(&mut self, direction: FolderDirection) -> Result<()> {
        let (book, _) = self.current_document().ok_or("No open book")?;
        let adjacent = with_storage(|| self.library.adjacent_book_in_folder(book, direction))?
            .ok_or(match direction {
                FolderDirection::Previous => "There is no previous unfinished file in this folder",
                FolderDirection::Next => "There is no next unfinished file in this folder",
            })?;
        let origin = self.surface_screen();
        primitive_app::App::save(self)?;
        self.open_book(adjacent)?;
        if self.screen == Screen::PdfPassword {
            self.folder_password_return = Some(origin);
        }
        Ok(())
    }

    pub(super) fn browse_from_book(&mut self, path: &Path) -> io::Result<()> {
        self.browser = self.read_book_folder(path)?;
        self.show(Screen::Library);
        self.reveal_book_row(path);
        Ok(())
    }

    fn reveal_book_row(&mut self, path: &Path) {
        self.library_query.clear();
        if let Some(index) = self
            .browser
            .listing
            .entries
            .iter()
            .position(|entry| entry.path == path)
        {
            let continue_row =
                self.show_continue() && self.library.recently_read().next().is_some();
            self.browser.page = (index + usize::from(continue_row)) / FILE_ROWS;
        }
    }

    pub(super) fn refresh_browser(&mut self) -> io::Result<()> {
        let mut directory = self.browser.directory.clone();
        let listing = with_storage(|| {
            loop {
                match self.library.browse(&directory) {
                    Ok(listing) => break Ok(listing),
                    Err(_) if directory.pop() => continue,
                    Err(error) => break Err(error),
                }
            }
        })?;
        if directory != self.browser.directory {
            self.browser.first_page();
        }
        self.browser.directory = directory;
        self.browser.listing = listing;
        self.browser.page = self
            .browser
            .page
            .min(self.library_item_count().saturating_sub(1) / FILE_ROWS);
        Ok(())
    }

    fn show_continue(&self) -> bool {
        self.browser.directory.as_os_str().is_empty() && self.library_query.is_empty()
    }

    pub(super) fn library_items(&self) -> impl Iterator<Item = Item> + '_ {
        let searching = !self.library_query.is_empty();
        let continue_reading = self
            .show_continue()
            .then(|| {
                self.library.recently_read().next().map(|book| {
                    Item::new("Continue reading", Action::Continue).detail(format!(
                        "{} · {}",
                        book.title,
                        file_type(&book.path)
                    ))
                })
            })
            .flatten();
        let folder = (!searching)
            .then_some(&self.browser.listing.entries)
            .into_iter()
            .flatten()
            .map(file_item);
        let search = searching
            .then(|| self.library.matching(&self.library_query))
            .into_iter()
            .flatten()
            .cloned()
            .map(library_book_item);
        continue_reading.into_iter().chain(folder).chain(search)
    }

    fn library_item_count(&self) -> usize {
        if self.library_query.is_empty() {
            self.browser.listing.entries.len()
                + usize::from(self.show_continue() && self.library.recently_read().next().is_some())
        } else {
            self.library.matching(&self.library_query).count()
        }
    }

    fn up_action(&self) -> Option<(Action, &'static str)> {
        if !self.library_query.is_empty() {
            Some((Action::ClearLibraryFilter, "Clear"))
        } else {
            self.browser
                .directory
                .parent()
                .map(|parent| (Action::Browse(parent.to_owned()), "Up"))
        }
    }

    pub(super) fn draw_library(&self, canvas: &mut Canvas) {
        header(canvas, "Reader", "Exit");
        let path = if !self.library_query.is_empty() {
            format!("Search results · {}", self.library_query)
        } else {
            let mut title = String::from("Documents");
            for part in &self.browser.directory {
                title.push_str(" / ");
                title.push_str(&crate::library::display_filename(&part.to_string_lossy()));
            }
            title
        };
        let mut path_x = LIST_X;
        if let Some((_, label)) = self.up_action() {
            canvas.button(UP, label, false, false);
            path_x = UP.right() + 22;
        }
        canvas.text(
            Rect::new(path_x, UP.y, WIDTH as i32 - LIST_X - path_x, UP.h),
            &path,
            BODY.bold(),
        );
        for (index, (label, _)) in TOOLBAR.iter().enumerate() {
            canvas.button(tool_button(index), label, false, false);
        }

        let first = self.browser.page * FILE_ROWS;
        let rows: Vec<_> = self.library_items().skip(first).take(FILE_ROWS).collect();
        if rows.is_empty() {
            canvas.text(
                Rect::new(LIST_X, LIBRARY_LIST_Y + 38, WIDTH as i32 - 2 * LIST_X, 82),
                if self.library_query.is_empty() {
                    "This folder is empty."
                } else {
                    "No documents match your search."
                },
                CAPTION.center(),
            );
        } else {
            let mut card = file_row(0);
            card.h *= rows.len() as i32;
            canvas.card(card);
            for (index, item) in rows.iter().enumerate() {
                let row = file_row(index);
                draw_file(canvas, row, item);
                if index + 1 < rows.len() {
                    canvas.fill(
                        Rect::new(row.x + 26, row.bottom() - 1, row.w - 52, 1),
                        HAIRLINE,
                    );
                }
            }
        }

        let count = self.library_item_count();
        if self.browser.page > 0 {
            canvas.button(PREV, "Previous", false, false);
        }
        if first + rows.len() < count {
            canvas.button(NEXT, "Next", false, false);
        }
        let page_label = (count > FILE_ROWS)
            .then(|| format!("{} / {}", self.browser.page + 1, count.div_ceil(FILE_ROWS)));
        let limit_notice = (self.library_query.is_empty() && self.browser.listing.truncated)
            .then_some("Showing part of this folder");
        if let Some(text) = self
            .notice
            .as_deref()
            .or(limit_notice)
            .or(page_label.as_deref())
        {
            canvas.text(
                Rect::new(310, HEIGHT as i32 - 104, WIDTH as i32 - 620, 80),
                text,
                CAPTION.center().lines(2),
            );
        }
    }

    pub(super) fn library_input(&mut self, input: Input) -> Change {
        let apply = |reader: &mut Reader, action| match reader.activate(action) {
            Ok(()) => Change::Partial,
            Err(error) => reader.report(error),
        };
        let direction = match input {
            Input::Tap(x, y) if back_button(WIDTH as i32).contains(x, y) => return self.exit(),
            Input::Tap(x, y) if UP.contains(x, y) => {
                return self
                    .up_action()
                    .map_or(Change::None, |(action, _)| apply(self, action));
            }
            Input::PageBack if self.browser.page == 0 => {
                return self
                    .up_action()
                    .map_or(Change::None, |(action, _)| apply(self, action));
            }
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Swipe { dx, dy, .. } if dy.abs() > dx.abs() => {
                if dy < 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Tap(x, y) if PREV.contains(x, y) => -1,
            Input::Tap(x, y) if NEXT.contains(x, y) => 1,
            Input::Tap(x, y) | Input::LongTap(x, y) => {
                if matches!(input, Input::Tap(..))
                    && let Some((_, (_, screen))) = TOOLBAR
                        .iter()
                        .enumerate()
                        .find(|(i, _)| tool_button(*i).contains(x, y))
                {
                    return apply(self, Action::Show(*screen));
                }
                let action = (0..FILE_ROWS)
                    .find(|&slot| file_row(slot).contains(x, y))
                    .and_then(|slot| {
                        self.library_items()
                            .nth(self.browser.page * FILE_ROWS + slot)
                    })
                    .map(|item| item.action);
                if matches!(input, Input::LongTap(..)) {
                    return match action {
                        Some(Action::Open(book)) => match self.organize_book(book, Screen::Library)
                        {
                            Ok(()) => Change::Partial,
                            Err(error) => self.report(error),
                        },
                        _ => Change::None,
                    };
                }
                return match action {
                    Some(Action::Heading) | None => Change::None,
                    Some(action) => apply(self, action),
                };
            }
            _ => return Change::None,
        };
        let Some(page) = self
            .browser
            .page
            .checked_add_signed(direction)
            .filter(|page| page * FILE_ROWS < self.library_item_count())
        else {
            return Change::None;
        };
        self.browser.page = page;
        self.notice = None;
        Change::Partial
    }
}

fn tool_button(index: usize) -> Rect {
    let width = (WIDTH as i32 - 2 * LIST_X - 3 * 18) / 4;
    Rect::new(LIST_X + index as i32 * (width + 18), 222, width, 62)
}

fn file_row(slot: usize) -> Rect {
    Rect::new(
        LIST_X,
        LIBRARY_LIST_Y + slot as i32 * ROW_HEIGHT,
        WIDTH as i32 - 2 * LIST_X,
        ROW_HEIGHT,
    )
}

fn file_item(entry: &BrowseEntry) -> Item {
    let (action, detail) = match &entry.kind {
        BrowseKind::Folder => (Action::Browse(entry.path.clone()), "Folder".into()),
        BrowseKind::Readable(book) => (
            Action::Open(book.clone()),
            file_detail(&entry.path, book.size, None),
        ),
        BrowseKind::KindleProtected { size } => (
            Action::Heading,
            file_detail(
                &entry.path,
                *size,
                Some("Protected · Open in Kindle Library"),
            ),
        ),
        BrowseKind::Unavailable { size, too_large } => (
            Action::Heading,
            file_detail(
                &entry.path,
                *size,
                Some(if *too_large {
                    "Too large for Reader"
                } else if entry
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("kfx"))
                {
                    "Open in Kindle Library"
                } else {
                    "Not supported in Reader"
                }),
            ),
        ),
    };
    let mut item = if matches!(entry.kind, BrowseKind::Readable(_))
        && crate::epub_members::is_folder_book(&entry.path)
    {
        Item::new("Read this book", action).detail("EPUB · Extracted publication")
    } else {
        Item::new(&entry.name, action).detail(detail)
    };
    item.folder = matches!(entry.kind, BrowseKind::Folder);
    item
}

pub(super) fn file_type(path: &Path) -> String {
    if crate::epub_members::is_folder_book(path) {
        return "EPUB".into();
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty() && extension.chars().count() <= 8)
        .map_or_else(|| "File".into(), str::to_uppercase)
}

fn file_detail(path: &Path, size: u64, status: Option<&str>) -> String {
    let size = if size < 1024 {
        format!("{size} B")
    } else if size < 1_048_576 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1_073_741_824 {
        format!("{:.1} MB", size as f64 / 1_048_576.0)
    } else {
        format!("{:.1} GB", size as f64 / 1_073_741_824.0)
    };
    let mut detail = format!("{} · {size}", file_type(path));
    if let Some(status) = status {
        detail.push_str(" · ");
        detail.push_str(status);
    }
    detail
}

fn draw_file(canvas: &mut Canvas, row: Rect, item: &Item) {
    let available = !matches!(item.action, Action::Heading);
    let pressed = available && canvas.control_pressed(row, false);
    if pressed {
        canvas.panel(row.inset(5), 16, 2, BLACK);
    }
    let ink = if pressed {
        PAPER
    } else if available {
        BLACK
    } else {
        SECONDARY
    };
    let icon = Rect::new(row.x + 29, row.y + (row.h - 52) / 2, 52, 52);
    canvas.icon(icon, if item.folder { FOLDER } else { FILE }, ink);
    let start = row.x + 110;
    let width = row.right() - start - 65;
    canvas.text(
        Rect::new(start, row.y + 16, width, 48),
        &item.title,
        BODY.bold().ink(ink),
    );
    if let Some(detail) = &item.detail {
        canvas.text(
            Rect::new(start, row.y + 66, width, 41),
            detail,
            if pressed { CAPTION.ink(220) } else { CAPTION },
        );
    }
    if available {
        chevron(
            canvas,
            Rect::new(row.right() - 46, row.y + (row.h - 30) / 2, 30, 30),
            ink,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen, tap_reading_control};
    use primitive_app::{App, Lifecycle};
    use std::fs;

    fn app(fixture: &Fixture) -> Reader {
        Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap()
    }

    fn visible_file(app: &Reader, path: &Path) -> bool {
        app.library_items()
            .skip(app.browser.page * FILE_ROWS)
            .take(FILE_ROWS)
            .any(|item| match item.action {
                Action::Open(book) => book.library_path() == path,
                Action::Browse(folder) => folder == path,
                _ => false,
            })
    }

    fn assert_button_feedback(reader: &Reader, label: &str) {
        let rect = reader.reading_button(label);
        let paths: Vec<_> = reader
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        let normal = primitive_app::frame(reader, &mut canvas, false);
        let pressed = Lifecycle::default()
            .press_feedback(
                reader,
                &mut canvas,
                Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32),
                false,
            )
            .unwrap();
        assert!(normal != pressed);
        assert!(
            normal
                .iter()
                .zip(&pressed)
                .enumerate()
                .all(|(index, (a, b))| {
                    a == b
                        || rect.contains(
                            (index % WIDTH as usize) as f32,
                            (index / WIDTH as usize) as f32,
                        )
                })
        );
    }

    #[test]
    fn current_book_folder_reveals_a_nested_resumed_epub_and_saves_its_bookmark() {
        let fixture = Fixture::new();
        let shelf = Path::new("Series/Harbor Shelf");
        let folder = fixture.root.join(shelf);
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir(fixture.root.join("Other books")).unwrap();
        for volume in (1..=11).chain(std::iter::once(13)) {
            fs::write(
                folder.join(format!("Volume {volume}.txt")),
                "The same harbor at dusk.",
            )
            .unwrap();
        }
        let relative = shelf.join("Volume 12.epub");
        let book = fixture.root.join(&relative);
        fs::copy(&fixture.book, &book).unwrap();

        let mut reader = app(&fixture);
        reader.open_path(&book).unwrap();
        reader.follow_reflow_page(1, 0).unwrap();
        let position = reader.current_position().unwrap();
        let id = reader.open_book_id().unwrap();
        reader.activate(Action::Show(Screen::Library)).unwrap();
        reader.browse_to(PathBuf::from("Other books")).unwrap();
        reader.library_query = "unrelated".into();
        reader.activate(Action::Continue).unwrap();
        assert_eq!(reader.library_query, "unrelated");
        assert_eq!(reader.current_position(), Some(position.clone()));
        reader.activate(Action::ToggleBookmark).unwrap();
        assert!(reader.dirty);

        reader.open_reading_edge(true, 630.0);
        assert_eq!(tap_reading_control(&mut reader, "Library"), Change::Partial);
        assert_button_feedback(&reader, "Current book folder");
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_BOOK_FOLDER_MENU_SCREENSHOT",
            false,
        );
        assert_eq!(
            tap_reading_control(&mut reader, "Current book folder"),
            Change::Partial
        );
        assert_eq!(reader.screen, Screen::Library);
        assert!(reader.open_book_id().is_none());
        assert_eq!(reader.browser.directory, shelf);
        assert_eq!(reader.browser.page, 1);
        assert!(reader.library_query.is_empty());
        assert!(visible_file(&reader, &relative));
        capture_test_screen(
            &reader,
            &reader.fonts,
            "READER_BOOK_FOLDER_SCREENSHOT",
            false,
        );

        let mut reopened = app(&fixture);
        assert_eq!(
            reopened.library.state(id).unwrap().position.as_ref(),
            Some(&position)
        );
        assert_eq!(reopened.library.state(id).unwrap().bookmarks.len(), 1);
        reopened.activate(Action::Continue).unwrap();
        assert_eq!(reopened.current_position(), Some(position));
    }

    #[test]
    fn current_book_folder_handles_fixed_pdf_and_an_extracted_publication() {
        let fixture = Fixture::new();
        let folder = fixture.root.join("Reference");
        let unpacked = folder.join("Coastal notes.epub");
        fs::create_dir_all(&unpacked).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(&fixture.book).unwrap()).unwrap();
        for index in 0..archive.len() {
            let mut member = archive.by_index(index).unwrap();
            let path = unpacked.join(member.enclosed_name().unwrap());
            if member.is_dir() {
                fs::create_dir_all(path).unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                io::copy(&mut member, &mut fs::File::create(path).unwrap()).unwrap();
            }
        }
        let pdf = folder.join("Harbor charts.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let mut reader = app(&fixture);

        reader.open_path(&pdf).unwrap();
        assert!(reader.fixed.is_some());
        reader.activate(Action::CurrentBookFolder).unwrap();
        assert_eq!(reader.browser.directory, Path::new("Reference"));
        assert!(visible_file(
            &reader,
            Path::new("Reference/Harbor charts.pdf")
        ));

        reader.open_path(&unpacked.join("mimetype")).unwrap();
        assert!(reader.active.is_some());
        reader
            .activate(Action::OpenFolderFile(FolderDirection::Next))
            .unwrap();
        assert!(reader.fixed.is_some());
        reader
            .activate(Action::OpenFolderFile(FolderDirection::Previous))
            .unwrap();
        assert!(reader.active.is_some());
        assert_eq!(
            reader.current_document().unwrap().0.path,
            Path::new("Reference/Coastal notes.epub/mimetype")
        );
        reader.activate(Action::CurrentBookFolder).unwrap();
        assert_eq!(reader.browser.directory, Path::new("Reference"));
        assert!(visible_file(
            &reader,
            Path::new("Reference/Coastal notes.epub")
        ));
    }

    #[test]
    fn current_book_folder_leaves_the_book_and_picker_intact_on_list_or_save_failure() {
        let fixture = Fixture::new();
        let folder = fixture.root.join("Series");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("Field Notes.epub");
        fs::copy(&fixture.book, &file).unwrap();
        let mut reader = app(&fixture);
        reader.open_path(&file).unwrap();
        let id = reader.open_book_id();
        let position = reader.current_position();
        let absent = fixture.root.join("Moved");
        fs::rename(&folder, &absent).unwrap();

        assert!(reader.activate(Action::CurrentBookFolder).is_err());
        assert_eq!(reader.screen, Screen::Book);
        assert_eq!(reader.open_book_id(), id);
        assert_eq!(reader.current_position(), position);
        assert!(reader.browser.directory.as_os_str().is_empty());

        fs::rename(&absent, &folder).unwrap();
        reader.save().unwrap();
        let backup = fixture.root.join("saved-library.json");
        fs::rename(&fixture.state, &backup).unwrap();
        fs::create_dir(&fixture.state).unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        assert!(reader.activate(Action::CurrentBookFolder).is_err());
        assert_eq!(reader.screen, Screen::Book);
        assert_eq!(reader.open_book_id(), id);
        assert_eq!(reader.current_position(), position);
        assert!(reader.browser.directory.as_os_str().is_empty());
    }

    #[test]
    fn adjacent_folder_files_resume_epub_and_pdf_and_stop_at_each_boundary() {
        use super::super::ReadingStatus;

        let fixture = Fixture::new();
        let shelf = Path::new("Series/Harbor Shelf");
        let folder = fixture.root.join(shelf);
        let other_folder = fixture.root.join("Other");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir(&other_folder).unwrap();
        let epub = folder.join("Volume 2.epub");
        let pdf = folder.join("Volume 10.pdf");
        let finished = folder.join("Volume 3.txt");
        let unrelated = other_folder.join("Tidal notes.txt");
        fs::copy(crate::test_fixture("long-chapter.epub"), &epub).unwrap();
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        fs::write(&finished, "Completed old").unwrap();
        fs::write(folder.join("Volume 4.bin"), "Unsupported").unwrap();
        fs::write(folder.join("Volume 5.kfx"), "Opens in Kindle Library").unwrap();
        fs::write(&unrelated, "An unrelated reading record.").unwrap();

        let mut reader = app(&fixture);
        let completed = reader.library.resolve(&finished).unwrap();
        let completed_id = reader.library.identify(&completed).unwrap();
        reader
            .library
            .track(&completed, completed_id)
            .set_status(ReadingStatus::Finished);
        reader.mark_book_changed();

        reader.open_path(&pdf).unwrap();
        reader.follow_fixed(1, None).unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        let pdf_position = reader.current_position().unwrap();
        let pdf_id = reader.open_book_id().unwrap();
        reader.open_path(&unrelated).unwrap();
        let unrelated_id = reader.open_book_id().unwrap();
        reader.browse_to(PathBuf::from("Other")).unwrap();
        reader.library_query = "tidal".into();

        reader.open_path(&epub).unwrap();
        reader.follow_reflow_page(0, 2).unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        let epub_position = reader.current_position().unwrap();
        let epub_id = reader.open_book_id().unwrap();
        assert_eq!(
            reader
                .library
                .recently_read()
                .find(|book| book.id != epub_id)
                .unwrap()
                .id,
            unrelated_id
        );
        assert_eq!(
            reader
                .library
                .next_book_in_folder(reader.current_document().unwrap().0)
                .unwrap()
                .unwrap()
                .path,
            shelf.join("Volume 3.txt")
        );
        let open_previous = Action::OpenFolderFile(FolderDirection::Previous);
        let open_next = Action::OpenFolderFile(FolderDirection::Next);
        let error = reader.activate(open_previous).unwrap_err().to_string();
        assert!(error.contains("no previous"));
        assert_eq!(reader.current_position(), Some(epub_position.clone()));

        reader.open_reading_edge(true, 630.0);
        assert_eq!(tap_reading_control(&mut reader, "Library"), Change::Partial);
        capture_test_screen(&reader, &reader.fonts, "READER_ADJACENT_FILE_MENU", false);
        assert_button_feedback(&reader, "Previous file in folder");
        assert_button_feedback(&reader, "Next file in folder");
        assert_eq!(
            tap_reading_control(&mut reader, "Next file in folder"),
            Change::Partial
        );
        assert_eq!(reader.open_book_id(), Some(pdf_id));
        assert_eq!(reader.current_position(), Some(pdf_position.clone()));
        assert!(reader.page_is_bookmarked());
        let error = reader.activate(open_next).unwrap_err().to_string();
        assert!(error.contains("no next"));
        assert_eq!(reader.current_position(), Some(pdf_position.clone()));

        reader.open_reading_edge(true, 630.0);
        assert_eq!(tap_reading_control(&mut reader, "Library"), Change::Partial);
        assert_eq!(
            tap_reading_control(&mut reader, "Previous file in folder"),
            Change::Partial
        );
        assert_eq!(reader.open_book_id(), Some(epub_id));
        assert_eq!(reader.current_position(), Some(epub_position.clone()));
        assert!(reader.page_is_bookmarked());
        capture_test_screen(&reader, &reader.fonts, "READER_ADJACENT_FILE_TARGET", false);
        assert_eq!(reader.browser.directory, Path::new("Other"));
        assert_eq!(reader.library_query, "tidal");
        let mut recovered = app(&fixture);
        recovered.activate(Action::OpenTracked(pdf_id)).unwrap();
        assert_eq!(recovered.current_position(), Some(pdf_position));
        assert!(recovered.page_is_bookmarked());
        recovered.activate(Action::OpenTracked(epub_id)).unwrap();
        assert_eq!(recovered.current_position(), Some(epub_position));
        assert!(recovered.page_is_bookmarked());
        assert_eq!(
            recovered.library.state(completed_id).unwrap().status(),
            ReadingStatus::Finished
        );
        assert_eq!(
            recovered.library.state(epub_id).unwrap().status(),
            ReadingStatus::Reading
        );
        assert_eq!(
            recovered.library.state(pdf_id).unwrap().status(),
            ReadingStatus::Reading
        );

        fs::write(&finished, "Unread volume").unwrap();
        recovered
            .activate(Action::OpenFolderFile(FolderDirection::Next))
            .unwrap();
        assert_eq!(
            recovered.current_document().unwrap().0.path,
            shelf.join("Volume 3.txt")
        );
        assert_ne!(recovered.open_book_id(), Some(completed_id));
    }

    #[test]
    fn adjacent_folder_files_cancel_wrong_pdf_password_and_can_retry() {
        use primitive_ui::{Key, Layout};

        let fixture = Fixture::new();
        let folder = fixture.root.join("Readings");
        fs::create_dir(&folder).unwrap();
        let epub = folder.join("Volume 1.epub");
        let locked = folder.join("Volume 2.pdf");
        fs::copy(&fixture.book, &epub).unwrap();
        fs::copy(crate::test_fixture("password-required.pdf"), &locked).unwrap();
        let mut reader = app(&fixture);
        reader.open_path(&epub).unwrap();
        reader.follow_reflow_page(1, 0).unwrap();
        let origin = reader.current_position().unwrap();
        let id = reader.open_book_id().unwrap();
        let next = || Action::OpenFolderFile(FolderDirection::Next);
        reader.activate(next()).unwrap();
        assert_eq!(reader.screen, Screen::PdfPassword);
        assert_eq!(reader.open_book_id(), Some(id));
        let submit = |reader: &mut Reader, password: &str| {
            reader.editor.text = password.chars().collect();
            reader.editor.caret = reader.editor.text.len();
            let (_, go) = Layout::new(WIDTH, HEIGHT, 4, 5)
                .keyboard_keys_with(reader.editor.layer, "Open")
                .into_iter()
                .find(|(key, _)| key.key == Key::Go)
                .unwrap();
            assert_eq!(
                reader.input(Input::Tap(
                    (go.x + go.w / 2) as f32,
                    (go.y + go.h / 2) as f32
                )),
                Change::Partial
            );
        };
        submit(&mut reader, "incorrect");
        assert_eq!(reader.screen, Screen::PdfPassword);
        assert!(
            reader
                .notice
                .as_deref()
                .unwrap()
                .contains("Password rejected")
        );
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(reader.screen, Screen::Book);
        assert_eq!(reader.open_book_id(), Some(id));
        assert_eq!(reader.current_position(), Some(origin.clone()));
        assert!(reader.locked.is_none());
        assert!(reader.editor.string().is_empty());

        reader.activate(next()).unwrap();
        submit(&mut reader, "reader-test");
        assert_eq!(reader.screen, Screen::Fixed);
        assert!(reader.fixed.as_ref().unwrap().reading.is_encrypted_pdf());
        assert!(reader.folder_password_return.is_none());
        reader
            .activate(Action::OpenFolderFile(FolderDirection::Previous))
            .unwrap();
        assert_eq!(reader.current_position(), Some(origin));
        assert_eq!(reader.open_book_id(), Some(id));

        reader.open_path(&locked).unwrap();
        assert_eq!(reader.screen, Screen::PdfPassword);
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(reader.screen, Screen::Library);
    }
}
