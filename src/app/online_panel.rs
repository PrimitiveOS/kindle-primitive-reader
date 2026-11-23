//! A lookup starts with a preview of the text and chosen provider over the current page.
use std::cell::OnceCell;
use std::ops::Range;
use std::time::Instant;

use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, Input, Layout, Press, Rect, header, keyboard_key,
};
use serde::{Deserialize, Serialize};

use super::{Action, Item, Reader, Screen};
use crate::Result;
use crate::online_lookup::{Article, Job, MAX_TEXT_BYTES, Request, Service, language};

const SHEET: Rect = Rect::new(58, 284, WIDTH as i32 - 116, 1122);
pub(super) const CLOSE: Rect = Rect::new(986, 318, 180, 102);
const TOPIC: Rect = Rect::new(100, 452, 1064, 110);
const PROVIDER: Rect = Rect::new(100, 597, 444, 108);
const FROM: Rect = Rect::new(562, 597, 283, 108);
const TO: Rect = Rect::new(865, 597, 300, 108);
const BODY_RECT: Rect = Rect::new(106, 566, 1048, 542);
const PREV: Rect = Rect::new(100, 1238, 212, 110);
const NEXT: Rect = Rect::new(330, 1238, 212, 110);
const ALTERNATE: Rect = Rect::new(563, 1238, 275, 110);
const SUBMIT: Rect = Rect::new(856, 1238, 310, 110);

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Languages {
    pub wikipedia: String,
    pub source: String,
    pub target: String,
    pub translator: Service,
    pub forget_queries: bool,
    #[serde(default)]
    pub show_romanizations: bool,
}

impl Default for Languages {
    fn default() -> Self {
        Self {
            wikipedia: "en".into(),
            source: "auto".into(),
            target: "en".into(),
            translator: Service::Google,
            forget_queries: false,
            show_romanizations: false,
        }
    }
}

impl Languages {
    pub fn normalize(&mut self) {
        if !language(&self.wikipedia) {
            self.wikipedia = "en".into();
        }
        if !language(&self.target) {
            self.target = "en".into();
        }
        if !language(&self.source) && self.source != "auto" {
            self.source = "auto".into();
        }
        if self.translator == Service::Wikipedia {
            self.translator = Service::Google;
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    Topic,
    From,
    To,
}

impl Field {
    fn title(self) -> &'static str {
        match self {
            Self::Topic => "Wikipedia topic",
            Self::From => "Source language",
            Self::To => "Lookup language",
        }
    }
}

struct Introductions {
    request: Request,
    articles: Vec<Article>,
    article: usize,
    page: usize,
}

pub(super) struct Panel {
    pub request: Request,
    back: Screen,
    field: Field,
    pub job: Option<Job>,
    introductions: Option<Introductions>,
    pub message: Option<String>,
    articles: Vec<Article>,
    article: usize,
    page: usize,
    pages: OnceCell<Vec<Range<usize>>>,
}

impl Panel {
    fn new(request: Request, back: Screen) -> Self {
        Self {
            request,
            back,
            field: Field::Topic,
            job: None,
            introductions: None,
            message: None,
            articles: Vec::new(),
            article: 0,
            page: 0,
            pages: OnceCell::new(),
        }
    }

    fn sheet(&self) -> Rect {
        if self.articles.is_empty() {
            Rect { h: 752, ..SHEET }
        } else {
            SHEET
        }
    }

    fn submit(&self) -> Rect {
        if self.articles.is_empty() {
            Rect { y: 886, ..SUBMIT }
        } else {
            SUBMIT
        }
    }

    fn page_count(&self) -> usize {
        self.pages.get().map_or(1, Vec::len).max(1)
    }

    fn step_page(&mut self, step: isize) -> Change {
        let Some(next) = self
            .page
            .checked_add_signed(step)
            .filter(|&p| p < self.page_count())
        else {
            return Change::None;
        };
        self.page = next;
        Change::Partial
    }
}

impl Reader {
    pub(super) fn prepare_online(&mut self, wiki: bool, whole_page: bool) -> Result<()> {
        let selected = matches!(self.screen, Screen::Book | Screen::Fixed) && !whole_page;
        let text = if whole_page {
            if let Some(book) = &self.active {
                book.reading.visible_text()
            } else if let Some(fixed) = &self.fixed {
                fixed.reading.page_text(fixed.reading.page())?.text
            } else {
                return Err("Open a document first.".into());
            }
        } else if selected {
            self.pdf_selection
                .as_ref()
                .map(|s| s.text())
                .or_else(|| self.selection.as_ref().map(|s| s.text.as_str()))
                .unwrap_or("")
                .to_owned()
        } else {
            String::new()
        };
        if (selected || whole_page) && text.trim().is_empty() {
            return Err(
                "This page has no readable text. Recognize the page or select a passage.".into(),
            );
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err("This page is too long. Select a shorter passage.".into());
        }
        self.prepare_online_text(wiki, &text)
    }

