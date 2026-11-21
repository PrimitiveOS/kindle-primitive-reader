//! Result, text position and nested lookup history live for the lifetime of one popup.
use super::*;
use std::cell::OnceCell;
use std::ops::Range;

const FONT_SIZE: i32 = 36;
const DEFINITION_TEXT: primitive_ui::TextStyle =
    primitive_ui::TextStyle::new(FONT_SIZE).break_long_words();
const LINE: i32 = 47;
const MAX_DEPTH: usize = 8;

#[derive(Default)]
pub(super) struct Navigation {
    origin: Option<(Screen, usize)>,
    origin_editor: Option<Editor>,
    trail: Vec<Previous>,
    view: View,
    online_return: Option<usize>,
}
struct Previous {
    word: String,
    definitions: Vec<dictionary::Definition>,
    index: usize,
    expanded: bool,
    view: View,
}
#[derive(Default)]
struct View {
    line: usize,
    start_at_end: bool,
    layout: OnceCell<Vec<TextLine>>,
    find: Option<Find>,
    editing: bool,
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
    words: OnceCell<Vec<(Range<usize>, i32, i32)>>,
}

fn body(popup: Rect) -> Rect {
    Rect::new(popup.x + 32, popup.y + 108, popup.w - 84, popup.h - 273)
}
fn action_row(popup: Rect) -> [Rect; 3] {
    let save = definition_vocabulary(popup);
    [
        save,
        Rect::new(save.x + save.w + 12, save.y, save.w, save.h),
        Rect::new(save.x + 2 * (save.w + 12), save.y, save.w, save.h),
    ]
}
fn action_button(
    canvas: &mut Canvas,
    rect: Rect,
    label: &str,
    icon: primitive_ui::Icon,
    selected: bool,
) {
    canvas.button(rect, label, selected, false);
    let filled = selected != canvas.is_control_pressed(rect);
    let size = 34;
    canvas.icon(
        Rect::new(rect.x + 16, rect.y + (rect.h - size) / 2, size, size),
        icon,
        if filled { 255 } else { 0 },
    );
}

fn layout(canvas: &mut Canvas, text: &str, width: i32) -> Vec<TextLine> {
    canvas
        .text_pages(Rect::new(0, 0, width, LINE), text, DEFINITION_TEXT)
        .into_iter()
        .map(|range| TextLine {
            range,
            words: OnceCell::new(),
        })
        .collect()
}

// The canvas draws each wrapped line separately. Resolve its visual runs before measuring
// word positions, so a hold on the right of an RTL run hits its first logical word.
fn range_boxes(
    canvas: &mut Canvas,
    text: &str,
    row: &Range<usize>,
    spans: &[Range<usize>],
) -> Vec<(Range<usize>, i32, i32)> {
    let line = text[row.clone()].trim_end();
    if line.is_empty() {
        return Vec::new();
    }
    let bidi = unicode_bidi::BidiInfo::new(line, None);
    let (levels, runs) = bidi.visual_runs(&bidi.paragraphs[0], 0..line.len());
    let mut boxes = Vec::new();
    let mut x = 0;
    for run in runs {
        let width = canvas.measure(&line[run.clone()], FONT_SIZE);
        for word in spans
            .iter()
            .filter(|word| word.end > run.start && word.start < run.end)
        {
            let a = canvas.measure(&line[run.start..word.start.max(run.start)], FONT_SIZE);
            let b = canvas.measure(&line[run.start..word.end.min(run.end)], FONT_SIZE);
            let (left, right) = if levels[run.start].is_rtl() {
                (width - b, width - a)
            } else {
                (a, b)
            };
            boxes.push((
                row.start + word.start..row.start + word.end,
                x + left,
                x + right,
            ));
        }
        x += width;
    }
    boxes
}

pub(super) fn definition_vocabulary(popup: Rect) -> Rect {
    Rect::new(94, popup.bottom() - 152, (popup.w - 90) / 3, 58)
}

const RESULTS_PER_PAGE: usize = 5;

fn definition_results_button(popup: Rect) -> Rect {
    Rect::new(WIDTH as i32 - 434, popup.y + 18, 172, 64)
}

