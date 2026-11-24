//! Export highlights from History, chosen books or collections without moving the reader.
use super::{Action, Book, BookConfig, Change, Engine, Item, Reader, Result, Screen, with_storage};
use crate::annotation_export::kindle_clippings::{self, Kind, Location};
use crate::highlight_style::{HighlightColor, HighlightStyle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Sources,
    Books,
    Collections,
    Options,
    Colors,
    Formats,
    Running,
}

impl Page {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Sources => "Export highlights",
            Self::Books => "Choose books",
            Self::Collections => "Choose collections",
            Self::Options => "Export options",
            Self::Colors => "Export highlight colors",
            Self::Formats => "Export format",
            Self::Running => "Exporting highlights",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Format {
    Json,
    Text,
    Markdown,
    Html,
    KindleClippings,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Text => "txt",
            Self::Markdown => "md",
            Self::Html => "html",
            Self::KindleClippings => "clippings.txt",
        }
    }
}

#[derive(Clone)]
pub(super) enum Command {
    Open,
    History,
    Choose(Page),
    ToggleBook(u64),
    ToggleCollection(String),
    ToggleAll,
    Apply,
    Style(Option<HighlightStyle>),
    Color(Option<HighlightColor>),
    ToggleKindleClippings,
    Start(Format),
    Cancel,
}

#[derive(Default)]
pub(super) struct Export {
    path: std::path::PathBuf,
    books: BTreeSet<u64>,
    collections: BTreeSet<String>,
    styles: Option<Vec<HighlightStyle>>,
    colors: Option<Vec<HighlightColor>>,
    kindle_clippings: Option<bool>,
    job: Option<Job>,
    saved: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    styles: Option<Vec<HighlightStyle>>,
    colors: Option<Vec<HighlightColor>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredPreferences {
    Legacy(Option<Vec<HighlightStyle>>),
    Current(Preferences),
}

impl Export {
    pub(super) fn load(path: std::path::PathBuf) -> Self {
        let saved: Option<StoredPreferences> = super::read_import_file(&path, 4096)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let saved = match saved {
            Some(StoredPreferences::Legacy(styles)) => Preferences {
                styles,
                colors: None,
            },
            Some(StoredPreferences::Current(current)) => current,
            // A damaged existing filter must not silently export more marks.
            // The visible All colors/All styles controls explicitly reset it.
            None if path.exists() => Preferences {
                styles: Some(Vec::new()),
                colors: Some(Vec::new()),
            },
            None => Preferences::default(),
        };
        let styles = saved.styles.map(|styles| {
            HighlightStyle::ALL
                .into_iter()
                .filter(|style| styles.contains(style))
                .collect()
        });
        let colors = saved.colors.map(|colors| {
            HighlightColor::ALL
                .into_iter()
                .filter(|color| colors.contains(color))
                .collect()
        });
        Self {
            path,
            styles,
            colors,
            ..Self::default()
        }
    }

    pub(super) fn kindle_clippings_enabled(&self) -> bool {
        self.kindle_clippings == Some(true)
    }