    pub(super) fn prepare_online_text(&mut self, wiki: bool, text: &str) -> Result<()> {
        let prefs = &self.lookup_history.online_languages;
        let request = Request {
            service: if wiki {
                Service::Wikipedia
            } else {
                prefs.translator
            },
            from: prefs.source.clone(),
            to: if wiki {
                prefs.wikipedia.clone()
            } else {
                prefs.target.clone()
            },
            text: text.trim().to_owned(),
            full_article: false,
            with_romanization: !wiki && prefs.show_romanizations,
        };
        let edit_topic = wiki && request.text.is_empty();
        let back = self.screen;
        self.online = Some(Panel::new(request, back));
        self.show(Screen::OnlineLookup);
        if edit_topic {
            self.edit_online(Field::Topic);
        }
        Ok(())
    }

    pub(super) fn online_history_items(&self) -> Vec<Item> {
        let mut items = vec![
            Item::new("Remember lookups", Action::ToggleOnlineHistory)
                .detail("Keep completed searches and translations on this Kindle"),
        ];
        if !self.lookup_history.online.is_empty() {
            items.push(Item::new(
                "Clear online history",
                Action::Show(Screen::ClearOnlineHistory),
            ));
        }
        items.extend(self.lookup_history.online.iter().enumerate().map(|(i, r)| {
            let title: String = r
                .text
                .split_whitespace()
                .take(12)
                .collect::<Vec<_>>()
                .join(" ");
            Item::new(title, Action::RepeatOnline(i)).detail(format!(
                "{} · {}",
                r.service.name(),
                r.to.to_uppercase()
            ))
        }));
        items
    }

    pub(super) fn translation_settings_items(&self) -> Vec<Item> {
        vec![
            Item::new("Show romanizations", Action::ToggleRomanizations)
                .detail("Show Latin text with Google Translate results"),
        ]
    }

    pub(super) fn draw_clear_online_history(&self, canvas: &mut Canvas) {
        let popup = Rect::new(155, 595, 954, 455);
        canvas.popup(popup);
        canvas.text(
            Rect::new(214, 655, 840, 85),
            "Clear online lookup history?",
            BODY.bold(),
        );
        canvas.text(
            Rect::new(214, 755, 830, 126),
            "This removes the saved Wikipedia searches and translations from this Kindle.",
            BODY.lines(3),
        );
        canvas.button(Rect::new(214, 914, 326, 100), "Cancel", false, false);
        canvas.button(Rect::new(632, 914, 400, 100), "Clear history", true, false);
    }

    pub(super) fn clear_online_history_input(&mut self, input: Input) -> Change {
        match input {
            Input::Tap(x, y) if Rect::new(632, 914, 400, 100).contains(x, y) => {
                match self.activate(Action::ClearOnlineHistory) {
                    Ok(()) => Change::Partial,
                    Err(e) => self.report(e),
                }
            }
            Input::PageBack => {
                self.show(Screen::OnlineHistory);
                Change::Partial
            }
            Input::Tap(x, y)
                if Rect::new(214, 914, 326, 100).contains(x, y)
                    || !Rect::new(155, 595, 954, 455).contains(x, y) =>
            {
                self.show(Screen::OnlineHistory);
                Change::Partial
            }
            _ => Change::None,
        }
    }

    pub(super) fn repeat_online(&mut self, index: usize) -> Result<()> {
        let r = self
            .lookup_history
            .online
            .get(index)
            .ok_or("This lookup is no longer in history.")?
            .clone();
        self.online = Some(Panel::new(r, Screen::OnlineHistory));
        self.show(Screen::OnlineLookup);
        Ok(())
    }

    fn edit_online(&mut self, field: Field) {
        let Some(p) = self.online.as_mut() else {
            return;
        };
        p.field = field;
        let (value, limit) = match field {
            Field::Topic => (&p.request.text, 150),
            Field::From => (&p.request.from, 24),
            Field::To => (&p.request.to, 24),
        };
        self.editor = Editor::new(limit);
        self.editor.text = value.chars().take(limit).collect();
        self.editor.caret = self.editor.text.len();
        self.show(Screen::OnlineEdit);
    }

