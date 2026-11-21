use std::io;
use std::path::{Path, PathBuf};

use primitive_ui::{BLACK, HAIRLINE, Icon, PAPER, SECONDARY, back_button, chevron, header};

use super::{
    Action, BODY, CAPTION, Canvas, Change, HEIGHT, Input, Item, LIST_X, NEXT, PREV, ROW_HEIGHT,
    Reader, Rect, Screen, WIDTH, library_book_item, with_storage,
};
use crate::library::{BrowseEntry, BrowseKind, BrowseList};

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
    pub(super) fn browse_to(&mut self, directory: PathBuf) -> io::Result<()> {
        let listing = with_storage(|| self.library.browse(&directory))?;
        self.browser = FileBrowser {
            directory,
            listing,
            page: 0,
        };
        self.show(Screen::Library);
        Ok(())
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