    fn save_preferences(&self) -> Result<()> {
        primitive_app::save(
            &self.path,
            &serde_json::to_vec(&Preferences {
                styles: self.styles.clone(),
                colors: self.colors.clone(),
            })?,
        )?;
        Ok(())
    }
}

#[derive(Serialize)]
struct Entry {
    sort: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drawer: Option<HighlightStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<HighlightColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chapter: Option<String>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    primitiveos: serde_json::Value,
}

#[derive(Serialize)]
struct Document {
    title: String,
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<std::path::PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primitiveos_book_id: Option<String>,
    entries: Vec<Entry>,
}

struct PendingBook {
    id: u64,
    source: crate::library::LibraryBook,
    document: Document,
    marks: VecDeque<(Entry, Option<(usize, crate::Anchor)>)>,
    config: Option<BookConfig>,
    probe: Option<(usize, crate::Probe)>,
}

struct Job {
    ids: VecDeque<u64>,
    total: usize,
    done: usize,
    skipped: usize,
    format: Format,
    preferences: super::export_settings::Preferences,
    documents: Vec<Document>,
    kindle_clippings: Vec<Document>,
    pending: Option<PendingBook>,
    next: Instant,
}

fn row(label: impl Into<String>, command: Command) -> Item {
    Item::new(label, Action::LibraryExport(command))
}

fn selected(label: impl Into<String>, command: Command, on: bool) -> Item {
    let item = row(label, command);
    if on { item.detail("Selected") } else { item }
}

fn has_highlights(book: &crate::library::BookState) -> bool {
    !book.highlights.is_empty() || !book.pdf_highlights.is_empty()
}

impl Reader {
    pub(super) fn draw_export_colors(&self, canvas: &mut primitive_ui::Canvas) {
        super::highlight_palette::draw(
            canvas,
            self.library_export.colors.is_none(),
            |color| {
                self.library_export
                    .colors
                    .as_ref()
                    .is_some_and(|colors| colors.contains(&color))
            },
            true,
        );
        if let Some(notice) = &self.notice {
            canvas.text(
                primitive_ui::Rect::new(40, 944, primitive_app::WIDTH as i32 - 80, 180),
                notice,
                primitive_ui::CAPTION.lines(4),
            );
        }
    }
    pub(super) fn export_colors_input(&mut self, input: primitive_ui::Input) -> Change {
        use primitive_ui::Input;
        if input == Input::PageBack
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(primitive_app::WIDTH as i32).contains(x,y))
        {
            self.show(Screen::LibraryExport(Page::Options));
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let Some(color) = super::highlight_palette::at(x, y) else {
            return Change::None;
        };
        match self.activate_library_export(Command::Color(color)) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
    pub(super) fn library_export_items(&self, page: Page) -> Vec<Item> {
        use Command as C;
        let export = &self.library_export;
        match page {
            Page::Sources => vec![
                row("History", C::History),
                row("Choose books", C::Choose(Page::Books)),
                row("Collections", C::Choose(Page::Collections)),
            ],
            Page::Books => {
                let mut rows = vec![
                    row("Use selected books", C::Apply)
                        .detail(format!("{} selected", export.books.len())),
                    row("Select or clear all", C::ToggleAll),
                ];
                rows.extend(
                    self.library
                        .recently_read()
                        .filter(|book| has_highlights(book))
                        .map(|book| {
                            selected(
                                &book.title,
                                C::ToggleBook(book.id),
                                export.books.contains(&book.id),
                            )
                            .detail(book.author.as_deref().unwrap_or("Unknown author"))
                        }),
                );
                // Preserve the selected state alongside the author, including duplicate titles.
                for item in &mut rows[2..] {
                    if let Action::LibraryExport(C::ToggleBook(id)) = item.action
                        && export.books.contains(&id)
                    {
                        item.detail = Some(format!(
                            "Selected · {}",
                            item.detail.as_deref().unwrap_or("")
                        ));
                    }
                }
                rows
            }
            Page::Collections => {
                let mut rows = vec![
                    row("Use selected collections", C::Apply)
                        .detail(format!("{} selected", export.collections.len())),
                ];
                rows.extend(self.library.collections().iter().map(|set| {
                    selected(
                        &set.name,
                        C::ToggleCollection(set.name.clone()),
                        export.collections.contains(&set.name),
                    )
                }));
                rows
            }
            Page::Options => {
                let mut rows = vec![
                    row("Choose export format", C::Choose(Page::Formats)).detail(
                        match export.books.len() {
                            1 => "1 book".to_owned(),
                            count => format!("{count} books"),
                        },
                    ),
                    Item::new(
                        "Export settings",
                        Action::ExportSettings(
                            super::export_settings::Origin::Library,
                            super::export_settings::Command::Open,
                        ),
                    ),
                    row("Highlight colors", C::Choose(Page::Colors)).detail(
                        export.colors.as_ref().map_or_else(
                            || "All colors".into(),
                            |colors| format!("{} selected", colors.len()),
                        ),
                    ),
                    selected(
                        "All highlight styles",
                        C::Style(None),
                        export.styles.is_none(),
                    ),
                ];
                if export.kindle_clippings.is_some() {
                    rows.push(if export.styles.is_none() && export.colors.is_none() {
                        row("Kindle clippings", C::ToggleKindleClippings)
                    } else {
                        Item::new("Kindle clippings", Action::Heading)
                            .detail("Requires all styles and colors")
                    });
                }
                rows.extend(HighlightStyle::ALL.into_iter().map(|style| {
                    selected(
                        style.label(),
                        C::Style(Some(style)),
                        export
                            .styles
                            .as_ref()
                            .is_some_and(|styles| styles.contains(&style)),
                    )
                }));
                rows
            }
            Page::Colors => Vec::new(),
            Page::Formats => {
                let mut rows = Vec::new();
                if let Some(saved) = &export.saved {
                    rows.push(Item::new("Export saved", Action::Heading).detail(saved));
                }
                rows.extend([
                    row("KOReader JSON", C::Start(Format::Json)),
                    row("Plain text", C::Start(Format::Text)),
                    row("Markdown", C::Start(Format::Markdown)),
                    row("HTML", C::Start(Format::Html)),
                    row("Kindle clippings text", C::Start(Format::KindleClippings)),
                ]);
                rows
            }
            Page::Running => {
                let Some(job) = &export.job else {
                    return Vec::new();
                };
                vec![
                    Item::new(
                        format!("{} of {} books checked", job.done, job.total),
                        Action::Heading,
                    ),
                    row("Cancel export", C::Cancel),
                ]
            }
        }
    }

    pub(super) fn activate_library_export(&mut self, command: Command) -> Result<()> {
        use Command as C;
        match command {
            C::Open => {
                self.library_export.books.clear();
                self.library_export.collections.clear();
                self.library_export.kindle_clippings = None;
                self.library_export.job = None;
                self.library_export.saved = None;
                self.show(Screen::LibraryExport(Page::Sources));
            }
            C::History => {
                self.library_export.kindle_clippings = with_storage(|| {
                    self.library
                        .absolute_path(Path::new(kindle_clippings::FILE))
                })
                .is_ok()
                .then_some(true);
                self.library_export.books = self
                    .library
                    .recently_read()
                    .filter(|book| has_highlights(book))
                    .map(|book| book.id)
                    .collect();
                self.show(Screen::LibraryExport(Page::Options));
            }
            C::Choose(page) => self.show(Screen::LibraryExport(page)),
            C::ToggleBook(id) => {
                let ids = &mut self.library_export.books;
                if !ids.remove(&id) {
                    ids.insert(id);
                }
            }
            C::ToggleCollection(name) => {
                let names = &mut self.library_export.collections;
                if !names.remove(&name) {
                    names.insert(name);
                }
            }
            C::ToggleAll => {
                let all: BTreeSet<_> = self
                    .library
                    .recently_read()
                    .filter(|book| has_highlights(book))
                    .map(|book| book.id)
                    .collect();
                self.library_export.books = if self.library_export.books == all {
                    BTreeSet::new()
                } else {
                    all
                };
            }
            C::Apply => {
                self.library_export.kindle_clippings = None;
                if self.screen == Screen::LibraryExport(Page::Collections) {
                    self.library_export.books = self
                        .library
                        .recently_read()
                        .filter(|book| has_highlights(book))
                        .filter(|book| {
                            self.library_export.collections.iter().any(|name| {
                                self.library
                                    .collection(name)
                                    .is_some_and(|set| set.contains(book.id))
                            })
                        })
                        .map(|book| book.id)
                        .collect();
                }
                self.show(Screen::LibraryExport(Page::Options));
            }
            C::Style(None) => {
                let prior = self.library_export.styles.clone();
                self.library_export.styles = None;
                if let Err(error) = self.library_export.save_preferences() {
                    self.library_export.styles = prior;
                    return Err(error);
                }
            }
            C::Style(Some(style)) => {
                let prior = self.library_export.styles.clone();
                let styles = self.library_export.styles.get_or_insert_with(Vec::new);
                if let Some(index) = styles.iter().position(|saved| *saved == style) {
                    styles.remove(index);
                } else {
                    styles.push(style);
                }
                if let Err(error) = self.library_export.save_preferences() {
                    self.library_export.styles = prior;
                    return Err(error);
                }
            }
            C::Color(color) => {
                let prior = self.library_export.colors.clone();
                if let Some(color) = color {
                    let colors = self.library_export.colors.get_or_insert_with(Vec::new);
                    if let Some(index) = colors.iter().position(|saved| *saved == color) {
                        colors.remove(index);
                    } else {
                        colors.push(color);
                    }
                } else {
                    self.library_export.colors = None;
                }
                if let Err(error) = self.library_export.save_preferences() {
                    self.library_export.colors = prior;
                    return Err(error);
                }
            }
            C::Cancel => {
                self.library_export.job = None;
                self.show(Screen::LibraryExport(Page::Options));
                self.notice = Some("Export cancelled".into());
            }
            C::ToggleKindleClippings => {
                if let Some(included) = &mut self.library_export.kindle_clippings {
                    *included = !*included;
                }
            }
            C::Start(format) => {
                let include_clippings = self.library_export.kindle_clippings_enabled()
                    && self.library_export.styles.is_none()
                    && self.library_export.colors.is_none();
                let kindle_clippings = if include_clippings {
                    self.read_kindle_clippings()?
                } else {
                    Vec::new()
                };
                if self.library_export.books.is_empty() && kindle_clippings.is_empty() {
                    return Err("Choose a book with highlights or include Kindle clippings".into());
                }
                self.save_library()?;
                self.library_export.saved = None;
                self.library_export.job = Some(Job {
                    ids: self.library_export.books.iter().copied().collect(),
                    total: self.library_export.books.len(),
                    done: 0,
                    skipped: 0,
                    format,
                    preferences: self.export_settings.preferences.clone(),
                    documents: Vec::new(),
                    kindle_clippings,
                    pending: None,
                    next: Instant::now(),
                });
                self.show(Screen::LibraryExport(Page::Running));
            }
        }
        Ok(())
    }

    fn prepare_export_book(&self, id: u64) -> Result<PendingBook> {
        let state = self.library.state(id).ok_or("That book is unavailable")?;
        let entry = self.locate_tracked(id)?;
        let absolute = with_storage(|| self.library.absolute_path(&entry.path))?;
        let book = with_storage(|| Ok(Book::open(&absolute).ok()))?;
        let mut marks = VecDeque::new();
        let styles = &self.library_export.styles;
        let colors = &self.library_export.colors;
        for mark in state.highlights.iter().filter(|mark| {
            styles
                .as_ref()
                .is_none_or(|styles| styles.contains(&mark.style))
                && colors
                    .as_ref()
                    .is_none_or(|colors| colors.contains(&mark.color))
        }) {
            let start = &mark.selection.start;
            let chapter = book
                .as_ref()
                .and_then(|book| book.chapter_index(&start.chapter));
            marks.push_back((
                Entry {
                    sort: "highlight",
                    text: mark.selection.text.clone(),
                    note: mark.note.clone(),
                    time: mark.created_at,
                    drawer: Some(mark.style),
                    color: Some(mark.color),
                    page: None,
                    chapter: chapter
                        .and_then(|index| book.as_ref()?.chapters.get(index))
                        .map(|chapter| chapter.label.clone()),
                    primitiveos: serde_json::json!({"start": start, "end": mark.selection.end}),
                },
                chapter.map(|index| (index, start.clone())),
            ));
        }
        for mark in state.pdf_highlights.iter().filter(|mark| {
            styles
                .as_ref()
                .is_none_or(|styles| styles.contains(&mark.style))
                && colors
                    .as_ref()
                    .is_none_or(|colors| colors.contains(&mark.color))
        }) {
            marks.push_back((Entry {
                sort: "highlight", text: mark.text.clone(), note: mark.note.clone(), time: mark.created_at,
                drawer: Some(mark.style), color: Some(mark.color), page: Some(serde_json::json!(mark.page + 1)), chapter: None,
                primitiveos: serde_json::json!({"page": mark.page, "bounds": mark.bounds, "continuation": mark.continuation}),
            }, None));
        }
        marks
            .make_contiguous()
            .sort_by(|(left, left_anchor), (right, right_anchor)| {
                let source_key = |entry: &Entry, anchor: &Option<(usize, crate::Anchor)>| {
                    anchor.as_ref().map_or_else(
                        || {
                            let page = entry.primitiveos["page"].as_u64().unwrap_or(u64::MAX);
                            let first = &entry.primitiveos["bounds"][0];
                            (
                                page,
                                first["y0"].as_f64().unwrap_or(0.0),
                                first["x0"].as_f64().unwrap_or(0.0),
                            )
                        },
                        |(chapter, anchor)| {
                            (*chapter as u64, anchor.block as f64, anchor.offset as f64)
                        },
                    )
                };
                let left = source_key(left, left_anchor);
                let right = source_key(right, right_anchor);
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.total_cmp(&right.1))
                    .then_with(|| left.2.total_cmp(&right.2))
            });
        let fonts = self
            .fonts_for(state.preferences.font_file.as_deref())
            .unwrap_or_else(|_| self.fonts.clone());
        let styles = self.style_store.data.snapshot(id)?;
        let mut settings = state.preferences.apply_to(self.device_reading_settings());
        settings.status_header_height = crate::reading_controls::TopHeader::height(
            self.top_header_for_book(id),
            state.preferences.scroll,
        );
        let config = book.map(|_| BookConfig {
            user_styles: styles,
            user_hyphenation: self.user_hyphenation.clone(),
            chapter_offsets: Default::default(),
            screen_pages: Default::default(),
            engine: Engine::Parley,
            book: absolute.clone(),
            fonts,
            settings,
            scroll: state.preferences.scroll,
            facing_pages: state.preferences.facing_pages,
            hide_nonlinear: state
                .hide_nonlinear
                .unwrap_or(self.controls.hide_nonlinear_default),
            hidden_flows: std::rc::Rc::new(state.hidden_flows.clone()),
        });
        Ok(PendingBook {
            id,
            source: entry,
            document: Document {
                title: state.title.clone(),
                author: state.author.clone(),
                file: Some(absolute),
                primitiveos_book_id: Some(format!("{id:016x}")),
                entries: Vec::new(),
            },
            marks,
            config,
            probe: None,
        })
    }

