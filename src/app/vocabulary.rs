//! Local vocabulary review data and the reading workflow that owns it.
//! Scheduling follows the observed KOReader vocabulary-builder contract.
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, PAPER, Press, Rect, keyboard_key};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{Action, Item, Reader, Result, Screen};

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Entry {
    pub book: String,
    pub created_at: Option<u64>,
    pub reviewed_at: Option<u64>,
    pub due_at: Option<u64>,
    pub reviews: u32,
    pub streak: u32,
    pub before: Option<String>,
    pub after: Option<String>,
    pub highlight: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Vocabulary {
    pub automatic: bool,
    pub with_context: bool,
    pub due_only: bool,
    pub excluded_books: BTreeSet<String>,
    pub words: BTreeMap<String, Entry>,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self {
            automatic: false,
            with_context: true,
            due_only: false,
            excluded_books: BTreeSet::new(),
            words: BTreeMap::new(),
        }
    }
}

impl Entry {
    fn review(&mut self, got_it: bool, now: u64) {
        self.reviews = if got_it {
            self.reviews.saturating_add(1)
        } else {
            self.reviews.saturating_sub(1)
        };
        self.streak = if got_it {
            self.streak.saturating_add(1)
        } else {
            0
        };
        let seconds = match self.streak {
            0 => 300,
            1 => 1800,
            2 => 43_200,
            3 => 86_400,
            4 => 172_800,
            5 => 345_600,
            6 => 604_800,
            7 => 1_296_000,
            n => 2_592_000u64 << (n - 8).min(6),
        };
        self.reviewed_at = Some(now);
        self.due_at = Some(now.saturating_add(seconds));
    }
}

impl Vocabulary {
    pub fn remember(&mut self, word: &str, mut entry: Entry, now: u64) {
        if let Some(prior) = self.words.get(word) {
            entry.reviews = prior.reviews.saturating_sub(1);
            entry.reviewed_at = prior.reviewed_at;
            if entry.before.is_none() {
                entry.before = prior.before.clone();
            }
            if entry.after.is_none() {
                entry.after = prior.after.clone();
            }
            if entry.highlight.is_none() {
                entry.highlight = prior.highlight.clone();
            }
        } else {
            entry.reviewed_at = Some(now);
        }
        entry.created_at = Some(now);
        entry.due_at = Some(now.saturating_add(300));
        entry.streak = 0;
        self.words.insert(word.to_owned(), entry);
    }

    fn visible(&self, query: &str, now: u64) -> Vec<(&String, &Entry)> {
        let query = query.to_lowercase();
        let mut entries: Vec<_> = self
            .words
            .iter()
            .filter(|(word, entry)| {
                !self.excluded_books.contains(&entry.book)
                    && (!self.due_only || entry.due_at.is_none_or(|due| due <= now))
                    && (query.is_empty() || word.to_lowercase().contains(&query))
            })
            .collect();
        entries.sort_by(|(a, x), (b, y)| {
            let dates = x.due_at.cmp(&y.due_at);
            (if self.due_only {
                dates.reverse()
            } else {
                dates
            })
            .then(a.cmp(b))
        });
        entries
    }
}

#[derive(Clone, Debug)]
pub(super) enum Command {
    Open(String),
    Context,
    Review(bool),
    Undo,
    Reset,
    Remove,
    ConfirmReset,
    ConfirmResetWord,
    ConfirmRemove,
    ToggleAutomatic,
    ToggleContext,
    ToggleDue,
    ToggleBook(String),
    AssignBook,
    ChooseBook(String),
    NewBook,
    ClearFilter,
    Search,
}

#[derive(Default)]
pub(super) struct View {
    pub word: Option<String>,
    pub query: String,
    undo: BTreeMap<String, Entry>,
    pub confirm: Option<bool>,
    reset_word: bool,
    pub repeated: Option<(String, Entry)>,
    confirmation_origin: Option<Screen>,
    text_page: usize,
    text_pages: std::cell::Cell<usize>,
    rename_book: Option<BookEdit>,
    assigning_book: bool,
}