    pub(super) fn draw_online_editor(&self, canvas: &mut Canvas) {
        let Some(p) = &self.online else { return };
        header(canvas, p.field.title(), "Back");
        canvas.single_line_field(
            Rect::new(40, 160, 1184, 124),
            &self.editor,
            p.field.title(),
            false,
        );
        let help = match p.field {
            Field::Topic => "Enter a word or a short phrase.",
            Field::From if p.request.service == Service::Google => {
                "Enter auto to detect the language, or a language code such as en, vi, fr or zh-CN."
            }
            _ => "Enter a language code such as en, vi, fr or zh-CN.",
        };
        canvas.text(
            Rect::new(46, 324, 1172, 180),
            self.notice.as_deref().unwrap_or(help),
            CAPTION.lines(3),
        );
        canvas.keyboard(self.editor.layer, "Apply", None);
    }

    pub(super) fn online_editor_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            if self
                .online
                .as_ref()
                .is_some_and(|p| p.request.text.is_empty())
            {
                self.close_online()
            } else {
                self.show(Screen::OnlineLookup)
            }
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let layout = Layout::new(WIDTH, HEIGHT, 4, 5);
        let Some(key) = keyboard_key(&layout, &self.editor, "Apply", x, y) else {
            return Change::None;
        };
        if !matches!(self.editor.press(key), Press::Go) {
            return Change::Partial;
        }
        let value = self.editor.string().trim().to_owned();
        let Some(p) = &mut self.online else {
            return Change::None;
        };
        if matches!(p.field, Field::Topic) {
            if value.is_empty() {
                return self.report("Enter a topic.");
            }
            p.request.text = value;
            p.request.full_article = false;
        } else {
            let value = value.to_ascii_lowercase();
            if !(language(&value)
                || matches!(p.field, Field::From)
                    && p.request.service == Service::Google
                    && value == "auto")
            {
                return self.report("Enter a valid language code.");
            }
            let mut prefs = self.lookup_history.online_languages.clone();
            match p.field {
                Field::From => {
                    p.request.from = value.clone();
                    prefs.source = value;
                }
                Field::To => {
                    p.request.to = value.clone();
                    if p.request.service == Service::Wikipedia {
                        prefs.wikipedia = value;
                    } else {
                        prefs.target = value;
                    }
                }
                _ => unreachable!(),
            }
            if let Err(e) = self.update_lookup_history(|h| h.online_languages = prefs) {
                return self.report(e);
            }
        }
        self.show(Screen::OnlineLookup);
        Change::Partial
    }

    fn close_online(&mut self) {
        if let Some(p) = &mut self.online
            && let Some(previous) = p.introductions.take()
        {
            p.job.take();
            p.request = previous.request;
            p.articles = previous.articles;
            p.article = previous.article;
            p.page = previous.page;
            p.message = None;
            p.pages.take();
            self.show(Screen::OnlineLookup);
            return;
        }
        let back = self.online.take().map_or(Screen::Library, |p| p.back);
        self.show(back);
        if back == Screen::Definition {
            self.restore_dictionary_online_return();
        }
    }

