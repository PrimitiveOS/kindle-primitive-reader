use super::*;
use std::collections::BTreeMap;

const PER_PAGE: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Menu {
    Flat,
    Sources,
    Source(usize),
}

#[derive(Default)]
pub(super) struct Menus {
    pub(super) active: Option<Menu>,
    flat_page: Option<usize>,
    sources_page: Option<usize>,
    source_pages: BTreeMap<usize, usize>,
    // The source button and its hit behavior use the same label fit.
    clipped_headwords: OnceCell<BTreeSet<usize>>,
}

impl Menus {
    pub(super) fn page(&self, menu: Menu) -> Option<usize> {
        match menu {
            Menu::Flat => self.flat_page,
            Menu::Sources => self.sources_page,
            Menu::Source(source) => self.source_pages.get(&source).copied(),
        }
    }

    fn set_page(&mut self, menu: Menu, page: usize) {
        match menu {
            Menu::Flat => self.flat_page = Some(page),
            Menu::Sources => self.sources_page = Some(page),
            Menu::Source(source) => {
                self.source_pages.insert(source, page);
            }
        }
    }

    fn headword_is_clipped(&self, index: usize) -> bool {
        self.clipped_headwords
            .get()
            .is_some_and(|headwords| headwords.contains(&index))
    }

    pub(super) fn dismiss(&mut self) {
        self.active = match self.active {
            Some(Menu::Source(_)) => Some(Menu::Sources),
            _ => None,
        };
    }
}

#[derive(Clone, Copy)]
enum Row {
    Definition(usize),
    Source { first: usize, count: usize },
}

fn rows(definitions: &[dictionary::LookupEntry], menu: Menu) -> Vec<Row> {
    match menu {
        Menu::Flat => (0..definitions.len()).map(Row::Definition).collect(),
        Menu::Sources => {
            let mut groups = Vec::<Row>::new();
            for (index, result) in definitions.iter().enumerate() {
                let group = groups.iter_mut().find_map(|row| match row {
                    Row::Source { first, count } if definitions[*first].source == result.source => {
                        Some(count)
                    }
                    _ => None,
                });
                if let Some(count) = group {
                    *count += 1;
                } else {
                    groups.push(Row::Source {
                        first: index,
                        count: 1,
                    });
                }
            }
            groups
        }
        Menu::Source(source) => definitions
            .iter()
            .enumerate()
            .filter(|(_, result)| result.source == definitions[source].source)
            .map(|(index, _)| Row::Definition(index))
            .collect(),
    }
}

impl Row {
    fn selected(self, definitions: &[dictionary::LookupEntry], index: usize) -> bool {
        match self {
            Self::Definition(result) => result == index,
            Self::Source { first, .. } => definitions
                .get(index)
                .is_some_and(|current| current.source == definitions[first].source),
        }
    }

    fn definition(self) -> usize {
        match self {
            Self::Definition(index) => index,
            Self::Source { first, .. } => first,
        }
    }
}

pub(super) fn height(definitions: &[dictionary::LookupEntry], menu: Menu, page: usize) -> i32 {
    let shown = rows(definitions, menu)
        .len()
        .saturating_sub(page * PER_PAGE);
    198 + shown.min(PER_PAGE) as i32 * 104
}

pub(super) fn definition_result_row(popup: Rect, row: usize) -> Rect {
    Rect::new(
        popup.x + 30,
        popup.y + 100 + row as i32 * 104,
        popup.w - 60,
        98,
    )
}

fn row_action(row: Rect, width: i32) -> Rect {
    Rect::new(row.right() - width, row.y, width, row.h)
}

pub(super) fn detail_button(row: Rect) -> Rect {
    row_action(row, 300)
}

pub(super) fn lookup_button(row: Rect) -> Rect {
    row_action(row, 180)
}

impl Reader {
    pub(super) fn open_definition_results(&mut self, menu: Menu) {
        let menus = &mut self.dictionary_navigation.view.results;
        if menus.page(menu).is_none() {
            let page = rows(&self.definitions, menu)
                .iter()
                .position(|row| row.selected(&self.definitions, self.list_page))
                .map_or(0, |index| index / PER_PAGE);
            menus.set_page(menu, page);
        }
        menus.active = Some(menu);
    }

