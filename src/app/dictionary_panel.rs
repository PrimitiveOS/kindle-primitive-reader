//! Result, text position and nested lookup history live for the lifetime of one popup.
use super::*;
use std::cell::OnceCell;
use std::ops::Range;

mod preset_lookup;
mod result_menus;
use preset_lookup::PresetPicker;
#[cfg(test)]
use result_menus::definition_result_row;
use result_menus::{Menu, Menus};
pub(super) mod buttons;
pub(super) mod font_size;
use buttons::{Button, Grid};

const FONT_SIZE: i32 = 36;
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
    // Dictionary history uses a row ordinal; other sources keep their list page.
    origin: Option<(Screen, usize)>,
    origin_editor: Option<Editor>,
    review_word: Option<String>,
    trail: Vec<Previous>,
    view: View,
    online_return: Option<usize>,
    preset_picker: Option<PresetPicker>,
    font_size_draft: Option<i32>,
    button_draft: Option<buttons::Draft>,
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
    action_page: usize,
    line: usize,
    start_at_end: bool,
    layout: OnceCell<Vec<TextLine>>,
    find: Option<Find>,
    editing: bool,
    query_editing: Option<QueryEdit>,
    save_highlight: bool,
    no_sources_for_book: bool,
    preset: Option<Vec<PathBuf>>,
    results: Menus,
}
impl View {
    fn reset_result(&mut self) {
        self.line = 0;
        self.start_at_end = false;
        self.layout.take();
        self.find = None;
    }

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryEdit {
    Word,
    FuzzyRetry,
}

impl QueryEdit {
    fn action(self) -> &'static str {
        match self {
            Self::Word => "Look up",
            Self::FuzzyRetry => "Fuzzy search",
        }
    }
}

enum LookupMode<'a> {
    Configured,
    FuzzyOnce,
    Reference(&'a Path),
    Preset {
        members: Vec<PathBuf>,
        replace_miss: bool,
    },
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
    font: i32,
}

#[derive(Clone, Copy)]
struct DefinitionTypography(i32);

impl DefinitionTypography {
    fn scale(self, base: i32) -> i32 {
        (base * self.0 + font_size::DEFAULT / 2) / font_size::DEFAULT
    }

    fn line(self) -> i32 {
        self.scale(LINE)
    }

    fn font(self) -> i32 {
        self.scale(FONT_SIZE)
    }
}

const DEFINITION_TOP: i32 = 126;
const STANDARD_FOOTER: i32 = 165;

fn definition_metadata_bounds(popup: Rect) -> Rect {
    Rect::new(popup.x + 32, popup.y + 84, popup.w - 84, 38)
}

#[cfg(test)]
fn body(popup: Rect) -> Rect {
    Rect::new(
        popup.x + 32,
        popup.y + DEFINITION_TOP,
        popup.w - 84,
        popup.h - DEFINITION_TOP - STANDARD_FOOTER,
    )
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
    let ink = if filled { 255 } else { 0 };
    if rect.h >= 100 {
        let label_size = 32;
        let icon_size = 36;
        let gap = 12;
        let width = canvas
            .measure(label, label_size)
            .min(rect.w - icon_size - gap - 24)
            + 4;
        let x = rect.x + (rect.w - width - icon_size - gap) / 2;
        canvas.icon(Rect::new(x, rect.y + 32, icon_size, icon_size), icon, ink);
        canvas.text(
            Rect::new(x + icon_size + gap, rect.y + 27, width, 46),
            label,
            primitive_ui::TextStyle::new(label_size).ink(ink),
        );
        return;
    }
    let size = 30;
    canvas.icon(
        Rect::new(rect.x + 12, rect.y + (rect.h - size) / 2, size, size),
        icon,
        ink,
    );
    let label_size = if label.chars().count() > 9 { 24 } else { 28 };
    canvas.text(
        Rect::new(rect.x + 50, rect.y + 2, rect.w - 56, rect.h - 4),
        label,
        primitive_ui::TextStyle::new(label_size).center().ink(ink),
    );
}

fn popup_button_label(button: Button) -> &'static str {
    match button {
        Button::Previous => "Previous",
        Button::Next => "Next",
        _ => button.label(),
    }
}

fn draw_action_button(
    canvas: &mut Canvas,
    button: Button,
    rect: Rect,
    label: &str,
    selected: bool,
) {
    let icon = match button {
        Button::SaveWord => bar_icons::action_icon(&Action::AddBookmark),
        Button::Find | Button::SearchBook => bar_icons::SEARCH,
        Button::Wikipedia => bar_icons::LINK,
        Button::Highlight => bar_icons::HIGHLIGHT,
        Button::Translate => bar_icons::action_icon(&Action::TranslatePage),
        Button::Previous | Button::Next | Button::Close | Button::Expand => {
            canvas.button(rect, label, selected, false);
            return;
        }
    };
    action_button(canvas, rect, label, icon, selected);
}

