use std::io;
use std::path::{Path, PathBuf};

use primitive_ui::{BLACK, HAIRLINE, Icon, PAPER, SECONDARY, chevron};

use super::{
    Action, BODY, CAPTION, Canvas, Change, Input, Item, LIST_X, ROW_HEIGHT, Reader, Rect, Result,
    Screen, library_book_item,
    navigation_layout::{NavigationLayout, pager_destination},
    with_storage,
};
use crate::library::{
    BrowseEntry, BrowseKind, BrowseList, FolderDirection, FolderOrder, FolderProgress, FolderSort,
};

pub(super) const LIBRARY_LIST_Y: i32 = 306;
const MAX_FILE_ROWS: usize = 10;
const UP: Rect = Rect::new(LIST_X, 150, 148, 54);
const TOOLBAR: [(&str, Screen, i32); 5] = [
    ("Find", Screen::LibraryFind, 20),
    ("Organize", Screen::OrganizeLibrary, 20),
    ("Controls", Screen::ReadingControls, 20),
    ("Reading data", Screen::ReadingData, 24),
    ("Sort", Screen::FolderOrder, 16),
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
    first_item: usize,
}

impl FileBrowser {
    pub(super) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(super) fn new(listing: BrowseList) -> Self {
        Self {
            directory: PathBuf::new(),
            listing,
            first_item: 0,
        }
    }

    fn page(&self, layout: NavigationLayout) -> usize {
        self.first_item / file_rows(layout)
    }

    pub(super) fn first_page(&mut self) {
        self.first_item = 0;
    }
}