    pub(super) fn draw_definition_results(&self, canvas: &mut Canvas, popup: Rect, menu: Menu) {
        let menus = &self.dictionary_navigation.view.results;
        let language = self.controls.interface_language;
        let page = menus.page(menu).unwrap_or(0);
        let first = page * PER_PAGE;
        let rows = rows(&self.definitions, menu);
        if menu == Menu::Sources {
            menus.clipped_headwords.get_or_init(|| {
                let label_width = detail_button(definition_result_row(popup, 0)).w - canvas.px(16);
                rows.iter()
                    .filter_map(|row| match row {
                        Row::Source { first, count: 1 }
                            if canvas.measure_with_weight(
                                &self.definitions[*first].word,
                                36,
                                true,
                            ) > label_width =>
                        {
                            Some(*first)
                        }
                        _ => None,
                    })
                    .collect()
            });
        }
        for (position, &row) in rows.iter().skip(first).take(PER_PAGE).enumerate() {
            let rect = definition_result_row(popup, position);
            let result = &self.definitions[row.definition()];
            let selected = row.selected(&self.definitions, self.list_page);
            let action = match row {
                Row::Definition(_) => lookup_button(rect),
                Row::Source { .. } => detail_button(rect),
            };
            let primary = Rect::new(rect.x, rect.y, action.x - rect.x - 8, rect.h);
            let filled = selected != canvas.control_pressed(primary, false);
            canvas.panel(primary, 16, 3, if filled { 0 } else { 255 });
            let ink = if filled { 255 } else { 0 };

            match row {
                Row::Definition(_) => {
                    let title = if menu == Menu::Flat {
                        format!("{} · {}", result.word, result.dictionary)
                    } else {
                        result.word.clone()
                    };
                    let wide_label = matches!(menu, Menu::Source(source) if menus.headword_is_clipped(source))
                        && rows.len() == 1;
                    let title_height = if wide_label { 88 } else { 48 };
                    canvas.text(
                        Rect::new(primary.x + 22, primary.y + 4, primary.w - 44, title_height),
                        &title,
                        BODY.bold().lines(if wide_label { 2 } else { 1 }).ink(ink),
                    );
                    if !wide_label {
                        let snippet: String = result
                            .text
                            .lines()
                            .next()
                            .unwrap_or_else(|| language.dictionary_popup_text(HEADWORD_ONLY))
                            .chars()
                            .take(120)
                            .collect();
                        canvas.text(
                            Rect::new(primary.x + 22, primary.y + 53, primary.w - 44, 39),
                            &snippet,
                            CAPTION.ink(if filled { 255 } else { 85 }),
                        );
                    }
                    canvas.button(
                        action,
                        language.dictionary_popup_text("Look up"),
                        false,
                        false,
                    );
                }
                Row::Source { count, .. } => {
                    let shares_title = rows.iter().any(|other| {
                        let other = &self.definitions[other.definition()];
                        other.dictionary == result.dictionary && other.source != result.source
                    });
                    let title_y = if shares_title { 4 } else { 26 };
                    canvas.text(
                        Rect::new(primary.x + 22, primary.y + title_y, primary.w - 44, 46),
                        &result.dictionary,
                        BODY.bold().ink(ink),
                    );
                    if shares_title {
                        canvas.text(
                            Rect::new(primary.x + 22, primary.y + 53, primary.w - 44, 39),
                            &result.source.to_string_lossy(),
                            CAPTION.ink(if filled { 255 } else { 85 }),
                        );
                    }
                    let label = if count == 1 {
                        result.word.clone()
                    } else {
                        language.dictionary_results_count(count)
                    };
                    canvas.button(action, &label, selected, false);
                }
            }
        }
        let [previous, switch, next] = definition_actions(popup);
        if page > 0 {
            canvas.button(
                previous,
                language.dictionary_popup_text("Previous"),
                false,
                false,
            );
        }
        if first + PER_PAGE < rows.len() {
            canvas.button(next, language.dictionary_popup_text("Next"), false, false);
        }
        let label = if menu == Menu::Sources {
            "All Results"
        } else {
            "Sources"
        };
        canvas.button(switch, language.dictionary_popup_text(label), false, false);
        if let Some(notice) = &self.notice {
            canvas.text(
                Rect::new(popup.x + 30, popup.bottom() - 118, popup.w - 60, 32),
                notice,
                CAPTION,
            );
        }
    }

    pub(super) fn definition_results_input(
        &mut self,
        input: Input,
        popup: Rect,
        menu: Menu,
    ) -> Change {
        let page = self
            .dictionary_navigation
            .view
            .results
            .page(menu)
            .unwrap_or(0);
        let rows = rows(&self.definitions, menu);
        let [previous, switch, next] = definition_actions(popup);
        let direction = match input {
            Input::PageBack => -1,
            Input::PageForward => 1,
            Input::Swipe { dx, dy, .. } => {
                let movement = if dy.abs() > dx.abs() { dy } else { dx };
                if movement < 0.0 { 1 } else { -1 }
            }
            Input::Tap(x, y) if previous.contains(x, y) => -1,
            Input::Tap(x, y) if next.contains(x, y) => 1,
            Input::Tap(x, y) if switch.contains(x, y) => {
                self.open_definition_results(if menu == Menu::Sources {
                    Menu::Flat
                } else {
                    Menu::Sources
                });
                return Change::Partial;
            }
            Input::Tap(x, y) | Input::LongTap(x, y) => {
                let Some((position, &row)) = rows
                    .iter()
                    .skip(page * PER_PAGE)
                    .take(PER_PAGE)
                    .enumerate()
                    .find(|(position, _)| definition_result_row(popup, *position).contains(x, y))
                else {
                    return Change::None;
                };
                let rect = definition_result_row(popup, position);
                return self.activate_definition_result(
                    row,
                    rect,
                    x,
                    y,
                    matches!(input, Input::LongTap(..)),
                );
            }
            _ => return Change::None,
        };
        if let Some(page) = page
            .checked_add_signed(direction)
            .filter(|page| page * PER_PAGE < rows.len())
        {
            self.dictionary_navigation.view.results.set_page(menu, page);
            return Change::Partial;
        }
        Change::None
    }

    fn activate_definition_result(
        &mut self,
        row: Row,
        rect: Rect,
        x: f32,
        y: f32,
        held: bool,
    ) -> Change {
        match row {
            Row::Definition(index) if held || lookup_button(rect).contains(x, y) => {
                let word = self.definitions[index].word.clone();
                return self.lookup_result_word(&word);
            }
            Row::Source { first, count } if detail_button(rect).contains(x, y) => {
                if held && count == 1 {
                    let word = self.definitions[first].word.clone();
                    return self.lookup_result_word(&word);
                }
                if !held
                    && (count > 1
                        || self
                            .dictionary_navigation
                            .view
                            .results
                            .headword_is_clipped(first))
                {
                    self.open_definition_results(Menu::Source(first));
                    return Change::Partial;
                }
            }
            _ => {}
        }
        if held {
            return Change::None;
        }
        self.select_definition(row.definition());
        self.dictionary_navigation.view.results.active = None;
        Change::Partial
    }

    fn lookup_result_word(&mut self, word: &str) -> Change {
        match self.look_up(word) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        }
    }
}
