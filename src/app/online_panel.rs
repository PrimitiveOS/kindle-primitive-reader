//! A lookup starts with a preview of the text and chosen provider over the current page.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Range;
use std::time::Instant;

#[cfg(test)]
use primitive_app::HEIGHT;
use primitive_app::{Change, WIDTH};
use primitive_ui::{
    BODY, CAPTION, Canvas, Editor, GrayImage, Input, Layout, PageTurn, Pager, Press, Rect,
    keyboard_key,
};
use serde::{Deserialize, Serialize};

use super::{Action, Item, Reader, Screen};
use crate::Result;
use crate::online_lookup::{
    Article, ImageJob, ImageRequest, Job, MAX_PICTURES, MAX_TEXT_BYTES, Picture, Request, Service,
    language,
};

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
const THUMB: Rect = Rect::new(946, 449, 220, 109);
const GALLERY: Rect = Rect::new(106, 567, 1048, 479);
const CAPTION_AREA: Rect = Rect::new(106, 1065, 1048, 138);
const IMAGE_PAGER: Rect = Rect::new(100, 1238, 1066, 110);
const BODY_LINES: usize = 10;
// The shared canvas lays out 36 px BODY text at 1.25 em per line.
const BODY_LINE: i32 = 45;
const FIGURE_GAP: i32 = 18;
const PHOTO_LIMIT: Rect = Rect::new(0, 0, BODY_RECT.w / 2, 5 * BODY_LINE - FIGURE_GAP);

// Keep the article's physical column width and line breaks across rotations.
// Moving the shorter request preview with the results keeps its header in place.
#[derive(Clone, Copy, Default)]
struct SheetPosition {
    x: i32,
    y: i32,
}

impl SheetPosition {
    fn new(viewport: crate::session::ReadingViewport) -> Self {
        let (width, height) = viewport.dimensions();
        Self {
            x: (width as i32 - WIDTH as i32) / 2,
            y: (height as i32 - SHEET.bottom() - 28).min(0),
        }
    }

    fn rect(self, local: Rect) -> Rect {
        Rect::new(local.x + self.x, local.y + self.y, local.w, local.h)
    }

    fn local(self, x: f32, y: f32) -> (f32, f32) {
        (x - self.x as f32, y - self.y as f32)
    }
}

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
    pub show_wiki_image: bool,
    pub show_wiki_more: bool,
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
            show_wiki_image: true,
            show_wiki_more: true,
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

#[derive(Default)]
struct Media {
    job: Option<ImageJob>,
    results: BTreeMap<usize, Vec<Picture>>,
    current: Option<usize>,
    show_caption: bool,
}

struct Introductions {
    request: Request,
    articles: Vec<Article>,
    article: usize,
    pages: ArticlePages,
    media: Media,
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
    pages: ArticlePages,
    media: Media,
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
            pages: ArticlePages::default(),
            media: Media::default(),
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
        self.pages.frames.borrow().len().max(1)
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.job
            .as_ref()
            .map(|job| job.poll_at)
            .into_iter()
            .chain(self.media.job.as_ref().map(|job| job.poll_at))
            .min()
    }

    pub(super) fn cancel_for_sleep(&mut self) {
        if self.job.take().is_some() {
            self.message = Some("Lookup cancelled when Reader went to sleep.".into());
        }
        self.media.job.take();
    }

    fn start_media(&mut self, prefs: &Languages) {
        self.media.job.take();
        self.media.current = None;
        self.media.show_caption = false;
        if self.request.service != Service::Wikipedia
            || !prefs.show_wiki_image
            || self.media.results.contains_key(&self.article)
        {
            return;
        }
        let Some(article) = self.articles.get(self.article) else {
            return;
        };
        let more = self.request.full_article && prefs.show_wiki_more;
        if more || article.thumbnail.is_some() {
            self.media.job = ImageJob::start_images(&ImageRequest {
                title: article.title.clone(),
                language: self.request.to.clone(),
                lead: article.thumbnail.clone(),
                more,
            })
            .ok();
        }
    }

    fn poll_media(&mut self, now: Instant) -> Option<Change> {
        let job = self.media.job.as_mut()?;
        if now < job.poll_at {
            return None;
        }
        let result = job.poll(now)?;
        self.media.job.take();
        let images: Vec<_> = result
            .unwrap_or_default()
            .into_iter()
            .take(MAX_PICTURES)
            .filter(Picture::valid)
            .collect();
        let changed = !images.is_empty();
        self.media.results.insert(self.article, images);
        Some(if changed {
            Change::Partial
        } else {
            Change::None
        })
    }

    fn images(&self) -> &[Picture] {
        self.media
            .results
            .get(&self.article)
            .map_or(&[], Vec::as_slice)
    }

    fn step_image(&mut self, step: isize) -> Change {
        if let Some(next) = self
            .media
            .current
            .and_then(|index| index.checked_add_signed(step))
            .filter(|&index| index < self.images().len())
        {
            self.media.current = Some(next);
            return Change::Partial;
        }
        Change::None
    }

    fn step_page(&mut self, step: isize) -> Change {
        let Some(next) = self
            .pages
            .index
            .get()
            .checked_add_signed(step)
            .filter(|&p| p < self.page_count())
        else {
            return Change::None;
        };
        self.pages.index.set(next);
        self.media.show_caption = false;
        Change::Partial
    }

    fn image_at(&self, position: SheetPosition, x: f32, y: f32) -> Option<usize> {
        let (x, y) = position.local(x, y);
        if self.request.full_article {
            return self
                .pages
                .current()?
                .figure
                .filter(|figure| figure.photo.contains(x, y))
                .map(|figure| figure.picture);
        }
        (THUMB.contains(x, y) && !self.images().is_empty()).then_some(0)
    }
}

