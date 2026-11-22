//! Local vocabulary review data and the reading workflow that owns it.
//! Scheduling follows the observed KOReader vocabulary-builder contract.
use primitive_app::Change;
use primitive_ui::{BODY, CAPTION, Canvas, Input, Rect};
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
                books
                    .into_iter()
                    .map(|book| {
                        Item::new(
                            if book.is_empty() {
                                "Dictionary lookup"
                            } else {
                                &book
                            },
                            Action::Vocabulary(Command::ToggleBook(book.clone())),
                        )
                        .detail(if data.excluded_books.contains(&book) {
                            "Excluded"
                        } else {
                            "Included"
                        })
                    })
                    .collect()
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
    let Some(seconds) = seconds.and_then(|s| libc::time_t::try_from(s).ok()) else {
        return "Unknown".into();
    };
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // localtime_r initializes this caller-owned tm or returns null for an invalid instant.
    let local = unsafe {
        if libc::localtime_r(&seconds, local.as_mut_ptr()).is_null() {
            return "Unknown".into();
        }
        local.assume_init()
    };
    let Some(year) = local.tm_year.checked_add(1900).filter(|year| (0..=9999).contains(year)) else {
        return "Unknown".into();
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min
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