    fn read_kindle_clippings(&self) -> Result<Vec<Document>> {
        let path = with_storage(|| {
            self.library
                .absolute_path(Path::new(kindle_clippings::FILE))
        })?;
        let bytes = super::read_import_file(&path, kindle_clippings::MAX_BYTES)?;
        let mut by_book: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for clip in kindle_clippings::parse(&bytes)? {
            by_book
                .entry((clip.title, clip.author))
                .or_default()
                .push(Entry {
                    sort: clip.kind.name(),
                    text: clip.text.replace("\r\n", "\n"),
                    note: None,
                    time: clip.time,
                    drawer: None,
                    color: None,
                    page: clip.location.map(|location| match location {
                        Location::Page(page) => serde_json::json!(page),
                        Location::Chapter(label) => serde_json::json!(label),
                        Location::Position(position) => {
                            serde_json::json!(format!("Location {position}"))
                        }
                    }),
                    chapter: None,
                    primitiveos: serde_json::Value::Null,
                });
        }
        Ok(by_book
            .into_iter()
            .map(|((title, author), mut entries)| {
                deduplicate_clips(&mut entries);
                Document {
                    title: title.to_owned(),
                    author: author.map(str::to_owned),
                    file: None,
                    primitiveos_book_id: None,
                    entries,
                }
            })
            .collect())
    }