#[derive(Clone)]
enum BookEdit {
    Rename(String),
    Assign,
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Reader {
    pub(super) fn vocabulary_items(&self) -> Vec<Item> {
        let data = &self.lookup_history.review;
        match self.screen {
            Screen::Vocabulary => {
                let entries = data.visible(&self.vocabulary_view.query, now());
                let mut items = vec![
                    Item::new("Review options", Action::Show(Screen::VocabularyOptions)).detail(
                        format!(
                            "{} words · {}",
                            entries.len(),
                            if data.due_only {
                                "Due for review"
                            } else {
                                "All scheduled words"
                            }
                        ),
                    ),
                    Item::new("Find a word", Action::Vocabulary(Command::Search))
                        .detail(&self.vocabulary_view.query),
                ];
                if !self.vocabulary_view.query.is_empty() {
                    items.push(Item::new(
                        "Clear search",
                        Action::Vocabulary(Command::ClearFilter),
                    ));
                }
                items.extend(entries.into_iter().map(|(word, entry)| {
                    Item::new(word, Action::Vocabulary(Command::Open(word.clone())))
                        .detail(format!("{} · {}", source(entry), due(entry)))
                }));
                items
            }
            Screen::VocabularyOptions => vec![
                Item::new(
                    "Show due words only",
                    Action::Vocabulary(Command::ToggleDue),
                )
                .detail(on_off(data.due_only)),
                Item::new(
                    "Add dictionary lookups automatically",
                    Action::Vocabulary(Command::ToggleAutomatic),
                )
                .detail(on_off(data.automatic)),
                Item::new(
                    "Save reading context",
                    Action::Vocabulary(Command::ToggleContext),
                )
                .detail(on_off(data.with_context)),
                Item::new("Filter by book", Action::Show(Screen::VocabularyBooks))
                    .detail(format!("{} books excluded", data.excluded_books.len())),
                Item::new(
                    "Reset all review progress",
                    Action::Vocabulary(Command::ConfirmReset),
                ),
            ],
            Screen::VocabularyBooks => {
                let books: BTreeSet<_> = data
                    .words
                    .values()
                    .map(|entry| entry.book.clone())
                    .collect();
                let assigning = self.vocabulary_view.assigning_book;
                let current = self
                    .vocabulary_view
                    .word
                    .as_ref()
                    .and_then(|word| data.words.get(word))
                    .map(|entry| entry.book.as_str());
                let mut items: Vec<_> = books
                    .into_iter()
                    .map(|book| {
                        Item::new(
                            if book.is_empty() {
                                "Dictionary lookup"
                            } else {
                                &book
                            },
                            Action::Vocabulary(if assigning {
                                Command::ChooseBook(book.clone())
                            } else {
                                Command::ToggleBook(book.clone())
                            }),
                        )
                        .detail(
                            if assigning && current == Some(book.as_str()) {
                                "Current source"
                            } else if assigning {
                                "Move this word here"
                            } else if data.excluded_books.contains(&book) {
                                "Excluded · Hold to rename"
                            } else {
                                "Included · Hold to rename"
                            },
                        )
                    })
                    .collect();
                if assigning {
                    items.insert(
                        0,
                        Item::new("New book title", Action::Vocabulary(Command::NewBook)),
                    );
                }
                items
            }
            Screen::VocabularyWord => {
                let Some(word) = &self.vocabulary_view.word else {
                    return vec![];
                };
                let Some(entry) = data.words.get(word) else {
                    return vec![];
                };
                let mut items = vec![Item::new(word, Action::Heading).detail(format!(
                    "{} · {}",
                    source(entry),
                    due(entry)
                ))];
                if entry.before.is_some() || entry.after.is_some() || entry.highlight.is_some() {
                    items.push(
                        Item::new("Reading context", Action::Vocabulary(Command::Context))
                            .detail(entry.highlight.as_deref().unwrap_or(word)),
                    );
                }
                items.extend([
                    Item::new("Show definition", Action::LookUp(word.clone())),
                    Item::new(
                        "Change source book",
                        Action::Vocabulary(Command::AssignBook),
                    )
                    .detail(source(entry)),
                    Item::new("Got it", Action::Vocabulary(Command::Review(true)))
                        .detail("Schedule the next review"),
                    Item::new("Forgot", Action::Vocabulary(Command::Review(false)))
                        .detail("Review again in 5 minutes"),
                    Item::new(
                        format!("{} reviews · {} consecutive", entry.reviews, entry.streak),
                        Action::Heading,
                    ),
                    Item::new(format!("Added {}", date(entry.created_at)), Action::Heading).detail(
                        format!(
                            "Reviewed {} · Scheduled {}",
                            date(entry.reviewed_at),
                            date(entry.due_at)
                        ),
                    ),
                ]);
                if self.vocabulary_view.undo.contains_key(word) {
                    items.push(Item::new(
                        "Undo last review",
                        Action::Vocabulary(Command::Undo),
                    ));
                }
                items.push(Item::new(
                    "Remove word",
                    Action::Vocabulary(Command::ConfirmRemove),
                ));
                items.push(Item::new(
                    "Reset this word's progress",
                    Action::Vocabulary(Command::ConfirmResetWord),
                ));
                items
            }
            _ => vec![],
        }
    }

    pub(super) fn vocabulary_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Open(word) => {
                self.vocabulary_view.word = Some(word);
                self.show(Screen::VocabularyWord);
            }
            Command::Context => {
                self.vocabulary_view.text_page = 0;
                self.show(Screen::VocabularyContext);
            }
            Command::Review(got) => {
                let word = self
                    .vocabulary_view
                    .word
                    .clone()
                    .ok_or("Choose a vocabulary word first")?;
                self.review_vocabulary_word(&word, got)?;
            }
            Command::Undo => {
                let word = self
                    .vocabulary_view
                    .word
                    .clone()
                    .ok_or("Choose a vocabulary word first")?;
                let prior = self
                    .vocabulary_view
                    .undo
                    .get(&word)
                    .cloned()
                    .ok_or("No review to undo")?;
                self.update_lookup_history(|h| {
                    h.review.words.insert(word.clone(), prior);
                })?;
                self.vocabulary_view.undo.remove(&word);
            }
            Command::Reset => {
                let only = if self.vocabulary_view.reset_word {
                    self.vocabulary_view.word.clone()
                } else {
                    None
                };
                self.update_lookup_history(|h| {
                    for (word, entry) in &mut h.review.words {
                        if only.as_ref().is_some_and(|only| only != word) {
                            continue;
                        }
                        entry.reviews = 0;
                        entry.streak = 0;
                        entry.due_at = Some(now());
                        if only.is_some() {
                            entry.reviewed_at = entry.due_at;
                        }
                    }
                })?;
                if let Some(word) = &only {
                    self.vocabulary_view.undo.remove(word);
                } else {
                    self.vocabulary_view.undo.clear();
                }
                self.show(if only.is_some() {
                    Screen::VocabularyWord
                } else {
                    Screen::VocabularyOptions
                });
            }
            Command::Remove => {
                let word = self
                    .vocabulary_view
                    .word
                    .clone()
                    .ok_or("Choose a vocabulary word first")?;
                self.update_lookup_history(|h| {
                    h.vocabulary.retain(|w| w != &word);
                    h.review.words.remove(&word);
                })?;
                self.vocabulary_view.undo.remove(&word);
                if self.vocabulary_view.confirmation_origin.take() != Some(Screen::Definition) {
                    self.show(Screen::Vocabulary);
                }
            }
            Command::ConfirmReset | Command::ConfirmResetWord | Command::ConfirmRemove => {
                self.vocabulary_view.reset_word = matches!(command, Command::ConfirmResetWord);
                self.vocabulary_view.confirm = Some(!matches!(command, Command::ConfirmRemove));
                self.vocabulary_view.confirmation_origin = Some(self.screen);
            }
            Command::ToggleAutomatic => {
                self.update_lookup_history(|h| h.review.automatic = !h.review.automatic)?
            }
            Command::ToggleContext => {
                self.update_lookup_history(|h| h.review.with_context = !h.review.with_context)?
            }
            Command::ToggleDue => {
                self.update_lookup_history(|h| h.review.due_only = !h.review.due_only)?
            }
            Command::ToggleBook(book) => self.update_lookup_history(|h| {
                if !h.review.excluded_books.remove(&book) {
                    h.review.excluded_books.insert(book);
                }
            })?,
            Command::AssignBook => {
                self.vocabulary_view.assigning_book = true;
                self.show(Screen::VocabularyBooks);
            }
            Command::ChooseBook(book) => self.assign_vocabulary_book(book)?,
            Command::NewBook => {
                self.editor = super::Editor::new(512);
                self.vocabulary_view.rename_book = Some(BookEdit::Assign);
                self.notice = None;
            }
            Command::ClearFilter => {
                self.vocabulary_view.query.clear();
                self.show(Screen::Vocabulary);
            }
            Command::Search => {
                self.editor = super::Editor::new(128);
                self.editor.text = self.vocabulary_view.query.chars().collect();
                self.editor.caret = self.editor.text.len();
                self.show(Screen::VocabularySearch);
            }
        }
        Ok(())
    }

    fn assign_vocabulary_book(&mut self, title: String) -> Result<()> {
        let word = self
            .vocabulary_view
            .word
            .clone()
            .ok_or("Choose a vocabulary word first")?;
        if !self.lookup_history.review.words.contains_key(&word) {
            return Err("That vocabulary word is unavailable".into());
        }
        self.update_lookup_history(|history| {
            if let Some(entry) = history.review.words.get_mut(&word) {
                entry.book.clone_from(&title);
            }
        })?;
        if let Some(previous) = self.vocabulary_view.undo.get_mut(&word) {
            previous.book = title;
        }
        self.vocabulary_view.assigning_book = false;
        self.vocabulary_view.rename_book = None;
        self.notice = None;
        self.show(Screen::VocabularyWord);
        Ok(())
    }

    pub(super) fn vocabulary_entry(&self, word: &str) -> Entry {
        let mut entry = Entry::default();
        if let Some(active) = &self.active {
            entry.book = active.reading.probe.title.clone();
            if self.lookup_history.review.with_context {
                if let Some(selection) = &self.selection
                    && selection.text.to_lowercase().contains(&word.to_lowercase())
                {
                    if let Some((before, after)) = active.reading.probe.word_context(selection, 15)
                    {
                        entry.before = Some(before);
                        entry.after = Some(after);
                        entry.highlight = Some(selection.text.clone());
                    }
                }
            }
        } else if let Some(fixed) = &self.fixed {
            entry.book = fixed
                .reading
                .title()
                .unwrap_or_else(|| fixed.entry.title.clone());
            if self.lookup_history.review.with_context
                && let Some(selection) = &self.pdf_selection
                && selection
                    .text()
                    .to_lowercase()
                    .contains(&word.to_lowercase())
            {
                let (before, after) = selection.word_context(15);
                entry.before = Some(before);
                entry.after = Some(after);
                entry.highlight = Some(selection.text().to_owned());
            }
        }
        entry
    }

    fn review_vocabulary_word(&mut self, word: &str, got: bool) -> Result<()> {
        let previous = self
            .lookup_history
            .review
            .words
            .get(word)
            .cloned()
            .ok_or("That word was removed")?;
        self.update_lookup_history(|h| h.review.words.get_mut(word).unwrap().review(got, now()))?;
        self.vocabulary_view.undo.insert(word.to_owned(), previous);
        Ok(())
    }

    pub(super) fn draw_vocabulary_dialog(&self, canvas: &mut Canvas) {
        if let Some(editor) = &self.vocabulary_view.rename_book {
            let layout = self.navigation_layout();
            canvas.fill(Rect::new(0, 0, layout.width, layout.height), PAPER);
            layout.header(
                canvas,
                match editor {
                    BookEdit::Rename(_) => "Change vocabulary book title",
                    BookEdit::Assign => "Move word to book",
                },
            );
            canvas.single_line_field(
                layout.body(super::LIST_Y, 130),
                &self.editor,
                "Book title",
                false,
            );
            if let Some(notice) = &self.notice {
                canvas.text(layout.body(super::LIST_Y + 140, 70), notice, CAPTION);
            }
            canvas.keyboard(self.editor.layer, "Save", None);
            return;
        }
        if let Some((word, _)) = &self.vocabulary_view.repeated {
            let Some(entry) = self.lookup_history.review.words.get(word) else {
                return;
            };
            canvas.popup(EXISTS);
            canvas.text(
                Rect::new(140, 550, 984, 90),
                "Word already saved",
                BODY.bold(),
            );
            canvas.text(
                Rect::new(140, 650, 984, 250),
                &format!(
                    "{word}\n{}\n{} · {} consecutive reviews",
                    source(entry),
                    due(entry),
                    entry.streak
                ),
                BODY.lines(4),
            );
            canvas.text(
                Rect::new(140, 930, 984, 100),
                "Keep reviewing, or replace its source and context with this lookup.",
                CAPTION.lines(2),
            );
            for (rect, label) in EXIST_CHOICES
                .into_iter()
                .zip(["Got it", "Forgot", "Replace"])
            {
                canvas.button(rect, label, false, false);
            }
            canvas.button(EXIST_CANCEL, "Cancel", false, false);
            return;
        }
        let Some(reset) = self.vocabulary_view.confirm else {
            return;
        };
        canvas.popup(DIALOG);
        canvas.text(
            Rect::new(210, 650, 840, 86),
            if reset {
                "Reset review progress?"
            } else {
                "Remove this word?"
            },
            BODY.bold(),
        );
        canvas.text(Rect::new(210, 760, 840, 160), if reset {
            if self.vocabulary_view.reset_word { "The word and reading context are kept. This word becomes due now." }
            else { "Words and reading context are kept. Every word becomes due now." }
        } else { "This removes the word, its context and its review history. You can save the word again." }, BODY.lines(3));
        canvas.button(CANCEL, "Cancel", false, false);
        canvas.button(CONFIRM, if reset { "Reset" } else { "Remove" }, true, false);
    }

    pub(super) fn vocabulary_dialog_input(&mut self, input: Input) -> Option<Change> {
        if let Some(editor) = self.vocabulary_view.rename_book.clone() {
            if matches!(input, Input::Sleep | Input::Wake) {
                return None;
            }
            let layout = self.navigation_layout();
            if input == Input::PageBack
                || matches!(input, Input::Tap(x, y) if layout.back().contains(x, y))
            {
                self.vocabulary_view.rename_book = None;
                self.notice = None;
                return Some(Change::Partial);
            }
            let Input::Tap(x, y) = input else {
                return Some(Change::None);
            };
            let Some(key) = keyboard_key(&layout.keyboard(), &self.editor, "Save", x, y) else {
                return Some(Change::None);
            };
            if self.editor.press(key) != Press::Go {
                self.notice = None;
                return Some(Change::Partial);
            }
            let title = self.editor.string().trim().to_owned();
            if title.is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
                self.notice = Some("Enter a nonempty title of at most 512 bytes".into());
                return Some(Change::Partial);
            }
            let BookEdit::Rename(original) = editor else {
                return Some(match self.assign_vocabulary_book(title) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                });
            };
            if title != original
                && (self
                    .lookup_history
                    .review
                    .words
                    .values()
                    .any(|entry| entry.book == title)
                    || self.lookup_history.review.excluded_books.contains(&title))
            {
                self.notice = Some("That vocabulary book title is already in use".into());
                return Some(Change::Partial);
            }
            if title != original {
                if let Err(error) = self.update_lookup_history(|history| {
                    for entry in history.review.words.values_mut() {
                        if entry.book == original {
                            entry.book.clone_from(&title);
                        }
                    }
                    if history.review.excluded_books.remove(&original) {
                        history.review.excluded_books.insert(title.clone());
                    }
                }) {
                    return Some(self.report(error));
                }
                // Undo snapshots belong to the same saved words and must not
                // silently resurrect a renamed source title on the next undo.
                for entry in self.vocabulary_view.undo.values_mut() {
                    if entry.book == original {
                        entry.book.clone_from(&title);
                    }
                }
            }
            self.vocabulary_view.rename_book = None;
            self.notice = None;
            return Some(Change::Partial);
        }
        if self.screen == Screen::VocabularyBooks && self.vocabulary_view.assigning_book {
            let back = self.navigation_layout().back();
            if input == Input::PageBack || matches!(input, Input::Tap(x, y) if back.contains(x, y))
            {
                self.vocabulary_view.assigning_book = false;
                self.show(Screen::VocabularyWord);
                return Some(Change::Partial);
            }
        }
        if self.screen == Screen::VocabularyBooks
            && !self.vocabulary_view.assigning_book
            && let Input::LongTap(x, y) = input
        {
            let (count, mut items) = self.list_window();
            let geometry = self.list_geometry();
            let Some(index) = geometry.item_at(self.list_page, count, x, y) else {
                return Some(Change::None);
            };
            let Action::Vocabulary(Command::ToggleBook(book)) =
                items.swap_remove(index % geometry.rows).action
            else {
                return Some(Change::None);
            };
            self.editor = super::Editor::new(512);
            self.editor.text = book.chars().collect();
            self.editor.caret = self.editor.text.len();
            self.vocabulary_view.rename_book = Some(BookEdit::Rename(book));
            self.notice = None;
            return Some(Change::Partial);
        }
        if let Some((word, entry)) = self.vocabulary_view.repeated.clone() {
            match input {
                Input::Sleep | Input::Wake => return None,
                Input::PageBack => self.vocabulary_view.repeated = None,
                Input::Tap(x, y) if EXIST_CANCEL.contains(x, y) || !EXISTS.contains(x, y) => {
                    self.vocabulary_view.repeated = None
                }
                Input::Tap(x, y) => {
                    let Some(choice) = EXIST_CHOICES.iter().position(|r| r.contains(x, y)) else {
                        return Some(Change::None);
                    };
                    let result = if choice < 2 {
                        self.review_vocabulary_word(&word, choice == 0)
                    } else {
                        self.update_lookup_history(|h| h.review.remember(&word, entry, now()))
                    };
                    return Some(match result {
                        Ok(()) => {
                            if choice == 2 {
                                self.vocabulary_view.undo.remove(&word);
                            }
                            self.vocabulary_view.repeated = None;
                            Change::Partial
                        }
                        Err(error) => self.report(error),
                    });
                }
                _ => return Some(Change::None),
            }
            return Some(Change::Partial);
        }
        let reset = self.vocabulary_view.confirm?;
        match input {
            Input::Sleep | Input::Wake => return None,
            Input::PageBack => self.vocabulary_view.confirm = None,
            Input::Tap(x, y) if CONFIRM.contains(x, y) => {
                self.vocabulary_view.confirm = None;
                return Some(
                    match self.vocabulary_command(if reset {
                        Command::Reset
                    } else {
                        Command::Remove
                    }) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    },
                );
            }
            Input::Tap(x, y) if CANCEL.contains(x, y) || !DIALOG.contains(x, y) => {
                self.vocabulary_view.confirm = None
            }
            _ => return Some(Change::None),
        }
        Some(Change::Partial)
    }

    pub(super) fn draw_vocabulary_context(&self, canvas: &mut Canvas) {
        let Some(word) = &self.vocabulary_view.word else {
            return;
        };
        let Some(entry) = self.lookup_history.review.words.get(word) else {
            return;
        };
        canvas.text(
            Rect::new(40, 154, 1184, 90),
            source(entry),
            CAPTION.center().lines(2),
        );
        let text = format!(
            "{}\n\n[{}]\n\n{}",
            entry.before.as_deref().unwrap_or(""),
            entry.highlight.as_deref().unwrap_or(word),
            entry.after.as_deref().unwrap_or("")
        );
        let bounds = Rect::new(60, 270, 1144, 1080);
        let style = BODY.lines(22).break_long_words();
        let pages = canvas.text_pages(bounds, &text, style);
        self.vocabulary_view.text_pages.set(pages.len());
        let page = self
            .vocabulary_view
            .text_page
            .min(pages.len().saturating_sub(1));
        if let Some(range) = pages.get(page) {
            canvas.text(bounds, &text[range.clone()], style);
        }
        if page > 0 {
            canvas.button(super::PREV, "Previous", false, false);
        }
        if page + 1 < pages.len() {
            canvas.button(super::NEXT, "Next", false, false);
        }
        canvas.text(
            Rect::new(450, 1480, 364, 60),
            &format!("{} of {}", page + 1, pages.len()),
            CAPTION.center(),
        );
    }

    pub(super) fn vocabulary_context_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::Tap(x,y) if primitive_ui::back_button(primitive_app::WIDTH as i32).contains(x,y))
        {
            self.show(Screen::VocabularyWord);
            return Change::Partial;
        }
        let direction = match input {
            Input::PageForward => 1,
            Input::PageBack => -1,
            Input::Tap(x, y) if super::PREV.contains(x, y) => -1,
            Input::Tap(x, y) if super::NEXT.contains(x, y) => 1,
            Input::Swipe { dx, dy, .. } => {
                if (if dy.abs() > dx.abs() { dy } else { dx }) < 0.0 {
                    1
                } else {
                    -1
                }
            }
            _ => return Change::None,
        };
        if let Some(page) = self.vocabulary_view.text_page.checked_add_signed(direction)
            && page < self.vocabulary_view.text_pages.get()
        {
            self.vocabulary_view.text_page = page;
            return Change::Partial;
        }
        Change::None
    }
}

