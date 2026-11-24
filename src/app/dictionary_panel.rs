//! Result, text position and nested lookup history live for the lifetime of one popup.
use super::*;
use std::cell::OnceCell;
use std::ops::Range;

const FONT_SIZE: i32 = 36;
const DEFINITION_TEXT: primitive_ui::TextStyle =
    primitive_ui::TextStyle::new(FONT_SIZE).break_long_words();
const LINE: i32 = 47;
const MAX_DEPTH: usize = 8;
const EXAMPLE_INSET: i32 = 36;
const MARKER_GUTTER: i32 = 112;

/// One job belongs to one popup. Dropping the popup cancels its storage work
/// and disconnects its result channel before another selected word can receive it.
pub(super) struct SpellingLookup {
    receiver: std::sync::mpsc::Receiver<io::Result<Vec<dictionary::LookupEntry>>>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) deadline: Instant,
}

impl Drop for SpellingLookup {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl SpellingLookup {
    pub(super) fn start(
        work: impl FnOnce(&std::sync::atomic::AtomicBool) -> io::Result<Vec<dictionary::LookupEntry>>
        + Send
        + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::Builder::new()
            .name("dictionary-spelling".into())
            .spawn(move || {
                let _ = sender.send(work(&flag));
            })?;
        Ok(Self {
            receiver,
            cancelled,
            deadline: Instant::now() + Duration::from_millis(150),
        })
    }
}

#[derive(Default)]
pub(super) struct Navigation {
    origin: Option<(Screen, usize)>,
    origin_editor: Option<Editor>,
    review_word: Option<String>,
    trail: Vec<Previous>,
    view: View,
    online_return: Option<usize>,
}
struct Previous {
    word: String,
    definitions: Vec<dictionary::LookupEntry>,
    index: usize,
    expanded: bool,
    suggestions_pending: bool,
    view: View,
}
#[derive(Default)]
struct View {
    line: usize,
    start_at_end: bool,
    layout: OnceCell<Vec<TextLine>>,
    find: Option<Find>,
    editing: bool,
    query_editing: bool,
    save_highlight: bool,
}
impl View {
    fn first_line(&self, visible: usize) -> usize {
        let last = self
            .layout
            .get()
            .map_or(0, Vec::len)
            .saturating_sub(visible);
        if self.start_at_end {
            last
        } else {
            self.line.min(last)
        }
    }
}
struct Find {
    query: String,
    hits: Vec<Range<usize>>,
    index: usize,
}
struct TextLine {
    range: Range<usize>,
    soft_wrap: bool,
    indent: i32,
    end_inset: i32,
    rtl: Option<bool>,
    marker: Option<dictionary::article::Marker>,
    placed: OnceCell<PlacedLine>,
    words: OnceCell<Vec<(Range<usize>, i32, i32)>>,
    links: OnceCell<Vec<(usize, i32, i32)>>,
}

#[derive(Debug)]
struct PlacedRun {
    range: Range<usize>,
    x: i32,
    width: i32,
    rtl: bool,
}

struct PlacedLine {
    runs: Vec<PlacedRun>,
    justified: bool,
    rtl: Option<bool>,
}

fn body(popup: Rect) -> Rect {
    Rect::new(popup.x + 32, popup.y + 108, popup.w - 84, popup.h - 273)
}
pub(super) fn action_row(popup: Rect) -> [Rect; 5] {
    let save = definition_vocabulary(popup);
    std::array::from_fn(|index| {
        Rect::new(save.x + index as i32 * (save.w + 8), save.y, save.w, save.h)
    })
}
fn action_button(
    canvas: &mut Canvas,
    rect: Rect,
    label: &str,
    icon: primitive_ui::Icon,
    selected: bool,
) {
    canvas.button(rect, "", selected, false);
    let filled = selected != canvas.is_control_pressed(rect);
    let size = 30;
    canvas.icon(
        Rect::new(rect.x + 12, rect.y + (rect.h - size) / 2, size, size),
        icon,
        if filled { 255 } else { 0 },
    );
    let label_size = if label.chars().count() > 9 { 24 } else { 28 };
    canvas.text(
        Rect::new(rect.x + 50, rect.y + 2, rect.w - 56, rect.h - 4),
        label,
        primitive_ui::TextStyle::new(label_size)
            .center()
            .ink(if filled { 255 } else { 0 }),
    );
}

fn layout(canvas: &mut Canvas, text: &str, width: i32) -> Vec<TextLine> {
    canvas
        .text_pages(Rect::new(0, 0, width, LINE), text, DEFINITION_TEXT)
        .into_iter()
        .map(|range| TextLine {
            soft_wrap: range.end < text.len()
                && !text[range.clone()].ends_with(['\n', '\r', '\u{0085}', '\u{2028}', '\u{2029}']),
            range,
            indent: 0,
            end_inset: 0,
            rtl: None,
            marker: None,
            placed: OnceCell::new(),
            words: OnceCell::new(),
            links: OnceCell::new(),
        })
        .collect()
}

fn article_layout(
    canvas: &mut Canvas,
    result: &dictionary::LookupEntry,
    width: i32,
) -> Vec<TextLine> {
    let Some(article) = result
        .article
        .as_ref()
        .filter(|article| !article.paragraphs.is_empty())
    else {
        return layout(canvas, &result.text, width);
    };
    let mut prior = 0;
    let mut paragraphs = article.paragraphs.iter().peekable();
    let mut rows = Vec::new();
    for paragraph in result.text.split_inclusive('\n') {
        let start = prior;
        prior += paragraph.len();
        let paragraph = paragraph.trim_end_matches('\n');
        let style = paragraphs.next_if(|style| style.range.start == start);
        let (indent, end_inset) = style.map_or((0, 0), |style| {
            let nested = (i32::from(style.indent) * EXAMPLE_INSET).min(width / 6);
            let gutter = if style.list_item { MARKER_GUTTER } else { 0 };
            ((nested + gutter).min(width / 3), nested)
        });
        let rtl = style.map(|style| paragraph_rtl(paragraph, style));
        for (index, mut line) in layout(canvas, paragraph, width - indent - end_inset)
            .into_iter()
            .enumerate()
        {
            line.range.start += start;
            line.range.end += start;
            line.indent = indent;
            line.end_inset = end_inset;
            line.rtl = rtl;
            if index == 0 {
                line.marker = style.and_then(|style| style.marker);
            }
            rows.push(line);
        }
    }
    rows
}

fn paragraph_rtl(text: &str, style: &dictionary::article::Paragraph) -> bool {
    use unicode_bidi::BidiClass::{AL, L, R};

    match style.direction {
        Some(parley::BaseDirection::Ltr) => return false,
        Some(parley::BaseDirection::Rtl) => return true,
        _ => {}
    }
    if let Some(direction) = text
        .chars()
        .map(unicode_bidi::bidi_class)
        .find(|class| matches!(class, L | R | AL))
    {
        return direction != L;
    }
    // When there is no strong character, use the declared language's script
    // to position a line of numbers or symbols. Explicit dir="auto" still
    // uses Unicode's default direction for neutral text.
    style.direction.is_none()
        && style
            .language
            .is_some_and(|language| match language.script() {
                Some(script) => {
                    matches!(script, "Arab" | "Hebr" | "Thaa" | "Syrc" | "Nkoo" | "Adlm")
                }
                None => matches!(
                    language.language(),
                    "ar" | "ara"
                        | "fa"
                        | "fas"
                        | "he"
                        | "heb"
                        | "ur"
                        | "urd"
                        | "ps"
                        | "pus"
                        | "yi"
                        | "yid"
                        | "dv"
                        | "div"
                        | "sd"
                        | "snd"
                        | "ug"
                        | "uig"
                ),
            })
}

impl TextLine {
    fn bounds(&self, row: Rect) -> Rect {
        Rect::new(
            row.x
                + if self.rtl == Some(true) {
                    self.end_inset
                } else {
                    self.indent
                },
            row.y,
            row.w - self.indent - self.end_inset,
            row.h,
        )
    }