    pub(super) fn draw_online(&self, canvas: &mut Canvas) {
        let Some(p) = &self.online else { return };
        let r = &p.request;
        canvas.popup(p.sheet());
        canvas.text(Rect::new(104, 332, 846, 80), r.service.name(), BODY.bold());
        canvas.button(
            CLOSE,
            if p.job.is_some() {
                "Cancel"
            } else if p.introductions.is_some() {
                "Back"
            } else {
                "Done"
            },
            false,
            false,
        );
        if let Some(article) = p.articles.get(p.article) {
            canvas.text(TOPIC, &article.title, BODY.bold().lines(2));
            let style = BODY.lines(10);
            let display_text = article.display_text();
            let pages = p
                .pages
                .get_or_init(|| canvas.text_pages(BODY_RECT, &display_text, style));
            if let Some(range) = pages.get(p.page) {
                canvas.text(BODY_RECT, &display_text[range.clone()], style);
            }
            let attribution = if r.service == Service::Wikipedia {
                format!("{}.wikipedia.org · Wikipedia contributors · CC BY-SA", r.to)
            } else {
                format!("{} · {} to {}", r.service.name(), r.from, r.to)
            };
            canvas.text(
                Rect::new(108, 1135, 1038, 84),
                &format!("{} / {}   ·   {attribution}", p.page + 1, p.page_count()),
                CAPTION,
            );
            if p.page > 0 {
                canvas.button(PREV, "Previous", false, false);
            }
            if p.page + 1 < p.page_count() {
                canvas.button(NEXT, "Next", false, false);
            }
            if p.articles.len() > 1 {
                canvas.button(ALTERNATE, "Next result", false, false);
            }
            if r.service == Service::Wikipedia && !r.full_article {
                canvas.button(SUBMIT, "Full article", false, false);
            }
            return;
        }
        canvas.text(TOPIC, &r.text, BODY.lines(2));
        if p.job.is_some() {
            canvas.text(BODY_RECT, "Looking up the text…", BODY);
            return;
        }
        if r.service != Service::Wikipedia {
            canvas.button(PROVIDER, r.service.name(), false, false);
            canvas.button(FROM, &format!("From {}", r.from), false, false);
        } else {
            canvas.button(PROVIDER, "Edit topic", false, false);
        }
        canvas.button(TO, &format!("Language {}", r.to), false, false);
        let hint = if r.service == Service::Wikipedia {
            format!("Search sends this topic to {}.wikipedia.org.", r.to)
        } else {
            format!(
                "Translate sends this text to {}. Open Language to change the translation language.",
                r.service.name()
            )
        };
        canvas.text(
            Rect::new(108, 733, 1040, 138),
            p.message
                .as_deref()
                .or(self.notice.as_deref())
                .unwrap_or(&hint),
            BODY.lines(3),
        );
        canvas.button(
            p.submit(),
            if r.service == Service::Wikipedia {
                "Search"
            } else {
                "Translate"
            },
            true,
            false,
        );
    }

    fn start_online(&mut self) -> Change {
        let Some(p) = &mut self.online else {
            return Change::None;
        };
        match Job::start(&p.request) {
            Ok(job) => {
                p.job = Some(job);
                p.message = None;
            }
            Err(e) => p.message = Some(e.to_string()),
        }
        Change::Partial
    }