const DIALOG: Rect = Rect::new(160, 590, 944, 510);
const CANCEL: Rect = Rect::new(210, 950, 380, 110);
const CONFIRM: Rect = Rect::new(674, 950, 380, 110);
const EXISTS: Rect = Rect::new(90, 490, 1084, 850);
const EXIST_CHOICES: [Rect; 3] = [
    Rect::new(140, 1060, 310, 110),
    Rect::new(477, 1060, 310, 110),
    Rect::new(814, 1060, 310, 110),
];
const EXIST_CANCEL: Rect = Rect::new(470, 1200, 324, 100);

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}
fn source(entry: &Entry) -> &str {
    if entry.book.is_empty() {
        "Dictionary lookup"
    } else {
        &entry.book
    }
}
fn date(seconds: Option<u64>) -> String {
    let Some(local) = seconds
        .and_then(|value| i64::try_from(value).ok())
        .and_then(crate::mark_time::local_parts)
        .filter(|time| (0..=9999).contains(&time.year))
    else {
        return "Unknown".into();
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.year, local.month, local.day, local.hour, local.minute
    )
}
fn due(entry: &Entry) -> String {
    let remaining = entry.due_at.unwrap_or(0).saturating_sub(now());
    if remaining == 0 {
        "Due now".into()
    } else if remaining < 3600 {
        format!("Due in {} min", remaining.div_ceil(60))
    } else if remaining < 86_400 {
        format!("Due in {} h", remaining.div_ceil(3600))
    } else {
        format!("Due in {} days", remaining.div_ceil(86_400))
    }
}

#[cfg(test)]
mod tests;