    pub(super) fn export_deadline(&self) -> Option<Instant> {
        self.library_export.job.as_ref().map(|job| job.next)
    }

    pub(super) fn step_library_export(&mut self, now: Instant) -> Result<Change> {
        let Some(mut job) = self.library_export.job.take() else {
            return Ok(Change::None);
        };
        let done_before = job.done;
        if let Some(pending) = &mut job.pending {
            if let Some((mut entry, anchor)) = pending.marks.pop_front() {
                if let (Some(config), Some((chapter, anchor))) = (&pending.config, anchor) {
                    if pending
                        .probe
                        .as_ref()
                        .is_none_or(|(index, _)| *index != chapter)
                    {
                        pending.probe = config.chapter(chapter).ok().map(|probe| (chapter, probe));
                    }
                    if let Some((_, probe)) = &pending.probe
                        && let Some(page) = probe.page_for(&anchor)
                    {
                        // A chapter-local label describes this layout without inventing a document-wide page.
                        entry.page = Some(serde_json::json!(format!(
                            "Chapter {}, page {}",
                            chapter + 1,
                            page + 1
                        )));
                        entry.primitiveos["chapter"] = serde_json::json!(chapter);
                        entry.primitiveos["page"] = serde_json::json!(page);
                    }
                }
                pending.document.entries.push(entry);
            } else {
                let pending = job.pending.take().unwrap();
                if !with_storage(|| self.library.identify(&pending.source))
                    .is_ok_and(|id| id == pending.id)
                {
                    job.skipped += 1;
                } else if !pending.document.entries.is_empty() {
                    job.documents.push(pending.document);
                }
                job.done += 1;
            }
        } else if let Some(id) = job.ids.pop_front() {
            match self.prepare_export_book(id) {
                Ok(pending) => job.pending = Some(pending),
                Err(_) => {
                    job.done += 1;
                    job.skipped += 1;
                }
            }
        } else {
            let includes_clippings = !job.kindle_clippings.is_empty();
            combine_clippings(&mut job.documents, job.kindle_clippings);
            if job.documents.is_empty() {
                self.show(Screen::LibraryExport(Page::Options));
                return Err("No highlights to export from the available books".into());
            }
            let contents = render(&job.documents, job.format)?;
            let destination = self.write_annotation_export(
                &job.preferences,
                None,
                job.format.extension(),
                &contents,
            )?;
            let marks = job
                .documents
                .iter()
                .map(|book| book.entries.len())
                .sum::<usize>();
            self.library_export.saved = Some(destination);
            self.show(Screen::LibraryExport(Page::Formats));
            let label = match (includes_clippings, marks) {
                (true, 1) => "clipping",
                (true, _) => "clippings",
                (false, 1) => "highlight",
                (false, _) => "highlights",
            };
            let books = job.documents.len();
            let book_label = if books == 1 { "book" } else { "books" };
            self.notice = Some(format!(
                "{marks} {label} · {books} {book_label} · {} unavailable",
                job.skipped
            ));
            return Ok(Change::Partial);
        }
        job.next = now + Duration::from_millis(40);
        let change = if done_before == job.done {
            Change::None
        } else {
            Change::Partial
        };
        self.library_export.job = Some(job);
        Ok(change)
    }