impl Reader {
    fn read_browser(&self, directory: PathBuf) -> io::Result<FileBrowser> {
        let listing = with_storage(|| {
            self.library
                .browse_with_order(&directory, self.controls.folder_order)
        })?;
        Ok(FileBrowser {
            directory,
            listing,
            first_item: 0,
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
        self.request_close(super::profile_panel::on_close::AfterClose::CurrentFolder {
            browser,
            reveal: Some(path),
        })?;
        Ok(())
    }

    pub(super) fn open_shortcut_folder(&mut self, directory: PathBuf) -> Result<()> {
        let browser = self.read_browser(directory)?;
        self.request_close(super::profile_panel::on_close::AfterClose::CurrentFolder {
            browser,
            reveal: None,
        })?;
        Ok(())
    }

    pub(super) fn open_folder_file(&mut self, direction: FolderDirection) -> Result<()> {
        let (book, _) = self.current_document().ok_or("No open book")?;
        let adjacent = with_storage(|| {
            self.library
                .adjacent_book_in_folder(book, direction, self.controls.folder_order)
        })?
        .ok_or(match direction {
            FolderDirection::Previous => "There is no previous unfinished file in this folder",
            FolderDirection::Next => "There is no next unfinished file in this folder",
        })?;
        let origin = self.surface_screen();
        primitive_app::App::save(self)?;
        self.request_close(super::profile_panel::on_close::AfterClose::Open {
            entry: adjacent,
            password_return: Some(origin),
        })?;
        Ok(())
    }

    pub(super) fn browse_from_book(&mut self, path: &Path) -> io::Result<()> {
        self.browser = self.read_book_folder(path)?;
        self.show(Screen::Library);
        self.reveal_book_row(path);
        Ok(())
    }

    pub(super) fn reveal_book_row(&mut self, path: &Path) {
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
            self.browser.first_item = index + usize::from(continue_row);
        }
    }

    pub(super) fn refresh_browser(&mut self) -> io::Result<()> {
        let mut directory = self.browser.directory.clone();
        let listing = with_storage(|| {
            loop {
                match self
                    .library
                    .browse_with_order(&directory, self.controls.folder_order)
                {
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
        self.browser.first_item = self
            .browser
            .first_item
            .min(self.library_item_count().saturating_sub(1));
        Ok(())
    }

    pub(super) fn folder_order_items(&self) -> Vec<Item> {
        let order = self.controls.folder_order;
        let mut items = [
            (
                FolderSort::NaturalName,
                "Natural name",
                "Compare numbers by value",
            ),
            (
                FolderSort::FileName,
                "File name",
                "Compare names alphabetically",
            ),
            (
                FolderSort::FileType,
                "File type",
                "Group files by extension",
            ),
            (
                FolderSort::LastRead,
                "Last read",
                if order.reverse {
                    "Unopened, then oldest reads · Browse only"
                } else {
                    "Newest first · Browse only"
                },
            ),
            (
                FolderSort::ReadingProgress,
                "Reading progress",
                if order.reverse {
                    "Finished first · Lowest active progress first"
                } else {
                    "Highest progress first · Finished last"
                },
            ),
        ]
        .into_iter()
        .map(|(sort, label, description)| {
            let item = Item::new(label, Action::SetFolderOrder(FolderOrder { sort, ..order }));
            if sort == order.sort {
                item.detail(format!("Selected · {description}"))
            } else {
                item.detail(description)
            }
        })
        .collect::<Vec<_>>();
        items.push(
            Item::new(
                "Reverse order",
                Action::SetFolderOrder(FolderOrder {
                    reverse: !order.reverse,
                    ..order
                }),
            )
            .detail(if order.reverse { "On" } else { "Off" }),
        );
        items
    }

    pub(super) fn set_folder_order(&mut self, order: FolderOrder) -> io::Result<()> {
        if order != self.controls.folder_order {
            let mut controls = self.controls;
            controls.folder_order = order;
            controls.save(&self.controls_path)?;
            self.controls = controls;
            self.browser.listing.sort(order, self.library.recents());
            if self.library_query.is_empty() {
                self.browser.first_page();
            }
        }
        Ok(())
    }

    pub(super) fn refresh_reading_order(&mut self) {
        let order = self.controls.folder_order;
        if order.uses_reading_data() {
            self.browser.listing.sort(order, self.library.recents());
        }
    }

    pub(super) fn folder_reading_percent(&self) -> Option<u8> {
        let fraction = if let Some(fixed) = &self.fixed {
            (fixed.reading.page() + 1) as f64 / fixed.reading.count() as f64
        } else {
            let reading = &self.active.as_ref()?.reading;
            let chapter = reading.chapter;
            let (page, count) = reading.visible_progress_pages(reading.page);
            let within = (page + 1) as f64 / count as f64;
            let weights = reading.progress_weights();
            match (weights.get(chapter..=chapter + 1), weights.last()) {
                (Some([start, end]), Some(&total)) if total > 0 => {
                    (*start as f64 + within * end.saturating_sub(*start) as f64) / total as f64
                }
                _ => (chapter as f64 + within) / reading.probe.chapter_count() as f64,
            }
        };
        fraction
            .is_finite()
            .then(|| (fraction.clamp(0., 1.) * 100.).round() as u8)
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
            .map(|entry| file_item(entry, self.controls.folder_order.sort));
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

    pub(super) fn back_to_parent_folder(&mut self) -> Change {
        match self.up_action() {
            Some((action, _)) => match self.activate(action) {
                Ok(()) => self.applied_change(),
                Err(error) => self.report(error),
            },
            None => Change::None,
        }
    }

    pub(super) fn draw_library(&self, canvas: &mut Canvas) {
        let layout = self.navigation_layout();
        layout.header_action(canvas, "Reader", "Exit");
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
            Rect::new(path_x, UP.y, layout.width - LIST_X - path_x, UP.h),
            &path,
            BODY.bold(),
        );
        for (label, _, rect) in toolbar_controls(layout) {
            canvas.button(rect, label, false, false);
        }

        let page = self.browser.page(layout);
        let page_size = file_rows(layout);
        let first = page * page_size;
        let rows: Vec<_> = self.library_items().skip(first).take(page_size).collect();
        if rows.is_empty() {
            canvas.text(
                layout.body(LIBRARY_LIST_Y + 38, 82),
                if self.library_query.is_empty() {
                    "This folder is empty."
                } else {
                    "No documents match your search."
                },
                CAPTION.center(),
            );
        } else {
            let mut card = file_row(layout, 0);
            card.h *= rows.len() as i32;
            canvas.card(card);
            for (index, item) in rows.iter().enumerate() {
                let row = file_row(layout, index);
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
        if page > 0 {
            canvas.button(layout.previous(), "Previous", false, false);
        }
        if first + rows.len() < count {
            canvas.button(layout.next(), "Next", false, false);
        }
        let page_label =
            (count > page_size).then(|| format!("{} / {}", page + 1, count.div_ceil(page_size)));
        let limit_notice = (self.library_query.is_empty() && self.browser.listing.truncated)
            .then_some("Showing part of this folder");
        if let Some(text) = self
            .notice
            .as_deref()
            .or(limit_notice)
            .or(page_label.as_deref())
        {
            canvas.text(
                layout.footer_text(310, 104, 80),
                text,
                CAPTION.center().lines(2),
            );
        }
    }

    pub(super) fn library_input(&mut self, input: Input) -> Change {
        let layout = self.navigation_layout();
        let page = self.browser.page(layout);
        let page_size = file_rows(layout);
        let count = self.library_item_count();
        let last = count.saturating_sub(1) / page_size;
        if let Some(page) = pager_destination(input, layout.previous(), layout.next(), page, last) {
            return self.navigate_folder_page(page, page_size);
        }
        let apply = |reader: &mut Reader, action| match reader.activate(action) {
            Ok(()) => Change::Partial,
            Err(error) => reader.report(error),
        };
        let direction = match input {
            Input::Tap(x, y) if layout.back().contains(x, y) => return self.exit(),
            Input::Tap(x, y) if UP.contains(x, y) => {
                return self
                    .up_action()
                    .map_or(Change::None, |(action, _)| apply(self, action));
            }
            Input::PageBack if page == 0 => {
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
            Input::Tap(x, y) | Input::LongTap(x, y) => {
                if matches!(input, Input::Tap(..))
                    && let Some((_, screen, _)) =
                        toolbar_controls(layout).find(|(_, _, rect)| rect.contains(x, y))
                {
                    return apply(self, Action::Show(screen));
                }
                let action = (0..page_size)
                    .find(|&slot| file_row(layout, slot).contains(x, y))
                    .and_then(|slot| self.library_items().nth(page * page_size + slot))
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
        let Some(page) = page
            .checked_add_signed(direction)
            .filter(|page| page * page_size < count)
        else {
            return Change::None;
        };
        self.navigate_folder_page(page, page_size)
    }

    fn navigate_folder_page(&mut self, page: usize, page_size: usize) -> Change {
        self.browser.first_item = page * page_size;
        self.notice = None;
        Change::Partial
    }
}

fn toolbar_controls(
    layout: NavigationLayout,
) -> impl Iterator<Item = (&'static str, Screen, Rect)> {
    let gap = 18;
    let width = layout.width - 2 * LIST_X - (TOOLBAR.len() as i32 - 1) * gap;
    let mut left = LIST_X;
    TOOLBAR.into_iter().map(move |(label, screen, percent)| {
        let button_width = width * percent / 100;
        let rect = Rect::new(left, 222, button_width, 62);
        left += button_width + gap;
        (label, screen, rect)
    })
}

fn file_rows(layout: NavigationLayout) -> usize {
    let body_height = layout.previous().y - LIBRARY_LIST_Y - 16;
    ((body_height / ROW_HEIGHT).max(1) as usize).min(MAX_FILE_ROWS)
}

fn file_row(layout: NavigationLayout, slot: usize) -> Rect {
    layout.body(LIBRARY_LIST_Y + slot as i32 * ROW_HEIGHT, ROW_HEIGHT)
}

fn file_item(entry: &BrowseEntry, sort: FolderSort) -> Item {
    let (action, detail) = match &entry.kind {
        BrowseKind::Folder => (Action::Browse(entry.path.clone()), "Folder".into()),
        BrowseKind::Readable(book) => (
            Action::Open(book.clone()),
            file_detail(
                &entry.path,
                book.size,
                reading_detail(entry, sort).as_deref(),
            ),
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

fn reading_detail(entry: &BrowseEntry, sort: FolderSort) -> Option<String> {
    let label = match sort {
        FolderSort::LastRead => {
            if entry.reading.last_read.is_some() {
                "Opened"
            } else {
                "Unopened"
            }
        }
        FolderSort::ReadingProgress => match entry.reading.progress {
            FolderProgress::Started(Some(percent)) => return Some(format!("{percent}%")),
            FolderProgress::Started(None) => "In progress",
            FolderProgress::OnHold => "On hold",
            FolderProgress::Unopened => "Unopened",
            FolderProgress::Finished => "Finished",
            FolderProgress::LastPage => "100%",
        },
        _ => return None,
    };
    Some(label.to_owned())
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
    use super::super::{HEIGHT, WIDTH};
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen, tap_reading_control};
    use crate::library::ReadingStatus;
    use primitive_app::{App, Lifecycle};
    use primitive_ui::back_button;
    use std::fs;

    fn tap(rect: Rect) -> Input {
        Input::Tap((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
    }

    fn app(fixture: &Fixture) -> Reader {
        Reader::open(
            &fixture.root,
            &fixture.state,
            vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
        )
        .unwrap()
    }

    fn visible_file(app: &Reader, path: &Path) -> bool {
        let layout = app.navigation_layout();
        let rows = file_rows(layout);
        app.library_items()
            .skip(app.browser.page(layout) * rows)
            .take(rows)
            .any(|item| match item.action {
                Action::Open(book) => book.library_path() == path,
                Action::Browse(folder) => folder == path,
                _ => false,
            })
    }

    fn assert_button_feedback(reader: &Reader, label: &str) {
        assert_rect_feedback(reader, reader.reading_button(label));
    }

    fn assert_rect_feedback(reader: &Reader, rect: Rect) {
        let paths: Vec<_> = reader
            .fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        let normal = primitive_app::frame(reader, &mut canvas, false);
        let pressed = Lifecycle::default()
            .press_feedback(reader, &mut canvas, tap(rect), false)
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

    fn library_titles(reader: &Reader) -> Vec<String> {
        reader.library_items().map(|row| row.title).collect()
    }

    fn tool_button(screen: Screen) -> Rect {
        toolbar_controls(NavigationLayout::PORTRAIT)
            .find(|(_, target, _)| *target == screen)
            .unwrap()
            .2
    }

    fn sort_button() -> Rect {
        tool_button(Screen::FolderOrder)
    }

    #[test]
    fn document_browser_keeps_the_same_file_visible_through_negotiated_rotation() {
        use crate::session::ReadingViewport;
        use primitive_app::viewport::{LogicalTurn, Viewport, ViewportDecision, ViewportRequest};
        use primitive_ui::Key;

        let fixture = Fixture::new();
        let folder = fixture.root.join("Field library");
        fs::create_dir(&folder).unwrap();
        fs::create_dir(folder.join("Reference")).unwrap();
        for i in 1..=19 {
            fs::write(
                folder.join(format!("Survey {i:02}.txt")),
                "Keep a record of the tides.",
            )
            .unwrap();
        }
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let mut reader = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        reader.browse_to(PathBuf::from("Field library")).unwrap();
        let portrait = NavigationLayout::PORTRAIT;
        assert_eq!(file_rows(portrait), 10);
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        let retained = Path::new("Field library/Survey 10.txt");
        assert!(visible_file(&reader, retained));

        reader.controls.sticky_orientation = Some(super::super::ReadingOrientation::LandscapeRight);
        let candidate = Viewport {
            generation: 1,
            turn: LogicalTurn::Clockwise,
            logical_size: (1680, 1264),
        };
        assert_eq!(
            reader.viewport_request(),
            Some(ViewportRequest::Fixed(candidate.turn))
        );
        let font_paths: Vec<_> = reader
            .fonts
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let mut canvas = Canvas::new_viewport(1680, 1264, 1264, &font_paths).unwrap();
        reader.stage_viewport(candidate).unwrap();
        canvas.begin();
        reader.draw_staged_viewport(&mut canvas).unwrap();
        let staged = canvas.finish(false);
        assert_eq!(reader.reading_viewport(), ReadingViewport::PORTRAIT);
        reader.finish_viewport(candidate, ViewportDecision::Abort);
        assert_eq!(reader.reading_viewport(), ReadingViewport::PORTRAIT);
        assert!(visible_file(&reader, retained));

        reader.stage_viewport(candidate).unwrap();
        canvas.begin();
        reader.draw_staged_viewport(&mut canvas).unwrap();
        assert_eq!(canvas.finish(false), staged);
        reader.finish_viewport(candidate, ViewportDecision::Commit);
        let landscape = reader.navigation_layout();
        assert_eq!(file_rows(landscape), 7);
        assert_eq!(reader.browser.page(landscape), 1);
        assert!(visible_file(&reader, retained));
        assert_eq!(primitive_app::frame(&reader, &mut canvas, false), staged);
        if let Some(path) = std::env::var_os("READER_LANDSCAPE_DOCUMENTS") {
            primitive_app::write_png(Path::new(&path), 1680, 1264, &staged).unwrap();
        }

        assert!(landscape.next().x > WIDTH as i32);
        assert_eq!(reader.input(tap(landscape.next())), Change::Partial);
        assert!(!visible_file(&reader, retained));
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert!(visible_file(&reader, retained));
        let find = toolbar_controls(landscape)
            .find(|(_, screen, _)| *screen == Screen::LibraryFind)
            .unwrap()
            .2;
        assert_eq!(reader.input(tap(find)), Change::Partial);
        assert_eq!(
            reader.viewport_request(),
            Some(ViewportRequest::Fixed(candidate.turn))
        );
        reader.editor.text = "Survey 19".chars().collect();
        reader.editor.caret = reader.editor.text.len();
        let (_, go) = landscape
            .keyboard()
            .keyboard_keys_with(reader.editor.layer, "Find")
            .into_iter()
            .find(|(key, _)| key.key == Key::Go)
            .unwrap();
        assert!(go.x + go.w / 2 > WIDTH as i32);
        if let Some(path) = std::env::var_os("READER_LANDSCAPE_DOCUMENT_FIND") {
            let frame = primitive_app::frame(&reader, &mut canvas, false);
            primitive_app::write_png(Path::new(&path), 1680, 1264, &frame).unwrap();
        }
        assert_eq!(reader.input(tap(go)), Change::Partial);
        assert_eq!(reader.screen, Screen::Library);
        assert_eq!(library_titles(&reader), ["Survey 19"]);
        let clear = reader.up_action().unwrap();
        assert_eq!(clear.1, "Clear");
        assert_eq!(reader.input(tap(UP)), Change::Partial);
        assert!(visible_file(&reader, Path::new("Field library/Reference")));

        let portrait_candidate = Viewport {
            generation: 2,
            turn: LogicalTurn::Upright,
            logical_size: (WIDTH, HEIGHT),
        };
        reader.stage_viewport(portrait_candidate).unwrap();
        reader.finish_viewport(portrait_candidate, ViewportDecision::Commit);
        assert_eq!(reader.reading_viewport(), ReadingViewport::PORTRAIT);
        assert_eq!(reader.browser.page(portrait), 0);
        assert!(visible_file(&reader, Path::new("Field library/Reference")));
    }

    #[test]
    fn folder_order_picker_keeps_query_pages_and_restores_the_saved_choice() {
        use primitive_ui::{Key, Layout};

        let fixture = Fixture::new();
        // Legacy profiles get natural ordering even when an older option is saved.
        fs::write(
            fixture.root.join("reading-controls.json"),
            r#"{"footnote_popups":false}"#,
        )
        .unwrap();
        let folder = fixture.root.join("Reading list");
        fs::create_dir(&folder).unwrap();
        for name in ["Set 2", "Set 10"] {
            fs::create_dir(folder.join(name)).unwrap();
        }
        for volume in 1..=12 {
            fs::write(
                folder.join(format!("Volume {volume}.txt")),
                format!("A field account from volume {volume}."),
            )
            .unwrap();
        }
        fs::copy(
            crate::test_fixture("pdf-password/plain.pdf"),
            folder.join("Atlas.pdf"),
        )
        .unwrap();
        let mut reader = app(&fixture);
        assert_eq!(reader.controls.folder_order, FolderOrder::default());
        assert!(!reader.controls.footnote_popups);
        reader.browse_to(PathBuf::from("Reading list")).unwrap();
        assert_eq!(
            &library_titles(&reader)[..3],
            ["Set 2", "Set 10", "Atlas.pdf"]
        );
        assert!(visible_file(
            &reader,
            Path::new("Reading list/Volume 2.txt")
        ));
        assert!(!visible_file(
            &reader,
            Path::new("Reading list/Volume 10.txt")
        ));
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert!(visible_file(
            &reader,
            Path::new("Reading list/Volume 10.txt")
        ));
        assert_eq!(reader.input(Input::PageBack), Change::Partial);

        assert_rect_feedback(&reader, sort_button());
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(reader.screen, Screen::FolderOrder);
        let file_name = reader.list_geometry().row(1);
        assert_rect_feedback(&reader, file_name);
        assert_eq!(reader.input(tap(file_name)), Change::Partial);
        capture_test_screen(&reader, &reader.fonts, "READER_FOLDER_ORDER_MENU", false);
        let back = tap(back_button(WIDTH as i32));
        assert_eq!(reader.input(back), Change::Partial);
        assert_eq!(
            &library_titles(&reader)[..3],
            ["Set 10", "Set 2", "Atlas.pdf"]
        );
        assert!(visible_file(
            &reader,
            Path::new("Reading list/Volume 10.txt")
        ));
        capture_test_screen(&reader, &reader.fonts, "READER_FOLDER_ORDER_PICKER", false);
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert!(visible_file(
            &reader,
            Path::new("Reading list/Volume 9.txt")
        ));
        assert!(!visible_file(
            &reader,
            Path::new("Reading list/Volume 10.txt")
        ));
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert!(visible_file(
            &reader,
            Path::new("Reading list/Volume 10.txt")
        ));

        // Change folder ordering while searching without changing the result ranking or page.
        assert_eq!(
            reader.input(tap(tool_button(Screen::LibraryFind))),
            Change::Partial
        );
        reader.editor.text = "Volume".chars().collect();
        reader.editor.caret = reader.editor.text.len();
        let (_, go) = Layout::new(WIDTH, HEIGHT, 4, 5)
            .keyboard_keys_with(reader.editor.layer, "Find")
            .into_iter()
            .find(|(key, _)| key.key == Key::Go)
            .unwrap();
        assert_eq!(reader.input(tap(go)), Change::Partial);
        let search = library_titles(&reader);
        assert_eq!(search.len(), 12);
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reader.input(tap(reader.list_geometry().row(5))),
            Change::Partial
        );
        assert_eq!(reader.input(back), Change::Partial);
        assert_eq!(reader.browser.page(reader.navigation_layout()), 1);
        assert_eq!(library_titles(&reader), search);
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        let reversed = library_titles(&reader);
        assert_eq!(&reversed[..3], ["Set 2", "Set 10", "Volume 9.txt"]);
        assert_eq!(reader.browser.directory, Path::new("Reading list"));

        let mut reopened = app(&fixture);
        assert_eq!(reopened.controls.folder_order, reader.controls.folder_order);
        assert!(reopened.controls.folder_order.reverse);
        assert!(!reopened.controls.footnote_popups);
        reopened.browse_to(PathBuf::from("Reading list")).unwrap();
        assert_eq!(library_titles(&reopened), reversed);
    }

    #[test]
    fn folder_order_write_failure_keeps_the_current_picker_and_saved_choice() {
        let fixture = Fixture::new();
        let mut reader = app(&fixture);
        let before = library_titles(&reader);
        fs::create_dir(&reader.controls_path).unwrap();
        let order = FolderOrder {
            sort: FolderSort::FileType,
            reverse: true,
        };
        assert!(reader.set_folder_order(order).is_err());
        assert_eq!(reader.controls.folder_order, FolderOrder::default());
        assert_eq!(library_titles(&reader), before);
        assert_eq!(app(&fixture).controls.folder_order, FolderOrder::default());
    }

    #[test]
    fn folder_order_uses_saved_reading_history_and_progress_without_touching_books() {
        let fixture = Fixture::new();
        let shelf = Path::new("Field library");
        let folder = fixture.root.join(shelf);
        fs::create_dir(&folder).unwrap();
        for name in ["Archives 2", "Archives 10"] {
            fs::create_dir(folder.join(name)).unwrap();
        }
        let publications = [
            ("Coast 10.pdf", 10, Some(4), None),
            ("Coast 2.pdf", 8, Some(3), None),
            ("Harbor survey.pdf", 4, Some(0), None),
            ("Old notebook.pdf", 6, Some(0), None),
            ("Supplement.pdf", 3, Some(1), Some(ReadingStatus::OnHold)),
            (
                "Finished survey.pdf",
                7,
                Some(0),
                Some(ReadingStatus::Finished),
            ),
            ("Last chart.pdf", 5, Some(4), None),
            ("Unopened field guide.pdf", 2, None, None),
            ("Unopened reading list.pdf", 1, None, None),
        ];
        let originals: Vec<_> = publications
            .iter()
            .map(|(name, pages, _, _)| {
                let path = folder.join(name);
                let bytes = crate::fixed_session::tests::striped_pdf_bytes(*pages);
                fs::write(&path, &bytes).unwrap();
                let modified = fs::metadata(&path).unwrap().modified().unwrap();
                (path, bytes, modified)
            })
            .collect();
        let mut reader = app(&fixture);
        for (name, _, page, status) in publications {
            let Some(page) = page else { continue };
            reader.open_path(&folder.join(name)).unwrap();
            reader.follow_fixed(page, None).unwrap();
            let id = reader.open_book_id().unwrap();
            let state = reader.library.state_mut(id).unwrap();
            if let Some(status) = status {
                state.set_status(status);
            }
            if name == "Old notebook.pdf" {
                state.reading_percent = None;
            }
        }
        let unread = reader
            .library
            .resolve(&shelf.join("Unopened reading list.pdf"))
            .unwrap();
        let unread_id = reader.library.identify(&unread).unwrap();
        reader.library.track(&unread, unread_id).favorite = true;
        reader.activate(Action::Show(Screen::Library)).unwrap();
        reader.browse_to(shelf.to_owned()).unwrap();
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reader.input(tap(reader.list_geometry().row(3))),
            Change::Partial
        );
        let back = tap(back_button(WIDTH as i32));
        assert_eq!(reader.input(back), Change::Partial);
        assert_eq!(
            library_titles(&reader),
            [
                "Archives 2",
                "Archives 10",
                "Last chart.pdf",
                "Finished survey.pdf",
                "Supplement.pdf",
                "Old notebook.pdf",
                "Harbor survey.pdf",
                "Coast 2.pdf",
                "Coast 10.pdf",
                "Unopened field guide.pdf",
                "Unopened reading list.pdf"
            ]
        );
        assert_eq!(reader.input(Input::PageForward), Change::Partial);
        assert!(visible_file(
            &reader,
            &shelf.join("Unopened reading list.pdf")
        ));
        assert_eq!(reader.input(Input::PageBack), Change::Partial);
        reader.open_path(&folder.join("Coast 10.pdf")).unwrap();
        let origin = reader.current_position();
        let previous = Action::OpenFolderFile(FolderDirection::Previous);
        assert!(
            reader
                .activate(previous)
                .unwrap_err()
                .to_string()
                .contains("Last read")
        );
        assert_eq!(reader.current_position(), origin);
        reader.activate(Action::Show(Screen::Library)).unwrap();
        assert_eq!(library_titles(&reader)[2], "Coast 10.pdf");

        reader.library_query = "Coast".into();
        let search = library_titles(&reader);
        assert_eq!(search.len(), 2);
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reader.input(tap(reader.list_geometry().row(4))),
            Change::Partial
        );
        capture_test_screen(&reader, &reader.fonts, "READER_READING_SORT_MENU", false);
        assert_eq!(reader.input(back), Change::Partial);
        assert_eq!(library_titles(&reader), search);
        reader.activate(Action::ClearLibraryFilter).unwrap();
        let expected = [
            "Archives 2",
            "Archives 10",
            "Coast 2.pdf",
            "Coast 10.pdf",
            "Harbor survey.pdf",
            "Old notebook.pdf",
            "Supplement.pdf",
            "Unopened field guide.pdf",
            "Unopened reading list.pdf",
            "Finished survey.pdf",
            "Last chart.pdf",
        ];
        assert_eq!(library_titles(&reader), expected);
        let details: Vec<_> = reader
            .library_items()
            .skip(2)
            .map(|item| item.detail.unwrap())
            .collect();
        assert!(details[0].ends_with("50%"));
        assert!(details[1].ends_with("50%"));
        assert!(details[2].ends_with("25%"));
        assert!(details[3].ends_with("In progress"));
        assert!(details[4].ends_with("On hold"));
        assert!(details[5].ends_with("Unopened"));
        assert!(details[7].ends_with("Finished"));
        assert!(details[8].ends_with("100%"));
        capture_test_screen(&reader, &reader.fonts, "READER_READING_SORT_PICKER", false);

        reader.open_path(&folder.join("Coast 2.pdf")).unwrap();
        let origin = reader.current_position();
        reader
            .activate(Action::OpenFolderFile(FolderDirection::Next))
            .unwrap();
        assert_eq!(
            reader.current_document().unwrap().0.path,
            shelf.join("Coast 10.pdf")
        );
        reader
            .activate(Action::OpenFolderFile(FolderDirection::Previous))
            .unwrap();
        assert_eq!(
            reader.current_document().unwrap().0.path,
            shelf.join("Coast 2.pdf")
        );
        assert_eq!(reader.current_position(), origin);
        reader.save().unwrap();
        let mut reopened = app(&fixture);
        assert_eq!(
            reopened.controls.folder_order.sort,
            FolderSort::ReadingProgress
        );
        reopened.browse_to(shelf.to_owned()).unwrap();
        assert_eq!(library_titles(&reopened), expected);
        assert_eq!(reopened.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reopened.input(tap(reopened.list_geometry().row(5))),
            Change::Partial
        );
        assert!(
            reopened.folder_order_items()[4]
                .detail
                .as_deref()
                .unwrap()
                .contains("Finished first")
        );
        assert_eq!(reopened.input(back), Change::Partial);
        assert_eq!(
            &library_titles(&reopened)[..3],
            ["Archives 10", "Archives 2", "Last chart.pdf"]
        );
        for (path, bytes, modified) in originals {
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
        }
    }

    #[test]
    fn folder_progress_is_saved_for_reflow_without_reopening_the_title_from_the_picker() {
        let fixture = Fixture::new();
        let mut reader = app(&fixture);
        reader.open_path(&fixture.book).unwrap();
        let id = reader.open_book_id().unwrap();
        let opening = reader.library.state(id).unwrap().reading_percent.unwrap();
        let reading = &reader.active.as_ref().unwrap().reading;
        let last = reading.probe.chapter_count() - 1;
        reader.follow_reflow_page(last, 0).unwrap();
        let reading = &reader.active.as_ref().unwrap().reading;
        reader
            .follow_reflow_page(last, reading.probe.page_count() - 1)
            .unwrap();
        assert!(opening < 100);
        assert_eq!(reader.library.state(id).unwrap().reading_percent, Some(100));
        reader.save().unwrap();
        let reopened = app(&fixture);
        assert!(reopened.open_book_id().is_none());
        assert_eq!(
            reopened.library.state(id).unwrap().reading_percent,
            Some(100)
        );
    }

    #[test]
    fn folder_order_by_type_matches_current_book_folder_and_adjacent_open() {
        let fixture = Fixture::new();
        let shelf = Path::new("Mixed books");
        let folder = fixture.root.join(shelf);
        fs::create_dir(&folder).unwrap();
        let epub = folder.join("Z Book.epub");
        let pdf = folder.join("B Atlas.PDF");
        fs::copy(&fixture.book, &epub).unwrap();
        fs::copy(crate::test_fixture("pdf-password/plain.pdf"), &pdf).unwrap();
        fs::write(folder.join("A Notes.txt"), "A text publication.").unwrap();
        fs::write(folder.join("C Appendix.xyz"), "An unsupported file.").unwrap();
        let mut reader = app(&fixture);
        reader.browse_to(shelf.to_owned()).unwrap();
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reader.input(tap(reader.list_geometry().row(2))),
            Change::Partial
        );
        assert_eq!(
            reader.input(tap(back_button(WIDTH as i32))),
            Change::Partial
        );
        let expected = [
            "Z Book.epub",
            "B Atlas.PDF",
            "A Notes.txt",
            "C Appendix.xyz",
        ];
        assert_eq!(library_titles(&reader), expected);
        assert_eq!(
            reader.input(tap(file_row(NavigationLayout::PORTRAIT, 0))),
            Change::Partial
        );
        reader.follow_reflow_page(1, 0).unwrap();
        let epub_position = reader.current_position().unwrap();
        reader.activate(Action::ToggleBookmark).unwrap();
        let next = || Action::OpenFolderFile(FolderDirection::Next);
        let previous = || Action::OpenFolderFile(FolderDirection::Previous);
        reader.activate(next()).unwrap();
        assert_eq!(
            reader.current_document().unwrap().0.path,
            shelf.join("B Atlas.PDF")
        );
        reader.activate(previous()).unwrap();
        assert_eq!(reader.current_position(), Some(epub_position.clone()));
        assert!(reader.page_is_bookmarked());

        reader.current_book_folder().unwrap();
        assert_eq!(reader.browser.directory, shelf);
        assert_eq!(library_titles(&reader), expected);
        assert_eq!(reader.input(tap(sort_button())), Change::Partial);
        assert_eq!(
            reader.input(tap(reader.list_geometry().row(5))),
            Change::Partial
        );
        assert_eq!(
            reader.input(tap(back_button(WIDTH as i32))),
            Change::Partial
        );
        assert_eq!(
            library_titles(&reader),
            expected.into_iter().rev().collect::<Vec<_>>()
        );
        reader.activate(Action::Continue).unwrap();
        assert_eq!(reader.current_position(), Some(epub_position));
        let error = reader.activate(next()).unwrap_err().to_string();
        assert!(error.contains("no next"));
        reader.activate(previous()).unwrap();
        assert_eq!(
            reader.current_document().unwrap().0.path,
            shelf.join("B Atlas.PDF")
        );
        reader.activate(previous()).unwrap();
        assert_eq!(
            reader.current_document().unwrap().0.path,
            shelf.join("A Notes.txt")
        );
        let error = reader.activate(previous()).unwrap_err().to_string();
        assert!(error.contains("no previous"));
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
        assert_eq!(reader.browser.page(reader.navigation_layout()), 1);
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
                .next_book_in_folder(reader.current_document().unwrap().0, FolderOrder::default())
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
