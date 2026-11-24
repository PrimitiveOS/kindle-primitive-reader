//! Independent navigation and configuration overlays for the reading viewport.

#[path = "bottom_config.rs"]
mod bottom_config;

use std::time::Instant;

use primitive_app::{App, Change, HEIGHT, WIDTH};
use primitive_ui::{CAPTION, Canvas, HAIRLINE, Icon, Input, Rect, TextStyle};

use self::bottom_config::{ConfigRow, SINGLE_VIEW_SECTIONS, TEXT_SECTIONS, VIEW_SECTIONS};
use super::bar_icons::{EXIT, action_icon, group_icon, panel_icon};
use super::top_config::TopGroup;
use super::{Action, Item, Reader, Screen};
use crate::session::{PAGE_HEIGHT, PagePaint};

pub(super) use crate::book_menus::BarPanel;

impl BarPanel {
    fn tab(self) -> Self {
        match self {
            Self::TextLayout
            | Self::TextMargins
            | Self::TextStyle
            | Self::TextSpacing
            | Self::TextScreen
            | Self::TextRendering => Self::Text,
            Self::ViewLayout | Self::ViewScreen => Self::View,
            _ => self,
        }
    }
}

enum Target {
    Invoke(Action),
    Apply(Action),
    Toggle(BarPanel),
    BottomSection(BarPanel),
    TopSection(TopGroup),
    Exit,
    Current(Action),
}

