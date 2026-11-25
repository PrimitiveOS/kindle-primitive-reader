//! Persistent dictionary actions, their editor, and shared popup geometry.
use super::*;

const ROW_HEIGHT: i32 = 100;
const GAP: i32 = 8;
const MAX_VISIBLE_ROWS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::app) enum Button {
    SaveWord,
    Find,
    Wikipedia,
    SearchBook,
    Highlight,
    Previous,
    Expand,
    Next,
    Translate,
    Close,
}

impl Button {
    const ALL: [Self; 10] = [
        Self::SaveWord,
        Self::Find,
        Self::Wikipedia,
        Self::SearchBook,
        Self::Highlight,
        Self::Previous,
        Self::Expand,
        Self::Next,
        Self::Translate,
        Self::Close,
    ];

    pub(in crate::app) fn label(self) -> &'static str {
        match self {
            Self::SaveWord => "Save word",
            Self::Find => "Find",
            Self::Wikipedia => "Wikipedia",
            Self::SearchBook => "Search book",
            Self::Highlight => "Highlight",
            Self::Previous => "Previous result",
            Self::Expand => "Expand",
            Self::Next => "Next result",
            Self::Translate => "Translate",
            Self::Close => "Close",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(in crate::app) struct Settings {
    order: Vec<Button>,
    visible: Vec<Button>,
    limits: Vec<u8>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            order: Button::ALL.to_vec(),
            visible: Button::ALL[..8].to_vec(),
            limits: vec![3; 3],
        }
    }
}

impl Settings {
    pub(in crate::app) fn is_valid(&self) -> bool {
        self.order.len() == Button::ALL.len()
            && Button::ALL.iter().all(|button| self.order.contains(button))
            && self.visible.len() <= Button::ALL.len()
            && self
                .visible
                .iter()
                .enumerate()
                .all(|(i, button)| !self.visible[..i].contains(button))
            && self.limits.len() <= Button::ALL.len()
            && self.limits.iter().all(|limit| (1..=4).contains(limit))
    }

    fn capacity(&self, row: usize) -> u8 {
        self.limits.get(row).copied().unwrap_or(3)
    }

    fn rows(&self) -> Vec<Vec<Button>> {
        let mut remaining = self
            .order
            .iter()
            .copied()
            .filter(|button| self.visible.contains(button))
            .peekable();
        let mut rows = Vec::new();
        while remaining.peek().is_some() {
            let count = usize::from(self.capacity(rows.len()));
            rows.push(remaining.by_ref().take(count).collect());
        }
        rows
    }

    fn toggle(&mut self, button: Button) {
        if let Some(index) = self.visible.iter().position(|&current| current == button) {
            self.visible.remove(index);
        } else {
            self.visible.push(button);
        }
        self.limits.truncate(self.rows().len());
    }

    fn set_capacity(&mut self, row: usize, value: u8) {
        self.limits.resize(self.limits.len().max(row + 1), 3);
        self.limits[row] = value;
        self.limits.truncate(self.rows().len());
    }

    pub(super) fn footer_height(&self) -> i32 {
        let count = self.rows().len();
        30 + (count.min(MAX_VISIBLE_ROWS) + usize::from(count > MAX_VISIBLE_ROWS)) as i32
            * (ROW_HEIGHT + GAP)
    }
}

pub(super) struct Grid {
    pub(super) items: Vec<(Button, Rect)>,
    // True means the later page.
    pager: Vec<(bool, Rect)>,
}

impl Grid {
    pub(super) fn standard(popup: Rect) -> Self {
        let [save, find, wiki, search, highlight] = super::action_row(popup);
        let [previous, expand, next] = definition_actions(popup);
        Self {
            items: vec![
                (Button::SaveWord, save),
                (Button::Find, find),
                (Button::Wikipedia, wiki),
                (Button::SearchBook, search),
                (Button::Highlight, highlight),
                (Button::Previous, previous),
                (Button::Expand, expand),
                (Button::Next, next),
            ],
            pager: Vec::new(),
        }
    }