fn draw_preview_button(canvas: &mut Canvas, button: Button, rect: Rect) {
    draw_action_button(canvas, button, rect, popup_button_label(button), false);
}

fn layout(
    canvas: &mut Canvas,
    text: &str,
    width: i32,
    typography: DefinitionTypography,
) -> Vec<TextLine> {
    canvas
        .text_pages(
            Rect::new(0, 0, width, typography.line()),
            text,
            primitive_ui::TextStyle::new(typography.font()).break_long_words(),
        )
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
    typography: DefinitionTypography,
) -> Vec<TextLine> {
    let Some(article) = result
        .article
        .as_ref()
        .filter(|article| !article.paragraphs.is_empty())
    else {
        return layout(canvas, &result.text, width, typography);
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
            let nested = (i32::from(style.indent) * typography.scale(EXAMPLE_INSET)).min(width / 6);
            let gutter = if style.list_item {
                typography.scale(MARKER_GUTTER)
            } else {
                0
            };
            ((nested + gutter).min(width / 3), nested)
        });
        let rtl = style.map(|style| paragraph_rtl(paragraph, style));
        for (index, mut line) in layout(canvas, paragraph, width - indent - end_inset, typography)
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

    fn draw_marker(&self, canvas: &mut Canvas, rect: Rect, typography: DefinitionTypography) {
        let text = match self.marker {
            Some(dictionary::article::Marker::Number(number)) => format!("{number}."),
            Some(dictionary::article::Marker::Bullet) => "•".to_owned(),
            None => return,
        };
        let spacing = typography.scale(14);
        let width = self.indent.min(typography.scale(MARKER_GUTTER)) - spacing;
        let (x, style) = if self.rtl == Some(true) {
            (
                rect.right() + spacing,
                primitive_ui::TextStyle::new(typography.scale(30)),
            )
        } else {
            (
                rect.x - spacing - width,
                primitive_ui::TextStyle::new(typography.scale(30)).right(),
            )
        };
        canvas.text(
            Rect::new(x, rect.y + 2, width, typography.line()),
            &text,
            style,
        );
    }
}

fn stretchable_space(ch: char) -> bool {
    matches!(ch, ' ' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{205f}' | '\u{3000}')
}

impl PlacedLine {
    fn new(
        canvas: &mut Canvas,
        line: &str,
        width: i32,
        justify: bool,
        rtl: Option<bool>,
        font: i32,
    ) -> Self {
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
                font,
            };
        };
        let (levels, ranges) = bidi.visual_runs(paragraph, paragraph.range.clone());
        if justify && let Some(runs) = Self::spread(canvas, line, width, &levels, &ranges, font) {
            return Self {
                runs,
                justified: true,
                rtl,
                font,
            };
        }
        let mut x = if rtl == Some(true) {
            (width - canvas.measure(line, font)).max(0)
        } else {
            0
        };
        let runs = ranges
            .into_iter()
            .map(|range| {
                let width = canvas.measure(&line[range.clone()], font);
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
            font,
        }
    }

    fn spread(
        canvas: &mut Canvas,
        line: &str,
        width: i32,
        levels: &[unicode_bidi::Level],
        ranges: &[Range<usize>],
        font: i32,
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
                canvas.measure(&line[range.clone()], font)
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
                    canvas.measure(&line[range.start..index], self.font)
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
        let style = primitive_ui::TextStyle::new(self.font).break_long_words();
        if self.justified {
            for run in &self.runs {
                let bounds = Rect::new(rect.x + run.x, rect.y, rect.w - run.x, rect.h);
                canvas.text(bounds, &line[run.range.clone()], style);
            }
        } else if let Some(rtl) = self.rtl {
            // Canvas auto-detects direction. A nonprinting mark pins the
            // paragraph base direction across wrapped rows without changing
            // the dictionary text or the hit-test byte positions.
            let mark = if rtl { '\u{200f}' } else { '\u{200e}' };
            let style = if rtl { style.right() } else { style };
            canvas.text(rect, &format!("{mark}{line}"), style);
        } else {
            canvas.text(rect, line, style);
        }
    }
}

pub(super) fn definition_vocabulary(popup: Rect) -> Rect {
    let width = (popup.w - 68 - 4 * 8) / 5;
    Rect::new(popup.x + 30, popup.bottom() - 152, width, 58)
}

fn definition_results_button(popup: Rect) -> Rect {
    Rect::new(popup.right() - 370, popup.y + 18, 172, 64)
}