    pub(super) fn back_from_library_export(&mut self, page: Page) {
        self.library_export.job = None;
        self.show(match page {
            Page::Sources => Screen::ReadingData,
            Page::Books | Page::Collections | Page::Options => Screen::LibraryExport(Page::Sources),
            Page::Formats | Page::Running => Screen::LibraryExport(Page::Options),
            Page::Colors => Screen::LibraryExport(Page::Options),
        });
    }

    pub(super) fn cancel_library_export(&mut self) {
        if self.library_export.job.take().is_some() {
            self.show(Screen::LibraryExport(Page::Options));
            self.notice = Some("Export cancelled when Reader went to sleep".into());
        }
    }
}

fn clip_key(entry: &Entry) -> (&str, &str, Option<&serde_json::Value>, Option<i64>) {
    (entry.sort, &entry.text, entry.page.as_ref(), entry.time)
}

fn deduplicate_clips(entries: &mut Vec<Entry>) {
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        let (kind, text, page, time) = clip_key(entry);
        seen.insert((kind.to_owned(), text.to_owned(), page.cloned(), time))
    });
}

fn combine_clippings(documents: &mut Vec<Document>, clippings: Vec<Document>) {
    let mut by_title = BTreeMap::new();
    for (index, doc) in documents.iter().enumerate() {
        by_title
            .entry((doc.title.clone(), doc.author.clone()))
            .and_modify(|match_index| *match_index = None)
            .or_insert(Some(index));
    }
    for mut book in clippings {
        let key = (book.title.clone(), book.author.clone());
        if let Some(Some(index)) = by_title.get(&key) {
            let native = &mut documents[*index];
            let native_keys: HashSet<_> = native.entries.iter().map(clip_key).collect();
            book.entries
                .retain(|entry| !native_keys.contains(&clip_key(entry)));
            native.entries.extend(book.entries);
        } else {
            documents.push(book);
        }
    }
}

