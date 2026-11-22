//! Reading-position controls over the native WebDAV progress protocol.
use std::path::PathBuf;
use std::time::Instant;

use primitive_app::{Change, HEIGHT, WIDTH, with_storage};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, Input, Layout, Press, Rect, header, keyboard_key,
};

use super::{Action, Item, Reader, Result, Screen};
use crate::progress_sync::{
    self as wire, Config, Decision, Job, Operation, Record, Remote, Reply, Request, Seen,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Folder,
    Username,
    Password,
    Device,
}
impl Field {
    pub fn title(self) -> &'static str {
        match self {
            Self::Folder => "WebDAV folder",
            Self::Username => "Sync username",
            Self::Password => "Sync password",
            Self::Device => "Device name",
        }
    }
    fn limit(self) -> usize {
        match self {
            Self::Folder | Self::Password => 2048,
            Self::Username => 512,
            Self::Device => 64,
        }
    }
}

#[derive(Clone)]
pub(super) enum Command {
    Open,
    Edit(Field),
    Toggle,
    LocalHttp,
    Sync,
    Fetch,
    Send,
    Receive,
    Cancel,
    ConfirmDelete,
    Delete,
    SignOut,
}
#[derive(Clone, Copy)]
enum Intent {
    Sync,
    Fetch,
    Send,
    Delete,
}

pub(super) struct View {
    config: Config,
    path: PathBuf,
    local: Option<Record>,
    remote: Option<Remote>,
    job: Option<Job>,
    intent: Intent,
    conflict: bool,
    delete_confirmation: bool,
    #[cfg(test)]
    test_worker: Option<PathBuf>,
}

impl View {
    pub fn load(path: PathBuf) -> Self {
        let mut config = Config::load(&path);
        if config.device.is_empty() {
            config.device = "Kindle".into();
        }
        Self {
            config,
            path,
            local: None,
            remote: None,
            job: None,
            intent: Intent::Fetch,
            conflict: false,
            delete_confirmation: false,
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
        self.job.as_ref().map(|job| job.poll_at)
    }
}

impl Reader {
    fn local_sync_record(&self) -> Result<Record> {
        let id = self
            .open_book_id()
            .ok_or("Open a book before syncing its position")?;
        let size = self
            .active
            .as_ref()
            .map(|b| b.entry.size)
            .or_else(|| self.fixed.as_ref().map(|b| b.entry.size))
            .ok_or("No document is open")?;
        let position = self
            .current_position()
            .ok_or("The current page is not ready")?;
        Ok(Record {
            version: 1,
            edition: wire::edition(id, size),
            book: id,
            position: Some(position),
            device: self.progress_sync.config.device.clone(),
            updated_at: crate::mark_time::now().and_then(|n| n.try_into().ok()),
        })
    }

