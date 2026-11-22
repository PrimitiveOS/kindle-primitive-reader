//! Catalog configuration, explicit browsing and acquiring a publication into Documents.
use super::*;
use crate::opds::{self as catalog, Auth, Catalog, Entry, Feed, Job, Link, Parsed, Reply, Request};
use primitive_ui::{Sheet, SheetAction, TextStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Catalogs,
    Settings,
    Editor,
    Feed,
    Details,
    Formats,
    Folder,
    Loading,
    Complete,
}
impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Catalogs => "Online catalogs",
            Self::Settings => "Catalog settings",
            Self::Editor => "Edit catalog",
            Self::Feed => "Catalog",
            Self::Details => "Book information",
            Self::Formats => "Download format",
            Self::Folder => "Download folder",
            Self::Loading => "Catalog request",
            Self::Complete => "Download complete",
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Field {
    Name,
    Url,
    Username,
    Password,
    Search,
}
impl Field {
    fn title(self) -> &'static str {
        match self {
            Self::Name => "Catalog name",
            Self::Url => "Catalog address",
            Self::Username => "Username",
            Self::Password => "Password",
            Self::Search => "Search catalog",
        }
    }
}
#[derive(Clone)]
pub(super) enum Command {
    Open,
    Add,
    Edit(usize),
    Connect(usize),
    Field(Field),
    Save,
    Remove,
    Fetch(String, bool),
    Select(usize),
    Formats,
    Download(usize),
    Search,
    Folder(PathBuf),
    UseFolder,
    Cancel,
    OpenDownloaded,
}
struct Transfer {
    temporary: PathBuf,
    title: String,
    link: Link,
}
pub(super) struct Panel {
    root: PathBuf,
    path: PathBuf,
    settings: catalog::Settings,
    draft: Catalog,
    edit_index: Option<usize>,
    selected_catalog: Option<usize>,
    field: Field,
    search_template: Option<String>,
    feed: Option<Feed>,
    history: Vec<Feed>,
    selected: Option<Entry>,
    folder: PathBuf,
    folder_back: Page,
    pub job: Option<Job>,
    transfer: Option<Transfer>,
    push_on_reply: bool,
    confirm_remove: bool,
    pub message: Option<String>,
    #[cfg(test)]
    worker: Option<PathBuf>,
}
impl Panel {
    pub fn load(root: &Path, path: PathBuf) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        let mut settings = catalog::Settings::load(&path);
        if catalog::folder(&root, &settings.folder).is_err() {
            settings.folder = PathBuf::new();
        }
        if let Some(pending) = settings.pending.take() {
            catalog::discard(&root, &pending);
            settings.save(&path)?;
        }
        let folder = settings.folder.clone();
        Ok(Self {
            root,
            path,
            settings,
            draft: Catalog::default(),
            edit_index: None,
            selected_catalog: None,
            field: Field::Name,
            search_template: None,
            feed: None,
            history: Vec::new(),
            selected: None,
            folder,
            folder_back: Page::Catalogs,
            job: None,
            transfer: None,
            push_on_reply: false,
            confirm_remove: false,
            message: None,
            #[cfg(test)]
            worker: None,
        })
    }
    fn auth(&self) -> Option<Auth> {
        Auth::catalog(self.settings.catalogs.get(self.selected_catalog?)?)
    }
    fn save(&self) -> Result<()> {
        with_storage(|| self.settings.save(&self.path))?;
        Ok(())
    }
    pub(super) fn cancel(&mut self) {
        self.confirm_remove = false;
        self.job = None;
        if let Some(transfer) = self.transfer.take() {
            catalog::discard(&self.root, &transfer.temporary);
        }
        if self.settings.pending.take().is_some() {
            let _ = self.save();
        }
    }
    fn start(&self, request: &Request) -> Result<Job> {
        #[cfg(test)]
        if let Some(path) = &self.worker {
            let mut command = std::process::Command::new(path);
            command.arg("--opds-worker");
            return Job::spawn(command, request);
        }
        Job::start(request)
    }
    fn request(&mut self, url: String, push: bool) -> Result<()> {
        self.cancel();
        let request = Request::Feed {
            url,
            auth: self.auth(),
        };
        self.job = Some(self.start(&request)?);
        self.push_on_reply = push;
        self.message = Some("Fetching catalog".into());
        Ok(())
    }
}
fn row(title: impl Into<String>, command: Command) -> Item {
    Item::new(title, Action::Catalog(command))
}
impl Reader {
    pub(super) fn cancel_catalog_for_sleep(&mut self) {
        self.catalog.cancel();
        if self.screen == Screen::Catalog(Page::Loading) {
            self.show(Screen::Catalog(if self.catalog.feed.is_some() {
                Page::Feed
            } else {
                Page::Catalogs
            }));
            self.notice = Some("Catalog request cancelled for sleep".into());
        }
    }
    pub(super) fn catalog_items(&self, page: Page) -> Vec<Item> {
        let panel = &self.catalog;
        match page {
            Page::Catalogs => {
                let mut rows = vec![
                    row("Add catalog", Command::Add),
                    row(
                        "Download folder",
                        Command::Folder(panel.settings.folder.clone()),
                    )
                    .detail(if panel.settings.folder.as_os_str().is_empty() {
                        "Documents".into()
                    } else {
                        panel.settings.folder.display().to_string()
                    }),
                ];
                for (index, catalog) in panel.settings.catalogs.iter().enumerate() {
                    rows.push(row(&catalog.name, Command::Connect(index)).detail(&catalog.url));
                    rows.push(row(format!("Edit {}", catalog.name), Command::Edit(index)));
                }
                if panel.settings.last_download.is_some() {
                    rows.push(row("Open last download", Command::OpenDownloaded));
                }
                rows
            }
            Page::Settings => vec![
                row("Name", Command::Field(Field::Name)).detail(&panel.draft.name),
                row("Address", Command::Field(Field::Url)).detail(&panel.draft.url),
                row("Username", Command::Field(Field::Username)).detail(
                    if panel.draft.username.is_empty() {
                        "None"
                    } else {
                        &panel.draft.username
                    },
                ),
                row("Password", Command::Field(Field::Password)).detail(
                    if panel.draft.password.is_empty() {
                        "None"
                    } else {
                        "Saved on this Kindle"
                    },
                ),
                row("Save catalog", Command::Save),
                row("Remove catalog", Command::Remove),
            ],
            Page::Feed => {
                let Some(feed) = &panel.feed else {
                    return Vec::new();
                };
                let mut rows = vec![Item::new(&feed.title, Action::Heading)];
                if feed.search.is_some() {
                    rows.push(row("Search catalog", Command::Search));
                }
                if let Some(link) = &feed.next {
                    rows.push(row(
                        "Next catalog page",
                        Command::Fetch(link.url.clone(), true),
                    ));
                }
                if let Some(link) = &feed.previous {
                    rows.push(row(
                        "Previous catalog page",
                        Command::Fetch(link.url.clone(), true),
                    ));
                }
                if let Some(link) = &feed.start {
                    rows.push(row("Catalog home", Command::Fetch(link.url.clone(), true)));
                }
                rows.push(row(
                    "Refresh catalog",
                    Command::Fetch(feed.url.clone(), false),
                ));
                rows.extend(feed.entries.iter().enumerate().map(|(index, entry)| {
                    row(&entry.title, Command::Select(index)).detail(
                        if entry.navigation.is_some() && entry.acquisitions.is_empty() {
                            "Browse collection".to_owned()
                        } else if entry.author.is_empty() {
                            "Publication".to_owned()
                        } else {
                            entry.author.clone()
                        },
                    )
                }));
                rows
            }
            Page::Formats => panel.selected.as_ref().map_or_else(Vec::new, |entry| {
                let mut rows = vec![
                    row(
                        "Download folder",
                        Command::Folder(panel.settings.folder.clone()),
                    )
                    .detail(if panel.settings.folder.as_os_str().is_empty() {
                        "Documents".into()
                    } else {
                        panel.settings.folder.display().to_string()
                    }),
                ];
                rows.extend(entry.acquisitions.iter().enumerate().map(|(index, link)| {
                    if let Some(extension) = link.extension() {
                        row(
                            format!("Download {}", extension.to_uppercase()),
                            Command::Download(index),
                        )
                        .detail(if link.title.is_empty() {
                            link.media_type.clone()
                        } else {
                            link.title.clone()
                        })
                    } else {
                        Item::new(
                            if link.title.is_empty() {
                                "Acquisition needs another service"
                            } else {
                                &link.title
                            },
                            Action::Heading,
                        )
                        .detail(&link.media_type)
                    }
                }));
                if let Some(link) = &entry.navigation {
                    rows.push(row(
                        "More publication details",
                        Command::Fetch(link.url.clone(), true),
                    ));
                }
                rows
            }),
            Page::Folder => {
                let mut rows = vec![row("Use this folder", Command::UseFolder).detail(
                    if panel.folder.as_os_str().is_empty() {
                        "Documents".into()
                    } else {
                        panel.folder.display().to_string()
                    },
                )];
                if let Some(parent) = panel.folder.parent() {
                    rows.push(row("Up", Command::Folder(parent.into())));
                }
                if let Ok(path) = catalog::folder(&panel.root, &panel.folder)
                    && let Ok(entries) = with_storage(|| fs::read_dir(path))
                {
                    let mut directories = entries
                        .filter_map(|entry| {
                            let entry = entry.ok()?;
                            let kind = entry.file_type().ok()?;
                            (kind.is_dir()
                                && !kind.is_symlink()
                                && catalog::folder(
                                    &panel.root,
                                    &panel.folder.join(entry.file_name()),
                                )
                                .is_ok())
                            .then_some(entry.file_name())
                        })
                        .collect::<Vec<_>>();
                    directories.sort();
                    rows.extend(directories.into_iter().map(|name| {
                        row(
                            name.to_string_lossy(),
                            Command::Folder(panel.folder.join(&name)),
                        )
                    }));
                }
                rows
            }
            Page::Loading => vec![
                Item::new(
                    panel.message.as_deref().unwrap_or("Waiting for catalog"),
                    Action::Heading,
                ),
                row("Cancel", Command::Cancel),
            ],
            Page::Complete => vec![
                Item::new(
                    panel
                        .message
                        .as_deref()
                        .unwrap_or("The publication is ready offline"),
                    Action::Heading,
                ),
                row("Open publication", Command::OpenDownloaded),
                row("Online catalogs", Command::Open),
            ],
            _ => Vec::new(),
        }
    }
    pub(super) fn catalog_action(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open => {
                self.catalog.cancel();
                self.show(Screen::Catalog(Page::Catalogs));
            }
            Command::Add => {
                if self.catalog.settings.catalogs.len() >= 32 {
                    return Err("You can configure up to 32 catalogs".into());
                }
                self.catalog.draft = Catalog::default();
                self.catalog.edit_index = None;
                self.show(Screen::Catalog(Page::Settings));
            }
            Command::Edit(index) => {
                self.catalog.draft = self
                    .catalog
                    .settings
                    .catalogs
                    .get(index)
                    .ok_or("That catalog is unavailable")?
                    .clone();
                self.catalog.edit_index = Some(index);
                self.show(Screen::Catalog(Page::Settings));
            }
            Command::Field(field) => self.edit_catalog_field(field),
            Command::Save => {
                self.catalog.draft.validate()?;
                let mut settings = self.catalog.settings.clone();
                if let Some(index) = self.catalog.edit_index {
                    *settings
                        .catalogs
                        .get_mut(index)
                        .ok_or("That catalog is unavailable")? = self.catalog.draft.clone();
                } else {
                    settings.catalogs.push(self.catalog.draft.clone());
                }
                with_storage(|| settings.save(&self.catalog.path))?;
                self.catalog.settings = settings;
                self.show(Screen::Catalog(Page::Catalogs));
            }
            Command::Remove => {
                if self.catalog.edit_index.is_some() {
                    self.catalog.confirm_remove = true;
                } else {
                    self.show(Screen::Catalog(Page::Catalogs));
                }
            }
            Command::Connect(index) => {
                let catalog = self
                    .catalog
                    .settings
                    .catalogs
                    .get(index)
                    .ok_or("That catalog is unavailable")?
                    .clone();
                self.catalog.selected_catalog = Some(index);
                self.catalog.selected = None;
                self.catalog.search_template = None;
                self.catalog.history.clear();
                self.catalog.feed = None;
                if catalog.url.contains("%s") || catalog.url.contains('{') {
                    self.catalog.search_template = Some(catalog.url.replace("%s", "{searchTerms}"));
                    self.edit_catalog_field(Field::Search);
                } else {
                    self.catalog.request(catalog.url, false)?;
                    self.show(Screen::Catalog(Page::Loading));
                }
            }
            Command::Fetch(url, push) => {
                self.catalog.request(url, push)?;
                self.show(Screen::Catalog(Page::Loading));
            }
            Command::Select(index) => {
                let entry = self
                    .catalog
                    .feed
                    .as_ref()
                    .and_then(|feed| feed.entries.get(index))
                    .ok_or("That entry is unavailable")?
                    .clone();
                if let Some(link) = &entry.navigation
                    && entry.acquisitions.is_empty()
                {
                    self.catalog.request(link.url.clone(), true)?;
                    self.show(Screen::Catalog(Page::Loading));
                } else {
                    self.catalog.selected = Some(entry);
                    self.show(Screen::Catalog(Page::Details));
                }
            }
            Command::Formats => self.show(Screen::Catalog(Page::Formats)),
            Command::Search => {
                let search = self
                    .catalog
                    .feed
                    .as_ref()
                    .and_then(|feed| feed.search.clone())
                    .ok_or("This catalog did not provide search")?;
                if search.url.contains('{') {
                    self.catalog.search_template = Some(search.url);
                    self.edit_catalog_field(Field::Search);
                } else {
                    self.catalog.request(search.url, false)?;
                    self.show(Screen::Catalog(Page::Loading));
                }
            }
            Command::Download(index) => {
                let entry = self
                    .catalog
                    .selected
                    .clone()
                    .ok_or("Choose a publication first")?;
                let link = entry
                    .acquisitions
                    .get(index)
                    .ok_or("That acquisition is unavailable")?
                    .clone();
                let extension = link
                    .extension()
                    .ok_or("This acquisition needs another application or service")?;
                let temporary = with_storage(|| {
                    catalog::temporary(&self.catalog.root, &self.catalog.settings.folder, extension)
                })?;
                self.catalog.cancel();
                self.catalog.settings.pending = Some(temporary.clone());
                self.catalog.save()?;
                let transfer = Transfer {
                    temporary: temporary.clone(),
                    title: entry.title.clone(),
                    link: link.clone(),
                };
                let request = Request::Download {
                    link,
                    auth: self.catalog.auth(),
                    root: self.catalog.root.clone(),
                    temporary,
                };
                self.catalog.transfer = Some(transfer);
                match self.catalog.start(&request) {
                    Ok(job) => self.catalog.job = Some(job),
                    Err(error) => {
                        self.catalog.cancel();
                        return Err(error);
                    }
                }
                self.catalog.message = Some("Downloading publication — maximum 64 MiB".into());
                self.show(Screen::Catalog(Page::Loading));
            }
            Command::Folder(folder) => {
                if self.screen != Screen::Catalog(Page::Folder) {
                    self.catalog.folder_back = if self.screen == Screen::Catalog(Page::Formats) {
                        Page::Formats
                    } else {
                        Page::Catalogs
                    };
                }
                with_storage(|| catalog::folder(&self.catalog.root, &folder))?;
                self.catalog.folder = folder;
                self.show(Screen::Catalog(Page::Folder));
            }
            Command::UseFolder => {
                with_storage(|| catalog::folder(&self.catalog.root, &self.catalog.folder))?;
                let mut settings = self.catalog.settings.clone();
                settings.folder = self.catalog.folder.clone();
                with_storage(|| settings.save(&self.catalog.path))?;
                self.catalog.settings = settings;
                self.show(Screen::Catalog(self.catalog.folder_back));
            }
            Command::Cancel => {
                self.catalog.cancel();
                self.show(Screen::Catalog(if self.catalog.feed.is_some() {
                    Page::Feed
                } else {
                    Page::Catalogs
                }));
            }
            Command::OpenDownloaded => {
                let path = self
                    .catalog
                    .settings
                    .last_download
                    .as_ref()
                    .ok_or("There is no completed download")?;
                let entry = with_storage(|| self.library.resolve(&self.catalog.root.join(path)))?;
                self.open_book(entry)?;
            }
        }
        Ok(())
    }
    fn edit_catalog_field(&mut self, field: Field) {
        self.catalog.field = field;
        self.editor = Editor::new(if matches!(field, Field::Name) {
            100
        } else {
            4096
        });
        let value = match field {
            Field::Name => &self.catalog.draft.name,
            Field::Url => &self.catalog.draft.url,
            Field::Username => &self.catalog.draft.username,
            Field::Password => &self.catalog.draft.password,
            Field::Search => "",
        };
        self.editor.text = value.chars().collect();
        self.editor.caret = self.editor.text.len();
        self.show(Screen::Catalog(Page::Editor));
    }
    pub(super) fn catalog_back(&mut self, page: Page) {
        if self.catalog.confirm_remove {
            self.catalog.confirm_remove = false;
            return;
        }
        match page {
            Page::Loading => {
                self.catalog.cancel();
                self.show(Screen::Catalog(if self.catalog.feed.is_some() {
                    Page::Feed
                } else {
                    Page::Catalogs
                }));
            }
            Page::Catalogs => self.show(Screen::OrganizeLibrary),
            Page::Settings | Page::Complete => self.show(Screen::Catalog(Page::Catalogs)),
            Page::Editor => self.show(Screen::Catalog(
                if matches!(self.catalog.field, Field::Search) {
                    if self.catalog.feed.is_some() {
                        Page::Feed
                    } else {
                        Page::Catalogs
                    }
                } else {
                    Page::Settings
                },
            )),
            Page::Details => self.show(Screen::Catalog(Page::Feed)),
            Page::Formats => self.show(Screen::Catalog(Page::Details)),
            Page::Folder => self.show(Screen::Catalog(self.catalog.folder_back)),
            Page::Feed => {
                if let Some(feed) = self.catalog.history.pop() {
                    self.catalog.feed = Some(feed);
                    self.show(Screen::Catalog(Page::Feed));
                } else {
                    self.show(Screen::Catalog(Page::Catalogs));
                }
            }
        }
    }
    pub(super) fn draw_catalog(&self, canvas: &mut Canvas, page: Page) -> bool {
        if page == Page::Editor {
            header(canvas, self.catalog.field.title(), "Back");
            canvas.single_line_field(
                Rect::new(LIST_X, LIST_Y, WIDTH as i32 - 2 * LIST_X, 130),
                &self.editor,
                "Enter a value",
                matches!(self.catalog.field, Field::Password),
            );
            if let Some(notice) = &self.notice {
                canvas.text(
                    Rect::new(LIST_X, 310, WIDTH as i32 - 2 * LIST_X, 125),
                    notice,
                    CAPTION.lines(2),
                );
            }
            canvas.keyboard(
                self.editor.layer,
                if matches!(self.catalog.field, Field::Search) {
                    "Search"
                } else {
                    "Apply"
                },
                None,
            );
            return true;
        }
        if page == Page::Details {
            header(canvas, "Book information", "Back");
            if let Some(entry) = &self.catalog.selected {
                canvas.text(
                    Rect::new(40, 168, WIDTH as i32 - 80, 145),
                    &entry.title,
                    TextStyle::new(44).bold().lines(2),
                );
                canvas.text(
                    Rect::new(40, 318, WIDTH as i32 - 80, 100),
                    &entry.author,
                    CAPTION.lines(2),
                );
                canvas.text(
                    Rect::new(40, 450, WIDTH as i32 - 80, 830),
                    &entry.description,
                    BODY.lines(15),
                );
                canvas.button(
                    Rect::new(40, HEIGHT as i32 - 230, WIDTH as i32 - 80, 100),
                    "Available formats",
                    true,
                    false,
                );
            }
            return true;
        }
        false
    }
    pub(super) fn draw_catalog_popup(&self, canvas: &mut Canvas) {
        if self.catalog.confirm_remove {
            Sheet::centered(Rect::new(0,0,WIDTH as i32,HEIGHT as i32),"Remove catalog?","The catalog and its saved credentials will be removed. Downloaded books remain in Documents.","Remove catalog").draw(canvas,None);
        }
    }
    pub(super) fn catalog_input(&mut self, page: Page, input: Input) -> Option<Change> {
        if self.catalog.confirm_remove {
            let Input::Tap(x, y) = input else {
                return Some(Change::None);
            };
            let sheet = Sheet::centered(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), "", "", "");
            if let Some(action) = sheet.hit(x, y) {
                self.catalog.confirm_remove = false;
                if action == SheetAction::Confirm
                    && let Some(index) = self.catalog.edit_index
                {
                    let mut settings = self.catalog.settings.clone();
                    settings.catalogs.remove(index);
                    return Some(match with_storage(|| settings.save(&self.catalog.path)) {
                        Ok(()) => {
                            self.catalog.settings = settings;
                            self.catalog.selected_catalog = None;
                            self.show(Screen::Catalog(Page::Catalogs));
                            Change::Partial
                        }
                        Err(error) => self.report(error),
                    });
                }
                return Some(Change::Partial);
            }
            return Some(Change::None);
        }
        if !matches!(page, Page::Editor | Page::Details) {
            return None;
        }
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.catalog_back(page);
            return Some(Change::Partial);
        }
        let Input::Tap(x, y) = input else {
            return Some(Change::None);
        };
        if page == Page::Details {
            return Some(
                if Rect::new(40, HEIGHT as i32 - 230, WIDTH as i32 - 80, 100).contains(x, y) {
                    match self.catalog_action(Command::Formats) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    }
                } else {
                    Change::None
                },
            );
        }
        let go = if matches!(self.catalog.field, Field::Search) {
            "Search"
        } else {
            "Apply"
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, go, x, y) else {
            return Some(Change::None);
        };
        if !matches!(self.editor.press(key), Press::Go) {
            return Some(Change::Partial);
        }
        let value = self.editor.string();
        let result: Result<()> = if matches!(self.catalog.field, Field::Search) {
            self.catalog
                .search_template
                .as_ref()
                .ok_or("The search template is unavailable".into())
                .and_then(|template| catalog::search_url(template, &value).map_err(Into::into))
                .and_then(|url| {
                    self.catalog.request(url, true)?;
                    self.show(Screen::Catalog(Page::Loading));
                    Ok(())
                })
        } else {
            match self.catalog.field {
                Field::Name => self.catalog.draft.name = value.trim().into(),
                Field::Url => self.catalog.draft.url = value.trim().into(),
                Field::Username => self.catalog.draft.username = value,
                Field::Password => self.catalog.draft.password = value,
                Field::Search => unreachable!(),
            }
            self.show(Screen::Catalog(Page::Settings));
            Ok(())
        };
        Some(match result {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }
    pub(super) fn poll_catalog(&mut self, now: Instant) -> Option<Change> {
        let job = self.catalog.job.as_mut()?;
        if now < job.poll_at {
            return None;
        }
        let reply = job.poll(now)?;
        self.catalog.job = None;
        let result: Result<()> = match reply {
            Ok(Reply::Catalog(Parsed::Feed(feed))) => {
                if self.catalog.push_on_reply
                    && let Some(previous) = self.catalog.feed.take()
                {
                    if self.catalog.history.len() == 8 {
                        self.catalog.history.remove(0);
                    }
                    self.catalog.history.push(previous);
                }
                self.catalog.feed = Some(feed);
                self.catalog.message = None;
                self.show(Screen::Catalog(Page::Feed));
                Ok(())
            }
            Ok(Reply::Catalog(Parsed::Search(template))) => {
                self.catalog.search_template = Some(template);
                self.edit_catalog_field(Field::Search);
                Ok(())
            }
            Ok(Reply::Downloaded { bytes }) => {
                if let Some(transfer) = self.catalog.transfer.take() {
                    match with_storage(|| {
                        catalog::finalize(
                            &self.catalog.root,
                            &transfer.temporary,
                            &transfer.title,
                            &transfer.link,
                        )
                    }) {
                        Ok(path) => {
                            self.catalog.settings.last_download = Some(path);
                            self.catalog.settings.pending = None;
                            self.catalog.message =
                                Some(format!("Saved {} KiB in Documents", bytes.div_ceil(1024)));
                            self.catalog
                                .save()
                                .and_then(|()| self.refresh_browser().map_err(Into::into))
                                .map(|()| self.show(Screen::Catalog(Page::Complete)))
                        }
                        Err(error) => {
                            catalog::discard(&self.catalog.root, &transfer.temporary);
                            self.catalog.settings.pending = None;
                            let _ = self.catalog.save();
                            Err(error.into())
                        }
                    }
                } else {
                    Err("The download has no matching request".into())
                }
            }
            Err(error) => {
                self.catalog.cancel();
                Err(error.into())
            }
        };
        Some(match result {
            Ok(()) => Change::Partial,
            Err(error) => {
                self.catalog.message = Some(error.to_string());
                self.show(Screen::Catalog(if self.catalog.feed.is_some() {
                    Page::Feed
                } else {
                    Page::Catalogs
                }));
                self.report(error)
            }
        })
    }
}
#[cfg(test)]
mod tests;