    pub(super) fn online_input(&mut self, input: Input) -> Change {
        let Some(p) = &mut self.online else {
            return Change::None;
        };
        if matches!(input,Input::Tap(x,y) if CLOSE.contains(x,y) || !p.sheet().contains(x,y)) {
            self.close_online();
            return Change::Partial;
        }
        if p.job.is_some() {
            return Change::None;
        }
        if !p.articles.is_empty() {
            return match input {
                Input::PageForward => p.step_page(1),
                Input::PageBack => p.step_page(-1),
                Input::Tap(x, y) if NEXT.contains(x, y) => p.step_page(1),
                Input::Tap(x, y) if PREV.contains(x, y) => p.step_page(-1),
                Input::Tap(x, y) if ALTERNATE.contains(x, y) && p.articles.len() > 1 => {
                    p.article = (p.article + 1) % p.articles.len();
                    p.pages.take();
                    p.page = 0;
                    Change::Partial
                }
                Input::Tap(x, y)
                    if SUBMIT.contains(x, y)
                        && p.request.service == Service::Wikipedia
                        && !p.request.full_article =>
                {
                    let title = p.articles[p.article].title.clone();
                    p.introductions = Some(Introductions {
                        request: p.request.clone(),
                        articles: std::mem::take(&mut p.articles),
                        article: p.article,
                        page: p.page,
                    });
                    p.request.text = title;
                    p.request.full_article = true;
                    p.page = 0;
                    p.articles.clear();
                    p.pages.take();
                    self.start_online()
                }
                _ => Change::None,
            };
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if p.submit().contains(x, y) {
            return self.start_online();
        }
        p.message = None;
        if TO.contains(x, y) {
            self.edit_online(Field::To);
        } else if FROM.contains(x, y) && p.request.service != Service::Wikipedia {
            self.edit_online(Field::From);
        } else if PROVIDER.contains(x, y) && p.request.service == Service::Wikipedia {
            self.edit_online(Field::Topic);
        } else if PROVIDER.contains(x, y) {
            let next = if p.request.service == Service::Google {
                Service::MyMemory
            } else {
                Service::Google
            };
            p.request.service = next;
            if let Err(e) = self.update_lookup_history(|h| h.online_languages.translator = next) {
                return self.report(e);
            }
        } else {
            return Change::None;
        }
        Change::Partial
    }

    pub(super) fn poll_online(&mut self, now: Instant) -> Option<Change> {
        let p = self.online.as_mut()?;
        let job = p.job.as_mut()?;
        if now < job.poll_at {
            return None;
        }
        let reply = job.poll(now)?;
        p.job.take();
        match reply {
            Ok(articles) => {
                p.articles = articles;
                p.article = 0;
                p.page = 0;
                p.pages.take();
                let request = p.request.clone();
                if !self.lookup_history.online_languages.forget_queries
                    && let Err(e) = self.update_lookup_history(|h| h.online(request))
                {
                    self.notice = Some(format!("Could not save lookup history: {e}"));
                }
            }
            Err(message) => p.message = Some(message),
        }
        Some(Change::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture;
    use primitive_app::{App, frame};
    use serde_json::json;
    use std::fs;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn romanization_setting_defaults_off_persists_and_is_used_by_google_requests() {
        let older_preferences: Languages = serde_json::from_value(json!({
            "wikipedia":"en",
            "source":"auto",
            "target":"en",
            "translator":"Google",
            "forget_queries":false
        }))
        .unwrap();
        assert!(!older_preferences.show_romanizations);

        let root = std::env::temp_dir().join(format!(
            "reader-romanization-settings-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let font = test_fixture("fonts/NotoSans-Regular.ttf");
        let state = root.join("library.json");
        let mut app = Reader::open(&root, &state, vec![font.clone()]).unwrap();
        assert!(!app.lookup_history.online_languages.show_romanizations);
        app.show(Screen::TranslationSettings);
        assert!(
            app.top_config_items(super::super::TopGroup::Search)
                .iter()
                .any(|item| item.title == "Translation settings")
        );
        let items = app.items();
        assert_eq!(items[0].title, "Show romanizations");
        assert_eq!(
            items[0].detail.as_deref(),
            Some("Show Latin text with Google Translate results")
        );

        let mut canvas =
            Canvas::new(WIDTH, HEIGHT, &[font.to_string_lossy().into_owned()]).unwrap();
        if let Some(dir) = std::env::var_os("PRIMITIVE_READER_LOOKUP_SCREENSHOTS") {
            fs::create_dir_all(&dir).unwrap();
            primitive_app::write_png(
                &std::path::Path::new(&dir).join("translation-settings-romanization-off.png"),
                WIDTH,
                HEIGHT,
                &frame(&app, &mut canvas, false),
            )
            .unwrap();
        }

        app.activate(Action::ToggleRomanizations).unwrap();
        assert!(app.lookup_history.online_languages.show_romanizations);
        let items = app.items();
        assert_eq!(
            items[0].detail.as_deref(),
            Some("Show Latin text with Google Translate results")
        );
        app.prepare_online_text(false, "你好").unwrap();
        assert!(app.online.as_ref().unwrap().request.with_romanization);
        app.online.as_mut().unwrap().articles = vec![crate::online_lookup::Article {
            title: "Translation".into(),
            text: "Good morning".into(),
            source: Some("Καλημέρα".into()),
            romanization: Some("kaliméra".into()),
        }];
        if let Some(dir) = std::env::var_os("PRIMITIVE_READER_LOOKUP_SCREENSHOTS") {
            fs::create_dir_all(&dir).unwrap();
            primitive_app::write_png(
                &std::path::Path::new(&dir).join("online-romanization-result.png"),
                WIDTH,
                HEIGHT,
                &frame(&app, &mut canvas, false),
            )
            .unwrap();
        }

        let mut reopened = Reader::open(&root, &state, vec![font]).unwrap();
        assert!(reopened.lookup_history.online_languages.show_romanizations);
        reopened.show(Screen::TranslationSettings);
        if let Some(dir) = std::env::var_os("PRIMITIVE_READER_LOOKUP_SCREENSHOTS") {
            fs::create_dir_all(&dir).unwrap();
            primitive_app::write_png(
                &std::path::Path::new(&dir).join("translation-settings-romanization-on.png"),
                WIDTH,
                HEIGHT,
                &frame(&reopened, &mut canvas, false),
            )
            .unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_input_renders_selection_lookup_pagination_and_restores_offline_history() {
        let root = std::env::temp_dir().join(format!("reader-online-panel-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let book = root.join("Field Notes.md");
        fs::write(&book, "# An afternoon at the shore\n\nWe walked down the stone steps just before noon. The tide had left a dark line along the harbour wall.\n\nAt the lowest step we opened the notebook and recorded the time. A small pool held a broken shell and a piece of sea glass.\n\nAn hour later the dry stones were under water. We marked the new level and followed the path home.").unwrap();
        let font = test_fixture("fonts/NotoSans-Regular.ttf");
        let state = root.join("library.json");
        let mut app = Reader::open(&root, &state, vec![font.clone()]).unwrap();
        app.controls.dictionary_on_single_word = Some(false);
        app.open_path(&book).unwrap();
        let mut canvas =
            Canvas::new(WIDTH, HEIGHT, &[font.to_string_lossy().into_owned()]).unwrap();
        let screenshot = |name: &str, pixels: &[u8]| {
            if let Some(dir) = std::env::var_os("PRIMITIVE_READER_LOOKUP_SCREENSHOTS") {
                fs::create_dir_all(&dir).unwrap();
                primitive_app::write_png(
                    &std::path::Path::new(&dir).join(name),
                    WIDTH,
                    HEIGHT,
                    pixels,
                )
                .unwrap();
            }
        };
        let reading = &app.active.as_ref().unwrap().reading;
        let start = reading.probe.matches("tide").into_iter().next().unwrap();
        let mut end = start.clone();
        end.offset += 4;
        let selection = reading.probe.select_range(&start, &end).unwrap();
        let bounds = reading.highlight_rects(&selection)[0];
        let x = (bounds.x + bounds.width / 2) as f32;
        let y = (bounds.y + bounds.height / 2) as f32;
        app.input(Input::LongTap(x, y));
        assert_eq!(app.selection.as_ref().unwrap().text, "tide");
        screenshot("selection-actions.png", &frame(&app, &mut canvas, false));
        let rect = app.selection_button(super::super::selection_popup::SelectionAction::Wikipedia);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ));
        let panel = app.online.as_mut().unwrap();
        assert_eq!(panel.request.text, "tide");
        assert!(panel.job.is_none());
        screenshot("online-preview.png", &frame(&app, &mut canvas, false));
        let panel = app.online.as_mut().unwrap();
        let reply = json!({"Ok":[{"title":"Tide","text":("A tide is the regular rise and fall of the sea. The height and timing vary from one coast to another. People who use a harbour record these changes against a fixed point on shore.\n\nA tide table lists predicted high and low waters. Observations in a field notebook can be compared across successive days.\n\n".to_owned() + &"Notes from a harbour can be arranged by date and water level. ".repeat(25))},{"title":"Tidal range","text":"The difference between high and low water."}]});
        let mut cmd = Command::new("/bin/sh");
        cmd.args([
            "-c",
            r#"cat >/dev/null && printf '%s' "$1""#,
            "lookup-fixture",
            &reply.to_string(),
        ]);
        panel.job = Some(Job::spawn(cmd, &panel.request).unwrap());
        let deadline = Instant::now() + Duration::from_secs(4);
        while app.online.as_ref().unwrap().job.is_some() {
            assert!(Instant::now() < deadline);
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
        screenshot("online-result.png", &frame(&app, &mut canvas, false));
        assert!(app.online.as_ref().unwrap().page_count() > 1);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().page, 1);
        let next = Input::Tap((ALTERNATE.x + 60) as f32, (ALTERNATE.y + 50) as f32);
        app.input(next);
        assert_eq!(app.online.as_ref().unwrap().article, 1);
        assert_eq!(app.online.as_ref().unwrap().page, 0);
        app.input(Input::Tap((CLOSE.x + 50) as f32, (CLOSE.y + 50) as f32));
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.selection.as_ref().unwrap().text, "tide");
        let mut reopened = Reader::open(&root, &state, vec![font]).unwrap();
        assert_eq!(reopened.lookup_history.online.last().unwrap().text, "tide");
        reopened.show(Screen::OnlineHistory);
        reopened.activate(Action::RepeatOnline(0)).unwrap();
        assert!(reopened.online.as_ref().unwrap().job.is_none());
        reopened.close_online();
        reopened.activate(Action::ToggleOnlineHistory).unwrap();
        reopened.show(Screen::ClearOnlineHistory);
        reopened.input(Input::PageBack);
        assert_eq!(reopened.lookup_history.online.len(), 1);
        reopened.show(Screen::ClearOnlineHistory);
        reopened.input(Input::Tap(800.0, 961.0));
        assert!(reopened.lookup_history.online.is_empty());
        assert!(reopened.lookup_history.online_languages.forget_queries);
        fs::remove_dir_all(root).unwrap();
    }
}