    fn draw_marker(&self, canvas: &mut Canvas, rect: Rect) {
        let text = match self.marker {
            Some(dictionary::article::Marker::Number(number)) => format!("{number}."),
            Some(dictionary::article::Marker::Bullet) => "•".to_owned(),
            None => return,
        };
        let width = self.indent.min(MARKER_GUTTER) - 14;
        let (x, style) = if self.rtl == Some(true) {
            (rect.right() + 14, primitive_ui::TextStyle::new(30))
        } else {
            (
                rect.x - 14 - width,
                primitive_ui::TextStyle::new(30).right(),
            )
        };
        canvas.text(Rect::new(x, rect.y + 2, width, LINE), &text, style);
    }
}

fn stretchable_space(ch: char) -> bool {
    matches!(ch, ' ' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{205f}' | '\u{3000}')
}

impl PlacedLine {
    fn new(canvas: &mut Canvas, line: &str, width: i32, justify: bool, rtl: Option<bool>) -> Self {
        let level = rtl.map(|rtl| {
            if rtl {
                unicode_bidi::Level::rtl()
            } else {
                unicode_bidi::Level::ltr()
            }
        });
        let bidi = unicode_bidi::BidiInfo::new(line, level);
        let Some(paragraph) = bidi.paragraphs.first() else {
            return Self {
                runs: Vec::new(),
                justified: false,
                rtl,
            };
        };
        let (levels, ranges) = bidi.visual_runs(paragraph, paragraph.range.clone());
        if justify && let Some(runs) = Self::spread(canvas, line, width, &levels, &ranges) {
            return Self {
                runs,
                justified: true,
                rtl,
            };
        }
        let mut x = if rtl == Some(true) {
            (width - canvas.measure(line, FONT_SIZE)).max(0)
        } else {
            0
        };
        let runs = ranges
            .into_iter()
            .map(|range| {
                let width = canvas.measure(&line[range.clone()], FONT_SIZE);
                let run = PlacedRun {
                    rtl: levels[range.start].is_rtl(),
                    range,
                    x,
                    width,
                };
                x += width;
                run
            })
            .collect();
        Self {
            runs,
            justified: false,
            rtl,
        }
    }

    fn spread(
        canvas: &mut Canvas,
        line: &str,
        width: i32,
        levels: &[unicode_bidi::Level],
        ranges: &[Range<usize>],
    ) -> Option<Vec<PlacedRun>> {
        let mut pieces = Vec::new();
        for run in ranges {
            let first = pieces.len();
            let rtl = levels[run.start].is_rtl();
            let mut start = run.start;
            for (offset, ch) in line[run.clone()]
                .char_indices()
                .filter(|(_, ch)| stretchable_space(*ch))
            {
                let offset = run.start + offset;
                if start < offset {
                    pieces.push((start..offset, rtl, false));
                }
                start = offset + ch.len_utf8();
                pieces.push((offset..start, rtl, true));
            }
            if start < run.end {
                pieces.push((start..run.end, rtl, false));
            }
            if rtl {
                pieces[first..].reverse();
            }
        }
        // Stretch separated words. Leave both paragraph boundaries, indentation,
        // and non-breaking spaces intact. RTL gaps follow the visual run order.
        let gaps = pieces.iter().filter(|(_, _, space)| *space).count() as i32;
        if gaps == 0 || pieces.first()?.2 || pieces.last()?.2 {
            return None;
        }
        let mut measured = Vec::with_capacity(pieces.len());
        let mut used = 0;
        for (range, rtl, space) in pieces {
            let advance = if space {
                0
            } else {
                canvas.measure(&line[range.clone()], FONT_SIZE)
            };
            used += advance;
            measured.push((range, rtl, space, advance));
        }
        if used + gaps >= width {
            return None;
        }
        let free = width - used;
        let mut seen = 0;
        let mut x = 0;
        let mut runs = Vec::new();
        for (range, rtl, space, advance) in measured {
            if space {
                x += (seen + 1) * free / gaps - seen * free / gaps;
                seen += 1;
            } else {
                runs.push(PlacedRun {
                    range,
                    rtl,
                    x,
                    width: advance,
                });
                x += advance;
            }
        }
        Some(runs)
    }

    // The same visual runs place glyphs, find emphasis and nested lookup hit targets.
    fn range_boxes(
        &self,
        canvas: &mut Canvas,
        line: &str,
        row_start: usize,
        spans: &[Range<usize>],
    ) -> Vec<(Range<usize>, i32, i32)> {
        let mut boxes = Vec::new();
        for PlacedRun {
            range,
            x,
            width,
            rtl,
        } in &self.runs
        {
            let edge = |index, canvas: &mut Canvas| {
                if index == range.start {
                    0
                } else if index == range.end {
                    *width
                } else {
                    canvas.measure(&line[range.start..index], FONT_SIZE)
                }
            };
            for span in spans
                .iter()
                .filter(|s| s.end > range.start && s.start < range.end)
            {
                let a = edge(span.start.max(range.start), canvas);
                let b = edge(span.end.min(range.end), canvas);
                let (left, right) = if *rtl { (width - b, width - a) } else { (a, b) };
                boxes.push((
                    row_start + span.start..row_start + span.end,
                    x + left,
                    x + right,
                ));
            }
        }
        boxes
    }

    fn draw(&self, canvas: &mut Canvas, rect: Rect, line: &str) {
        if self.justified {
            for run in &self.runs {
                let bounds = Rect::new(rect.x + run.x, rect.y, rect.w - run.x, rect.h);
                canvas.text(bounds, &line[run.range.clone()], DEFINITION_TEXT);
            }
        } else if let Some(rtl) = self.rtl {
            // Canvas auto-detects direction. A nonprinting mark pins the
            // paragraph base direction across wrapped rows without changing
            // the dictionary text or the hit-test byte positions.
            let mark = if rtl { '\u{200f}' } else { '\u{200e}' };
            let style = if rtl {
                DEFINITION_TEXT.right()
            } else {
                DEFINITION_TEXT
            };
            canvas.text(rect, &format!("{mark}{line}"), style);
        } else {
            canvas.text(rect, line, DEFINITION_TEXT);
        }
    }
}

pub(super) fn definition_vocabulary(popup: Rect) -> Rect {
    let width = (popup.right() - 38 - 94 - 4 * 8) / 5;
    Rect::new(94, popup.bottom() - 152, width, 58)
}

const RESULTS_PER_PAGE: usize = 5;

fn definition_results_button(popup: Rect) -> Rect {
    Rect::new(WIDTH as i32 - 434, popup.y + 18, 172, 64)
}

fn definition_edit_button(popup: Rect, has_results_button: bool) -> Rect {
    let before = if has_results_button {
        definition_results_button(popup).x
    } else {
        WIDTH as i32 - 254
    };
    Rect::new(before - 108, popup.y, 100, 100)
}

fn definition_edit_glyph(button: Rect) -> Rect {
    Rect::new(button.x + 18, button.y + 17, 64, 66)
}

fn definition_result_row(popup: Rect, row: usize) -> Rect {
    Rect::new(
        popup.x + 30,
        popup.y + 100 + row as i32 * 104,
        popup.w - 60,
        98,
    )
}

const HEADWORD_ONLY: &str = "Headword only";

impl Reader {
    fn draw_definition_results(&self, canvas: &mut Canvas, popup: Rect, page: usize) {
        let first = page * RESULTS_PER_PAGE;
        for (row, result) in self
            .definitions
            .iter()
            .skip(first)
            .take(RESULTS_PER_PAGE)
            .enumerate()
        {
            let index = first + row;
            let rect = definition_result_row(popup, row);
            let selected = index == self.list_page;
            let filled = selected != canvas.control_pressed(rect, false);
            canvas.panel(rect, 16, 3, if filled { 0 } else { 255 });
            let ink = if filled { 255 } else { 0 };
            canvas.text(
                Rect::new(rect.x + 22, rect.y + 4, rect.w - 44, 48),
                &format!("{} · {}", result.word, result.dictionary),
                BODY.bold().ink(ink),
            );
            let snippet: String = result
                .text
                .lines()
                .next()
                .unwrap_or(HEADWORD_ONLY)
                .chars()
                .take(120)
                .collect();
            canvas.text(
                Rect::new(rect.x + 22, rect.y + 53, rect.w - 44, 39),
                &snippet,
                CAPTION.ink(if filled { 255 } else { 85 }),
            );
        }
        let [previous, _, next] = definition_actions(popup);
        if page > 0 {
            canvas.button(previous, "Previous", false, false);
        }
        if first + RESULTS_PER_PAGE < self.definitions.len() {
            canvas.button(next, "Next", false, false);
        }
    }

    fn definition_results_input(&mut self, input: Input, popup: Rect, page: usize) -> Change {
        let [previous, _, next] = definition_actions(popup);
        let direction = match input {
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Tap(x, y) if previous.contains(x, y) => -1,
            Input::Tap(x, y) if next.contains(x, y) => 1,
            Input::Tap(x, y) => {
                let first = page * RESULTS_PER_PAGE;
                if let Some(row) = (0..RESULTS_PER_PAGE)
                    .find(|row| definition_result_row(popup, *row).contains(x, y))
                    && first + row < self.definitions.len()
                {
                    self.select_definition(first + row);
                }
                self.definition_results_page = None;
                return Change::Partial;
            }
            _ => return Change::None,
        };
        if let Some(page) = page
            .checked_add_signed(direction)
            .filter(|page| page * RESULTS_PER_PAGE < self.definitions.len())
        {
            self.definition_results_page = Some(page);
            return Change::Partial;
        }
        Change::None
    }

    fn vocabulary_available(&self) -> bool {
        !self.definition_word.is_empty()
    }

    fn draw_vocabulary_action(&self, canvas: &mut Canvas, rect: Rect) {
        if self.vocabulary_available() {
            let saved = self
                .lookup_history
                .vocabulary
                .contains(&self.definition_word);
            let label = if saved { "Remove word" } else { "Save word" };
            action_button(
                canvas,
                rect,
                label,
                bar_icons::action_icon(&Action::AddBookmark),
                saved,
            );
        }
    }

    pub(super) fn poll_spelling_lookup(&mut self, now: Instant) -> Option<Change> {
        use std::sync::mpsc::TryRecvError;
        let job = self.spelling_lookup.as_mut()?;
        if now < job.deadline {
            return None;
        }
        match job.receiver.try_recv() {
            Err(TryRecvError::Empty) => {
                job.deadline = now + Duration::from_millis(150);
                return None;
            }
            Ok(Ok(definitions)) => {
                self.definitions = definitions;
                self.dictionary_navigation.view = View::default();
            }
            Ok(Err(error)) => self.notice = Some(error.to_string()),
            Err(TryRecvError::Disconnected) => {
                self.notice = Some("Dictionary search ended early".into())
            }
        }
        self.spelling_lookup = None;
        Some(Change::Partial)
    }

    pub(super) fn look_up(&mut self, word: &str) -> Result<()> {
        self.look_up_entry(word, None)
    }

    fn look_up_entry(&mut self, word: &str, source: Option<&Path>) -> Result<()> {
        let nested = self.screen == Screen::Definition;
        let word = word.trim();
        if !lookup_history::valid(word) {
            return Err("Enter a word or a short phrase".into());
        }
        if nested && self.dictionary_navigation.trail.len() >= MAX_DEPTH {
            return Err("Go back one lookup to open another definition".into());
        }
        let results = with_storage(|| match source {
            Some(source) => self.dictionaries.lookup_reference(source, word),
            None => self.lookup_sources(word),
        })?;
        if source.is_some() && results.is_empty() {
            return Err("Reference not found in this dictionary".into());
        }
        let entry = self.vocabulary_entry(word);
        let reviewing = if self.screen == Screen::VocabularyWord {
            self.vocabulary_view.word.as_deref() == Some(word)
        } else {
            nested && self.dictionary_navigation.review_word.as_deref() == Some(word)
        };
        let repeated = self.lookup_history.review.automatic
            && !reviewing
            && self.lookup_history.review.words.contains_key(word);
        self.update_lookup_history(|history| {
            history.lookup(word);
            if history.review.automatic && !reviewing && !repeated {
                if !history.vocabulary.iter().any(|saved| saved == word) {
                    history.vocabulary.insert(0, word.to_owned());
                }
                history
                    .review
                    .remember(word, entry.clone(), super::vocabulary::now());
            }
        })?;
        if nested {
            self.dictionary_navigation.trail.push(Previous {
                word: std::mem::take(&mut self.definition_word),
                definitions: std::mem::take(&mut self.definitions),
                index: self.list_page,
                expanded: self.definition_expanded,
                suggestions_pending: self.spelling_lookup.is_some(),
                view: std::mem::take(&mut self.dictionary_navigation.view),
            });
        } else {
            self.dictionary_navigation = Navigation {
                origin: Some((self.screen, self.list_page)),
                origin_editor: (self.screen == Screen::DictionaryEntry)
                    .then(|| self.editor.clone()),
                review_word: (self.screen == Screen::VocabularyWord)
                    .then(|| self.vocabulary_view.word.clone())
                    .flatten(),
                ..Default::default()
            };
            self.definition_expanded = self.dictionary_settings.large_window;
        }
        self.definition_word = word.into();
        self.definitions = results;
        self.definition_results_page = None;
        self.show(Screen::Definition);
        if repeated {
            self.vocabulary_view.repeated = Some((word.to_owned(), entry));
        }
        if source.is_some() {
            self.spelling_lookup = None;
        } else {
            self.start_spelling_lookup(word);
        }
        Ok(())
    }

    pub(super) fn restore_dictionary_online_return(&mut self) {
        if let Some(index) = self.dictionary_navigation.online_return.take() {
            self.list_page = index;
        }
    }
    fn return_from_definition(&mut self, close_all: bool) {
        self.spelling_lookup = None;
        self.notice = None;
        self.definition_results_page = None;
        let save_highlight = self.dictionary_navigation.view.save_highlight
            || close_all
                && self
                    .dictionary_navigation
                    .trail
                    .iter()
                    .any(|parent| parent.view.save_highlight);
        let highlight_error = if save_highlight {
            let saved = if self.pdf_selection.is_some() {
                self.save_pdf_highlight_in_place(None)
            } else {
                self.save_highlight_in_place(None)
            };
            if saved.is_ok() {
                for parent in &mut self.dictionary_navigation.trail {
                    parent.view.save_highlight = false;
                }
            }
            saved.err()
        } else {
            None
        };
        if let Some(previous) = (!close_all)
            .then(|| self.dictionary_navigation.trail.pop())
            .flatten()
        {
            self.definition_word = previous.word;
            self.definitions = previous.definitions;
            self.list_page = previous.index;
            self.definition_expanded = previous.expanded;
            self.dictionary_navigation.view = previous.view;
            if previous.suggestions_pending {
                self.start_spelling_lookup(&self.definition_word.clone());
            }
        } else {
            let back = self.dictionary_navigation.origin.take().unwrap_or((
                if self.fixed.is_some() {
                    Screen::Fixed
                } else {
                    Screen::Book
                },
                0,
            ));
            let editor = self.dictionary_navigation.origin_editor.take();
            if back.0 == Screen::VocabularyWord {
                self.vocabulary_view.word = self.dictionary_navigation.review_word.take();
            }
            self.dictionary_navigation = Navigation::default();
            self.show(back.0);
            self.list_page = back.1;
            if let Some(editor) = editor {
                self.editor = editor;
            }
        }
        if let Some(error) = highlight_error {
            self.notice = Some(error.to_string());
        }
    }
    fn select_definition(&mut self, index: usize) {
        self.list_page = index;
        self.dictionary_navigation.view = View::default();
        self.notice = None;
    }
    fn result_step(&mut self, step: isize, wrap: bool) -> Change {
        let count = self.definitions.len();
        if count < 2 {
            return Change::None;
        }
        let index = self.list_page as isize + step;
        if !wrap && !(0..count as isize).contains(&index) {
            return Change::None;
        }
        self.select_definition(index.rem_euclid(count as isize) as usize);
        Change::Partial
    }
    fn scroll_definition(&mut self, step: isize, across_results: bool) -> Change {
        let rows = self.definition_layout().2;
        let view = &mut self.dictionary_navigation.view;
        let total = view.layout.get().map_or(0, Vec::len);
        let current = view.first_line(rows);
        let next = current
            .saturating_add_signed(step * rows as isize)
            .min(total.saturating_sub(rows));
        if next != current {
            view.line = next;
            view.start_at_end = false;
            Change::Partial
        } else if across_results {
            let change = self.result_step(step, true);
            if change == Change::Partial && step < 0 {
                self.dictionary_navigation.view.start_at_end = true;
            }
            change
        } else {
            Change::None
        }
    }
    fn dictionary_row_at(&self, popup: Rect, x: f32, y: f32) -> Option<(&TextLine, i32)> {
        let rect = body(popup);
        if !rect.contains(x, y) {
            return None;
        }
        let view = &self.dictionary_navigation.view;
        let rows = (rect.h / LINE) as usize;
        let row = (y as i32 - rect.y) as usize / LINE as usize;
        if row >= rows {
            return None;
        }
        let n = view.first_line(rows) + row;
        let line = view.layout.get()?.get(n)?;
        let pos = x as i32 - line.bounds(rect).x;
        Some((line, pos))
    }

    fn dictionary_word_at(&self, popup: Rect, x: f32, y: f32) -> Option<String> {
        let (line, pos) = self.dictionary_row_at(popup, x, y)?;
        let (bytes, _, _) = line
            .words
            .get()?
            .iter()
            .find(|(_, left, right)| (*left..*right).contains(&pos))?;
        let text = &self.definitions.get(self.list_page)?.text;
        Some(text[crate::selection::word_range(text, bytes.start)?].to_owned())
    }

    fn dictionary_reference_at(
        &self,
        popup: Rect,
        x: f32,
        y: f32,
    ) -> Option<(&Path, &dictionary::article::Link)> {
        let (line, pos) = self.dictionary_row_at(popup, x, y)?;
        let (index, _, _) = line
            .links
            .get()?
            .iter()
            .find(|(_, start, end)| (*start..*end).contains(&pos))?;
        let current = self.definitions.get(self.list_page)?;
        Some((
            &current.source,
            current.article.as_ref()?.links.get(*index)?,
        ))
    }
    fn find_step(&mut self, step: isize) -> Change {
        let rows = self.definition_layout().2;
        let view = &mut self.dictionary_navigation.view;
        let Some(find) = &mut view.find else {
            return Change::None;
        };
        let Some(next) = find
            .index
            .checked_add_signed(step)
            .filter(|&i| i < find.hits.len())
        else {
            return Change::None;
        };
        find.index = next;
        view.start_at_end = false;
        if let Some(lines) = view.layout.get() {
            view.line = lines
                .partition_point(|line| line.range.end <= find.hits[next].start)
                .min(lines.len().saturating_sub(rows));
        }
        Change::Partial
    }
    fn begin_find(&mut self) {
        self.editor = Editor::new(96);
        if let Some(find) = &self.dictionary_navigation.view.find {
            self.editor.text = find.query.chars().collect();
            self.editor.caret = self.editor.text.len();
        }
        self.notice = None;
        self.dictionary_navigation.view.editing = true;
    }
    fn begin_edit_query(&mut self, current_result: bool) {
        let query = if current_result {
            self.definitions
                .get(self.list_page)
                .map_or(&self.definition_word, |result| &result.word)
        } else {
            &self.definition_word
        };
        self.editor = Editor::new(96);
        self.editor.text = query.chars().take(96).collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
        self.dictionary_navigation.view.query_editing = true;
    }
    fn edit_query_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.dictionary_navigation.view.query_editing = false;
            self.notice = None;
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let Some(key) = keyboard_key(
            &Layout::new(WIDTH, HEIGHT, 4, 5),
            &self.editor,
            "Look up",
            x,
            y,
        ) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let query = self.editor.string();
            self.dictionary_navigation.view.query_editing = false;
            match self.look_up(&query) {
                Ok(()) => Change::Partial,
                Err(error) => {
                    self.dictionary_navigation.view.query_editing = true;
                    self.report(error)
                }
            }
        } else {
            Change::Partial
        }
    }
    fn finish_find(&mut self, query: &str) -> Change {
        let Some(result) = self.definitions.get(self.list_page) else {
            return Change::None;
        };
        let hits = match crate::search::definition_hits(&result.text, query) {
            Ok(h) => h,
            Err(e) => return self.report(e),
        };
        if hits.is_empty() {
            return self.report("No matches in this definition");
        }
        self.notice = None;
        let view = &mut self.dictionary_navigation.view;
        view.editing = false;
        view.find = Some(Find {
            query: query.into(),
            hits,
            index: 0,
        });
        self.find_step(0)
    }
    fn edit_find_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if primitive_ui::back_button(WIDTH as i32).contains(x,y))
        {
            self.dictionary_navigation.view.editing = false;
            self.notice = None;
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let Some(key) = keyboard_key(
            &Layout::new(WIDTH, HEIGHT, 4, 5),
            &self.editor,
            "Find",
            x,
            y,
        ) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            self.finish_find(&self.editor.string())
        } else {
            Change::Partial
        }
    }

    pub(super) fn definition_input(&mut self, input: Input) -> Change {
        if self.dictionary_navigation.view.query_editing {
            return self.edit_query_input(input);
        }
        if self.dictionary_navigation.view.editing {
            return self.edit_find_input(input);
        }
        let (popup, back, _) = self.definition_layout();
        if matches!(input, Input::LongTap(x, y) if back.contains(x, y)) {
            self.return_from_definition(true);
            return Change::Partial;
        }
        let edit = definition_edit_button(popup, self.definitions.len() > 1);
        if !self.definition_word.is_empty() {
            match input {
                Input::LongTap(x, y) if edit.contains(x, y) => {
                    self.begin_edit_query(true);
                    return Change::Partial;
                }
                Input::Tap(x, y) if edit.contains(x, y) => {
                    self.begin_edit_query(false);
                    return Change::Partial;
                }
                _ => {}
            }
        }
        if let Some(page) = self.definition_results_page {
            if matches!(input,Input::Tap(x,y) if back.contains(x,y) || !popup.contains(x,y) || definition_results_button(popup).contains(x,y))
            {
                self.definition_results_page = None;
                return Change::Partial;
            }
            return self.definition_results_input(input, popup, page);
        }
        if let Input::LongTap(x, y) = input
            && let Some(word) = self.dictionary_word_at(popup, x, y)
        {
            return match self.look_up(&word) {
                Ok(()) => Change::Partial,
                Err(e) => self.report(e),
            };
        }
        if let Input::Swipe { dx, dy, .. } = input {
            if dy.abs() > dx.abs() {
                return self.scroll_definition(if dy < 0.0 { 1 } else { -1 }, false);
            }
            return self.result_step(if dx < 0.0 { 1 } else { -1 }, true);
        }
        let [prev, expand, next] = definition_actions(popup);
        let found = self.dictionary_navigation.view.find.is_some();
        match input {
            Input::PageForward if found => self.find_step(1),
            Input::PageBack if found => self.find_step(-1),
            Input::PageForward => self.scroll_definition(1, true),
            Input::PageBack => self.scroll_definition(-1, true),
            Input::Tap(x, y) => {
                if self.definition_expanded
                    && self.back_collapses_definition()
                    && back.contains(x, y)
                {
                    self.definition_expanded = false;
                    return Change::Partial;
                }
                if back.contains(x, y) || !popup.contains(x, y) {
                    self.return_from_definition(false);
                    return Change::Partial;
                }
                if self.definitions.len() > 1 && definition_results_button(popup).contains(x, y) {
                    self.definition_results_page = Some(self.list_page / RESULTS_PER_PAGE);
                    return Change::Partial;
                }
                if prev.contains(x, y) {
                    return if found {
                        self.find_step(-1)
                    } else {
                        self.result_step(-1, false)
                    };
                }
                if next.contains(x, y) {
                    return if found {
                        self.find_step(1)
                    } else {
                        self.result_step(1, false)
                    };
                }
                if expand.contains(x, y) && !self.definitions.is_empty() {
                    if found {
                        self.dictionary_navigation.view.find = None;
                    } else {
                        self.definition_expanded = !self.definition_expanded;
                    }
                    return Change::Partial;
                }
                let [save, find, wiki, search_book, highlight] = action_row(popup);
                if self.vocabulary_available() && save.contains(x, y) {
                    let word = self.definition_word.clone();
                    if self.lookup_history.review.words.contains_key(&word) {
                        self.vocabulary_view.word = Some(word);
                        return match self
                            .vocabulary_command(super::vocabulary::Command::ConfirmRemove)
                        {
                            Ok(()) => Change::Partial,
                            Err(error) => self.report(error),
                        };
                    }
                    let entry = self.vocabulary_entry(&word);
                    return match self.update_lookup_history(|h| {
                        let added = h.save_word(&word);
                        if added {
                            h.review.remember(&word, entry, super::vocabulary::now());
                        }
                        added
                    }) {
                        Ok(_) => Change::Partial,
                        Err(e) => self.report(e),
                    };
                }
                if find.contains(x, y) && !self.definitions.is_empty() {
                    self.begin_find();
                    return Change::Partial;
                }
                if wiki.contains(x, y) {
                    let word = self
                        .definitions
                        .get(self.list_page)
                        .map_or(&self.definition_word, |result| &result.word)
                        .clone();
                    self.dictionary_navigation.online_return = Some(self.list_page);
                    return match self.prepare_online_text(true, &word) {
                        Ok(()) => Change::Partial,
                        Err(e) => self.report(e),
                    };
                }
                if search_book.contains(x, y) && self.selected_passage().is_ok() {
                    return match self.find_selection(SelectionSearchOrigin::Dictionary) {
                        Ok(()) => Change::Partial,
                        Err(error) => self.report(error),
                    };
                }
                if highlight.contains(x, y) && self.selected_passage().is_ok() {
                    self.dictionary_navigation.view.save_highlight =
                        !self.dictionary_navigation.view.save_highlight;
                    return Change::Partial;
                }
                if body(popup).contains(x, y) {
                    if let Some((source, reference)) = self.dictionary_reference_at(popup, x, y) {
                        let Some(target) = reference.target.clone() else {
                            return Change::None;
                        };
                        let source = source.to_owned();
                        return match self.look_up_entry(&target, Some(&source)) {
                            Ok(()) => Change::Partial,
                            Err(error) => self.report(error),
                        };
                    }
                    let step = if x < (popup.x + popup.w / 2) as f32 {
                        -1
                    } else {
                        1
                    };
                    return if found {
                        self.find_step(step)
                    } else {
                        self.scroll_definition(step, true)
                    };
                }
                Change::None
            }
            _ => Change::None,
        }
    }

    pub(super) fn definition_over_book(&self) -> bool {
        self.screen == Screen::Definition && !self.definition_expanded
    }
    pub(super) fn dictionary_editor_active(&self) -> bool {
        self.dictionary_navigation.view.query_editing || self.dictionary_navigation.view.editing
    }

    fn back_collapses_definition(&self) -> bool {
        !self.dictionary_settings.large_window && self.dictionary_navigation.trail.is_empty()
    }

    pub(super) fn definition_layout(&self) -> (Rect, Rect, usize) {
        let height = if let Some(index) = self.definition_results_page {
            let shown = self
                .definitions
                .len()
                .saturating_sub(index * RESULTS_PER_PAGE)
                .min(RESULTS_PER_PAGE);
            let paging = if self.definitions.len() > RESULTS_PER_PAGE {
                82
            } else {
                0
            };
            116 + shown as i32 * 104 + paging
        } else {
            let lines = self.definitions.get(self.list_page).map_or(2, |result| {
                result
                    .text
                    .lines()
                    .take(8)
                    .map(|line| line.chars().take(360).count().max(1).div_ceil(45))
                    .sum::<usize>()
                    .clamp(3, 8)
            });
            273 + LINE * lines as i32
        };
        let popup = if self.definition_expanded {
            Rect::new(64, 24, WIDTH as i32 - 128, HEIGHT as i32 - 70)
        } else {
            definition_bounds(height, self.selection_bounds())
        };
        let back = Rect::new(WIDTH as i32 - 254, popup.y + 16, 168, 66);
        (popup, back, (body(popup).h / LINE) as usize)
    }

    pub(super) fn draw_definition_popup(&self, canvas: &mut Canvas) {
        let view = &self.dictionary_navigation.view;
        if view.query_editing {
            canvas.fill(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), 255);
            header(canvas, "Edit lookup word", "Back");
            canvas.single_line_field(
                Rect::new(40, 160, 1184, 124),
                &self.editor,
                "Word or phrase",
                false,
            );
            if let Some(message) = &self.notice {
                canvas.text(Rect::new(46, 320, 1172, 90), message, CAPTION);
            }
            canvas.keyboard(self.editor.layer, "Look up", None);
            return;
        }
        if view.editing {
            canvas.fill(Rect::new(0, 0, WIDTH as i32, HEIGHT as i32), 255);
            header(canvas, "Find in definition", "Back");
            canvas.single_line_field(Rect::new(40, 160, 1184, 124), &self.editor, "Find", false);
            if let Some(message) = &self.notice {
                canvas.text(Rect::new(46, 320, 1172, 90), message, CAPTION);
            }
            canvas.keyboard(self.editor.layer, "Find", None);
            return;
        }
        let (popup, back, rows) = self.definition_layout();
        let [prev, expand, next] = definition_actions(popup);
        let [save, find, wiki, search_book, highlight] = action_row(popup);
        let edit = definition_edit_button(popup, self.definitions.len() > 1);
        canvas.popup(popup);
        let choose = definition_results_button(popup);
        let end = edit.x - 12;
        let title = self
            .definitions
            .get(self.list_page)
            .map(|result| format!("{} · {}", result.word, result.dictionary));
        canvas.icon(
            Rect::new(popup.x + 28, popup.y + 31, 40, 40),
            bar_icons::BOOK,
            0,
        );
        canvas.text(
            Rect::new(popup.x + 84, popup.y + 20, end - popup.x - 84, 64),
            title.as_deref().unwrap_or(&self.definition_word),
            BODY.bold(),
        );
        let edit_glyph = definition_edit_glyph(edit);
        canvas.button(edit_glyph, "", false, false);
        canvas.icon(
            Rect::new(edit_glyph.x + 17, edit_glyph.y + 13, 34, 34),
            bar_icons::EDIT,
            0,
        );
        canvas.button(
            back,
            if self.definition_expanded && self.back_collapses_definition()
                || self.definition_results_page.is_some()
                || !self.dictionary_navigation.trail.is_empty()
            {
                "Back"
            } else {
                "Done"
            },
            false,
            false,
        );
        if self.definitions.len() > 1 {
            canvas.button(
                choose,
                "Results",
                self.definition_results_page.is_some(),
                false,
            );
        }
        if let Some(page) = self.definition_results_page {
            self.draw_definition_results(canvas, popup, page);
            return;
        }
        let rect = body(popup);
        if let Some(result) = self.definitions.get(self.list_page) {
            let text = &result.text;
            if text.is_empty() {
                canvas.text(rect, HEADWORD_ONLY, CAPTION);
            }
            let lines = view
                .layout
                .get_or_init(|| article_layout(canvas, result, rect.w));
            let current = view.find.as_ref().map(|f| &f.hits[f.index]);
            let first = view.first_line(rows);
            for (row, line) in lines.iter().skip(first).take(rows).enumerate() {
                let y = rect.y + row as i32 * LINE;
                let line_rect = line.bounds(Rect::new(rect.x, y, rect.w, LINE));
                line.draw_marker(canvas, line_rect);
                let content = text[line.range.clone()].trim_end();
                let placement = line.placed.get_or_init(|| {
                    PlacedLine::new(
                        canvas,
                        content,
                        line_rect.w,
                        self.dictionary_settings.justify_text && line.soft_wrap,
                        line.rtl,
                    )
                });
                line.words.get_or_init(|| {
                    let words: Vec<_> = crate::selection::word_ranges(content).collect();
                    placement.range_boxes(canvas, content, line.range.start, &words)
                });
                if let Some(article) = &result.article {
                    let links = line.links.get_or_init(|| {
                        let first = article
                            .links
                            .partition_point(|link| link.range.end <= line.range.start);
                        let mut boxes = Vec::new();
                        for (index, link) in article
                            .links
                            .iter()
                            .enumerate()
                            .skip(first)
                            .take_while(|(_, link)| link.range.start < line.range.end)
                        {
                            let span = link.range.start.saturating_sub(line.range.start)
                                ..link.range.end.min(line.range.end) - line.range.start;
                            boxes.extend(
                                placement
                                    .range_boxes(canvas, content, line.range.start, &[span])
                                    .into_iter()
                                    .map(|(_, left, right)| {
                                        (index, left.max(0), right.min(line_rect.w))
                                    })
                                    .filter(|(_, left, right)| left < right),
                            );
                        }
                        boxes
                    });
                    for &(index, left, right) in links {
                        if article.links[index].target.is_some() {
                            canvas.fill(
                                Rect::new(line_rect.x + left, y + LINE - 3, right - left, 2),
                                0,
                            );
                        }
                    }
                }
                if let Some(hit) = current
                    && line.range.start < hit.end
                    && hit.start < line.range.end
                {
                    let part = hit.start.max(line.range.start) - line.range.start
                        ..hit.end.min(line.range.end) - line.range.start;
                    for (_, left, right) in placement.range_boxes(
                        canvas,
                        content,
                        line.range.start,
                        std::slice::from_ref(&part),
                    ) {
                        canvas.fill(
                            Rect::new(line_rect.x + left, y, (right - left).max(2), LINE),
                            205,
                        );
                    }
                }
                placement.draw(canvas, line_rect, content);
            }
            if lines.len() > rows {
                let height = (rect.h * rows as i32 / lines.len() as i32).max(12);
                let at = first as i32 * (rect.h - height) / (lines.len() - rows) as i32;
                canvas.fill(Rect::new(rect.right() + 10, rect.y, 4, rect.h), 210);
                canvas.fill(Rect::new(rect.right() + 8, rect.y + at, 8, height), 50);
            }
        } else {
            let message = if self.spelling_lookup.is_some() {
                "Searching for similar words"
            } else {
                "No matches in installed dictionaries"
            };
            canvas.text(rect, message, BODY.lines(2));
        }
        self.draw_vocabulary_action(canvas, save);
        if !self.definitions.is_empty() {
            action_button(
                canvas,
                find,
                "Find",
                bar_icons::group_icon(TopGroup::Search),
                false,
            );
        }
        if !self.definition_word.is_empty() {
            action_button(canvas, wiki, "Wikipedia", bar_icons::LINK, false);
        }
        if self.selected_passage().is_ok() {
            action_button(
                canvas,
                search_book,
                "Search book",
                bar_icons::group_icon(TopGroup::Search),
                false,
            );
            let save_highlight = self.dictionary_navigation.view.save_highlight;
            action_button(
                canvas,
                highlight,
                if save_highlight {
                    "Unhighlight"
                } else {
                    "Highlight"
                },
                bar_icons::HIGHLIGHT,
                save_highlight,
            );
        }
        let counter = if let Some(f) = &view.find {
            (f.index, f.hits.len())
        } else {
            (self.list_page, self.definitions.len())
        };
        if counter.0 > 0 {
            canvas.button(prev, "Previous", false, false);
        }
        if counter.0 + 1 < counter.1 {
            canvas.button(next, "Next", false, false);
        }
        if counter.1 > 0 {
            canvas.button(
                expand,
                if view.find.is_some() {
                    "End find"
                } else if self.definition_expanded {
                    "Collapse"
                } else {
                    "Expand"
                },
                false,
                false,
            );
        }
        let status = match &view.find {
            Some(find) => format!(
                "Match {} of {} · Result {} of {}",
                find.index + 1,
                find.hits.len(),
                self.list_page + 1,
                self.definitions.len()
            ),
            None if !self.definitions.is_empty() => format!(
                "Result {} of {}",
                self.list_page + 1,
                self.definitions.len()
            ),
            None => String::new(),
        };
        let show_close_hint = self.notice.is_none() && !self.dictionary_navigation.trail.is_empty();
        let status_width = rect.w - if show_close_hint { 354 } else { 0 };
        canvas.text(
            Rect::new(rect.x, popup.y + 78, status_width, 34),
            self.notice.as_deref().unwrap_or(&status),
            CAPTION,
        );
        if show_close_hint {
            canvas.text(
                Rect::new(
                    rect.x + status_width,
                    popup.y + 78,
                    rect.w - status_width,
                    34,
                ),
                "Hold Back to close all",
                CAPTION.right(),
            );
        }
    }
}

#[cfg(test)]
mod tests;