#[derive(Clone, Copy)]
struct Figure {
    picture: usize,
    photo: Rect,
    beside: Rect,
    below: Rect,
    split: usize,
    wrapped_lines: usize,
}

#[derive(Clone)]
struct ArticlePage {
    source: Range<usize>,
    figure: Option<Figure>,
}

#[derive(Default)]
struct ArticlePages {
    index: Cell<usize>,
    pictures: Cell<Option<usize>>,
    frames: RefCell<Vec<ArticlePage>>,
}

impl ArticlePages {
    fn current(&self) -> Option<ArticlePage> {
        self.frames.borrow().get(self.index.get()).cloned()
    }

    fn prepare(&self, canvas: &mut Canvas, text: &str, pictures: &[Picture]) {
        if self.pictures.get() == Some(pictures.len()) {
            return;
        }
        // Keep the first byte of the currently displayed page in view when
        // background images change line widths and therefore page boundaries.
        let anchor = self.current().map_or(0, |page| page.source.start);
        let mut frames = self.frames.borrow_mut();
        *frames = article_pages(canvas, text, pictures);
        self.index.set(
            frames
                .iter()
                .position(|page| page.source.contains(&anchor))
                .unwrap_or(0),
        );
        self.pictures.set(Some(pictures.len()));
    }
}

fn article_pages(canvas: &mut Canvas, text: &str, pictures: &[Picture]) -> Vec<ArticlePage> {
    let mut pages = Vec::new();
    let mut start = 0;
    for (index, picture) in pictures.iter().enumerate() {
        if start >= text.len() {
            break;
        }
        let (width, height) = fitted_picture_size(PHOTO_LIMIT, picture);
        // Keep a usable touch and caption area even for a small raster.
        let width = (width as i32).max(250);
        let height = (height as i32).max(122);
        let wrapped_lines = (height + FIGURE_GAP + BODY_LINE - 1) / BODY_LINE;
        let photo = Rect::new(BODY_RECT.right() - width, BODY_RECT.y, width, height);
        let beside = Rect::new(
            BODY_RECT.x,
            BODY_RECT.y,
            photo.x - BODY_RECT.x - FIGURE_GAP,
            wrapped_lines * BODY_LINE,
        );
        let below = Rect::new(
            BODY_RECT.x,
            beside.bottom(),
            BODY_RECT.w,
            BODY_RECT.h - beside.h,
        );
        let wrapped_lines = wrapped_lines as usize;
        let split = start + page_end(canvas, beside, &text[start..], wrapped_lines);
        let end = split + page_end(canvas, below, &text[split..], BODY_LINES - wrapped_lines);
        pages.push(ArticlePage {
            source: start..end,
            figure: Some(Figure {
                picture: index,
                photo,
                beside,
                below,
                split,
                wrapped_lines,
            }),
        });
        start = end;
    }
    pages.extend(
        canvas
            .text_pages(BODY_RECT, &text[start..], BODY.lines(BODY_LINES))
            .into_iter()
            .map(|range| ArticlePage {
                source: start + range.start..start + range.end,
                figure: None,
            }),
    );
    pages
}

