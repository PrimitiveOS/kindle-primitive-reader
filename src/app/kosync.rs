//! Manual KOReader-server controls. Reflow pointers are deliberately unavailable.
use super::{Action, Item, Reader, Result, Screen};
use crate::{
    kosync::{self as wire, Config, Job, Matching, Operation, Position, Reply, Request},
    library::DocumentFormat,
};
use primitive_app::{Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{BODY, CAPTION, Canvas, Editor, Input, Layout, Press, Rect, keyboard_key};
use std::{path::PathBuf, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Server,
    Username,
    Password,
    Device,
}
impl Field {
    pub fn title(self) -> &'static str {
        match self {
            Self::Server => "KOReader server",
            Self::Username => "KOReader username",
            Self::Password => "KOReader password",
            Self::Device => "Device name",
        }
    }
    fn limit(self) -> usize {
        match self {
            Self::Server | Self::Password => 2048,
            Self::Username => 512,
            Self::Device => 64,
        }
    }
}
#[derive(Clone)]
pub(super) enum Command {
    Close,
    Settings,
    Open,
    Edit(Field),
    Toggle,
    LocalHttp,
    Matching,
    Register,
    Authorize,
    SignOut,
    Fetch,
    Push,
    Receive,
    Cancel,
}
#[derive(Clone)]
struct Book {
    id: u64,
    document: String,
    count: usize,
}
struct Pending {
    epoch: u64,
    book: Option<Book>,
}
pub(super) struct View {
    config: Config,
    path: PathBuf,
    job: Option<Job>,
    epoch: u64,
    pending: Option<Pending>,
    remote: Option<(Book, Position)>,
    preview: bool,
    back: Option<(Screen, usize, Option<String>)>,
    #[cfg(test)]
    test_worker: Option<PathBuf>,
}
impl View {
    pub fn load(path: PathBuf) -> Self {
        let mut config = Config::load(&path);
        if config.device_id.is_empty() {
            config.device_id = wire::device_id()
        }
        Self {
            config,
            path,
            job: None,
            epoch: 0,
            pending: None,
            remote: None,
            preview: false,
            back: None,
            #[cfg(test)]
            test_worker: None,
        }
    }
    fn persist(&mut self, config: Config) -> Result<()> {
        with_storage(|| config.save(&self.path))?;
        self.config = config;
        Ok(())
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.job.as_ref().map(|j| j.poll_at)
    }
}
impl Reader {
    fn kosync_book(&self) -> Result<Book> {
        let fixed = self
            .fixed
            .as_ref()
            .filter(|f| f.entry.format == DocumentFormat::Pdf)
            .ok_or("PDFs only; use WebDAV for other books.")?;
        let path = self.library.absolute_path(&fixed.entry.path)?;
        Ok(Book {
            id: fixed.id,
            document: with_storage(|| wire::document(&path, self.kosync.config.matching))?,
            count: fixed.reading.count(),
        })
    }
    fn verify_kosync_book(&self, book: &Book) -> Result<()> {
        let current = self.kosync_book()?;
        if current.id != book.id || current.document != book.document || current.count != book.count
        {
            return Err("The document changed; check its server position again".into());
        }
        Ok(())
    }
    pub(super) fn kosync_items(&self) -> Vec<Item> {
        let v = &self.kosync;
        let action = |s: &str, c| Item::new(s, Action::KoSync(c));
        if self.screen == Screen::KoSyncSettings {
            return vec![
                action("KOReader sync", Command::Toggle).detail(if v.config.enabled {
                    "On · Manual actions"
                } else {
                    "Off"
                }),
                action("Server URL", Command::Edit(Field::Server)).detail(&v.config.server),
                action("Username", Command::Edit(Field::Username)).detail(
                    if v.config.username.is_empty() {
                        "Not set"
                    } else {
                        &v.config.username
                    },
                ),
                action("Password", Command::Edit(Field::Password)).detail(
                    if v.config.key.is_empty() {
                        "Not set"
                    } else {
                        "Saved on this Kindle"
                    },
                ),
                action("Device name", Command::Edit(Field::Device)).detail(&v.config.device),
                action("Match documents by", Command::Matching).detail(
                    if v.config.matching == Matching::Binary {
                        "File contents · KOReader default"
                    } else {
                        "Filename · Same name can match different content"
                    },
                ),
                action("Allow HTTP for local servers", Command::LocalHttp).detail(
                    if v.config.allow_local_http {
                        "On"
                    } else {
                        "Off · Use HTTPS"
                    },
                ),
                action("Sign in", Command::Authorize).detail(if v.config.authorized {
                    "Signed in · Check account again"
                } else {
                    "Use an existing account"
                }),
                action("Register account", Command::Register)
                    .detail("Create a KOReader sync account"),
                action("Sign out", Command::SignOut)
                    .detail("Remove this account; keep local reading data"),
            ];
        }
        let mut rows = vec![
            Item::new("KOReader sync settings", Action::KoSync(Command::Settings)).detail(
                if v.config.enabled {
                    "On · Manual PDF page sync"
                } else {
                    "Off"
                },
            ),
        ];
        if self
            .fixed
            .as_ref()
            .is_some_and(|f| f.entry.format == DocumentFormat::Pdf)
        {
            rows.push(
                action("Check server position", Command::Fetch)
                    .detail("Preview the saved PDF source page"),
            );
            rows.push(
                action("Push this PDF's position", Command::Push)
                    .detail("Replaces the page saved by another device"),
            );
        }
        rows.push(
            Item::new("WebDAV progress sync", Action::Show(Screen::ProgressSync))
                .detail("Use a separate WebDAV account"),
        );
        rows
    }
    fn show_kosync(&mut self, screen: Screen) {
        self.screen = screen;
        self.list_page = 0;
    }
    pub(super) fn kosync_screen(&self) -> bool {
        matches!(
            self.screen,
            Screen::KoSync | Screen::KoSyncSettings | Screen::KoSyncEdit(_)
        )
    }
    pub(super) fn kosync_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.leave_progress_sync();
                self.cancel_kosync();
                self.kosync.remote = None;
                if self.kosync.back.is_none() {
                    self.kosync.back = Some((self.screen, self.list_page, self.notice.clone()));
                }
                self.show_kosync(Screen::KoSync);
            }
            Command::Close => {
                self.cancel_kosync();
                self.kosync.remote = None;
                if let Some((screen, page, notice)) = self.kosync.back.take() {
                    self.screen = screen;
                    self.list_page = page;
                    self.notice = notice;
                } else {
                    self.screen = self.tools_screen();
                }
            }
            Command::Settings => {
                self.cancel_kosync();
                self.show_kosync(Screen::KoSyncSettings);
            }
            Command::Edit(field) => {
                self.cancel_kosync();
                self.editor = Editor::new(field.limit());
                self.editor.text = match field {
                    Field::Server => &self.kosync.config.server,
                    Field::Username => &self.kosync.config.username,
                    Field::Device => &self.kosync.config.device,
                    Field::Password => "",
                }
                .chars()
                .collect();
                self.editor.caret = self.editor.text.len();
                self.show_kosync(Screen::KoSyncEdit(field));
            }
            Command::Toggle => {
                self.cancel_kosync();
                let mut config = self.kosync.config.clone();
                if !config.enabled {
                    if !config.authorized {
                        return Err("Sign in before turning on KOReader sync".into());
                    }
                    config.enabled = true;
                    config.ready().map_err(std::io::Error::other)?;
                } else {
                    config.enabled = false;
                }
                self.kosync.persist(config)?;
            }
            Command::LocalHttp => {
                self.cancel_kosync();
                let mut config = self.kosync.config.clone();
                config.allow_local_http = !config.allow_local_http;
                if config.origin().is_err() {
                    config.enabled = false;
                    config.authorized = false;
                }
                self.kosync.persist(config)?;
                self.kosync.remote = None;
            }
            Command::Matching => {
                self.cancel_kosync();
                let mut config = self.kosync.config.clone();
                config.matching = if config.matching == Matching::Binary {
                    Matching::Filename
                } else {
                    Matching::Binary
                };
                self.kosync.persist(config)?;
                self.kosync.remote = None;
            }
            Command::SignOut => {
                self.cancel_kosync();
                let mut config = self.kosync.config.clone();
                config.enabled = false;
                config.authorized = false;
                config.key.clear();
                config.username.clear();
                self.kosync.persist(config)?;
                self.kosync.remote = None;
                self.notice =
                    Some("KOReader account signed out. Local reading and WebDAV are kept.".into());
            }
            Command::Register => self.start_kosync(Operation::Register, None)?,
            Command::Authorize => self.start_kosync(Operation::Authorize, None)?,
            Command::Fetch => {
                let book = self.kosync_book()?;
                self.start_kosync(
                    Operation::Fetch {
                        document: book.document.clone(),
                        count: book.count,
                    },
                    Some(book),
                )?;
            }
            Command::Push => {
                let book = self.kosync_book()?;
                self.record_position();
                self.save_library()?;
                let page = self
                    .fixed
                    .as_ref()
                    .ok_or("Open a PDF first")?
                    .reading
                    .page();
                let position = Position::pdf(page, book.count, &self.kosync.config)
                    .map_err(std::io::Error::other)?;
                self.start_kosync(
                    Operation::Push {
                        document: book.document.clone(),
                        position,
                        count: book.count,
                    },
                    Some(book),
                )?;
            }
            Command::Receive => self.open_kosync_position()?,
            Command::Cancel => {
                self.cancel_kosync();
                self.notice = Some("KOReader sync cancelled. The current page is kept.".into());
            }
        }
        Ok(())
    }
    fn start_kosync(&mut self, operation: Operation, book: Option<Book>) -> Result<()> {
        self.cancel_kosync();
        self.kosync.remote = None;
        if let Some(book) = &book {
            self.verify_kosync_book(book)?;
        }
        let request = Request {
            config: self.kosync.config.clone(),
            operation,
        };
        #[cfg(test)]
        let job = if let Some(exe) = &self.kosync.test_worker {
            Job::spawn(std::process::Command::new(exe), &request)?
        } else {
            Job::start(&request)?
        };
        #[cfg(not(test))]
        let job = Job::start(&request)?;
        self.kosync.pending = Some(Pending {
            epoch: self.kosync.epoch,
            book,
        });
        self.kosync.job = Some(job);
        self.show_kosync(Screen::KoSync);
        Ok(())
    }
    pub(super) fn cancel_kosync(&mut self) {
        self.kosync.job.take();
        self.kosync.epoch = self.kosync.epoch.wrapping_add(1);
        self.kosync.pending = None;
        self.kosync.preview = false;
    }
    pub(super) fn leave_kosync(&mut self) {
        self.cancel_kosync();
        self.kosync.remote = None;
        self.kosync.back = None;
    }
    pub(super) fn poll_kosync(&mut self, now: Instant) -> Option<Change> {
        let reply = self.kosync.job.as_mut()?.poll(now)?;
        self.kosync.job.take();
        Some(
            match reply.and_then(|reply| self.finish_kosync(reply).map_err(|e| e.to_string())) {
                Ok(()) => Change::Partial,
                Err(e) => {
                    self.kosync.pending = None;
                    self.report(e)
                }
            },
        )
    }
    fn finish_kosync(&mut self, reply: Reply) -> Result<()> {
        let pending = self
            .kosync
            .pending
            .take()
            .ok_or("This sync request is no longer active")?;
        if pending.epoch != self.kosync.epoch {
            return Err("The sync account changed; the reply was ignored".into());
        }
        if let Some(book) = &pending.book {
            self.verify_kosync_book(book)?;
        }
        match reply {
            Reply::Authorized => {
                if pending.book.is_some() {
                    return Err("Unexpected account response".into());
                }
                let mut config = self.kosync.config.clone();
                config.authorized = true;
                self.kosync.persist(config)?;
                self.notice =
                    Some("Signed in. Turn on KOReader sync to use manual PDF actions.".into());
            }
            Reply::Missing => {
                if pending.book.is_none() {
                    return Err("Unexpected position response".into());
                }
                self.notice = Some("No position is saved for this PDF.".into());
            }
            Reply::Found(position) => {
                let book = pending.book.ok_or("Unexpected position response")?;
                position.page(book.count).map_err(std::io::Error::other)?;
                self.kosync.remote = Some((book, position));
                self.kosync.preview = true;
            }
            Reply::Stored(position) => {
                let book = pending.book.ok_or("Unexpected position response")?;
                position.page(book.count).map_err(std::io::Error::other)?;
                self.kosync.remote = Some((book, position));
                self.notice = Some("PDF position confirmed on the KOReader server.".into());
            }
            Reply::Changed(position) => {
                self.notice = Some(
                    "The server changed after the push. Check it before sending again.".into(),
                );
                if let (Some(book), Some(position)) = (pending.book, position) {
                    position.page(book.count).map_err(std::io::Error::other)?;
                    self.kosync.remote = Some((book, position));
                    self.kosync.preview = true;
                }
            }
        }
        Ok(())
    }
    fn open_kosync_position(&mut self) -> Result<()> {
        let (book, position) = self
            .kosync
            .remote
            .clone()
            .ok_or("Check the server position first")?;
        self.verify_kosync_book(&book)?;
        let page = position.page(book.count).map_err(std::io::Error::other)?;
        let previous = self
            .current_history_location()
            .ok_or("The current page is not ready")?;
        let prior = self
            .library
            .state(book.id)
            .ok_or("This PDF is no longer tracked")?
            .clone();
        self.jump_fixed(page, None)?;
        if let Err(error) = self.save_library() {
            let _ = self.go_to_location(&previous);
            *self
                .library
                .state_mut(book.id)
                .ok_or("This PDF is no longer tracked")? = prior;
            return Err(error.into());
        }
        self.push_history(previous);
        self.pdf_selection = None;
        self.kosync.preview = false;
        self.notice = Some("Opened the server PDF source page.".into());
        Ok(())
    }
    pub(super) fn draw_kosync_editor(&self, canvas: &mut Canvas, field: Field) {
        // Pictures are composited after ordinary fills. Register the keyboard's
        // picture cover before the form so both keyboard and Back keep their targets.
        canvas.popup(Layout::new(WIDTH, HEIGHT, 4, 5).keyboard_area());
        canvas.popup(EDIT);
        canvas.text(Rect::new(115, 135, 770, 75), field.title(), BODY.bold());
        canvas.button(EDIT_BACK, "Back", false, false);
        canvas.single_line_field(
            Rect::new(115, 245, 1034, 130),
            &self.editor,
            field.title(),
            field == Field::Password,
        );
        if let Some(n) = &self.notice {
            canvas.text(Rect::new(115, 395, 1034, 145), n, CAPTION.lines(3));
        }
        canvas.keyboard(self.editor.layer, "Save", None);
    }
    pub(super) fn draw_kosync_sheet(&self, canvas: &mut Canvas) {
        if let Screen::KoSyncEdit(field) = self.screen {
            self.draw_kosync_editor(canvas, field);
            return;
        }
        let settings = self.screen == Screen::KoSyncSettings;
        let sheet = if settings { SETTINGS } else { SHEET };
        canvas.popup(sheet);
        canvas.text(
            Rect::new(sheet.x + 35, sheet.y + 25, sheet.w - 300, 80),
            if settings {
                "KOReader sync settings"
            } else {
                "KOReader sync"
            },
            BODY.bold(),
        );
        canvas.button(
            sheet_back(settings),
            if settings { "Back" } else { "Done" },
            false,
            false,
        );
        let items = self.kosync_items();
        for (index, item) in items.iter().enumerate() {
            let rect = tile(index, settings);
            let available = !matches!(item.action, Action::Heading);
            let pressed = available && canvas.control_pressed(rect, false);
            let ink = if pressed { 255 } else { 0 };
            canvas.panel(rect, 20, 2, if pressed { 0 } else { 255 });
            canvas.icon(
                Rect::new(rect.x + 18, rect.y + 14, 36, 36),
                super::bar_icons::action_icon(&item.action),
                ink,
            );
            canvas.text(
                Rect::new(rect.x + 18, rect.y + 58, rect.w - 36, 70),
                &item.title,
                primitive_ui::TextStyle::new(if settings { 27 } else { 28 })
                    .bold()
                    .ink(ink)
                    .lines(2),
            );
            if let Some(detail) = &item.detail {
                canvas.text(
                    Rect::new(rect.x + 18, rect.y + 130, rect.w - 36, rect.h - 136),
                    detail,
                    primitive_ui::TextStyle::new(22)
                        .ink(if pressed { 255 } else { 85 })
                        .lines(2),
                );
            }
        }
        if !settings {
            canvas.text(Rect::new(115,1080,1034,75),"PDFs only; use WebDAV for other books. Saved server positions cannot be removed here.",CAPTION.lines(2));
        }
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(sheet.x + 35, sheet.bottom() - 80, sheet.w - 70, 65),
                notice,
                CAPTION.lines(2),
            );
        }
    }
    pub(super) fn kosync_screen_input(&mut self, input: Input) -> Option<Change> {
        if !matches!(self.screen, Screen::KoSync | Screen::KoSyncSettings) {
            return None;
        }
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let settings = self.screen == Screen::KoSyncSettings;
        let sheet = if settings { SETTINGS } else { SHEET };
        let action = match input {
            Input::PageBack => {
                if settings {
                    Action::KoSync(Command::Open)
                } else {
                    Action::KoSync(Command::Close)
                }
            }
            Input::Tap(x, y) if sheet_back(settings).contains(x, y) || !sheet.contains(x, y) => {
                if settings {
                    Action::KoSync(Command::Open)
                } else {
                    Action::KoSync(Command::Close)
                }
            }
            Input::Tap(x, y) => {
                let items = self.kosync_items();
                let Some(item) = items
                    .into_iter()
                    .enumerate()
                    .find_map(|(i, item)| tile(i, settings).contains(x, y).then_some(item))
                else {
                    return Some(Change::None);
                };
                item.action
            }
            _ => return Some(Change::None),
        };
        Some(match self.activate(action) {
            Ok(()) => Change::Partial,
            Err(e) => self.report(e),
        })
    }
    pub(super) fn kosync_editor_input(&mut self, input: Input, field: Field) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.editor = Editor::new(1);
            self.show_kosync(Screen::KoSyncSettings);
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let Some(key) = keyboard_key(
            &Layout::new(WIDTH, HEIGHT, 4, 5),
            &self.editor,
            "Save",
            x,
            y,
        ) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let mut config = self.kosync.config.clone();
            let value = self.editor.string();
            let identity = field != Field::Device;
            let result = (|| -> Result<()> {
                match field {
                    Field::Server => {
                        config.server = value.trim().into();
                        config.origin().map_err(std::io::Error::other)?;
                    }
                    Field::Username => {
                        if value.trim().is_empty()
                            || value.len() > 512
                            || value.chars().any(char::is_control)
                        {
                            return Err("Enter a username without control characters".into());
                        }
                        config.username = value.trim().into();
                    }
                    Field::Password => {
                        config.key = wire::password_key(&value).map_err(std::io::Error::other)?
                    }
                    Field::Device => {
                        if value.trim().is_empty() || value.chars().any(char::is_control) {
                            return Err("Enter a device name without control characters".into());
                        }
                        config.device = value.trim().into();
                    }
                }
                if identity {
                    config.authorized = false;
                    config.enabled = false;
                }
                self.cancel_kosync();
                self.kosync.remote = None;
                self.kosync.persist(config)?;
                Ok(())
            })();
            self.editor = Editor::new(field.limit());
            return match result {
                Ok(()) => {
                    self.show_kosync(Screen::KoSyncSettings);
                    Change::Partial
                }
                Err(e) => self.report(e),
            };
        }
        Change::Partial
    }
    pub(super) fn draw_kosync_popup(&self, canvas: &mut Canvas) {
        let v = &self.kosync;
        if v.job.is_none() && !v.preview {
            return;
        }
        canvas.popup(DIALOG);
        if v.job.is_some() {
            canvas.text(
                Rect::new(190, 590, 884, 95),
                "Contacting KOReader server",
                BODY.bold(),
            );
            canvas.text(
                Rect::new(190, 710, 884, 170),
                "Your local page is kept.\nThis may take up to 40 seconds.",
                BODY.lines(3),
            );
            canvas.button(CANCEL, "Cancel", false, false);
        } else if let Some((_, p)) = &v.remote {
            canvas.text(
                Rect::new(190, 560, 884, 95),
                "Open server position?",
                BODY.bold(),
            );
            canvas.text(
                Rect::new(190, 675, 884, 210),
                &format!(
                    "PDF source page {}\nFrom {}\nLocal zoom and reading data are kept.",
                    p.progress, p.device
                ),
                BODY.lines(5),
            );
            canvas.button(OPEN, "Open page", false, false);
            canvas.button(KEEP, "Keep page", false, false);
        }
    }
    pub(super) fn kosync_popup_input(&mut self, input: Input) -> Option<Change> {
        if self.kosync.job.is_none() && !self.kosync.preview {
            return None;
        }
        if matches!(input, Input::Sleep | Input::Wake) {
            return None;
        }
        let command = match input {
            Input::PageBack => Command::Cancel,
            Input::Tap(x, y)
                if self.kosync.job.is_some()
                    && (CANCEL.contains(x, y) || !DIALOG.contains(x, y)) =>
            {
                Command::Cancel
            }
            Input::Tap(x, y) if self.kosync.preview && OPEN.contains(x, y) => Command::Receive,
            Input::Tap(x, y)
                if self.kosync.preview && (KEEP.contains(x, y) || !DIALOG.contains(x, y)) =>
            {
                Command::Cancel
            }
            _ => return Some(Change::None),
        };
        Some(match self.kosync_command(command) {
            Ok(()) => Change::Partial,
            Err(e) => self.report(e),
        })
    }
}
const DIALOG: Rect = Rect::new(140, 490, 984, 650);
const CANCEL: Rect = Rect::new(270, 930, 724, 110);
const OPEN: Rect = Rect::new(190, 950, 422, 110);
const KEEP: Rect = Rect::new(652, 950, 422, 110);
#[cfg(test)]
mod tests;

const SHEET: Rect = Rect::new(80, 420, 1104, 820);
const SETTINGS: Rect = Rect::new(70, 135, 1124, 1440);
const EDIT: Rect = Rect::new(80, 100, 1104, 480);
const EDIT_BACK: Rect = Rect::new(934, 125, 210, 80);
fn sheet_back(settings: bool) -> Rect {
    let s = if settings { SETTINGS } else { SHEET };
    Rect::new(s.right() - 245, s.y + 22, 210, 80)
}
fn tile(index: usize, settings: bool) -> Rect {
    if settings {
        Rect::new(
            105 + (index % 2) as i32 * 537,
            265 + (index / 2) as i32 * 242,
            512,
            222,
        )
    } else {
        Rect::new(
            115 + (index % 3) as i32 * 349,
            550 + (index / 3) as i32 * 270,
            324,
            250,
        )
    }
}