fn definition_edit_button(popup: Rect, has_results_button: bool) -> Rect {
    let before = if has_results_button {
        definition_results_button(popup).x
    } else {
        popup.right() - 190
    };
    Rect::new(before - 108, popup.y, 100, 100)
}

fn definition_edit_glyph(button: Rect) -> Rect {
    Rect::new(button.x + 18, button.y + 17, 64, 66)
}

const HEADWORD_ONLY: &str = "Headword only";

impl Reader {
    fn vocabulary_available(&self) -> bool {
        !self.definition_word.is_empty()
    }

    fn definition_counter(&self) -> (usize, usize) {
        self.dictionary_navigation
            .view
            .find
            .as_ref()
            .map_or((self.list_page, self.definitions.len()), |find| {
                (find.index, find.hits.len())
            })
    }

    fn dictionary_button_enabled(&self, button: Button) -> bool {
        let (index, count) = self.definition_counter();
        match button {
            Button::SaveWord | Button::Wikipedia => self.vocabulary_available(),
            Button::Translate => self.vocabulary_available() && self.open_book_id().is_some(),
            Button::Find | Button::Expand => !self.definitions.is_empty(),
            Button::SearchBook | Button::Highlight => self.selected_passage().is_ok(),
            Button::Previous => index > 0,
            Button::Next => index + 1 < count,
            Button::Close => true,
        }
    }

    fn draw_dictionary_button(&self, canvas: &mut Canvas, button: Button, rect: Rect) {
        let view = &self.dictionary_navigation.view;
        let found = view.find.is_some();
        let (label, selected) = match button {
            Button::SaveWord
                if self
                    .lookup_history
                    .vocabulary
                    .contains(&self.definition_word) =>
            {
                ("Remove word", true)
            }
            Button::Highlight if view.save_highlight => ("Unhighlight", true),
            Button::Find if found && self.dictionary_settings.popup_buttons.is_some() => {
                ("End find", true)
            }
            Button::Expand if found && self.dictionary_settings.popup_buttons.is_none() => {
                ("End find", false)
            }
            Button::Expand if self.definition_expanded => ("Collapse", false),
            _ => (popup_button_label(button), false),
        };
        // This label comes from the fixed Button enum/state, not a dictionary.
        let label = self
            .controls
            .interface_language
            .dictionary_popup_text(label);
        if self.dictionary_button_enabled(button) {
            draw_action_button(canvas, button, rect, label, selected);
        } else if self.dictionary_settings.popup_buttons.is_some() {
            canvas.panel(rect, 16, 2, 255);
            canvas.text(
                Rect::new(rect.x, rect.y + (rect.h - 44) / 2, rect.w, 44),
                label,
                CAPTION.center(),
            );
        }
    }