fn page_end(canvas: &mut Canvas, rect: Rect, text: &str, lines: usize) -> usize {
    canvas
        .text_pages(rect, text, BODY.lines(lines).break_long_words())
        .first()
        .map_or(text.len(), |range| range.end)
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

    pub(super) fn wikipedia_settings_items(&self) -> Vec<Item> {
        let enabled = self.lookup_history.online_languages.show_wiki_image;
        vec![
            Item::new("Show image in search results", Action::ToggleWikipediaImage)
                .detail("Include the lead image in Wikipedia lookups"),
            if enabled {
                Item::new(
                    "Show more images in full article",
                    Action::ToggleWikipediaMoreImages,
                )
                .detail("Include figures and gallery images")
            } else {
                Item::new("Show more images in full article", Action::Heading)
                    .detail("Turn on search result images first")
            },
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
        self.draw_reading_editor_header(canvas, p.field.title());
        let width = canvas.bounds().w;
        canvas.single_line_field(
            Rect::new(40, 160, width - 80, 124),
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
            Rect::new(46, 324, width - 92, 180),
            self.notice.as_deref().unwrap_or(help),
            CAPTION.lines(3),
        );
        canvas.keyboard(self.editor.layer, "Apply", None);
    }

    pub(super) fn online_editor_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if self.reading_editor_back().contains(x,y))
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
        let (width, height) = self.reading_viewport().dimensions();
        let layout = Layout::new(width, height, 4, 5);
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
            && p.media.current.take().is_some()
        {
            return;
        }
        if let Some(p) = &mut self.online
            && let Some(previous) = p.introductions.take()
        {
            p.job.take();
            p.media = previous.media;
            p.request = previous.request;
            p.articles = previous.articles;
            p.article = previous.article;
            p.pages = previous.pages;
            p.message = None;
            p.start_media(&self.lookup_history.online_languages);
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
        let position = SheetPosition::new(self.reading_viewport());
        canvas.popup(position.rect(p.sheet()));
        canvas.text(
            position.rect(Rect::new(104, 332, 846, 80)),
            r.service.name(),
            BODY.bold(),
        );
        canvas.button(
            position.rect(CLOSE),
            if p.job.is_some() {
                "Cancel"
            } else if p.introductions.is_some() || p.media.current.is_some() {
                "Back"
            } else {
                "Done"
            },
            false,
            false,
        );
        if let Some(article) = p.articles.get(p.article) {
            if let Some(index) = p.media.current
                && let Some(picture) = p.images().get(index)
            {
                self.draw_wikipedia_picture(
                    canvas,
                    article,
                    picture,
                    index,
                    p.images().len(),
                    position,
                );
                return;
            }
            let mut title_rect = position.rect(TOPIC);
            if !r.full_article
                && let Some(picture) = p.images().first()
            {
                let thumbnail = position.rect(THUMB);
                title_rect.w = thumbnail.x - title_rect.x - 20;
                let pressed = canvas.control_pressed(thumbnail, false);
                canvas.panel(thumbnail, 16, 4, if pressed { 0 } else { 255 });
                let mut photo = thumbnail.inset(8);
                photo.w -= 54;
                draw_picture(canvas, photo, picture);
                let glyph = Rect::new(thumbnail.right() - 53, thumbnail.y + 35, 36, 36);
                canvas.icon(
                    glyph,
                    super::bar_icons::SEARCH,
                    if pressed { 255 } else { 0 },
                );
            }
            canvas.text(title_rect, &article.title, BODY.bold().lines(2));
            let display_text = article.display_text();
            let pictures = if r.full_article { p.images() } else { &[] };
            p.pages.prepare(canvas, &display_text, pictures);
            let current = p.pages.current();
            if let Some(page) = &current {
                draw_article_page(
                    canvas,
                    &display_text,
                    page,
                    pictures,
                    p.media.show_caption,
                    position,
                );
            }
            let index = p.pages.index.get();
            let attribution = if r.service == Service::Wikipedia {
                format!("{}.wikipedia.org · Wikipedia contributors · CC BY-SA", r.to)
            } else {
                format!("{} · {} to {}", r.service.name(), r.from, r.to)
            };
            canvas.text(
                position.rect(Rect::new(108, 1135, 1038, 84)),
                &format!("{} / {}   ·   {attribution}", index + 1, p.page_count()),
                CAPTION,
            );
            if index > 0 {
                canvas.button(position.rect(PREV), "Previous", false, false);
            }
            if index + 1 < p.page_count() {
                canvas.button(position.rect(NEXT), "Next", false, false);
            }
            if p.articles.len() > 1 {
                canvas.button(position.rect(ALTERNATE), "Next result", false, false);
            }
            if r.service == Service::Wikipedia && !r.full_article {
                canvas.button(position.rect(SUBMIT), "Full article", false, false);
            } else if current.is_some_and(|page| page.figure.is_some()) {
                canvas.button(position.rect(SUBMIT), "Open image", false, false);
            }
            return;
        }
        canvas.text(position.rect(TOPIC), &r.text, BODY.lines(2));
        if p.job.is_some() {
            canvas.text(position.rect(BODY_RECT), "Looking up the text…", BODY);
            return;
        }
        if r.service != Service::Wikipedia {
            canvas.button(position.rect(PROVIDER), r.service.name(), false, false);
            canvas.button(
                position.rect(FROM),
                &format!("From {}", r.from),
                false,
                false,
            );
        } else {
            canvas.button(position.rect(PROVIDER), "Edit topic", false, false);
        }
        canvas.button(
            position.rect(TO),
            &format!("Language {}", r.to),
            false,
            false,
        );
        let hint = if r.service == Service::Wikipedia {
            format!("Search sends this topic to {}.wikipedia.org.", r.to)
        } else {
            format!(
                "Translate sends this text to {}. Open Language to change the translation language.",
                r.service.name()
            )
        };
        canvas.text(
            position.rect(Rect::new(108, 733, 1040, 138)),
            p.message
                .as_deref()
                .or(self.notice.as_deref())
                .unwrap_or(&hint),
            BODY.lines(3),
        );
        canvas.button(
            position.rect(p.submit()),
            if r.service == Service::Wikipedia {
                "Search"
            } else {
                "Translate"
            },
            true,
            false,
        );
    }

    fn draw_wikipedia_picture(
        &self,
        canvas: &mut Canvas,
        article: &Article,
        picture: &Picture,
        index: usize,
        total: usize,
        position: SheetPosition,
    ) {
        canvas.text(position.rect(TOPIC), &article.title, BODY.bold().lines(2));
        draw_picture(canvas, position.rect(GALLERY), picture);
        let filename = picture.name.replace('_', " ");
        let details = if picture.caption.is_empty() {
            format!("Wikimedia · {filename}")
        } else {
            format!("{}\nWikimedia · {filename}", picture.caption)
        };
        canvas.text(position.rect(CAPTION_AREA), &details, CAPTION.lines(4));
        picture_pager(index, total, position).draw(canvas, None);
    }

    fn start_online(&mut self) -> Change {
        let Some(p) = &mut self.online else {
            return Change::None;
        };
        match Job::start(
            &p.request,
            self.lookup_history.online_languages.show_wiki_image,
        ) {
            Ok(job) => {
                p.job = Some(job);
                p.message = None;
            }
            Err(e) => p.message = Some(e.to_string()),
        }
        Change::Partial
    }

    pub(super) fn online_input(&mut self, input: Input) -> Change {
        let position = SheetPosition::new(self.reading_viewport());
        let body = position.rect(BODY_RECT);
        let Some(p) = &mut self.online else {
            return Change::None;
        };
        if matches!(input,Input::Tap(x,y) if position.rect(CLOSE).contains(x,y) || !position.rect(p.sheet()).contains(x,y))
        {
            self.close_online();
            return Change::Partial;
        }
        if p.job.is_some() {
            return Change::None;
        }
        if let Some(index) = p.media.current {
            let direction = match input {
                Input::PageForward => Some(PageTurn::Next),
                Input::PageBack => Some(PageTurn::Previous),
                Input::Tap(x, y) => picture_pager(index, p.images().len(), position).hit(x, y),
                _ => None,
            };
            return match direction {
                Some(PageTurn::Next) => p.step_image(1),
                Some(PageTurn::Previous) => p.step_image(-1),
                None => Change::None,
            };
        }
        if !p.articles.is_empty() {
            return match input {
                Input::PageForward => p.step_page(1),
                Input::PageBack => p.step_page(-1),
                Input::Swipe { dx, dy, start_y }
                    if p.request.full_article
                        && dy.abs() > dx.abs()
                        && (body.y as f32..body.bottom() as f32).contains(&start_y) =>
                {
                    p.step_page(if dy < 0.0 { 1 } else { -1 })
                }
                Input::Tap(x, y) if position.rect(NEXT).contains(x, y) => p.step_page(1),
                Input::Tap(x, y) if position.rect(PREV).contains(x, y) => p.step_page(-1),
                Input::Tap(x, y) if let Some(index) = p.image_at(position, x, y) => {
                    if p.request.full_article {
                        p.media.show_caption = !p.media.show_caption;
                    } else {
                        p.media.current = Some(index);
                    }
                    Change::Partial
                }
                Input::LongTap(x, y)
                | Input::HoldRelease {
                    start_x: x,
                    start_y: y,
                    ..
                } if p.request.full_article
                    && let Some(index) = p.image_at(position, x, y) =>
                {
                    p.media.current = Some(index);
                    Change::Partial
                }
                Input::Tap(x, y)
                    if position.rect(SUBMIT).contains(x, y)
                        && let Some(figure) = p.pages.current().and_then(|page| page.figure) =>
                {
                    p.media.current = Some(figure.picture);
                    Change::Partial
                }
                Input::Tap(x, y)
                    if position.rect(ALTERNATE).contains(x, y) && p.articles.len() > 1 =>
                {
                    p.article = (p.article + 1) % p.articles.len();
                    p.pages = ArticlePages::default();
                    p.start_media(&self.lookup_history.online_languages);
                    Change::Partial
                }
                Input::Tap(x, y)
                    if position.rect(SUBMIT).contains(x, y)
                        && p.request.service == Service::Wikipedia
                        && !p.request.full_article =>
                {
                    let title = p.articles[p.article].title.clone();
                    p.media.job.take();
                    p.introductions = Some(Introductions {
                        request: p.request.clone(),
                        articles: std::mem::take(&mut p.articles),
                        article: p.article,
                        pages: std::mem::take(&mut p.pages),
                        media: std::mem::take(&mut p.media),
                    });
                    p.request.text = title;
                    p.request.full_article = true;
                    self.start_online()
                }
                _ => Change::None,
            };
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if position.rect(p.submit()).contains(x, y) {
            return self.start_online();
        }
        p.message = None;
        if position.rect(TO).contains(x, y) {
            self.edit_online(Field::To);
        } else if position.rect(FROM).contains(x, y) && p.request.service != Service::Wikipedia {
            self.edit_online(Field::From);
        } else if position.rect(PROVIDER).contains(x, y) && p.request.service == Service::Wikipedia
        {
            self.edit_online(Field::Topic);
        } else if position.rect(PROVIDER).contains(x, y) {
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
        let Some(job) = p.job.as_mut() else {
            return p.poll_media(now);
        };
        if now < job.poll_at {
            return None;
        }
        let reply = job.poll(now)?;
        p.job.take();
        match reply {
            Ok(articles) => {
                p.articles = articles;
                p.article = 0;
                p.pages = ArticlePages::default();
                p.start_media(&self.lookup_history.online_languages);
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

fn draw_article_page(
    canvas: &mut Canvas,
    text: &str,
    page: &ArticlePage,
    pictures: &[Picture],
    show_caption: bool,
    position: SheetPosition,
) {
    let Some(figure) = page.figure else {
        canvas.text(
            position.rect(BODY_RECT),
            &text[page.source.clone()],
            BODY.lines(BODY_LINES),
        );
        return;
    };
    canvas.text(
        position.rect(figure.beside),
        text[page.source.start..figure.split].trim_end(),
        BODY.lines(figure.wrapped_lines).break_long_words(),
    );
    canvas.text(
        position.rect(figure.below),
        text[figure.split..page.source.end].trim_end(),
        BODY.lines(BODY_LINES - figure.wrapped_lines)
            .break_long_words(),
    );
    let picture = &pictures[figure.picture];
    let photo = position.rect(figure.photo);
    if show_caption {
        canvas.panel(photo, 12, 2, 255);
        let name = picture.name.replace('_', " ");
        let details = if picture.caption.is_empty() {
            name
        } else {
            format!("{name}\n{}", picture.caption)
        };
        canvas.text(
            photo.inset(10),
            &details,
            CAPTION
                .break_long_words()
                .lines(((figure.photo.h - 20) / 35).max(1) as usize),
        );
    } else {
        draw_picture(canvas, photo, picture);
    }
}

fn picture_pager(index: usize, count: usize, position: SheetPosition) -> Pager {
    Pager {
        bounds: position.rect(IMAGE_PAGER),
        page: index,
        pages: count,
    }
}

fn fitted_picture_size(slot: Rect, picture: &Picture) -> (u32, u32) {
    let ratio = (slot.w as f32 / picture.width as f32)
        .min(slot.h as f32 / picture.height as f32)
        .min(1.5);
    (
        (picture.width as f32 * ratio).round().max(1.0) as u32,
        (picture.height as f32 * ratio).round().max(1.0) as u32,
    )
}

fn draw_picture(canvas: &mut Canvas, slot: Rect, picture: &Picture) {
    let (width, height) = fitted_picture_size(slot, picture);
    let Some(source) =
        image::GrayImage::from_raw(picture.width, picture.height, picture.pixels.clone())
    else {
        return;
    };
    // The shared canvas transfers pixels one-for-one. Give it the fitted
    // raster so thumbnails show the entire subject and center in their slot.
    let fitted = if (width, height) == source.dimensions() {
        source
    } else {
        image::imageops::resize(
            &source,
            width,
            height,
            image::imageops::FilterType::Triangle,
        )
    };
    let frame = Rect::new(
        slot.x + (slot.w - width as i32) / 2,
        slot.y + (slot.h - height as i32) / 2,
        width as i32,
        height as i32,
    );
    canvas.picture(
        frame,
        GrayImage {
            width,
            height,
            pixels: fitted.into_raw(),
        },
    );
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
            thumbnail: None,
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
        assert_eq!(app.online.as_ref().unwrap().pages.index.get(), 1);
        let next = Input::Tap((ALTERNATE.x + 60) as f32, (ALTERNATE.y + 50) as f32);
        app.input(next);
        assert_eq!(app.online.as_ref().unwrap().article, 1);
        assert_eq!(app.online.as_ref().unwrap().pages.index.get(), 0);
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

#[cfg(test)]
mod wikipedia_image_tests {
    use super::*;
    use crate::app::tests::{Fixture, capture_test_screen};
    use primitive_app::{App, frame};
    use std::process::Command;
    use std::time::Duration;

    const EXTRACT: &str = include_str!("../../tests/fixtures/wikipedia-full-article-extract.txt");

    fn tap(reader: &mut Reader, rect: Rect) -> Change {
        reader.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    fn drawing(second: bool) -> Picture {
        let (width, height) = (416_i32, 260_i32);
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let horizon = 143;
                    if second && (50..360).contains(&x) && (118..145).contains(&y) {
                        return 40;
                    }
                    if (113..305).contains(&x)
                        && (174..191).contains(&y)
                        && x > 113 + (y - 174) * 3
                        && x < 305 - (y - 174) * 2
                    {
                        return 26;
                    }
                    if (192..197).contains(&x) && (54..174).contains(&y) {
                        return 48;
                    }
                    if (59..160).contains(&y) && (199..235).contains(&x) && x - 199 < (y - 59) / 3 {
                        return 247;
                    }
                    if y > horizon {
                        if (y / 9 + x / 65) % 5 == 0 {
                            205
                        } else {
                            146 + (y - horizon) as u8 / 3
                        }
                    } else if ((x - 310).pow(2) as u32 + (y - 69).pow(2) as u32) < 650 {
                        234
                    } else {
                        188 + y as u8 / 4
                    }
                })
            })
            .collect();
        Picture {
            width: width as u32,
            height: height as u32,
            pixels,
            caption: if second {
                "The jetty at the edge of the channel."
            } else {
                "A sailing boat entering the harbor at dusk."
            }
            .into(),
            name: if second { "Jetty.png" } else { "Harbor.png" }.into(),
        }
    }

    fn canvases() -> (Vec<std::path::PathBuf>, Canvas) {
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf"),
            crate::test_fixture("fonts/NotoSans-Bold.ttf"),
        ];
        let paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
        let canvas = Canvas::new(WIDTH, HEIGHT, &paths).unwrap();
        (fonts, canvas)
    }

    fn ready(app: &mut Reader) {
        app.prepare_online_text(true, "Harbor navigation").unwrap();
        let panel = app.online.as_mut().unwrap();
        panel.articles.push(Article {
            title: "Harbor navigation".into(),
            text: "A nautical chart identifies navigable water, obstructions, and the channels used to enter a harbor. Captains use marks on shore with tide tables to estimate their position. ".repeat(20),
            source: None,
            romanization: None,
            thumbnail: None,
        });
    }

    fn full_article(app: &mut Reader) {
        ready(app);
        let panel = app.online.as_mut().unwrap();
        panel.request.full_article = true;
        panel.articles[0].text = EXTRACT.into();
    }

    fn reply_images(app: &mut Reader, root: &std::path::Path, images: Vec<Picture>) {
        let reply = root.join("authored-picture-reply.json");
        std::fs::write(
            &reply,
            serde_json::to_vec(&Ok::<_, String>(images)).unwrap(),
        )
        .unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "read -r line; exec cat \"$1\"", "fixture"])
            .arg(&reply);
        app.online.as_mut().unwrap().media.job =
            Some(ImageJob::fixture(command, Duration::from_secs(3)).unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.online.as_ref().unwrap().media.job.is_some() {
            assert!(
                Instant::now() < deadline,
                "Authored image worker did not reply"
            );
            let _ = app.poll_online(Instant::now());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn wikipedia_media_settings_default_on_gate_the_second_switch_and_survive_reopen() {
        let fixture = Fixture::new();
        let (fonts, mut canvas) = canvases();
        let previous: Languages = serde_json::from_str(r#"{"wikipedia":"en"}"#).unwrap();
        assert!(previous.show_wiki_image && previous.show_wiki_more);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.show(Screen::WikipediaSettings);
        capture_test_screen(&app, &fonts, "READER_WIKI_IMAGE_SETTINGS", false);
        let row0 = Rect::new(40, 154, WIDTH as i32 - 80, 116);
        let row1 = Rect {
            y: row0.y + 116,
            ..row0
        };
        for row in [row0, row1] {
            let screen = frame(&app, &mut canvas, false);
            assert!(canvas.press_at(200.0, (row.y + 45) as f32));
            let pressed = frame(&app, &mut canvas, false);
            assert!(screen.iter().zip(&pressed).enumerate().any(|(i, (a, b))| {
                a != b && row.contains((i as u32 % WIDTH) as f32, (i as u32 / WIDTH) as f32)
            }));
            canvas.begin();
        }
        assert_eq!(tap(&mut app, row1), Change::Partial);
        assert!(!app.lookup_history.online_languages.show_wiki_more);
        assert_eq!(tap(&mut app, row0), Change::Partial);
        assert!(matches!(app.items()[1].action, Action::Heading));
        app.activate(Action::ToggleWikipediaMoreImages).unwrap();
        assert!(!app.lookup_history.online_languages.show_wiki_more);
        drop(app);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        let prefs = &app.lookup_history.online_languages;
        assert!(!prefs.show_wiki_image && !prefs.show_wiki_more);
        app.activate(Action::ToggleWikipediaImage).unwrap();
        assert!(app.lookup_history.online_languages.show_wiki_image);
        assert!(!app.lookup_history.online_languages.show_wiki_more);
    }

    #[test]
    fn wikipedia_media_keeps_text_pages_clicks_through_images_and_restores_the_reading_location() {
        let fixture = Fixture::new();
        let (fonts, mut canvas) = canvases();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let before = app.current_history_location();
        ready(&mut app);
        frame(&app, &mut canvas, false);
        assert_eq!(tap(&mut app, NEXT), Change::Partial);
        let pages = app.online.as_ref().unwrap().page_count();
        reply_images(&mut app, &fixture.root, vec![drawing(false)]);
        assert_eq!(app.online.as_ref().unwrap().images().len(), 1);
        let pixels = frame(&app, &mut canvas, false);
        let subject = Rect::new(THUMB.x + 12, THUMB.y + 12, 130, THUMB.h - 24);
        let ink = (subject.y..subject.bottom())
            .flat_map(|y| {
                (subject.x..subject.right()).map(move |x| y as usize * WIDTH as usize + x as usize)
            })
            .filter(|&offset| pixels[offset] < 65)
            .count();
        assert!(
            ink > 20,
            "The thumbnail did not include its dark sailboat hull"
        );
        capture_test_screen(&app, &fonts, "READER_WIKI_RESULT_IMAGE", false);
        assert_eq!(app.online.as_ref().unwrap().page_count(), pages);
        assert_eq!(app.online.as_ref().unwrap().pages.index.get(), 1);

        assert_eq!(tap(&mut app, THUMB), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(0));
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().pages.index.get(), 1);
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), before);
    }

    #[test]
    fn online_full_article_figures_flow_in_screen_page_order_and_preserve_the_source_anchor() {
        let fixture = Fixture::new();
        let (fonts, mut canvas) = canvases();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        app.open_path(&fixture.book).unwrap();
        let origin = app.current_history_location();
        full_article(&mut app);
        frame(&app, &mut canvas, false);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        let before = app.online.as_ref().unwrap().pages.current().unwrap().source;
        let mut signal = drawing(false);
        signal.name = "Signal.png".into();
        signal.caption = "Signal flag in the gallery.".into();
        reply_images(
            &mut app,
            &fixture.root,
            vec![drawing(false), drawing(true), signal],
        );
        frame(&app, &mut canvas, false);
        {
            let panel = app.online.as_ref().unwrap();
            assert!(
                panel
                    .pages
                    .current()
                    .unwrap()
                    .source
                    .contains(&before.start)
            );
            let frames = panel.pages.frames.borrow();
            assert!(frames.len() > 3);
            assert_eq!(
                frames
                    .iter()
                    .map(|page| &EXTRACT[page.source.clone()])
                    .collect::<String>(),
                EXTRACT
            );
            for (index, page) in frames.iter().enumerate() {
                if index < 3 {
                    let figure = page.figure.unwrap();
                    assert_eq!(figure.picture, index);
                    assert_eq!(figure.photo.y, BODY_RECT.y);
                    assert_eq!(figure.photo.right(), BODY_RECT.right());
                    assert!(figure.beside.right() < figure.photo.x);
                    assert!(figure.below.y > figure.photo.bottom());
                    assert!(page.source.start < figure.split && figure.split < page.source.end);
                } else {
                    assert!(page.figure.is_none());
                }
            }
            assert_eq!(
                panel
                    .images()
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>(),
                ["Harbor.png", "Jetty.png", "Signal.png"]
            );
        }

        while app.input(Input::PageBack) == Change::Partial {}
        let first = app.online.as_ref().unwrap().pages.current().unwrap();
        let photo = first.figure.unwrap().photo;
        let image_pixels = frame(&app, &mut canvas, false);
        let dark_pixels = frame(&app, &mut canvas, true);
        let center = ((photo.y + photo.h / 2) * WIDTH as i32 + photo.x + photo.w / 2) as usize;
        assert_eq!(image_pixels[center], dark_pixels[center]);
        capture_test_screen(&app, &fonts, "READER_WIKI_INLINE_LIGHT", false);
        assert_eq!(tap(&mut app, photo), Change::Partial);
        assert!(app.online.as_ref().unwrap().media.show_caption);
        let caption_pixels = frame(&app, &mut canvas, false);
        assert!(image_pixels.iter().zip(&caption_pixels).enumerate().all(
            |(index, (before, after))| {
                before == after
                    || photo.contains(
                        (index % WIDTH as usize) as f32,
                        (index / WIDTH as usize) as f32,
                    )
            }
        ));
        capture_test_screen(&app, &fonts, "READER_WIKI_INLINE_CAPTION", false);
        assert_eq!(
            app.online.as_ref().unwrap().pages.current().unwrap().source,
            first.source
        );
        assert_eq!(
            app.input(Input::LongTap((photo.x + 60) as f32, (photo.y + 60) as f32)),
            Change::Partial
        );
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(0));
        capture_test_screen(&app, &fonts, "READER_WIKI_FULL_IMAGE", false);
        let next = picture_pager(0, 2, SheetPosition::default())
            .button(PageTurn::Next)
            .unwrap();
        assert_eq!(tap(&mut app, next), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(1));
        capture_test_screen(&app, &fonts, "READER_WIKI_FULL_IMAGE_DARK", true);
        assert_eq!(app.input(Input::PageBack), Change::Partial);
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert!(app.online.as_ref().unwrap().media.show_caption);
        assert_eq!(
            app.online.as_ref().unwrap().pages.current().unwrap().source,
            first.source
        );
        assert_eq!(
            app.input(Input::Swipe {
                dx: 5.0,
                dy: -400.0,
                start_y: 845.0
            }),
            Change::Partial
        );
        let panel = app.online.as_ref().unwrap();
        assert!(!panel.media.show_caption);
        assert_eq!(panel.pages.current().unwrap().figure.unwrap().picture, 1);
        capture_test_screen(&app, &fonts, "READER_WIKI_INLINE_DARK", true);
        assert_eq!(tap(&mut app, SUBMIT), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(1));
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), origin);
    }

    #[test]
    fn wikipedia_media_timeout_and_sleep_leave_already_received_text_available() {
        let fixture = Fixture::new();
        let (fonts, mut canvas) = canvases();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&fixture.book).unwrap();
        full_article(&mut app);
        assert_eq!(
            frame(&app, &mut canvas, false).len(),
            (WIDTH * HEIGHT) as usize
        );
        assert_eq!(app.input(Input::PageForward), Change::Partial);
        let plain_pixels = frame(&app, &mut canvas, false);
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "read -r line; exec sleep 30"]);
        app.online.as_mut().unwrap().media.job =
            Some(ImageJob::fixture(command, Duration::from_millis(20)).unwrap());
        let now = Instant::now() + Duration::from_secs(1);
        assert_eq!(app.poll_online(now), Some(Change::None));
        let panel = app.online.as_ref().unwrap();
        assert!(panel.media.job.is_none());
        assert!(panel.images().is_empty());
        assert_eq!(panel.articles[0].text, EXTRACT);
        assert_eq!(frame(&app, &mut canvas, false), plain_pixels);
        reply_images(&mut app, &fixture.root, Vec::new());
        assert_eq!(frame(&app, &mut canvas, false), plain_pixels);

        let mut command = Command::new("/bin/sh");
        command.args(["-c", "read -r line; exec sleep 30"]);
        app.online.as_mut().unwrap().media.job =
            Some(ImageJob::fixture(command, Duration::from_secs(3)).unwrap());
        app.input(Input::Sleep);
        let panel = app.online.as_ref().unwrap();
        assert!(panel.media.job.is_none());
        assert_eq!(panel.articles[0].text, EXTRACT);
        assert_eq!(frame(&app, &mut canvas, false), plain_pixels);
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "read -r line; exec sleep 30"]);
        app.online.as_mut().unwrap().media.job =
            Some(ImageJob::fixture(command, Duration::from_secs(3)).unwrap());
        assert_eq!(tap(&mut app, CLOSE), Change::Partial);
        assert!(app.online.is_none());
        assert_eq!(
            app.poll_online(Instant::now() + Duration::from_secs(10)),
            None
        );
        assert_eq!(app.screen, Screen::Book);
    }

    #[test]
    fn selection_viewport_wikipedia_figure_captions_and_gallery_share_painted_coordinates() {
        use crate::app::selection_viewport_tests::adopt;
        use crate::session::ReadingViewport;

        let fixture = Fixture::new();
        let (fonts, _) = canvases();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&fixture.book).unwrap();
        let viewport = ReadingViewport::LANDSCAPE;
        let mut canvas = adopt(&mut app, viewport, 2);
        let origin = app.current_history_location();
        full_article(&mut app);
        reply_images(&mut app, &fixture.root, vec![drawing(false), drawing(true)]);
        let picture_frame = frame(&app, &mut canvas, false);
        let position = SheetPosition::new(viewport);
        let figure = app
            .online
            .as_ref()
            .unwrap()
            .pages
            .current()
            .unwrap()
            .figure
            .unwrap();
        let visible = position.rect(figure.photo);
        let (x, y) = (
            (visible.x + visible.w / 2) as f32,
            (visible.y + visible.h / 2) as f32,
        );
        assert_eq!(tap(&mut app, visible), Change::Partial);
        assert!(app.online.as_ref().unwrap().media.show_caption);
        assert!(frame(&app, &mut canvas, false) != picture_frame);
        assert_eq!(app.input(Input::LongTap(x, y)), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(0));
        let next = picture_pager(0, 2, position)
            .button(PageTurn::Next)
            .unwrap();
        assert_eq!(tap(&mut app, next), Change::Partial);
        assert_eq!(app.online.as_ref().unwrap().media.current, Some(1));
        let close = position.rect(CLOSE);
        assert_eq!(tap(&mut app, close), Change::Partial);
        assert!(app.online.as_ref().unwrap().media.current.is_none());
        assert_eq!(tap(&mut app, close), Change::Partial);
        assert_eq!(app.current_history_location(), origin);
    }
}