    pub(super) fn configured(popup: Rect, config: &Settings, page: usize) -> Self {
        let rows = config.rows();
        let pages = rows.len().div_ceil(MAX_VISIBLE_ROWS).max(1);
        let page = page.min(pages - 1);
        let count = rows.len().min(MAX_VISIBLE_ROWS);
        let x = popup.x + 30;
        let width = popup.w - 60;
        let pager_y = popup.bottom() - 18 - ROW_HEIGHT;
        let floor = popup.bottom() - 18 - if pages > 1 { ROW_HEIGHT + GAP } else { 0 };
        let mut items = Vec::with_capacity(Button::ALL.len());
        for (index, row) in rows
            .iter()
            .skip(page * MAX_VISIBLE_ROWS)
            .take(MAX_VISIBLE_ROWS)
            .enumerate()
        {
            let y = floor - (count - index) as i32 * (ROW_HEIGHT + GAP);
            let available = width - (row.len() as i32 - 1) * GAP;
            let paired =
                row.len() == 4 && row.contains(&Button::Previous) && row.contains(&Button::Next);
            let mut left = x;
            for &button in row {
                let length = if paired {
                    if matches!(button, Button::Previous | Button::Next) {
                        available * 15 / 100
                    } else {
                        available * 35 / 100
                    }
                } else {
                    available / row.len() as i32
                };
                items.push((button, Rect::new(left, y, length, ROW_HEIGHT)));
                left += length + GAP;
            }
        }
        let mut pager = Vec::with_capacity(2);
        if page > 0 {
            pager.push((false, Rect::new(x, pager_y, 360, ROW_HEIGHT)));
        }
        if page + 1 < pages {
            pager.push((true, Rect::new(x + width - 360, pager_y, 360, ROW_HEIGHT)));
        }
        Self { items, pager }
    }