impl Target {
    fn icon(&self) -> Option<Icon> {
        match self {
            Self::Invoke(action) | Self::Apply(action) => Some(action_icon(action)),
            Self::Toggle(panel) | Self::BottomSection(panel) => Some(panel_icon(*panel)),
            Self::TopSection(group) => Some(group_icon(*group)),
            Self::Exit => Some(EXIT),
            Self::Current(_) => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Visual {
    Value,
    Shortcut(Icon),
    Tile(Icon),
}

struct Button {
    rect: Rect,
    label: String,
    detail: Option<String>,
    selected: bool,
    target: Target,
    visual: Visual,
}

impl Button {
    fn draw(self, canvas: &mut Canvas, size: i32) {
        let rect = self.rect;
        let size = if rect.w < 170 { size.min(28) } else { size };
        let pressed = canvas.control_pressed(rect, false);
        match self.visual {
            Visual::Tile(icon) => {
                // Browser's four-column cards use a left-aligned icon, title and optional value.
                let active = pressed || self.selected;
                if active {
                    canvas.panel(rect, 22, 2, 0);
                } else {
                    canvas.card(rect);
                }
                let ink = if active { 255 } else { 0 };
                canvas.icon(Rect::new(rect.x + 18, rect.y + 12, 40, 40), icon, ink);
                canvas.text(
                    Rect::new(rect.x + 18, rect.y + 57, rect.w - 36, 68),
                    &self.label,
                    TextStyle::new(size).bold().ink(ink).lines(2),
                );
                if let Some(detail) = self.detail {
                    canvas.text(
                        Rect::new(rect.x + 18, rect.y + 125, rect.w - 36, rect.h - 127),
                        &detail,
                        TextStyle::new(24).ink(if active { 255 } else { 85 }),
                    );
                }
            }
            Visual::Shortcut(icon) => {
                let size = self.shortcut_label_size(canvas, size);
                let ink = if pressed { 255 } else { 0 };
                if pressed {
                    canvas.panel(rect.inset(4), 16, 2, 0);
                }
                canvas.icon(
                    Rect::new(rect.x + (rect.w - 40) / 2, rect.y + 9, 40, 40),
                    icon,
                    ink,
                );
                canvas.text(
                    Rect::new(rect.x + 4, rect.y + 54, rect.w - 8, size + 10),
                    &self.label,
                    TextStyle::new(size).bold().center().ink(ink),
                );
                self.draw_selected(canvas, ink);
            }
            Visual::Value => {
                if pressed {
                    canvas.panel(rect.inset(4), 16, 2, 0);
                }
                let ink = if pressed { 255 } else { 0 };
                canvas.text(
                    Rect::new(rect.x, rect.y + (rect.h - size) / 2, rect.w, size + 10),
                    &self.label,
                    TextStyle::new(size).bold().center().ink(ink),
                );
                self.draw_selected(canvas, ink);
            }
        }
    }

    fn shortcut_label_size(&self, canvas: &mut Canvas, preferred: i32) -> i32 {
        let bounds = Rect::new(0, 0, self.rect.w - 8, self.rect.h);
        [preferred, 28, 26, 24]
            .into_iter()
            .filter(|size| *size <= preferred)
            .find(|size| {
                canvas
                    .text_pages(
                        bounds,
                        &self.label,
                        TextStyle::new(*size).bold().break_long_words(),
                    )
                    .len()
                    == 1
            })
            .unwrap_or(preferred.min(24))
    }

    fn draw_selected(&self, canvas: &mut Canvas, ink: u8) {
        if self.selected {
            let rect = self.rect;
            canvas.fill(
                Rect::new(rect.x + 12, rect.bottom() - 4, rect.w - 24, 3),
                ink,
            );
        }
    }
}

#[derive(Default)]
struct Bars {
    top: Vec<Rect>,
    bottom: Vec<Rect>,
    title: Option<Rect>,
    row_labels: Vec<(Rect, String)>,
    buttons: Vec<Button>,
}

impl Bars {
    fn push(&mut self, rect: Rect, label: impl Into<String>, target: Target, selected: bool) {
        self.buttons.push(Button {
            rect,
            label: label.into(),
            detail: None,
            selected,
            target,
            visual: Visual::Value,
        });
    }

    fn shortcut(&mut self, rect: Rect, label: &str, target: Target, selected: bool) {
        let visual = target.icon().map_or(Visual::Value, Visual::Shortcut);
        self.buttons.push(Button {
            rect,
            label: label.to_owned(),
            detail: None,
            selected,
            target,
            visual,
        });
    }

    fn group(&mut self, above: i32, items: Vec<Item>, large: bool, at_top: bool) {
        if items.is_empty() {
            return;
        }
        let count = items.len();
        let columns = count.min(4);
        let rows = count.div_ceil(columns);
        let row_height = if large { 180 } else { 170 };
        let height = rows as i32 * row_height + 20;
        let y = if at_top { above } else { above - height };
        let panel = Rect::new(0, y, WIDTH as i32, height);
        if at_top {
            self.top.push(panel);
        } else {
            self.bottom.push(panel);
        }
        for (index, item) in items.into_iter().enumerate() {
            let row = Rect::new(
                32,
                y + 10 + index as i32 / columns as i32 * row_height,
                panel.w - 64,
                row_height,
            );
            let row_columns = (count - (index / columns) * columns).min(columns);
            let rect = cell(row, index % columns, row_columns).inset(4);
            let visual = Visual::Tile(action_icon(&item.action));
            self.buttons.push(Button {
                rect,
                label: item.title,
                detail: item.detail,
                selected: false,
                target: if matches!(item.action, Action::SetReadingControls(_)) {
                    Target::Apply(item.action)
                } else {
                    Target::Invoke(item.action)
                },
                visual,
            });
        }
    }

    fn configure(
        &mut self,
        above: i32,
        large: bool,
        current: BarPanel,
        sections: &[(BarPanel, &str)],
        rows: Vec<ConfigRow>,
    ) {
        if rows.is_empty() {
            return;
        }
        let row_height = if large { 116 } else { 104 };
        let section_height = if sections.is_empty() {
            0
        } else if large {
            116
        } else {
            104
        };
        let height = rows.len() as i32 * row_height + section_height + 8;
        let width = WIDTH as i32;
        let panel = Rect::new(0, above - height, width, height);
        self.bottom.push(panel);
        if !sections.is_empty() {
            let row = Rect::new(32, panel.y + 2, width - 64, section_height);
            for (index, &(section, label)) in sections.iter().enumerate() {
                self.shortcut(
                    cell(row, index, sections.len()),
                    label,
                    Target::BottomSection(section),
                    section == current,
                );
            }
        }
        for (index, row) in rows.into_iter().enumerate() {
            let y = panel.y + section_height + 4 + index as i32 * row_height;
            let label_width = if row.label.is_empty() { 0 } else { 272 };
            if label_width != 0 {
                self.row_labels
                    .push((Rect::new(34, y, label_width - 18, row_height), row.label));
            }
            let options = Rect::new(16 + label_width, y, width - 32 - label_width, row_height);
            let count = row.choices.len();
            for (index, choice) in row.choices.into_iter().enumerate() {
                let target = if choice.selected {
                    Target::Current(choice.action)
                } else if choice.live {
                    Target::Apply(choice.action)
                } else {
                    Target::Invoke(choice.action)
                };
                let rect = cell(options, index, count);
                if !choice.live && !choice.selected {
                    self.shortcut(rect, &choice.title, target, false);
                } else {
                    self.push(rect, choice.title, target, choice.selected);
                }
            }
        }
    }

    fn button_at(&mut self, x: f32, y: f32) -> Option<Target> {
        let index = self
            .buttons
            .iter()
            .position(|button| button.rect.contains(x, y))?;
        Some(self.buttons.swap_remove(index).target)
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        self.top
            .iter()
            .chain(&self.bottom)
            .any(|rect| rect.contains(x, y))
    }

    fn visible(&self, height: u32) -> std::ops::Range<u32> {
        let top = self.top.last().map_or(0, |rect| rect.bottom() as u32);
        let bottom = self.bottom.last().map_or(height, |rect| rect.y as u32);
        top..bottom.max(top)
    }
}

fn cell(row: Rect, index: usize, count: usize) -> Rect {
    let x = row.x + row.w * index as i32 / count as i32;
    let right = row.x + row.w * (index as i32 + 1) / count as i32;
    Rect::new(x + 3, row.y + 2, right - x - 6, row.h - 4)
}

impl Reader {
    pub(super) fn swipe_display_control(&mut self, input: Input, start_x: f32) -> Option<Change> {
        let Input::Swipe { dx, dy, start_y } = input else {
            return None;
        };
        if self.touch_locked
            || self.top_group != Some(TopGroup::Settings)
            || dx.abs() < 48.0
            || dx.abs() <= dy.abs() * 1.5
        {
            return None;
        }
        let control = self.reading_bars().buttons.into_iter().find_map(|button| {
            if !button.rect.contains(start_x, start_y) {
                return None;
            }
            match button.target {
                Target::Invoke(Action::SetDisplayLevel(control, _))
                | Target::Apply(Action::SetDisplayLevel(control, _)) => Some(control),
                _ => None,
            }
        })?;
        Some(match self.quick_display.swipe(control, dx) {
            Ok(()) => self.applied_change(),
            Err(error) => self.report(error),
        })
    }

    fn reading_bars(&self) -> Bars {
        let mut bars = Bars::default();
        let large = self.controls.large_toolbar;
        let width = WIDTH as i32;
        let fixed = self.fixed.as_ref().map(|document| &document.reading);
        let has_scan = self.supports_scan_panel();

        if self.reading_top {
            let height = if large { 112 } else { 100 };
            bars.top.push(Rect::new(0, 0, width, height));
            bars.title = Some(Rect::new(190, 36, width - 709, height - 41));
            let top = |x, w| Rect::new(x, 0, w, height);
            for (rect, label, target) in [
                (
                    top(12, 168),
                    "Documents",
                    Target::Invoke(Action::Show(Screen::Library)),
                ),
                (
                    top(width - 500, 145),
                    "Pages",
                    Target::Toggle(BarPanel::Pages),
                ),
                (
                    top(width - 355, 189),
                    "Controls",
                    Target::Invoke(Action::Show(Screen::ReadingControls)),
                ),
                (top(width - 166, 154), "Exit", Target::Exit),
            ] {
                let selected =
                    matches!(target, Target::Toggle(panel) if self.reading_panel == Some(panel));
                bars.shortcut(rect, label, target, selected);
            }
            let tabs = Rect::new(0, height, width, if large { 112 } else { 104 });
            bars.top.push(tabs);
            for (index, group) in TopGroup::ALL.into_iter().enumerate() {
                bars.shortcut(
                    cell(tabs, index, TopGroup::ALL.len()),
                    group.label(),
                    Target::TopSection(group),
                    self.top_group == Some(group),
                );
            }
            if let Some(group) = self.top_group {
                bars.group(tabs.bottom(), self.top_config_items(group), large, true);
            }
        }

        if self.reading_bottom {
            let height = if large { 112 } else { 104 };
            let bottom = Rect::new(0, HEIGHT as i32 - height, width, height);
            bars.bottom.push(bottom);
            let fixed_book = fixed.is_some();
            let supports_text = fixed.is_none_or(|reading| reading.supports_text());
            let tabs = [
                Some((
                    "Contents",
                    Target::Invoke(Action::Show(if fixed_book {
                        Screen::FixedContents
                    } else {
                        Screen::Contents
                    })),
                )),
                Some((
                    "Bookmarks",
                    Target::Invoke(Action::Show(if fixed_book {
                        Screen::FixedBookmarks
                    } else {
                        Screen::Bookmarks
                    })),
                )),
                supports_text.then_some((
                    "Highlights",
                    Target::Invoke(Action::Show(Screen::Highlights)),
                )),
                supports_text.then_some(("Find", Target::Invoke(Action::Show(Screen::Find)))),
                Some(("Go to", Target::Invoke(Action::Show(Screen::GoToPage)))),
                Some(if fixed_book {
                    ("View", Target::Toggle(BarPanel::View))
                } else {
                    ("Text", Target::Toggle(BarPanel::Text))
                }),
                has_scan.then_some(("Scan", Target::Toggle(BarPanel::Scan))),
            ];
            let document_tabs = if has_scan { 7 } else { 6 };
            let columns = document_tabs + usize::from(!self.reading_top);
            for (index, tab) in tabs.into_iter().enumerate() {
                if let Some((label, target)) = tab {
                    let selected = matches!(target, Target::Toggle(panel) if self.reading_panel.map(BarPanel::tab) == Some(panel));
                    bars.shortcut(cell(bottom, index, columns), label, target, selected);
                }
            }
            if !self.reading_top {
                bars.shortcut(
                    cell(bottom, document_tabs, columns),
                    "Exit",
                    Target::Exit,
                    false,
                );
            }
        }

        if let Some(panel) = self.reading_panel {
            let above = bars
                .bottom
                .first()
                .map_or(PAGE_HEIGHT as i32, |rect| rect.y);
            if panel == BarPanel::Pages {
                bars.group(above, self.page_actions(), large, false);
            } else if self.reading_bottom {
                let sections: &[(BarPanel, &str)] = match panel.tab() {
                    BarPanel::Text if self.supports_word_spacing() => &TEXT_SECTIONS,
                    BarPanel::Text => &TEXT_SECTIONS[..TEXT_SECTIONS.len() - 1],
                    BarPanel::View if self.supports_view_layout() => &VIEW_SECTIONS,
                    BarPanel::View => &SINGLE_VIEW_SECTIONS,
                    _ => &[],
                };
                bars.configure(
                    above,
                    large,
                    panel,
                    sections,
                    self.bottom_config_rows(panel),
                );
            }
        }
        bars
    }

    #[cfg(test)]
    pub(super) fn reading_button_in_row(&self, row_label: &str, button_label: &str) -> Rect {
        let bars = self.reading_bars();
        let row = bars
            .row_labels
            .iter()
            .find(|(_, label)| label == row_label)
            .unwrap_or_else(|| panic!("No reading settings row {row_label}"))
            .0;
        bars.buttons
            .iter()
            .find(|button| {
                button.label == button_label
                    && (row.y..row.y + row.h).contains(&(button.rect.y + button.rect.h / 2))
            })
            .unwrap_or_else(|| panic!("No {button_label} choice in {row_label}"))
            .rect
    }

    fn page_actions(&self) -> Vec<Item> {
        let mut items = vec![
            Item::new("Thumbnails", Action::Overview),
            Item::new("Skim", Action::Skim),
            Item::new("Add bookmark", Action::AddBookmark),
        ];
        if self
            .active
            .as_ref()
            .is_some_and(|active| !active.reading.links().is_empty())
        {
            items.push(Item::new("Links", Action::Show(Screen::Links)));
        }
        if !self.back.is_empty() {
            items.push(Item::new("History", Action::Show(Screen::LocationHistory)));
            items.push(Item::new("Previous location", Action::ReturnToLocation));
        }
        if !self.hits.is_empty() || !self.pdf_hits.is_empty() {
            items.push(Item::new("Matches", Action::Show(Screen::Matches)));
        }
        items
    }

    pub(super) fn draw_reading_tools(&self, canvas: &mut Canvas) {
        let bars = self.reading_bars();
        for (edge_top, regions) in [(true, &bars.top), (false, &bars.bottom)] {
            for rect in regions {
                canvas.fill(*rect, 255);
                let y = if edge_top { rect.bottom() - 2 } else { rect.y };
                canvas.fill(Rect::new(rect.x, y, rect.w, 2), HAIRLINE);
            }
        }

        let size = if self.controls.large_toolbar { 34 } else { 28 };
        if let Some(title_rect) = bars.title {
            let title = self
                .active
                .as_ref()
                .map(|book| book.reading.probe.title.as_str())
                .or_else(|| self.fixed.as_ref().map(|book| book.entry.title.as_str()))
                .unwrap_or("Reader");
            canvas.text(title_rect, self.notice.as_deref().unwrap_or(title), CAPTION);
        }
        for (rect, label) in bars.row_labels {
            canvas.text(
                Rect::new(rect.x, rect.y + (rect.h - 62) / 2, rect.w, 62),
                &label,
                CAPTION.lines(2),
            );
        }
        for button in bars.buttons {
            button.draw(canvas, size);
        }
    }

    pub(super) fn page_paint(&self) -> PagePaint {
        let inverted = self.uses_night_pages();
        let height = if self.overlapping_book_page() {
            HEIGHT
        } else {
            PAGE_HEIGHT
        };
        let visible = if matches!(self.screen, Screen::BookTools | Screen::FixedTools) {
            self.reading_bars().visible(height)
        } else {
            0..height
        };
        let header_height = if self.top_header_visible() {
            crate::reading_controls::TopHeader::HEIGHT
        } else {
            0
        };
        PagePaint {
            visible,
            header_height,
            inverted,
            highlight_appearance: self.controls.highlight_appearance,
            note_marker: self.controls.note_marker,
            selection_color: (self.controls.selection_uses_highlight_color
                && self.effective_highlight_style()
                    != crate::highlight_style::HighlightStyle::Invert)
                .then(|| self.effective_highlight_color())
                .filter(|color| !color.is_default()),
        }
    }

    pub(super) fn tools_screen(&self) -> Screen {
        if self.fixed.is_some() {
            Screen::FixedTools
        } else {
            Screen::BookTools
        }
    }

    pub(super) fn surface_screen(&self) -> Screen {
        if self.fixed.is_some() {
            Screen::Fixed
        } else {
            Screen::Book
        }
    }

    fn last_bottom_panel(&self) -> BarPanel {
        if self.fixed.is_some() {
            match self.last_fixed_panel {
                BarPanel::Scan if !self.supports_scan_panel() => BarPanel::View,
                BarPanel::ViewLayout if !self.supports_view_layout() => BarPanel::View,
                panel => panel,
            }
        } else if self.last_text_panel == BarPanel::TextSpacing && !self.supports_word_spacing() {
            BarPanel::Text
        } else {
            self.last_text_panel
        }
    }

    pub(super) fn restore_reading_menus(&mut self, id: u64) {
        let menu = self
            .library
            .state(id)
            .map(|state| state.menus)
            .unwrap_or_default();
        self.last_top_group = menu.top.unwrap_or(TopGroup::Navigation);
        self.last_text_panel = menu
            .bottom
            .filter(|p| p.tab() == BarPanel::Text)
            .unwrap_or(BarPanel::Text);
        self.last_fixed_panel = menu
            .bottom
            .filter(|p| matches!(p.tab(), BarPanel::View | BarPanel::Scan))
            .unwrap_or(BarPanel::View);
    }

    fn remember_menu(&mut self, update: impl FnOnce(&mut crate::book_menus::BookMenus)) {
        let state = if let Some(fixed) = &self.fixed {
            self.library
                .remember(&fixed.entry, fixed.id, &fixed.entry.title)
        } else if let Some(book) = &self.active {
            self.library
                .remember(&book.entry, book.id, &book.reading.probe.title)
        } else {
            return;
        };
        let before = state.menus;
        update(&mut state.menus);
        if state.menus != before {
            self.mark_book_changed();
        }
    }

    fn remember_group(&mut self, group: TopGroup) {
        self.last_top_group = group;
        self.remember_menu(|menu| menu.top = Some(group));
    }

    fn remember_bottom_panel(&mut self, panel: BarPanel) {
        match panel.tab() {
            BarPanel::Text => self.last_text_panel = panel,
            BarPanel::View | BarPanel::Scan => self.last_fixed_panel = panel,
            _ => return,
        }
        self.remember_menu(|menu| menu.bottom = Some(panel));
    }

    pub(super) fn open_reading_edge(&mut self, from_top: bool, x: f32) -> Change {
        let width = WIDTH as f32;
        self.show(self.tools_screen());
        self.reading_top = from_top;
        self.reading_bottom = !from_top || self.controls.both_toolbars;
        if from_top {
            let group = if x < width / 3.0 {
                TopGroup::ALL[0]
            } else if x > width * 2.0 / 3.0 {
                TopGroup::ALL[TopGroup::ALL.len() - 1]
            } else {
                self.last_top_group
            };
            self.top_group = Some(group);
            self.remember_group(group);
        }
        if self.reading_bottom {
            self.reading_panel = Some(self.last_bottom_panel());
        }
        Change::Partial
    }

    pub(super) fn reveal_reading_tools(&mut self, x: f32, y: f32, screen: Screen) -> bool {
        if self.controls.menu_activation == crate::reading_controls::MenuActivation::Swipe {
            return false;
        }
        let middle = WIDTH as f32 / 2.0;
        if (x - middle).abs() <= 150.0
            && (HEIGHT as f32 * 0.8..HEIGHT as f32 * 12.0 / 13.0).contains(&y)
        {
            // The lower menu strip stops where the status footer takes over.
            self.show(screen);
            self.reading_panel = Some(self.last_bottom_panel());
            true
        } else if (WIDTH as f32 / 3.0..=WIDTH as f32 * 2.0 / 3.0).contains(&x)
            && (0.0..PAGE_HEIGHT as f32 * 0.55).contains(&y)
        {
            self.show(screen);
            true
        } else {
            false
        }
    }

    fn apply_overlay_action(&mut self, action: Action) -> Change {
        let (screen, top, bottom, panel, group) = (
            self.screen,
            self.reading_top,
            self.reading_bottom,
            self.reading_panel,
            self.top_group,
        );
        match self.activate(action) {
            Ok(()) => {
                if self.screen == self.surface_screen() {
                    self.screen = screen;
                    self.reading_top = top;
                    self.reading_bottom = bottom;
                    self.reading_panel = panel;
                    self.top_group = group;
                    let now = Instant::now();
                    self.update_stats_activity(now);
                    self.sync_auto_turn(now);
                }
                self.applied_change()
            }
            Err(error) => self.report(error),
        }
    }

    pub(super) fn reading_tools_input(&mut self, input: Input) -> Change {
        let reading_screen = self.surface_screen();
        match input {
            Input::LongTap(x, y) => match self.reading_bars().button_at(x, y) {
                Some(Target::Invoke(Action::ReadAlarm)) => self.read_alarm.stop(),
                Some(
                    Target::Apply(Action::TopStatusHeader(enabled))
                    | Target::Current(Action::TopStatusHeader(enabled)),
                ) => self.apply_overlay_action(Action::ConfirmTopStatusHeaderDefault(enabled)),
                _ => Change::None,
            },
            Input::Tap(x, y) => {
                let mut bars = self.reading_bars();
                match bars.button_at(x, y) {
                    Some(Target::Exit) => self.exit(),
                    Some(Target::Toggle(panel)) => {
                        self.top_group = None;
                        self.reading_panel =
                            (self.reading_panel.map(BarPanel::tab) != Some(panel)).then_some(panel);
                        if let Some(panel) = self.reading_panel {
                            self.remember_bottom_panel(panel);
                        }
                        Change::Partial
                    }
                    Some(Target::TopSection(group)) => {
                        self.reading_panel = None;
                        self.top_group = (self.top_group != Some(group)).then_some(group);
                        self.remember_group(group);
                        Change::Partial
                    }
                    Some(Target::BottomSection(panel)) => {
                        self.reading_panel = Some(panel);
                        self.remember_bottom_panel(panel);
                        Change::Partial
                    }
                    Some(Target::Apply(action)) => self.apply_overlay_action(action),
                    Some(Target::Invoke(action)) => match self.activate(action) {
                        Ok(()) => self.applied_change(),
                        Err(error) => self.report(error),
                    },
                    Some(Target::Current(_)) => Change::None,
                    None if !bars.contains(x, y) => {
                        let bottom_was_open = self.reading_panel.take().is_some();
                        let top_was_open = self.top_group.take().is_some();
                        if !bottom_was_open && !top_was_open {
                            self.show(reading_screen);
                        }
                        Change::Partial
                    }
                    None => Change::None,
                }
            }
            Input::PageBack | Input::PageForward | Input::Swipe { .. } => {
                self.show(reading_screen);
                match self.input(input) {
                    Change::None => Change::Partial,
                    change => change,
                }
            }
            _ => Change::None,
        }
    }

    #[cfg(test)]
    pub(super) fn reading_button(&self, label: &str) -> Rect {
        self.reading_bars()
            .buttons
            .into_iter()
            .find(|button| button.label == label)
            .unwrap_or_else(|| panic!("Missing reading control {label}"))
            .rect
    }

    #[cfg(test)]
    pub(super) fn screen_row_labels(&self) -> Vec<String> {
        self.bottom_config_rows(BarPanel::TextScreen)
            .into_iter()
            .map(|row| row.label)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{fs, path::PathBuf};

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tap(app: &mut Reader, label: &str) -> Change {
        let rect = app.reading_button(label);
        app.input(Input::Tap(
            (rect.x + rect.w / 2) as f32,
            (rect.y + rect.h / 2) as f32,
        ))
    }

    #[test]
    fn compact_and_large_shortcuts_keep_complete_labels_and_the_reading_anchor() {
        let fixture = crate::app::tests::Fixture::new();
        let pdf = fixture.root.join("Harbor chart.pdf");
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let fonts = std::env::var_os("READER_TOOLBAR_REVIEW_FONTS")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_else(|| vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")]);
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let font_paths: Vec<_> = fonts.iter().map(|p| p.display().to_string()).collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let mut layouts = Vec::new();
        for (format, file) in [("epub", &fixture.book), ("pdf", &pdf)] {
            app.open_path(file).unwrap();
            let origin = app.current_history_location();
            for large in [false, true] {
                app.controls.large_toolbar = large;
                app.open_reading_edge(false, 500.0);
                assert!(app.reading_bottom && !app.reading_top);
                let bars = app.reading_bars();
                let buttons: Vec<_> = bars
                    .buttons
                    .iter()
                    .filter(|button| button.rect.y >= HEIGHT as i32 - 112)
                    .map(|button| {
                        serde_json::json!({
                            "label":button.label,
                            "x":button.rect.x,
                            "y":button.rect.y,
                            "w":button.rect.w,
                            "h":button.rect.h,
                        })
                    })
                    .collect();
                assert!(buttons.len() >= 7);
                if let Some(directory) = std::env::var_os("READER_TOOLBAR_REVIEW") {
                    for dark in [false, true] {
                        let file = format!(
                            "{format}-{}{}.png",
                            if large { "large" } else { "compact" },
                            if dark { "-dark" } else { "" }
                        );
                        let path = PathBuf::from(&directory).join(&file);
                        let image = primitive_app::frame(&app, &mut canvas, dark);
                        primitive_app::write_png(&path, WIDTH, HEIGHT, &image).unwrap();
                        layouts
                            .push(serde_json::json!({"file":file,"dark":dark,"buttons":buttons}));
                    }
                }
                tap(&mut app, "Bookmarks");
                assert!(matches!(
                    app.screen,
                    Screen::Bookmarks | Screen::FixedBookmarks
                ));
                assert_eq!(app.current_history_location(), origin);
                let back = primitive_ui::back_button(WIDTH as i32);
                app.input(Input::Tap((back.x + 30) as f32, (back.y + 30) as f32));
                app.show(if app.fixed.is_some() {
                    Screen::Fixed
                } else {
                    Screen::Book
                });
            }
        }
        if let Some(directory) = std::env::var_os("READER_TOOLBAR_REVIEW") {
            fs::write(
                PathBuf::from(directory).join("controls.json"),
                serde_json::to_vec_pretty(&layouts).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn top_header_lower_config_keeps_book_choice_default_and_footer_independent() {
        use crate::app::tests::{Fixture, capture_test_screen};
        use crate::reading_controls::TopHeader;
        use primitive_ui::SheetAction;

        fn choice(app: &Reader, default: bool, requested: bool) -> (f32, f32) {
            let rect = app
                .reading_bars()
                .buttons
                .into_iter()
                .find(|button| match &button.target {
                    Target::Apply(action) | Target::Current(action) => match action {
                        Action::TopStatusHeader(enabled) => !default && *enabled == requested,
                        Action::ConfirmTopStatusHeaderDefault(enabled) => {
                            default && *enabled == requested
                        }
                        _ => false,
                    },
                    _ => false,
                })
                .unwrap()
                .rect;
            ((rect.x + rect.w / 2) as f32, (rect.y + rect.h / 2) as f32)
        }

        let fixture = Fixture::new();
        let second = fixture.root.join("Sea Wall.epub");
        let third = fixture.root.join("Two Columns.epub");
        let pdf = fixture.root.join("Harbor chart.pdf");
        fs::write(
            &second,
            include_bytes!("../../tests/fixtures/long-chapter.epub"),
        )
        .unwrap();
        fs::write(
            &third,
            include_bytes!("../../tests/fixtures/many-chapters.epub"),
        )
        .unwrap();
        fs::write(&pdf, crate::fixed::tests::document()).unwrap();
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts.clone()).unwrap();
        let system_settings = fixture.root.join("launcher-settings.txt");
        fs::write(&system_settings, "dark_mode=off\n").unwrap();
        app.system_theme = Some(crate::app::SystemTheme::new(system_settings));
        app.open_path(&fixture.book).unwrap();
        app.activate(Action::Jump(crate::epub::Destination {
            path: "EPUB/chapters/dusk.xhtml".into(),
            fragment: Some("door".into()),
        }))
        .unwrap();
        let initial = app.current_history_location();
        let footer_mode = app.controls.status_bar;
        let footer = app.status_caption();
        let font_paths: Vec<_> = fonts
            .iter()
            .map(|font| font.display().to_string())
            .collect();
        let mut canvas = Canvas::new(WIDTH, HEIGHT, &font_paths).unwrap();
        let footer_start = (WIDTH * PAGE_HEIGHT) as usize;
        let old_frame = primitive_app::frame(&app, &mut canvas, false);
        assert!(app.top_header_caption().is_none());
        assert_eq!(
            app.swipe(
                Input::Swipe {
                    dx: 0.0,
                    dy: -120.0,
                    start_y: HEIGHT as f32 - 4.0
                },
                500.0
            ),
            Change::Partial
        );
        assert_eq!(tap(&mut app, "Screen"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::TextScreen));
        let (on_x, on_y) = choice(&app, false, true);
        assert_eq!(app.input(Input::Tap(on_x, on_y)), Change::Partial);
        assert_eq!(app.current_history_location(), initial);
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(app.reading_panel, Some(BarPanel::TextScreen));
        assert!(app.top_header_caption().is_some());
        assert!(!app.controls.top_header.default_enabled());
        capture_test_screen(
            &app,
            &fonts,
            "READER_TOP_HEADER_BOOK_CONFIG_SCREENSHOT",
            false,
        );

        assert_eq!(app.input(Input::LongTap(on_x, on_y)), Change::Partial);
        assert_eq!(app.top_header_default_prompt, Some(true));
        capture_test_screen(
            &app,
            &fonts,
            "READER_TOP_HEADER_DEFAULT_PROMPT_SCREENSHOT",
            false,
        );
        assert_eq!(app.input(Input::Tap(10.0, 10.0)), Change::Partial);
        assert!(!app.controls.top_header.default_enabled());
        assert!(app.top_header_caption().is_some());
        let (default_x, default_y) = choice(&app, true, true);
        assert_eq!(app.input(Input::Tap(default_x, default_y)), Change::Partial);
        let confirm = Reader::top_header_default_sheet(true).button(SheetAction::Confirm);
        assert_eq!(
            app.input(Input::Tap((confirm.x + 10) as f32, (confirm.y + 10) as f32)),
            Change::Partial
        );
        assert!(app.controls.top_header.default_enabled());
        assert_eq!(app.reading_panel, Some(BarPanel::TextScreen));
        assert_eq!(app.input(Input::Tap(20.0, 400.0)), Change::Partial);
        assert_eq!(app.input(Input::Tap(20.0, 400.0)), Change::Partial);
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(app.current_history_location(), initial);
        capture_test_screen(&app, &fonts, "READER_TOP_HEADER_BOOK_SCREENSHOT", false);
        assert_eq!(app.controls.status_bar, footer_mode);
        assert_eq!(app.status_caption(), footer);
        let frame = primitive_app::frame(&app, &mut canvas, false);
        assert_eq!(
            &frame[footer_start..],
            &old_frame[footer_start..],
            "enabling the header must preserve the whole footer, including Exit"
        );

        let regular_controls = app.controls;
        app.activate(Action::SetReadingControls(
            crate::reading_controls::ReadingControls {
                status_bar: crate::reading_controls::StatusBar::Minimal,
                overlap_footer: true,
                ..regular_controls
            },
        ))
        .unwrap();
        let hidden_frame = primitive_app::frame(&app, &mut canvas, false);
        let exit = crate::session::DONE;
        assert!((exit.y..exit.bottom()).any(|y| {
            let row = y as usize * WIDTH as usize;
            hidden_frame[row + exit.x as usize..row + exit.right() as usize].contains(&0)
        }));
        app.activate(Action::SetReadingControls(regular_controls))
            .unwrap();

        app.open_path(&second).unwrap();
        assert!(app.top_header_caption().is_some());
        app.open_reading_edge(false, 500.0);
        if app.reading_panel != Some(BarPanel::TextScreen) {
            tap(&mut app, "Screen");
        }
        let (off_x, off_y) = choice(&app, false, false);
        assert_eq!(app.input(Input::Tap(off_x, off_y)), Change::Partial);
        assert!(app.top_header_caption().is_none());
        app.save().unwrap();
        let mut app = Reader::open(&fixture.root, &fixture.state, fonts).unwrap();
        app.open_path(&second).unwrap();
        assert!(app.top_header_caption().is_none());
        assert!(app.controls.top_header.default_enabled());
        app.open_path(&fixture.book).unwrap();
        assert!(app.top_header_caption().is_some());
        app.activate(Action::ResetText).unwrap();
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .status_header_height,
            TopHeader::HEIGHT
        );
        app.open_path(&third).unwrap();
        assert!(app.top_header_caption().is_some());
        app.open_path(&pdf).unwrap();
        assert!(app.top_header_caption().is_none());
        assert!(
            app.bottom_config_rows(BarPanel::ViewScreen)
                .iter()
                .all(|row| { !row.label.contains("header") && !row.label.contains("new books") })
        );
    }

    #[test]
    fn live_text_spacing_keeps_its_panel_open_and_reopens_with_saved_preferences() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = Scratch(std::env::temp_dir().join(format!(
            "reader-controls-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        )));
        let documents = root.0.join("documents");
        fs::create_dir_all(&documents).unwrap();
        let book = documents.join("Sea wall survey.xhtml");
        fs::write(
            &book,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Sea wall survey</title></head>
            <body><h1>Sea wall survey</h1>
            <p style="text-align:justify">At low tide, we marked the line where the sandstone met the wall.
            The survey pole followed the curve of the shore. Two narrow channels carried water back to the bay.</p>
            <p style="text-align:justify">The next observation was made beside the western pier.
            A thin layer of salt covered the dry stones, and a pool near the cliff still reflected the sky.</p>
            </body></html>"#,
        ).unwrap();
        let state = root.0.join("reading.json");
        let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let mut app = Reader::open(&documents, &state, fonts.clone()).unwrap();
        app.open_path(&book).unwrap();

        assert_eq!(app.open_reading_edge(false, 500.0), Change::Partial);
        assert!(!app.reading_top && app.reading_bottom);
        assert_eq!(tap(&mut app, "Spacing"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
        assert_eq!(tap(&mut app, "Small"), Change::Partial);
        assert_eq!(app.screen, Screen::BookTools);
        assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
        assert!(!app.reading_top && app.reading_bottom);
        assert_eq!(
            app.active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .percent_word_spacing,
            Some(crate::WordSpacePercent::SMALL)
        );

        assert_eq!(tap(&mut app, "Some"), Change::Partial);
        assert_eq!(tap(&mut app, "+1%"), Change::Partial);
        assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
        crate::app::tests::capture_test_screen(&app, &fonts, "READER_WORD_EXPANSION_SCREEN", false);
        app.open_reading_edge(false, 500.0);
        assert_eq!(app.reading_panel, Some(BarPanel::TextSpacing));
        app.save().unwrap();
        let mut restored = Reader::open(&documents, &state, fonts).unwrap();
        restored.open_path(&book).unwrap();
        assert_eq!(
            restored
                .active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .word_expansion,
            6
        );
        assert_eq!(
            restored
                .active
                .as_ref()
                .unwrap()
                .reading
                .config
                .settings
                .percent_word_spacing,
            Some(crate::WordSpacePercent::SMALL)
        );
    }
}

#[cfg(test)]
#[path = "cjk_width_tests.rs"]
mod cjk_width_tests;

#[cfg(test)]
#[path = "reflow_margin_tests.rs"]
mod margin_tests;

#[cfg(test)]
#[path = "render_dpi_tests.rs"]
mod render_dpi_tests;

#[cfg(test)]
#[path = "kerning_modes_tests.rs"]
mod kerning_modes_tests;