#[cfg(test)]
#[test]
fn selection_viewport_translate_keeps_request_result_pager_and_return_inside_landscape() {
    use crate::app::selection_popup::SelectionAction;
    use crate::app::selection_viewport_tests::{adopt, capture, key, press, tap};
    use crate::app::tests::Fixture;
    use crate::epub::Book;
    use crate::session::ReadingViewport;
    use primitive_app::App;
    use primitive_ui::Key;

    let fixture = Fixture::new();
    let mut app = Reader::open(
        &fixture.root,
        &fixture.state,
        vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")],
    )
    .unwrap();
    app.controls.dictionary_on_single_word = Some(false);
    app.open_path(&fixture.book).unwrap();
    let source = &app.active.as_ref().unwrap().reading.probe.chapter;
    app.follow(Book::resolve_link(source, "dusk.xhtml#door").unwrap())
        .unwrap();
    let viewport = ReadingViewport::LANDSCAPE;
    let mut canvas = adopt(&mut app, viewport, 2);
    let original = app.current_history_location();
    let reading = &app.active.as_ref().unwrap().reading;
    let start = reading.probe.matches("harbor").into_iter().next().unwrap();
    let mut end = start.clone();
    end.offset += "harbor".len();
    let selected = reading.probe.select_range(&start, &end).unwrap();
    let word = reading.highlight_rects(&selected)[0];
    assert_eq!(
        app.input(Input::LongTap(
            (word.x + word.width / 2) as f32,
            (word.y + word.height / 2) as f32
        )),
        Change::Partial
    );
    assert_eq!(app.selected_passage().unwrap(), "harbor");
    let translate = app.selection_button(SelectionAction::Translate);
    tap(&mut app, translate);
    assert_eq!(app.screen, Screen::OnlineLookup);
    let position = SheetPosition::new(viewport);
    let close = position.rect(CLOSE);
    let language = position.rect(TO);
    assert!(close.x + close.w / 2 > WIDTH as i32);
    press(&app, &mut canvas, close, false);
    assert!(app.online.as_ref().unwrap().job.is_none());
    tap(&mut app, language);
    assert_eq!(app.screen, Screen::OnlineEdit);
    let existing = app.editor.text.len();
    for _ in 0..existing {
        let delete = key(&app, Key::Delete, "Apply");
        tap(&mut app, delete);
    }
    for ch in ['f', 'r'] {
        let letter = key(&app, Key::Char(ch), "Apply");
        tap(&mut app, letter);
    }
    let save = key(&app, Key::Go, "Apply");
    assert!(save.x > WIDTH as i32);
    tap(&mut app, save);
    assert_eq!(app.screen, Screen::OnlineLookup);
    assert_eq!(app.online.as_ref().unwrap().request.to, "fr");

    // Inject a provider response so the controller and renderer run offline.
    app.online.as_mut().unwrap().articles = vec![Article {
        title: "Translation".into(),
        text: "Port. Abri naturel ou aménagé où les navires peuvent accoster. Les cartes nautiques indiquent la profondeur et les repères à suivre pour entrer dans le port. ".repeat(20),
        source: Some("harbor".into()),
        romanization: None,
        thumbnail: None,
    }];
    let next = position.rect(NEXT);
    let (_, height) = viewport.dimensions();
    assert!(next.bottom() < height as i32 && NEXT.bottom() > height as i32);
    press(&app, &mut canvas, next, false);
    capture(&app, &mut canvas, "landscape-translation-result.png", false);
    tap(&mut app, next);
    assert_eq!(app.online.as_ref().unwrap().pages.index.get(), 1);
    tap(&mut app, close);
    assert_eq!(app.screen, Screen::Book);
    assert_eq!(app.current_history_location(), original);
    assert_eq!(app.selected_passage().unwrap(), "harbor");
}