    pub(super) fn sync_items(&self) -> Vec<Item> {
        let view = &self.progress_sync;
        if self.screen == Screen::ProgressSyncSettings {
            return vec![
                Item::new("Progress sync", Action::ProgressSync(Command::Toggle))
                    .detail(if view.config.enabled { "On" } else { "Off" }),
                Item::new(
                    "WebDAV folder URL",
                    Action::ProgressSync(Command::Edit(Field::Folder)),
                )
                .detail(if view.config.folder.is_empty() {
                    "Not set"
                } else {
                    &view.config.folder
                }),
                Item::new(
                    "Username",
                    Action::ProgressSync(Command::Edit(Field::Username)),
                )
                .detail(if view.config.username.is_empty() {
                    "Not set"
                } else {
                    &view.config.username
                }),
                Item::new(
                    "Password",
                    Action::ProgressSync(Command::Edit(Field::Password)),
                )
                .detail(if view.config.password.is_empty() {
                    "Not set"
                } else {
                    "Saved on this Kindle"
                }),
                Item::new(
                    "Device name",
                    Action::ProgressSync(Command::Edit(Field::Device)),
                )
                .detail(&view.config.device),
                Item::new(
                    "Allow HTTP for local servers",
                    Action::ProgressSync(Command::LocalHttp),
                )
                .detail(if view.config.allow_local_http {
                    "On"
                } else {
                    "Off · Use HTTPS"
                }),
                Item::new("Sign out", Action::ProgressSync(Command::SignOut))
                    .detail("Turn off sync and remove saved credentials"),
            ];
        }
        if view.job.is_some() {
            return vec![
                Item::new("Syncing reading position", Action::Heading)
                    .detail("Keep reading data until the server confirms"),
                Item::new("Cancel sync", Action::ProgressSync(Command::Cancel)),
            ];
        }
        let mut items = vec![
            Item::new("KOReader server", Action::KoSync(super::kosync::Command::Open)).detail("Separate account · Manual PDF source pages"),
            Item::new("Sync settings", Action::Show(Screen::ProgressSyncSettings)).detail(
                if view.config.enabled {
                    "WebDAV · On"
                } else {
                    "Off · No background sync"
                },
            ),
        ];
        if let Some(book) = self.open_book_id().and_then(|id| self.library.state(id)) {
            items.push(
                Item::new(&book.title, Action::Heading).detail("This book's reading position"),
            );
        }
        if self.open_book_id().is_some() {
            items.extend([
                Item::new("Sync now", Action::ProgressSync(Command::Sync))
                    .detail("Compare this book's local and server positions"),
                Item::new(
                    "Check server position",
                    Action::ProgressSync(Command::Fetch),
                )
                .detail("Preview before opening or replacing it"),
                Item::new(
                    "Send this device's position",
                    Action::ProgressSync(Command::Send),
                ),
            ]);
            if let Some(remote) = &view.remote {
                items.push(
                    Item::new("Server position", Action::Heading).detail(summary(&remote.record)),
                );
                if remote.record.position.is_some() {
                    items.push(Item::new(
                        "Open server position",
                        Action::ProgressSync(Command::Receive),
                    ));
                    items.push(Item::new(
                        "Remove server position",
                        Action::ProgressSync(Command::ConfirmDelete),
                    ));
                }
            }
        } else {
            items.push(Item::new(
                "Open a book to sync its reading position",
                Action::Heading,
            ));
        }
        items
    }