fn render(documents: &[Document], format: Format) -> Result<Vec<u8>> {
    if matches!(format, Format::KindleClippings) {
        let mut output = String::new();
        for document in documents {
            for entry in &document.entries {
                let kind = match entry.sort {
                    "bookmark" => Kind::Bookmark,
                    "note" => Kind::Note,
                    _ => Kind::Highlight,
                };
                let location = entry.page.as_ref().map(|value| {
                    value
                        .as_str()
                        .map_or_else(|| format!("page {value}"), str::to_owned)
                });
                let title = &document.title;
                let author = document.author.as_deref();
                if kind == Kind::Highlight {
                    kindle_clippings::write_highlight(
                        &mut output,
                        title,
                        author,
                        location.as_deref(),
                        entry.time,
                        &entry.text,
                        entry.note.as_deref(),
                    );
                } else {
                    kindle_clippings::write(
                        &mut output,
                        title,
                        author,
                        kind,
                        location.as_deref(),
                        entry.time,
                        &entry.text,
                    );
                }
            }
        }
        return Ok(output.into_bytes());
    }
    if matches!(format, Format::Json) {
        let mut value = if documents.len() == 1 {
            serde_json::to_value(&documents[0])?
        } else {
            serde_json::json!({"documents": documents})
        };
        if let Some(time) = crate::mark_time::now() {
            value["created_on"] = serde_json::json!(time);
        }
        value["version"] =
            serde_json::json!(concat!("PrimitiveOS Reader ", env!("CARGO_PKG_VERSION")));
        let mut bytes = serde_json::to_vec_pretty(&value)?;
        bytes.push(b'\n');
        return Ok(bytes);
    }
    if matches!(format, Format::Html) {
        let mut output = String::from(
            "<!doctype html>\n<html lang=\"en\"><meta charset=\"utf-8\"><title>Reading highlights</title><main>\n",
        );
        for document in documents {
            output.push_str("<section><h1>");
            crate::annotation_export::html_text(&mut output, &document.title);
            output.push_str("</h1>\n");
            if let Some(author) = &document.author {
                output.push_str("<p>");
                crate::annotation_export::html_text(&mut output, author);
                output.push_str("</p>\n");
            }
            for entry in &document.entries {
                output.push_str("<article><p>");
                crate::annotation_export::html_text(&mut output, &caption(entry));
                output.push_str("</p><blockquote>");
                crate::annotation_export::html_text(&mut output, &entry.text);
                output.push_str("</blockquote>");
                if let Some(note) = &entry.note {
                    output.push_str("<p><strong>Note</strong><br>");
                    crate::annotation_export::html_text(&mut output, note);
                    output.push_str("</p>");
                }
                output.push_str("</article>\n");
            }
            output.push_str("</section>\n");
        }
        output.push_str("</main></html>\n");
        return Ok(output.into_bytes());
    }
    let format = match format {
        Format::Text => crate::annotation_export::Format::Text,
        Format::Markdown => crate::annotation_export::Format::Markdown,
        Format::Html | Format::Json | Format::KindleClippings => unreachable!(),
    };
    let mut output = String::new();
    for document in documents {
        let title = document.author.as_ref().map_or_else(
            || document.title.clone(),
            |author| format!("{} — {author}", document.title),
        );
        let excerpts: Vec<_> = document
            .entries
            .iter()
            .map(|entry| {
                let caption = caption(entry);
                let quote = if caption.is_empty() {
                    entry.text.clone()
                } else {
                    format!("{caption}\n\n{}", entry.text)
                };
                (quote, entry.note.as_deref())
            })
            .collect();
        output.push_str(&format.render(
            &title,
            excerpts.iter().map(|(quote, note)| (quote.as_str(), *note)),
        ));
    }
    Ok(output.into_bytes())
}

fn caption(entry: &Entry) -> String {
    let location = entry.page.as_ref().map_or_else(String::new, |page| {
        page.as_str()
            .map_or_else(|| format!("Page {page}"), str::to_owned)
    });
    let mut parts = Vec::new();
    match entry.sort {
        "bookmark" => parts.push("Bookmark".to_owned()),
        "note" => parts.push("Note".to_owned()),
        _ => {}
    }
    parts.extend(entry.chapter.iter().cloned());
    if !location.is_empty() {
        parts.push(location);
    }
    if let Some(time) = entry.time.and_then(crate::mark_time::local_datetime) {
        parts.push(time);
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests;
