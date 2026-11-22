//! Export highlights from History, chosen books or collections without moving the reader.
use super::{Action, Book, BookConfig, Change, Engine, Item, Reader, Result, Screen, with_storage};
use crate::highlight_style::HighlightStyle;
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Sources,
    Books,
    Collections,
    Options,
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
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Text => "txt",
            Self::Markdown => "md",
            Self::Html => "html",
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
    Start(Format),
    Cancel,
}

#[derive(Default)]
pub(super) struct Export {
    path: std::path::PathBuf,
    books: BTreeSet<u64>,
    collections: BTreeSet<String>,
    styles: Option<Vec<HighlightStyle>>,
    job: Option<Job>,
    saved: Option<String>,
}

impl Export {
    pub(super) fn load(path: std::path::PathBuf) -> Self {
        let styles: Option<Vec<HighlightStyle>> = super::read_import_file(&path, 4096)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .flatten();
        let styles = styles.filter(|styles| styles.len() <= HighlightStyle::ALL.len());
        Self {
            path,
            styles,
            ..Self::default()
        }
    }

    fn save_preferences(&self) -> Result<()> {
        primitive_app::save(&self.path, &serde_json::to_vec(&self.styles)?)?;
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
    drawer: HighlightStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chapter: Option<String>,
    primitiveos: serde_json::Value,
}

#[derive(Serialize)]
struct Document {
    title: String,
    author: Option<String>,
    file: std::path::PathBuf,
    primitiveos_book_id: String,
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
    documents: Vec<Document>,
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
                    row("Choose export format", C::Choose(Page::Formats))
                        .detail(format!("{} books", export.books.len())),
                    selected(
                        "All highlight styles",
                        C::Style(None),
                        export.styles.is_none(),
                    ),
                ];
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
                self.library_export.job = None;
                self.library_export.saved = None;
                self.show(Screen::LibraryExport(Page::Sources));
            }
            C::History => {
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
                self.library_export.styles = None;
                self.library_export.save_preferences()?;
            }
            C::Style(Some(style)) => {
                let styles = self.library_export.styles.get_or_insert_with(Vec::new);
                if let Some(index) = styles.iter().position(|saved| *saved == style) {
                    styles.remove(index);
                } else {
                    styles.push(style);
                }
                self.library_export.save_preferences()?;
            }
            C::Cancel => {
                self.library_export.job = None;
                self.show(Screen::LibraryExport(Page::Options));
                self.notice = Some("Export cancelled".into());
            }
            C::Start(format) => {
                if self.library_export.books.is_empty() {
                    return Err("Choose a book with highlights".into());
                }
                self.save_library()?;
                self.library_export.saved = None;
                self.library_export.job = Some(Job {
                    ids: self.library_export.books.iter().copied().collect(),
                    total: self.library_export.books.len(),
                    done: 0,
                    skipped: 0,
                    format,
                    documents: Vec::new(),
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
        for mark in state.highlights.iter().filter(|mark| {
            styles
                .as_ref()
                .is_none_or(|styles| styles.contains(&mark.style))
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
                    drawer: mark.style,
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
        }) {
            marks.push_back((Entry {
                sort: "highlight", text: mark.text.clone(), note: mark.note.clone(), time: mark.created_at,
                drawer: mark.style, page: Some(serde_json::json!(mark.page + 1)), chapter: None,
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
        let config = book.map(|_| BookConfig {
            user_hyphenation: self.user_hyphenation.clone(),
            chapter_offsets: Default::default(),
            engine: Engine::Parley,
            book: absolute.clone(),
            fonts,
            settings: state.preferences.apply_to(self.device_reading_settings()),
            scroll: state.preferences.scroll,
            facing_pages: state.preferences.facing_pages,
            hide_nonlinear: state.hide_nonlinear.unwrap_or(self.controls.hide_nonlinear_default),
            hidden_flows: std::rc::Rc::new(state.hidden_flows.clone()),
        });
        Ok(PendingBook {
            id,
            source: entry,
            document: Document {
                title: state.title.clone(),
                author: state.author.clone(),
                file: absolute,
                primitiveos_book_id: format!("{id:016x}"),
                entries: Vec::new(),
            },
            marks,
            config,
            probe: None,
        })
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
            if job.documents.is_empty() {
                self.show(Screen::LibraryExport(Page::Options));
                return Err("No highlights to export from the available books".into());
            }
            let contents = render(&job.documents, job.format)?;
            with_storage(|| std::fs::create_dir_all(&self.exports))?;
            let filename = format!("library-highlights.{}", job.format.extension());
            primitive_app::save(&self.exports.join(&filename), &contents)?;
            let marks = job
                .documents
                .iter()
                .map(|book| book.entries.len())
                .sum::<usize>();
            self.library_export.saved = Some(format!("Reader data/exports/{filename}"));
            self.show(Screen::LibraryExport(Page::Formats));
            self.notice = Some(format!(
                "{marks} highlights · {} books · {} unavailable",
                job.documents.len(),
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
        });
    }

    pub(super) fn cancel_library_export(&mut self) {
        if self.library_export.job.take().is_some() {
            self.show(Screen::LibraryExport(Page::Options));
            self.notice = Some("Export cancelled when Reader went to sleep".into());
        }
    }
}

fn render(documents: &[Document], format: Format) -> Result<Vec<u8>> {
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
        Format::Html | Format::Json => unreachable!(),
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
    let mut parts: Vec<_> = entry.chapter.iter().cloned().collect();
    if !location.is_empty() {
        parts.push(location);
    }
    if let Some(time) = entry.time.and_then(local_datetime) {
        parts.push(time);
    }
    parts.join(" · ")
}

fn local_datetime(time: i64) -> Option<String> {
    let time = libc::time_t::try_from(time).ok()?;
    let mut local = std::mem::MaybeUninit::uninit();
    // localtime_r writes the caller-owned buffer and returns null for an unrepresentable date.
    let local = unsafe {
        if libc::localtime_r(&time, local.as_mut_ptr()).is_null() {
            return None;
        }
        local.assume_init()
    };
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.tm_year + 1900,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
        local.tm_sec
    ))
}

#[cfg(test)]
mod tests;