fn definition_result_row(popup: Rect, row: usize) -> Rect {
    Rect::new(
        popup.x + 30,
        popup.y + 100 + row as i32 * 104,
        popup.w - 60,
        98,
    )
}

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
                .unwrap_or_default()
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
            let label = if saved { "Word saved" } else { "Save word" };
            action_button(
                canvas,
                rect,
                label,
                bar_icons::action_icon(&Action::AddBookmark),
                saved,
            );
        }
    }

    pub(super) fn look_up(&mut self, word: &str) -> Result<()> {
        let nested = self.screen == Screen::Definition;
        let word = word.trim();
        if !lookup_history::valid(word) {
            return Err("Enter a word or a short phrase".into());
        }
        if nested && self.dictionary_navigation.trail.len() >= MAX_DEPTH {
            return Err("Go back one lookup to open another definition".into());
        }
        let results = with_storage(|| {
            dictionary::lookup_preferred(
                &self.dictionaries,
                word,
                self.preferred_dictionary.as_deref(),
            )
        })?;
        self.update_lookup_history(|history| history.lookup(word))?;
        if nested {
            self.dictionary_navigation.trail.push(Previous {
                word: std::mem::take(&mut self.definition_word),
                definitions: std::mem::take(&mut self.definitions),
                index: self.list_page,
                expanded: self.definition_expanded,
                view: std::mem::take(&mut self.dictionary_navigation.view),
            });
        } else {
            self.dictionary_navigation = Navigation {
                origin: Some((self.screen, self.list_page)),
                origin_editor: (self.screen == Screen::DictionaryEntry)
                    .then(|| self.editor.clone()),
                ..Default::default()
            };
        }
        self.definition_word = word.into();
        self.definitions = results;
        self.definition_expanded = false;
        self.definition_results_page = None;
        self.show(Screen::Definition);
        Ok(())
    }

    pub(super) fn restore_dictionary_online_return(&mut self) {
        if let Some(index) = self.dictionary_navigation.online_return.take() {
            self.list_page = index;
        }
    }
    fn return_from_definition(&mut self) {
        self.notice = None;
        if let Some(previous) = self.dictionary_navigation.trail.pop() {
            self.definition_word = previous.word;
            self.definitions = previous.definitions;
            self.list_page = previous.index;
            self.definition_expanded = previous.expanded;
            self.definition_results_page = None;
            self.dictionary_navigation.view = previous.view;
            return;
        }
        let back = self.dictionary_navigation.origin.take().unwrap_or((
            if self.fixed.is_some() {
                Screen::Fixed
            } else {
                Screen::Book
            },
            0,
        ));
        let editor = self.dictionary_navigation.origin_editor.take();
        self.dictionary_navigation = Navigation::default();
        self.show(back.0);
        self.list_page = back.1;
        if let Some(editor) = editor {
            self.editor = editor;
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
    fn dictionary_word_at(&self, popup: Rect, x: f32, y: f32) -> Option<String> {
        let rect = body(popup);
        if !rect.contains(x, y) {
            return None;
        }
        let view = &self.dictionary_navigation.view;
        let n = view.first_line((rect.h / LINE) as usize)
            + (y as i32 - rect.y) as usize / LINE as usize;
        let line = view.layout.get()?.get(n)?;
        let pos = x as i32 - rect.x;
        let (bytes, _, _) = line
            .words
            .get()?
            .iter()
            .find(|(_, left, right)| (*left..*right).contains(&pos))?;
        let text = &self.definitions.get(self.list_page)?.text;
        Some(text[crate::selection::word_range(text, bytes.start)?].to_owned())
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
        if self.dictionary_navigation.view.editing {
            return self.edit_find_input(input);
        }
        let (popup, back, _) = self.definition_layout();
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
                if self.definition_expanded && back.contains(x, y) {
                    self.definition_expanded = false;
                    return Change::Partial;
                }
                if back.contains(x, y) || !popup.contains(x, y) {
                    self.return_from_definition();
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
                let [save, find, wiki] = action_row(popup);
                if self.vocabulary_available() && save.contains(x, y) {
                    let word = self.definition_word.clone();
                    return match self.update_lookup_history(|h| h.save_word(&word)) {
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
                if body(popup).contains(x, y) {
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
        let [save, find, wiki] = action_row(popup);
        canvas.popup(popup);
        let choose = definition_results_button(popup);
        let end = if self.definitions.len() > 1 {
            choose.x
        } else {
            back.x
        };
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
            Rect::new(popup.x + 84, popup.y + 20, end - popup.x - 100, 64),
            title.as_deref().unwrap_or(&self.definition_word),
            BODY.bold(),
        );
        canvas.button(
            back,
            if self.definition_expanded
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
            let lines = view.layout.get_or_init(|| layout(canvas, text, rect.w));
            let current = view.find.as_ref().map(|f| &f.hits[f.index]);
            let first = view.first_line(rows);
            for (row, line) in lines.iter().skip(first).take(rows).enumerate() {
                let y = rect.y + row as i32 * LINE;
                line.words.get_or_init(|| {
                    let words: Vec<_> =
                        crate::selection::word_ranges(text[line.range.clone()].trim_end())
                            .collect();
                    range_boxes(canvas, text, &line.range, &words)
                });
                if let Some(hit) = current
                    && line.range.start < hit.end
                    && hit.start < line.range.end
                {
                    let part = hit.start.max(line.range.start) - line.range.start
                        ..hit.end.min(line.range.end) - line.range.start;
                    for (_, left, right) in
                        range_boxes(canvas, text, &line.range, std::slice::from_ref(&part))
                    {
                        canvas.fill(
                            Rect::new(rect.x + left, y, (right - left).max(2), LINE),
                            205,
                        );
                    }
                }
                canvas.text(
                    Rect::new(rect.x, y, rect.w, LINE),
                    text[line.range.clone()].trim_end(),
                    DEFINITION_TEXT,
                );
            }
            if lines.len() > rows {
                let height = (rect.h * rows as i32 / lines.len() as i32).max(12);
                let at = first as i32 * (rect.h - height) / (lines.len() - rows) as i32;
                canvas.fill(Rect::new(rect.right() + 10, rect.y, 4, rect.h), 210);
                canvas.fill(Rect::new(rect.right() + 8, rect.y + at, 8, height), 50);
            }
        } else {
            canvas.text(rect, "No matches in installed dictionaries", BODY.lines(2));
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
        canvas.text(
            Rect::new(rect.x, popup.y + 78, rect.w, 34),
            self.notice.as_deref().unwrap_or(&status),
            CAPTION,
        );
    }
}

#[cfg(test)]
mod tests;