    fn use_dictionary_button(&mut self, button: Button) -> Change {
        if !self.dictionary_button_enabled(button) {
            return Change::None;
        }
        let found = self.dictionary_navigation.view.find.is_some();
        match button {
            Button::Previous | Button::Next => {
                let step = if button == Button::Previous { -1 } else { 1 };
                if found {
                    self.find_step(step)
                } else {
                    self.result_step(step, false)
                }
            }
            Button::Expand => {
                if found && self.dictionary_settings.popup_buttons.is_none() {
                    self.dictionary_navigation.view.find = None;
                } else {
                    self.definition_expanded = !self.definition_expanded;
                }
                Change::Partial
            }
            Button::Find => {
                if found && self.dictionary_settings.popup_buttons.is_some() {
                    self.dictionary_navigation.view.find = None;
                } else {
                    self.begin_find();
                }
                Change::Partial
            }
            Button::Wikipedia | Button::Translate => {
                let wiki = button == Button::Wikipedia;
                let topic = if wiki {
                    self.definitions
                        .get(self.list_page)
                        .map_or(&self.definition_word, |result| &result.word)
                } else {
                    &self.definition_word
                }
                .clone();
                self.dictionary_navigation.online_return = Some(self.list_page);
                match self.prepare_online_text(wiki, &topic) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                }
            }
            Button::SaveWord => self.save_vocabulary_from_definition(),
            Button::SearchBook => match self.find_selection(SelectionSearchOrigin::Dictionary) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            },
            Button::Highlight => {
                self.dictionary_navigation.view.save_highlight =
                    !self.dictionary_navigation.view.save_highlight;
                Change::Partial
            }
            Button::Close => {
                self.return_from_definition(false);
                Change::Partial
            }
        }
    }

    fn save_vocabulary_from_definition(&mut self) -> Change {
        let word = self.definition_word.clone();
        if self.lookup_history.review.words.contains_key(&word) {
            self.vocabulary_view.word = Some(word);
            return match self.vocabulary_command(super::vocabulary::Command::ConfirmRemove) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            };
        }
        let entry = self.vocabulary_entry(&word);
        match self.update_lookup_history(|history| {
            let added = history.save_word(&word);
            if added {
                history
                    .review
                    .remember(&word, entry, super::vocabulary::now());
            }
            added
        }) {
            Ok(_) => Change::Partial,
            Err(error) => self.report(error),
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
                self.dictionary_navigation.view.reset_result();
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
        self.look_up_entry(word, LookupMode::Configured)
    }

    fn look_up_entry(&mut self, word: &str, mode: LookupMode<'_>) -> Result<()> {
        let nested = self.screen == Screen::Definition;
        let retry = matches!(mode, LookupMode::FuzzyOnce);
        let replace = retry
            || matches!(
                mode,
                LookupMode::Preset {
                    replace_miss: true,
                    ..
                }
            );
        let preset = match &mode {
            LookupMode::Preset { members, .. } => Some(members.clone()),
            _ if nested => self.dictionary_navigation.view.preset.clone(),
            _ => None,
        };
        let word = word.trim();
        if !lookup_history::valid(word) {
            return Err("Enter a word or a short phrase".into());
        }
        if nested && !replace && self.dictionary_navigation.trail.len() >= MAX_DEPTH {
            return Err("Go back one lookup to open another definition".into());
        }
        let (definitions, searched) = with_storage(|| match mode {
            LookupMode::Reference(source) => self
                .dictionaries
                .lookup_reference(source, word)
                .map(|entries| (entries, false)),
            _ => self
                .lookup_with_dictionary_preset(word, preset.as_deref())
                .map(|report| (report.entries, report.searched)),
        })?;
        if matches!(mode, LookupMode::Reference(_)) && definitions.is_empty() {
            return Err("Reference not found in this dictionary".into());
        }
        let mut review = None;
        if !replace || self.definition_word != word {
            let entry = self.vocabulary_entry(word);
            let reviewing = if self.screen == Screen::VocabularyWord {
                self.vocabulary_view.word.as_deref() == Some(word)
            } else {
                nested && self.dictionary_navigation.review_word.as_deref() == Some(word)
            };
            let repeated = self.lookup_history.review.automatic
                && !reviewing
                && self.lookup_history.review.words.contains_key(word);
            let record = !self.dictionary_settings.disable_dictionary_history;
            let remember = self.lookup_history.review.automatic && !reviewing && !repeated;
            if record || remember {
                self.update_lookup_history(|history| {
                    if record {
                        history.lookup(word, &entry.book, crate::mark_time::now());
                    }
                    if remember {
                        if !history.vocabulary.iter().any(|saved| saved == word) {
                            history.vocabulary.insert(0, word.to_owned());
                        }
                        history
                            .review
                            .remember(word, entry.clone(), super::vocabulary::now());
                    }
                })?;
            }
            if repeated {
                review = Some((word.to_owned(), entry));
            }
        }
        if replace {
            self.dictionary_navigation.view = View {
                save_highlight: self.dictionary_navigation.view.save_highlight,
                ..View::default()
            };
        } else if nested {
            self.dictionary_navigation.trail.push(Previous {
                word: std::mem::take(&mut self.definition_word),
                definitions: std::mem::take(&mut self.definitions),
                index: self.list_page,
                expanded: self.definition_expanded,
                suggestions_pending: self.spelling_lookup.is_some(),
                view: std::mem::take(&mut self.dictionary_navigation.view),
            });
        } else {
            let previous = if self.screen == Screen::DictionaryHistory {
                self.reading_list_ordinal()
            } else {
                self.list_page
            };
            self.dictionary_navigation = Navigation {
                origin: Some((self.screen, previous)),
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
        let try_similar = searched && definitions.is_empty() && dictionary::spelling_eligible(word);
        self.dictionary_navigation.view.no_sources_for_book =
            !searched && !self.book_dictionary_exclusions().is_empty();
        self.dictionary_navigation.view.preset = preset;
        self.definitions = definitions;
        self.spelling_lookup = None;
        self.show(Screen::Definition);
        if let Some(review) = review {
            self.vocabulary_view.repeated = Some(review);
        }
        if try_similar {
            if retry || self.fuzzy_search_enabled() {
                self.start_spelling_lookup(word, self.dictionary_navigation.view.preset.clone());
            } else {
                self.set_query_editor(word.to_owned(), QueryEdit::FuzzyRetry);
            }
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
                self.start_spelling_lookup(
                    &self.definition_word.clone(),
                    self.dictionary_navigation.view.preset.clone(),
                );
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
            if back.0 == Screen::DictionaryHistory {
                self.restore_reading_list_ordinal(back.1);
            } else {
                self.list_page = back.1;
            }
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
        self.dictionary_navigation.view.reset_result();
        self.notice = None;
    }
    fn jump_to_definition(&mut self, index: usize) -> Change {
        if index == self.list_page || index >= self.definitions.len() {
            return Change::None;
        }
        self.select_definition(index);
        Change::Partial
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
        self.jump_to_definition(index.rem_euclid(count as isize) as usize)
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
        let rect = self.definition_body(popup);
        if !rect.contains(x, y) {
            return None;
        }
        let view = &self.dictionary_navigation.view;
        let line_height = self.definition_typography().line();
        let rows = (rect.h / line_height) as usize;
        let row = ((y as i32 - rect.y) / line_height) as usize;
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
        self.set_query_editor(query.to_owned(), QueryEdit::Word);
    }

    fn set_query_editor(&mut self, query: String, mode: QueryEdit) {
        self.editor = Editor::new(96);
        self.editor.text = query.chars().take(96).collect();
        self.editor.caret = self.editor.text.len();
        self.notice = None;
        self.dictionary_navigation.view.query_editing = Some(mode);
    }

    fn fuzzy_retry_layout(&self) -> (Rect, Rect) {
        let (width, height) = self.reading_viewport().dimensions();
        let keyboard = Layout::new(width, height, 4, 5).keyboard_area();
        let panel_width = (width as i32 - 80).min(1024);
        let panel_height = if self.dictionary_settings.presets.is_empty() {
            288
        } else {
            338
        };
        let top = 24;
        let bottom = (keyboard.y - panel_height - 24).max(top);
        let y = match self.selection_bounds() {
            Some(selection) if selection.y + selection.h / 2 < keyboard.y / 2 => bottom,
            Some(_) => top,
            None => (top + bottom) / 2,
        };
        let panel = Rect::new(
            (width as i32 - panel_width) / 2,
            y,
            panel_width,
            panel_height,
        );
        let cancel = Rect::new(panel.right() - 194, panel.y + 22, 166, 72);
        (panel, cancel)
    }

    fn edit_query_input(&mut self, input: Input, mode: QueryEdit) -> Change {
        let (width, height) = self.reading_viewport().dimensions();
        let back = match mode {
            QueryEdit::Word => self.reading_editor_back(),
            QueryEdit::FuzzyRetry => self.fuzzy_retry_layout().1,
        };
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x,y) if back.contains(x,y))
        {
            if mode == QueryEdit::FuzzyRetry {
                self.return_from_definition(false);
            } else {
                self.dictionary_navigation.view.query_editing = None;
                self.notice = None;
            }
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        if let Some(change) = self.open_dictionary_preset_picker(x, y) {
            return change;
        }
        let Some(key) = keyboard_key(
            &Layout::new(width, height, 4, 5),
            &self.editor,
            mode.action(),
            x,
            y,
        ) else {
            return Change::None;
        };
        if matches!(self.editor.press(key), Press::Go) {
            let query = self.editor.string();
            self.dictionary_navigation.view.query_editing = None;
            let lookup = match mode {
                QueryEdit::Word => LookupMode::Configured,
                QueryEdit::FuzzyRetry => LookupMode::FuzzyOnce,
            };
            match self.look_up_entry(&query, lookup) {
                Ok(()) => Change::Partial,
                Err(error) => {
                    self.dictionary_navigation.view.query_editing = Some(mode);
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
        let (width, height) = self.reading_viewport().dimensions();
        if matches!(input, Input::PageBack)
            || matches!(input,Input::Tap(x,y) if self.reading_editor_back().contains(x,y))
        {
            self.dictionary_navigation.view.editing = false;
            self.notice = None;
            return Change::Partial;
        }
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let Some(key) = keyboard_key(
            &Layout::new(width, height, 4, 5),
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
        if let Some(mode) = self.dictionary_navigation.view.query_editing {
            return self.edit_query_input(input, mode);
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
        let choose = definition_results_button(popup);
        if let Some(menu) = self.dictionary_navigation.view.results.active {
            if matches!(input, Input::Tap(x, y) if back.contains(x, y) || !popup.contains(x, y) || choose.contains(x, y))
            {
                self.dictionary_navigation.view.results.dismiss();
                return Change::Partial;
            }
            return self.definition_results_input(input, popup, menu);
        }
        if self.definitions.len() > 1
            && matches!(input, Input::LongTap(x, y) if choose.contains(x, y))
        {
            self.open_definition_results(Menu::Sources);
            return Change::Partial;
        }
        let grid = self.definition_button_grid(popup);
        let found = self.dictionary_navigation.view.find.is_some();
        if let Input::LongTap(x, y) = input {
            match grid.at(x, y) {
                Some(Button::Close) => {
                    self.return_from_definition(true);
                    return Change::Partial;
                }
                Some(Button::Previous) if !found => return self.jump_to_definition(0),
                Some(Button::Next) if !found => {
                    return self.jump_to_definition(self.definitions.len().saturating_sub(1));
                }
                _ => {}
            }
            if let Some(word) = self.dictionary_word_at(popup, x, y) {
                return match self.look_up(&word) {
                    Ok(()) => Change::Partial,
                    Err(e) => self.report(e),
                };
            }
        }
        if let Input::Swipe { dx, dy, .. } = input {
            if dy.abs() > dx.abs() {
                return self.scroll_definition(if dy < 0.0 { 1 } else { -1 }, false);
            }
            return self.result_step(if dx < 0.0 { 1 } else { -1 }, true);
        }
        match input {
            Input::PageForward if found => self.find_step(1),
            Input::PageBack if found => self.find_step(-1),
            Input::PageForward => self.scroll_definition(1, true),
            Input::PageBack => self.scroll_definition(-1, true),
            Input::Tap(x, y) => {
                if found && back.contains(x, y) {
                    self.dictionary_navigation.view.find = None;
                    return Change::Partial;
                }
                if back.contains(x, y) || !popup.contains(x, y) {
                    self.return_from_definition(false);
                    return Change::Partial;
                }
                if self.definitions.len() > 1 && choose.contains(x, y) {
                    self.open_definition_results(Menu::Flat);
                    return Change::Partial;
                }
                if let Some(page) =
                    grid.page_after_tap(x, y, self.dictionary_navigation.view.action_page)
                {
                    self.dictionary_navigation.view.action_page = page;
                    return Change::Partial;
                }
                if let Some(change) = self.open_dictionary_preset_picker(x, y) {
                    return change;
                }
                if let Some(button) = grid.at(x, y) {
                    return self.use_dictionary_button(button);
                }
                if self.definition_body(popup).contains(x, y) {
                    if let Some((source, reference)) = self.dictionary_reference_at(popup, x, y) {
                        let Some(target) = reference.target.clone() else {
                            return Change::None;
                        };
                        let source = source.to_owned();
                        return match self.look_up_entry(&target, LookupMode::Reference(&source)) {
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
        self.screen == Screen::Definition
            && (!self.definition_expanded
                || self.dictionary_navigation.view.query_editing == Some(QueryEdit::FuzzyRetry))
    }
    pub(super) fn dictionary_editor_fullscreen(&self) -> bool {
        self.dictionary_navigation.view.query_editing == Some(QueryEdit::Word)
            || self.dictionary_navigation.view.editing
    }

    fn definition_typography(&self) -> DefinitionTypography {
        DefinitionTypography(self.dictionary_settings.definition_font_size)
    }

    fn definition_footer_height(&self) -> i32 {
        self.dictionary_settings
            .popup_buttons
            .as_ref()
            .map_or(STANDARD_FOOTER, buttons::Settings::footer_height)
    }

    pub(super) fn definition_body(&self, popup: Rect) -> Rect {
        Rect::new(
            popup.x + 32,
            popup.y + DEFINITION_TOP,
            popup.w - 84,
            popup.h - DEFINITION_TOP - self.definition_footer_height(),
        )
    }

    fn definition_button_grid(&self, popup: Rect) -> Grid {
        self.dictionary_settings.popup_buttons.as_ref().map_or_else(
            || Grid::standard(popup),
            |config| Grid::configured(popup, config, self.dictionary_navigation.view.action_page),
        )
    }

    pub(super) fn definition_layout(&self) -> (Rect, Rect, usize) {
        let viewport = self.reading_viewport();
        let (screen_width, screen_height) = viewport.dimensions();
        let typography = self.definition_typography();
        let line_height = typography.line();
        let results = &self.dictionary_navigation.view.results;
        let height = if let Some(menu) = results.active {
            let page = results.page(menu).unwrap_or(0);
            let notice_height = if self.notice.is_some() { 36 } else { 0 };
            result_menus::height(&self.definitions, menu, page) + notice_height
        } else {
            let empty_lines = if self.can_retry_with_preset() { 4 } else { 2 };
            let capacity = (45 * font_size::DEFAULT / typography.0) as usize;
            let lines = self
                .definitions
                .get(self.list_page)
                .map_or(empty_lines, |result| {
                    result
                        .text
                        .lines()
                        .take(8)
                        .map(|line| {
                            line.chars()
                                .take(capacity * 8)
                                .count()
                                .max(1)
                                .div_ceil(capacity)
                        })
                        .sum::<usize>()
                        .clamp(3, 8)
                });
            // The miss message and retry control keep their own readable size.
            let minimum_body = if self.can_retry_with_preset() {
                188
            } else if self.definitions.is_empty() {
                94
            } else {
                0
            };
            let body_height = (line_height * lines as i32).max(minimum_body);
            (DEFINITION_TOP + self.definition_footer_height() + body_height)
                .min(screen_height as i32 - 146)
        };
        // Keep the column width stable across viewport changes. Nested results
        // retain cached line breaks and text positions within that column.
        let popup = if self.definition_expanded {
            definition_column(screen_width, 24, screen_height as i32 - 70)
        } else {
            definition_bounds(height, self.selection_bounds(), viewport)
        };
        let back = Rect::new(popup.right() - 190, popup.y + 16, 168, 66);
        (
            popup,
            back,
            (self.definition_body(popup).h.max(0) / line_height) as usize,
        )
    }

    pub(super) fn draw_definition_popup(&self, canvas: &mut Canvas) {
        let view = &self.dictionary_navigation.view;
        let bounds = canvas.bounds();
        if view.query_editing == Some(QueryEdit::FuzzyRetry) {
            let (panel, cancel) = self.fuzzy_retry_layout();
            canvas.popup(panel);
            canvas.text(
                Rect::new(panel.x + 30, panel.y + 20, panel.w - 244, 76),
                "No results found",
                BODY.bold(),
            );
            canvas.button(cancel, "Cancel", false, false);
            canvas.single_line_field(
                Rect::new(panel.x + 30, panel.y + 112, panel.w - 60, 92),
                &self.editor,
                "Word or phrase",
                false,
            );
            if self.dictionary_settings.presets.is_empty() {
                canvas.text(
                    Rect::new(panel.x + 30, panel.y + 216, panel.w - 60, 54),
                    self.notice
                        .as_deref()
                        .unwrap_or("Try similar spellings for this lookup"),
                    CAPTION,
                );
            } else {
                self.draw_lookup_preset_button(canvas);
                if let Some(notice) = &self.notice {
                    canvas.text(
                        Rect::new(panel.x + 30, panel.y + 219, panel.w - 470, 100),
                        notice,
                        CAPTION.lines(2),
                    );
                }
            }
            // The document bitmap is composited after the vector controls.
            // Register the keyboard band as an occluder like the retry sheet.
            if !self.dictionary_preset_picker_open() {
                let band = Layout::new(bounds.w as u32, bounds.h as u32, 4, 5).keyboard_area();
                canvas.popup(Rect::new(band.x - 40, band.y, band.w + 80, band.h));
                canvas.keyboard(self.editor.layer, QueryEdit::FuzzyRetry.action(), None);
            }
            return;
        }
        if view.query_editing == Some(QueryEdit::Word) {
            canvas.fill(bounds, 255);
            self.navigation_layout().header(canvas, "Edit lookup word");
            canvas.single_line_field(
                Rect::new(40, 160, bounds.w - 80, 124),
                &self.editor,
                "Word or phrase",
                false,
            );
            if let Some(message) = &self.notice {
                canvas.text(Rect::new(46, 320, bounds.w - 92, 90), message, CAPTION);
            }
            self.draw_lookup_preset_button(canvas);
            if !self.dictionary_preset_picker_open() {
                canvas.keyboard(self.editor.layer, "Look up", None);
            }
            return;
        }
        if view.editing {
            canvas.fill(bounds, 255);
            self.navigation_layout()
                .header(canvas, "Find in definition");
            canvas.single_line_field(
                Rect::new(40, 160, bounds.w - 80, 124),
                &self.editor,
                "Find",
                false,
            );
            if let Some(message) = &self.notice {
                canvas.text(Rect::new(46, 320, bounds.w - 92, 90), message, CAPTION);
            }
            canvas.keyboard(self.editor.layer, "Find", None);
            return;
        }
        let (popup, back, rows) = self.definition_layout();
        let edit = definition_edit_button(popup, self.definitions.len() > 1);
        canvas.popup(popup);
        let choose = definition_results_button(popup);
        let end = edit.x - 12;
        let title = match view.results.active {
            Some(Menu::Flat) => Some(
                self.controls
                    .interface_language
                    .dictionary_popup_text("Dictionary Results"),
            ),
            Some(Menu::Sources) => Some(
                self.controls
                    .interface_language
                    .dictionary_popup_text("Dictionary Sources"),
            ),
            Some(Menu::Source(index)) => self
                .definitions
                .get(index)
                .map(|result| result.dictionary.as_str()),
            None => self
                .definitions
                .get(self.list_page)
                .map(|result| result.dictionary.as_str()),
        };
        canvas.icon(
            Rect::new(popup.x + 28, popup.y + 31, 40, 40),
            bar_icons::BOOK,
            0,
        );
        canvas.text(
            Rect::new(popup.x + 84, popup.y + 20, end - popup.x - 84, 64),
            title.unwrap_or(&self.definition_word),
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
            self.controls.interface_language.dictionary_popup_text(
                if view.results.active.is_some() || !self.dictionary_navigation.trail.is_empty() {
                    "Back"
                } else {
                    "Done"
                },
            ),
            false,
            false,
        );
        if self.definitions.len() > 1 {
            canvas.button(
                choose,
                self.controls
                    .interface_language
                    .dictionary_popup_text("Results"),
                view.results.active.is_some(),
                false,
            );
        }
        if let Some(menu) = view.results.active {
            self.draw_definition_results(canvas, popup, menu);
            return;
        }
        self.draw_definition_metadata(canvas, popup);
        let rect = self.definition_body(popup);
        let typography = self.definition_typography();
        let line_height = typography.line();
        if let Some(result) = self.definitions.get(self.list_page) {
            let text = &result.text;
            if text.is_empty() {
                canvas.text(
                    rect,
                    self.controls
                        .interface_language
                        .dictionary_popup_text(HEADWORD_ONLY),
                    CAPTION,
                );
            }
            let lines = view
                .layout
                .get_or_init(|| article_layout(canvas, result, rect.w, typography));
            let current = view.find.as_ref().map(|f| &f.hits[f.index]);
            let first = view.first_line(rows);
            for (row, line) in lines.iter().skip(first).take(rows).enumerate() {
                let y = rect.y + row as i32 * line_height;
                let line_rect = line.bounds(Rect::new(rect.x, y, rect.w, line_height));
                line.draw_marker(canvas, line_rect, typography);
                let content = text[line.range.clone()].trim_end();
                let placement = line.placed.get_or_init(|| {
                    PlacedLine::new(
                        canvas,
                        content,
                        line_rect.w,
                        self.dictionary_settings.justify_text && line.soft_wrap,
                        line.rtl,
                        typography.font(),
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
                                Rect::new(line_rect.x + left, y + line_height - 3, right - left, 2),
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
                            Rect::new(line_rect.x + left, y, (right - left).max(2), line_height),
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
            } else if view.preset.is_some() {
                "No matches from this preset"
            } else if view.no_sources_for_book {
                "No dictionaries enabled for this book"
            } else {
                "No matches in installed dictionaries"
            };
            canvas.text(
                rect,
                self.controls
                    .interface_language
                    .dictionary_popup_text(message),
                BODY.lines(2),
            );
            self.draw_lookup_preset_button(canvas);
        }
        let grid = self.definition_button_grid(popup);
        for &(button, rect) in &grid.items {
            self.draw_dictionary_button(canvas, button, rect);
        }
        grid.draw_pager(canvas, self.controls.interface_language);
    }

    fn draw_definition_metadata(&self, canvas: &mut Canvas, popup: Rect) {
        let bounds = definition_metadata_bounds(popup);
        let Some(result) = self.definitions.get(self.list_page) else {
            if let Some(message) = &self.notice {
                canvas.text(bounds, message, CAPTION);
            }
            return;
        };
        let counter = format!("{} / {}", self.list_page + 1, self.definitions.len());
        let counter_width = canvas.measure(&counter, 30) + 16;
        let counter_x = bounds.right() - counter_width;
        let nested = !self.dictionary_navigation.trail.is_empty();
        let hint = "Hold Back to close all";
        let detail = match (&self.notice, &self.dictionary_navigation.view.find) {
            (Some(notice), _) => notice.clone(),
            (None, Some(find)) => {
                let mut text = format!("Match {} / {}", find.index + 1, find.hits.len());
                if nested {
                    text.push_str(" · ");
                    text.push_str(hint);
                }
                text
            }
            (None, None) if nested => hint.into(),
            (None, None) => String::new(),
        };
        let detail_width = if detail.is_empty() {
            0
        } else {
            (canvas.measure(&detail, 28) + 12).min(bounds.w * 3 / 5)
        };
        let detail_x = counter_x - detail_width - 16;
        canvas.text(
            Rect::new(bounds.x, bounds.y, detail_x - bounds.x - 12, bounds.h),
            &result.word,
            primitive_ui::TextStyle::new(30).bold(),
        );
        if detail_width > 0 {
            canvas.text(
                Rect::new(detail_x, bounds.y, detail_width, bounds.h),
                &detail,
                CAPTION.right(),
            );
        }
        canvas.text(
            Rect::new(counter_x, bounds.y, counter_width, bounds.h),
            &counter,
            primitive_ui::TextStyle::new(30).right(),
        );
    }
}

#[cfg(test)]
mod tests;