    pub(super) fn at(&self, x: f32, y: f32) -> Option<Button> {
        self.items
            .iter()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(button, _)| *button)
    }

    pub(super) fn page_after_tap(&self, x: f32, y: f32, page: usize) -> Option<usize> {
        let (next, _) = self.pager.iter().find(|(_, rect)| rect.contains(x, y))?;
        Some(if *next {
            page + 1
        } else {
            page.saturating_sub(1)
        })
    }

    pub(super) fn draw_pager(
        &self,
        canvas: &mut Canvas,
        language: crate::app::interface_language::InterfaceLanguage,
    ) {
        for &(next, rect) in &self.pager {
            canvas.button(
                rect,
                language.dictionary_popup_text(if next {
                    "Next actions"
                } else {
                    "Previous actions"
                }),
                false,
                false,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum Page {
    Main,
    Sort,
    Item(Button),
    Capacity(usize),
    Preview,
    Reset,
}

impl Page {
    pub(in crate::app) fn title(self) -> &'static str {
        match self {
            Self::Main => "Customize buttons",
            Self::Sort => "Sort and toggle buttons",
            Self::Item(button) => button.label(),
            Self::Capacity(_) => "Maximum buttons in row",
            Self::Preview => "Button layout preview",
            Self::Reset => "Restore default layout?",
        }
    }
}

pub(in crate::app) enum Command {
    Open,
    Show(Page),
    Toggle(Button),
    Move(Button, isize),
    SetCapacity(usize, u8),
    Apply,
    Reset,
}

pub(super) struct Draft {
    settings: Settings,
    preview_page: usize,
}

impl Reader {
    pub(in crate::app) fn close_dictionary_button_editor(&mut self) {
        self.dictionary_navigation.button_draft = None;
    }

    pub(in crate::app) fn dictionary_button_items(&self, page: Page) -> Vec<Item> {
        let Some(draft) = &self.dictionary_navigation.button_draft else {
            return Vec::new();
        };
        let prefs = &draft.settings;
        let row = |name: &str, action| Item::new(name, Action::DictionaryButtons(action));
        match page {
            Page::Main => {
                let mut options = vec![row("Sort and toggle buttons", Command::Show(Page::Sort))];
                for index in 0..prefs.rows().len() {
                    options.push(
                        row(
                            &format!("Max buttons in row {}", index + 1),
                            Command::Show(Page::Capacity(index)),
                        )
                        .detail(prefs.capacity(index).to_string()),
                    );
                }
                options.extend([
                    row("Test button layout", Command::Show(Page::Preview)),
                    row("Apply layout", Command::Apply),
                    row("Restore default layout", Command::Show(Page::Reset)),
                ]);
                options
            }
            Page::Sort => {
                let layout = prefs.rows();
                prefs
                    .order
                    .iter()
                    .map(|&button| {
                        let detail = layout
                            .iter()
                            .position(|row| row.contains(&button))
                            .map_or_else(|| "Hidden".into(), |row| format!("Row {}", row + 1));
                        row(button.label(), Command::Show(Page::Item(button))).detail(detail)
                    })
                    .collect()
            }
            Page::Item(button) => {
                let enabled = prefs.visible.contains(&button);
                let mut options = vec![
                    row("Show button", Command::Toggle(button)).detail(if enabled {
                        "On"
                    } else {
                        "Off"
                    }),
                ];
                if prefs.order.first() != Some(&button) {
                    options.push(row("Move earlier", Command::Move(button, -1)));
                }
                if prefs.order.last() != Some(&button) {
                    options.push(row("Move later", Command::Move(button, 1)));
                }
                options
            }
            Page::Capacity(index) => (1..=4)
                .map(|count| {
                    let item = row(&count.to_string(), Command::SetCapacity(index, count));
                    if count == prefs.capacity(index) {
                        item.detail("Selected")
                    } else {
                        item
                    }
                })
                .collect(),
            Page::Reset => vec![row("Restore default layout", Command::Reset)],
            Page::Preview => Vec::new(),
        }
    }

    pub(in crate::app) fn change_dictionary_buttons(&mut self, action: Command) -> Result<()> {
        let next_page = match action {
            Command::Open => {
                self.dictionary_navigation.button_draft = Some(Draft {
                    settings: self
                        .dictionary_settings
                        .popup_buttons
                        .clone()
                        .unwrap_or_default(),
                    preview_page: 0,
                });
                Page::Main
            }
            Command::Show(page) => {
                if page == Page::Preview
                    && let Some(draft) = &mut self.dictionary_navigation.button_draft
                {
                    draft.preview_page = 0;
                }
                page
            }
            Command::Apply | Command::Reset => {
                let mut next = self.dictionary_settings.clone();
                next.popup_buttons = if matches!(action, Command::Reset) {
                    None
                } else {
                    self.dictionary_navigation
                        .button_draft
                        .as_ref()
                        .map(|draft| draft.settings.clone())
                };
                self.save_source_preferences(next)?;
                self.show(Screen::Dictionaries);
                return Ok(());
            }
            action => {
                let draft = self
                    .dictionary_navigation
                    .button_draft
                    .as_mut()
                    .ok_or("Open the button editor first")?;
                match action {
                    Command::Toggle(button) => {
                        draft.settings.toggle(button);
                        Page::Item(button)
                    }
                    Command::Move(button, direction) => {
                        let order = &mut draft.settings.order;
                        let index = order
                            .iter()
                            .position(|&entry| entry == button)
                            .ok_or("Button unavailable")?;
                        if let Some(next) = index
                            .checked_add_signed(direction)
                            .filter(|&next| next < order.len())
                        {
                            order.swap(index, next);
                        }
                        Page::Item(button)
                    }
                    Command::SetCapacity(row, count) => {
                        if row < draft.settings.rows().len() && (1..=4).contains(&count) {
                            draft.settings.set_capacity(row, count);
                        }
                        Page::Main
                    }
                    _ => unreachable!(),
                }
            }
        };
        if self.screen != Screen::DictionaryButtons(next_page) {
            self.show(Screen::DictionaryButtons(next_page));
        }
        Ok(())
    }

    pub(in crate::app) fn dictionary_button_editor_back(&mut self, page: Page) {
        match page {
            Page::Main => self.show(Screen::Dictionaries),
            Page::Item(button) => {
                self.show(Screen::DictionaryButtons(Page::Sort));
                if let Some(draft) = &self.dictionary_navigation.button_draft
                    && let Some(index) =
                        draft.settings.order.iter().position(|&item| item == button)
                {
                    self.list_page = index / self.list_geometry().rows;
                }
            }
            _ => self.show(Screen::DictionaryButtons(Page::Main)),
        }
    }

    fn dictionary_button_preview_location(&self, settings: &Settings) -> Rect {
        let (width, height) = self.reading_viewport().dimensions();
        let box_height = 235 + settings.footer_height();
        definition_column(width, (height as i32 - box_height) / 2, box_height)
    }

    pub(in crate::app) fn draw_dictionary_button_preview(&self, canvas: &mut Canvas) {
        let Some(draft) = &self.dictionary_navigation.button_draft else {
            return;
        };
        self.navigation_layout()
            .header(canvas, Page::Preview.title());
        let popup = self.dictionary_button_preview_location(&draft.settings);
        canvas.popup(popup);
        canvas.text(
            Rect::new(popup.x + 34, popup.y + 20, popup.w - 68, 70),
            "Beacon · Sample definition",
            BODY.bold(),
        );
        canvas.text(
            Rect::new(popup.x + 34, popup.y + 100, popup.w - 68, 92),
            "A light or marker that guides boats toward a harbour.",
            BODY.lines(2),
        );
        let grid = Grid::configured(popup, &draft.settings, draft.preview_page);
        for &(button, rect) in &grid.items {
            super::draw_preview_button(canvas, button, rect);
        }
        grid.draw_pager(canvas, self.controls.interface_language);
    }

    pub(in crate::app) fn dictionary_button_preview_input(&mut self, input: Input) -> Change {
        if matches!(input, Input::PageBack)
            || matches!(input, Input::Tap(x, y) if self.navigation_layout().back().contains(x, y))
        {
            self.dictionary_button_editor_back(Page::Preview);
            return Change::Partial;
        }
        let Some(draft) = &self.dictionary_navigation.button_draft else {
            return Change::None;
        };
        let Input::Tap(x, y) = input else {
            return Change::None;
        };
        let popup = self.dictionary_button_preview_location(&draft.settings);
        let grid = Grid::configured(popup, &draft.settings, draft.preview_page);
        let Some(page) = grid.page_after_tap(x, y, draft.preview_page) else {
            return Change::None;
        };
        self.dictionary_navigation
            .button_draft
            .as_mut()
            .unwrap()
            .preview_page = page;
        Change::Partial
    }
}

#[cfg(test)]
mod tests;