    pub(super) fn progress_sync_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.cancel_progress_sync();
                self.progress_sync.remote = None;
                self.show(Screen::ProgressSync);
            }
            Command::Edit(field) => {
                self.cancel_progress_sync();
                let text = match field {
                    Field::Folder => &self.progress_sync.config.folder,
                    Field::Username => &self.progress_sync.config.username,
                    Field::Password => &self.progress_sync.config.password,
                    Field::Device => &self.progress_sync.config.device,
                };
                self.editor = Editor::new(field.limit());
                self.editor.text = text.chars().collect();
                self.editor.caret = self.editor.text.len();
                self.show(Screen::SyncEdit(field));
            }
            Command::Toggle => {
                self.cancel_progress_sync();
                let mut config = self.progress_sync.config.clone();
                config.enabled = !config.enabled;
                if config.enabled {
                    config.validate().map_err(std::io::Error::other)?;
                }
                self.progress_sync.persist(config)?;
            }
            Command::SignOut => {
                self.cancel_progress_sync();
                let mut config = self.progress_sync.config.clone();
                config.enabled = false;
                config.username.clear();
                config.password.clear();
                config.seen.clear();
                self.progress_sync.persist(config)?;
                self.progress_sync.remote = None;
                self.notice = Some("Signed out. Local progress and notes are kept.".into());
            }
            Command::LocalHttp => {
                self.cancel_progress_sync();
                let mut config = self.progress_sync.config.clone();
                config.allow_local_http = !config.allow_local_http;
                if !config.allow_local_http && config.folder.starts_with("http://") {
                    config.enabled = false;
                }
                self.progress_sync.persist(config)?;
            }
            Command::Sync | Command::Fetch => {
                self.record_position();
                self.save_library()?;
                self.progress_sync.local = Some(self.local_sync_record()?);
                self.progress_sync.remote = None;
                self.progress_sync.intent = if matches!(command, Command::Sync) {
                    Intent::Sync
                } else {
                    Intent::Fetch
                };
                self.start_progress_request(Operation::Fetch)?;
            }
            Command::Send => {
                self.record_position();
                self.save_library()?;
                self.progress_sync.local = Some(self.local_sync_record()?);
                let record = self
                    .progress_sync
                    .local
                    .clone()
                    .ok_or("The current position is not ready")?;
                let expected = self.progress_sync.remote.as_ref().map(|r| r.etag.clone());
                self.progress_sync.intent = Intent::Send;
                self.start_progress_request(Operation::Put { record, expected })?;
            }
            Command::Receive => {
                let remote = self
                    .progress_sync
                    .remote
                    .clone()
                    .ok_or("There is no server position to open")?;
                self.open_synced_position(&remote)?;
            }
            Command::Cancel => {
                self.cancel_progress_sync();
                self.progress_sync.conflict = false;
                self.progress_sync.delete_confirmation = false;
                self.notice = Some("Sync cancelled. The current page is kept.".into());
            }
            Command::ConfirmDelete => self.progress_sync.delete_confirmation = true,
            Command::Delete => {
                let expected = self.progress_sync.remote.as_ref().map(|r| r.etag.clone());
                if self.progress_sync.local.is_none() {
                    return Err("Check this book's server position first".into());
                }
                self.progress_sync.intent = Intent::Delete;
                self.start_progress_request(Operation::Delete { expected })?;
            }
        }
        Ok(())
    }

    fn start_progress_request(&mut self, operation: Operation) -> Result<()> {
        self.progress_sync.conflict = false;
        self.progress_sync.delete_confirmation = false;
        let current = self.local_sync_record()?;
        let local = self
            .progress_sync
            .local
            .clone()
            .unwrap_or_else(|| current.clone());
        if local.book != current.book || local.edition != current.edition {
            return Err("The book changed; check its server position again".into());
        }
        if let Some(remote) = &self.progress_sync.remote {
            remote
                .record
                .validate(&current.edition, current.book)
                .map_err(std::io::Error::other)?;
        }
        if let Operation::Put { record, .. } = &operation {
            record
                .validate(&current.edition, current.book)
                .map_err(std::io::Error::other)?;
        }
        let request = Request {
            config: self.progress_sync.config.clone(),
            edition: local.edition.clone(),
            book: local.book,
            operation,
        };
        #[cfg(test)]
        let job = if let Some(exe) = &self.progress_sync.test_worker {
            Job::spawn(std::process::Command::new(exe), &request)?
        } else {
            Job::start(&request)?
        };
        #[cfg(not(test))]
        let job = Job::start(&request)?;
        self.progress_sync.local = Some(local);
        self.progress_sync.job = Some(job);
        self.show(Screen::ProgressSync);
        Ok(())
    }

    pub(super) fn cancel_progress_sync(&mut self) {
        self.progress_sync.job.take();
    }
    pub(super) fn leave_progress_sync(&mut self) {
        self.cancel_progress_sync();
        self.progress_sync.local = None;
        self.progress_sync.remote = None;
        self.progress_sync.conflict = false;
        self.progress_sync.delete_confirmation = false;
    }

    fn remember_server_position(&mut self, remote: &Remote) -> Result<()> {
        let mut config = self.progress_sync.config.clone();
        config.seen.insert(
            remote.record.edition.clone(),
            Seen {
                etag: remote.etag.clone(),
                position: remote.record.position.clone(),
            },
        );
        self.progress_sync.persist(config)
    }

    pub(super) fn poll_progress_sync(&mut self, now: Instant) -> Option<Change> {
        let reply = self.progress_sync.job.as_mut()?.poll(now)?;
        self.progress_sync.job.take();
        Some(
            match reply
                .and_then(|reply| self.finish_progress_sync(reply).map_err(|e| e.to_string()))
            {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
        )
    }

    fn finish_progress_sync(&mut self, reply: Reply) -> Result<()> {
        let captured = self
            .progress_sync
            .local
            .clone()
            .ok_or("The book was closed during sync")?;
        let local = self.local_sync_record()?;
        if local.book != captured.book || local.edition != captured.edition {
            return Err("The book changed during sync".into());
        }
        self.progress_sync.local = Some(local.clone());
        match reply {
            Reply::Found(remote) => {
                remote
                    .record
                    .validate(&local.edition, local.book)
                    .map_err(std::io::Error::other)?;
                let decision = wire::decide(
                    local
                        .position
                        .as_ref()
                        .ok_or("The local position is not ready")?,
                    Some(&remote),
                    self.progress_sync.config.seen.get(&local.edition),
                );
                self.progress_sync.remote = Some(remote.clone());
                if matches!(self.progress_sync.intent, Intent::Fetch) {
                    self.notice = Some("Server position loaded. Choose whether to open it.".into());
                } else {
                    match decision {
                        Decision::Equal => {
                            self.remember_server_position(&remote)?;
                            self.notice = Some("Reading positions match".into());
                        }
                        Decision::Send => self.progress_sync_command(Command::Send)?,
                        Decision::Receive => self.open_synced_position(&remote)?,
                        Decision::Cleared => {
                            self.remember_server_position(&remote)?;
                            self.notice = Some(
                                "The shared position is cleared. Local reading is kept.".into(),
                            );
                        }
                        _ => self.progress_sync.conflict = true,
                    }
                }
            }
            Reply::Missing => {
                self.progress_sync.remote = None;
                if matches!(self.progress_sync.intent, Intent::Fetch) {
                    self.notice = Some("No reading position is saved on the server".into());
                } else if self.progress_sync.config.seen.contains_key(&local.edition) {
                    self.progress_sync.conflict = true;
                } else {
                    self.progress_sync_command(Command::Send)?;
                }
            }
            Reply::Stored(remote) => {
                remote
                    .record
                    .validate(&local.edition, local.book)
                    .map_err(std::io::Error::other)?;
                self.remember_server_position(&remote)?;
                self.progress_sync.remote = Some(remote);
                self.notice = Some("Reading position saved on the server".into());
            }
            Reply::Deleted(remote) => {
                self.remember_server_position(&remote)?;
                self.progress_sync.remote = Some(remote);
                self.notice =
                    Some("Server position removed. This device keeps its current page.".into());
            }
            Reply::Changed => {
                self.progress_sync.intent = Intent::Fetch;
                self.start_progress_request(Operation::Fetch)?;
                self.notice =
                    Some("Another device changed the position. Check the latest version.".into());
            }
        }
        Ok(())
    }

    fn open_synced_position(&mut self, remote: &Remote) -> Result<()> {
        let local = self.local_sync_record()?;
        remote
            .record
            .validate(&local.edition, local.book)
            .map_err(std::io::Error::other)?;
        let target = remote
            .record
            .position
            .as_ref()
            .ok_or("The server position was cleared")?;
        let previous = self
            .current_history_location()
            .ok_or("The current page is not ready")?;
        if let Some(active) = &self.active {
            if target.chapter >= active.reading.probe.chapter_count() {
                return Err("The server chapter is outside this book".into());
            }
            let candidate = crate::session::Reading::restore(
                active.reading.config.clone(),
                target.chapter,
                target.page,
                target.anchor.as_ref(),
            )?;
            if let Some(anchor) = &target.anchor {
                if candidate.probe.page_for(anchor).is_none() {
                    return Err("The server passage cannot be resolved in this edition".into());
                }
            } else if candidate.anchor().is_some() || target.page != 0 {
                return Err("A reflowed server position needs a source passage".into());
            }
            // Persist the requested position before changing the displayed session.
            let prior = self
                .library
                .state(local.book)
                .ok_or("This book is no longer tracked")?
                .clone();
            self.library.state_mut(local.book).unwrap().position = Some(target.clone());
            if let Err(error) = self.library.persist() {
                *self.library.state_mut(local.book).unwrap() = prior;
                return Err(error.into());
            }
            self.active.as_mut().unwrap().reading = candidate;
            self.show(Screen::Book);
            self.record_position();
        } else {
            let fixed = self.fixed.as_ref().ok_or("Open a document first")?;
            if target.chapter != 0
                || target.anchor.is_some()
                || target.page >= fixed.reading.count()
            {
                return Err("The server page is outside this document".into());
            }
            let prior = self
                .library
                .state(local.book)
                .ok_or("This document is no longer tracked")?
                .clone();
            self.jump_fixed(target.page, None)?;
            if let Err(error) = self.save_library() {
                let _ = self.go_to_location(&previous);
                *self.library.state_mut(local.book).unwrap() = prior;
                return Err(error.into());
            }
        }
        self.push_history(previous);
        self.selection = None;
        self.pdf_selection = None;
        // A failed version save still leaves local progress equal to the server;
        // a later fetch can acknowledge it without replacing either side.
        if let Err(error) = self.remember_server_position(remote) {
            self.notice = Some(format!(
                "Position opened; its sync version could not be saved: {error}"
            ));
        } else {
            self.notice = Some("Opened the server reading position".into());
        }
        Ok(())
    }

    pub(super) fn draw_sync_editor(&self, canvas: &mut Canvas, field: Field) {
        header(canvas, field.title(), "Back");
        canvas.single_line_field(
            Rect::new(40, 154, 1184, 130),
            &self.editor,
            field.title(),
            field == Field::Password,
        );
        canvas.keyboard(self.editor.layer, "Save", None);
        if let Some(notice) = &self.notice {
            canvas.text(Rect::new(40, 320, 1184, 150), notice, CAPTION.lines(3));
        }
    }

    pub(super) fn sync_editor_input(&mut self, input: Input, field: Field) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.show(Screen::ProgressSyncSettings);
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
            let mut config = self.progress_sync.config.clone();
            let value = self.editor.string();
            match field {
                Field::Folder => config.folder = value.trim().to_owned(),
                Field::Username => config.username = value,
                Field::Password => config.password = value,
                Field::Device => config.device = value.trim().to_owned(),
            }
            if matches!(field, Field::Folder) && !config.folder.is_empty() {
                let mut checking = config.clone();
                checking.enabled = true;
                if let Err(error) = checking.validate() {
                    return self.report(error);
                }
            }
            if matches!(field, Field::Username) && config.username.contains(':') {
                return self.report("A WebDAV username cannot contain a colon");
            }
            if field == Field::Folder && config.folder.is_empty() {
                config.enabled = false;
            }
            if config.enabled
                && let Err(error) = config.validate()
            {
                return self.report(error);
            }
            let account_changed = config.folder != self.progress_sync.config.folder
                || config.username != self.progress_sync.config.username
                || config.password != self.progress_sync.config.password;
            if account_changed {
                config.seen.clear();
            }
            return match self.progress_sync.persist(config) {
                Ok(()) => {
                    if account_changed {
                        self.progress_sync.remote = None;
                        self.progress_sync.local = None;
                    }
                    self.show(Screen::ProgressSyncSettings);
                    Change::Partial
                }
                Err(error) => self.report(error),
            };
        }
        Change::Partial
    }

    pub(super) fn draw_sync_dialog(&self, canvas: &mut Canvas) {
        let view = &self.progress_sync;
        if view.job.is_some() {
            canvas.popup(WAITING);
            canvas.text(
                Rect::new(210, 640, 844, 100),
                "Syncing reading position",
                BODY.bold(),
            );
            let phase = match view.intent {
                Intent::Sync | Intent::Fetch => "Checking the server position",
                Intent::Send => "Saving this device's position",
                Intent::Delete => "Clearing the shared position",
            };
            canvas.text(
                Rect::new(210, 760, 844, 150),
                &format!("{phase}\nThis may take up to 40 seconds."),
                BODY.lines(3),
            );
            canvas.button(WAIT_CANCEL, "Cancel", false, false);
            return;
        }
        if !view.conflict && !view.delete_confirmation {
            return;
        }
        canvas.popup(DIALOG);
        canvas.text(
            Rect::new(180, 505, 904, 80),
            if view.delete_confirmation {
                "Remove server position?"
            } else {
                "Choose reading position"
            },
            BODY.bold(),
        );
        let local = view.local.as_ref().map(summary).unwrap_or_default();
        let remote = view
            .remote
            .as_ref()
            .map(|r| summary(&r.record))
            .unwrap_or_else(|| "The server position was removed".into());
        let text = if view.delete_confirmation {
            "This removes only the saved server position. This device keeps its page, bookmarks and notes.".into()
        } else {
            format!("This device: {local}\n\nServer: {remote}")
        };
        canvas.text(Rect::new(180, 625, 904, 315), &text, BODY.lines(7));
        canvas.button(
            LOCAL,
            if view.delete_confirmation {
                "Remove"
            } else {
                "Keep this device"
            },
            true,
            false,
        );
        if !view.delete_confirmation {
            canvas.button(
                REMOTE,
                if view.remote.is_some() {
                    "Use server"
                } else {
                    "Keep server clear"
                },
                false,
                false,
            );
        }
        canvas.button(CANCEL, "Cancel", false, false);
    }

    pub(super) fn sync_dialog_input(&mut self, input: Input) -> Option<Change> {
        if self.progress_sync.job.is_some() {
            let cancel = match input {
                Input::Sleep | Input::Wake => return None,
                Input::PageBack => true,
                Input::Tap(x, y) => WAIT_CANCEL.contains(x, y) || !WAITING.contains(x, y),
                _ => false,
            };
            return Some(if cancel {
                match self.progress_sync_command(Command::Cancel) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                }
            } else {
                Change::None
            });
        }
        if !self.progress_sync.conflict && !self.progress_sync.delete_confirmation {
            return None;
        }
        let command = match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => Command::Cancel,
            Input::Tap(x, y) if CANCEL.contains(x, y) || !DIALOG.contains(x, y) => Command::Cancel,
            Input::Tap(x, y) if LOCAL.contains(x, y) => {
                if self.progress_sync.delete_confirmation {
                    Command::Delete
                } else {
                    Command::Send
                }
            }
            Input::Tap(x, y)
                if REMOTE.contains(x, y) && !self.progress_sync.delete_confirmation =>
            {
                if self.progress_sync.remote.is_none() {
                    Command::Delete
                } else {
                    Command::Receive
                }
            }
            _ => return Some(Change::None),
        };
        self.progress_sync.conflict = false;
        self.progress_sync.delete_confirmation = false;
        Some(match self.progress_sync_command(command) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }
}

fn summary(record: &Record) -> String {
    let Some(position) = &record.position else {
        return "The shared position is cleared".into();
    };
    if position.anchor.is_some() {
        format!(
            "{} · Chapter {} · Page {}",
            record.device,
            position.chapter + 1,
            position.page + 1
        )
    } else {
        format!("{} · Page {}", record.device, position.page + 1)
    }
}
const DIALOG: Rect = Rect::new(120, 450, 1024, 810);
const LOCAL: Rect = Rect::new(180, 970, 438, 110);
const REMOTE: Rect = Rect::new(646, 970, 438, 110);
const CANCEL: Rect = Rect::new(470, 1110, 324, 100);
const WAITING: Rect = Rect::new(160, 580, 944, 540);
const WAIT_CANCEL: Rect = Rect::new(470, 960, 324, 110);

#[cfg(test)]
mod tests;

#[cfg(test)]
mod extra_tests;
