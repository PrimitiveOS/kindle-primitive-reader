//! Search the current book's reading controls without applying a match.
use std::io::Read;
use std::path::{Path, PathBuf};

use primitive_ui::{Canvas, Rect, Sheet, SheetAction};

use super::reading_bars::BarPanel;
use super::top_config::TopGroup;
use super::{Action, Change, Editor, Input, Item, Reader, Screen};
use crate::Result;

const QUERY_LIMIT: usize = 96;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Top(TopGroup),
    Bottom(BarPanel),
    List(Screen),
}

#[derive(Clone, Copy)]
struct Destination {
    menu: Menu,
    ordinal: usize,
}

struct Hit {
    destination: Destination,
    title: String,
    breadcrumb: String,
}

#[derive(Clone, Copy)]
struct Origin {
    book: u64,
    toolbar: Screen,
    top: bool,
    bottom: bool,
    group: Option<TopGroup>,
    panel: Option<BarPanel>,
}

pub(super) struct Search {
    path: PathBuf,
    query: String,
    origin: Option<Origin>,
    hits: Vec<Hit>,
    preview: Option<usize>,
    focus: Option<Destination>,
}

impl Search {
    pub(super) fn open(state: &Path) -> Self {
        let path = state.with_file_name("menu-search.txt");
        let mut query = String::new();
        if let Ok(file) = std::fs::File::open(&path) {
            let mut data = file.take((QUERY_LIMIT * 4) as u64 + 1);
            if data.read_to_string(&mut query).is_err()
                || query.chars().count() > QUERY_LIMIT
                || query.chars().any(char::is_control)
            {
                query.clear();
            }
        }
        Self {
            path,
            query,
            origin: None,
            hits: Vec::new(),
            preview: None,
            focus: None,
        }
    }

    pub(super) fn clear_overlays(&mut self) {
        self.preview = None;
        self.focus = None;
    }

    pub(super) fn items(&self) -> Vec<Item> {
        self.hits
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                Item::new(&hit.title, Action::PreviewReadingMenu(index)).detail(&hit.breadcrumb)
            })
            .collect()
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn preview(&mut self, index: usize) -> Result<()> {
        self.hits
            .get(index)
            .ok_or("That menu entry is unavailable")?;
        self.preview = Some(index);
        Ok(())
    }

    fn focused(&self, menu: Menu) -> Option<usize> {
        self.focus
            .filter(|focus| focus.menu == menu)
            .map(|focus| focus.ordinal)
    }

    pub(super) fn focused_top(&self, group: TopGroup) -> Option<usize> {
        self.focused(Menu::Top(group))
    }

    pub(super) fn focused_bottom(&self, panel: BarPanel) -> Option<usize> {
        self.focused(Menu::Bottom(panel))
    }

    pub(super) fn focused_list(&self, screen: Screen) -> Option<usize> {
        self.focused(Menu::List(screen))
    }
}

pub(super) fn mark_control(canvas: &mut Canvas, rect: Rect) {
    // Mark the control's edge without repainting its label, icon or touch target.
    canvas.fill(Rect::new(rect.x + 5, rect.y + 12, 6, rect.h - 24), 0);
}

/// List screens that represent control submenus and can be generated from the
/// open book. Page lists, user content, editors and confirmations are leaves.
fn child_menu(action: &Action) -> Option<Screen> {
    match *action {
        Action::Show(
            screen @ (Screen::ReadingNotifications
            | Screen::NavigationSettings
            | Screen::BackOptions(_)
            | Screen::PageTurns
            | Screen::PageTurnZones
            | Screen::PageTurnDefaults
            | Screen::ReadingGuides
            | Screen::ContentsSettings
            | Screen::ChapterNavigation
            | Screen::PageNumbering
            | Screen::NumberingDefaults
            | Screen::AutoTurn
            | Screen::AutoTurnDistance
            | Screen::AutoTurnPan
            | Screen::Gestures
            | Screen::GestureBinding(_)
            | Screen::Scrolling
            | Screen::BookmarkListSettings
            | Screen::AnnotationListSettings
            | Screen::AnnotationExchange
            | Screen::SelectionPosition
            | Screen::LongPressSettings
            | Screen::VeryLongPressSettings
            | Screen::HighlightPromptSettings
            | Screen::FooterSettings
            | Screen::FooterItemWidths
            | Screen::HeaderBatterySettings
            | Screen::FooterPresets
            | Screen::EndActionSettings
            | Screen::MetadataSaveSettings
            | Screen::LinkSettings
            | Screen::StyleTweaks
            | Screen::ParagraphSpacing
            | Screen::LinkStyles
            | Screen::FootnoteTweaks
            | Screen::InPageFootnoteSize(_)
            | Screen::ImageStyles
            | Screen::TableStyles
            | Screen::PseudoElementStyles
            | Screen::ListItemStyles
            | Screen::NormalLineHeightStyles
            | Screen::TypographyRules
            | Screen::TypographyLanguages(_)
            | Screen::FontRendering(_)
            | Screen::PublisherRenderDefaults
            | Screen::HyphenationOptions
            | Screen::HyphenationDefaults
            | Screen::SearchSettings
            | Screen::TranslationSettings
            | Screen::WikipediaSettings),
        ) => Some(screen),
        Action::ShowDictionaries => Some(Screen::Dictionaries),
        _ => None,
    }
}

struct Catalog<'a> {
    reader: &'a Reader,
    term: &'a str,
    hits: Vec<Hit>,
}

impl Catalog<'_> {
    fn walk(&mut self, menu: Menu, breadcrumb: &str, visited: &mut Vec<Screen>) {
        let items = match menu {
            Menu::Top(group) => self.reader.top_config_items(group),
            Menu::Bottom(panel) => self.reader.bottom_menu_items(panel),
            Menu::List(screen) => self.reader.items_for(screen),
        };
        for (ordinal, item) in items.into_iter().enumerate() {
            if item.title.is_empty() || matches!(item.action, Action::SearchReadingMenus) {
                continue;
            }
            let path = format!("{breadcrumb} › {}", item.title);
            // Localized top actions and configuration rows retain English
            // aliases without duplicate hits or altered destination ordinals.
            let english_alias: std::borrow::Cow<'_, str> = match menu {
                Menu::List(
                    Screen::SelectionPosition
                    | Screen::LongPressSettings
                    | Screen::VeryLongPressSettings
                    | Screen::HighlightPromptSettings,
                ) => self
                    .reader
                    .controls
                    .interface_language
                    .selection_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::BookmarkListSettings | Screen::AnnotationListSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .bookmark_list_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::StyleTweaks) => self
                    .reader
                    .controls
                    .interface_language
                    .bottom_typography_english(&item.title)
                    .into(),
                Menu::List(Screen::MetadataSaveSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .metadata_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::AnnotationExchange) => self
                    .reader
                    .controls
                    .interface_language
                    .annotation_exchange_english(&item.title)
                    .into(),
                Menu::List(Screen::FontRendering(_) | Screen::PublisherRenderDefaults) => self
                    .reader
                    .controls
                    .interface_language
                    .typography_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::Scrolling) => self
                    .reader
                    .controls
                    .interface_language
                    .scrolling_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::NavigationSettings | Screen::BackOptions(_)) => self
                    .reader
                    .controls
                    .interface_language
                    .navigation_settings_english(&item.title)
                    .into(),
                Menu::List(
                    Screen::PageTurns | Screen::PageTurnZones | Screen::PageTurnDefaults,
                ) => self
                    .reader
                    .controls
                    .interface_language
                    .page_turn_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::LinkSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .link_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::ReadingNotifications) => self
                    .reader
                    .controls
                    .interface_language
                    .notification_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::ReadingGuides | Screen::ReadingGuidesAbout) => self
                    .reader
                    .controls
                    .interface_language
                    .reading_guide_english(&item.title)
                    .into(),
                Menu::List(Screen::PageNumbering | Screen::NumberingDefaults) => self
                    .reader
                    .controls
                    .interface_language
                    .numbering_english(&item.title)
                    .into(),
                Menu::List(Screen::ContentsSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .contents_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::ChapterNavigation) => self
                    .reader
                    .controls
                    .interface_language
                    .chapter_navigation_english(&item.title)
                    .into(),
                Menu::List(Screen::SearchSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .search_settings_english(&item.title)
                    .into(),
                Menu::List(Screen::EndActionSettings) => self
                    .reader
                    .controls
                    .interface_language
                    .end_document_english(&item.title)
                    .into(),
                Menu::Bottom(BarPanel::Pages) => self
                    .reader
                    .controls
                    .interface_language
                    .page_action_english(&item.title)
                    .into(),
                Menu::Bottom(
                    BarPanel::Text
                    | BarPanel::TextMargins
                    | BarPanel::TextSpacing
                    | BarPanel::TextRendering
                    | BarPanel::View
                    | BarPanel::Scan,
                ) => self
                    .reader
                    .controls
                    .interface_language
                    .other_bottom_english(&item.title)
                    .into(),
                Menu::Bottom(BarPanel::TextLayout) => self
                    .reader
                    .controls
                    .interface_language
                    .text_layout_english(&item.title)
                    .into(),
                Menu::Bottom(BarPanel::ViewLayout) => self
                    .reader
                    .controls
                    .interface_language
                    .view_layout_english(&item.title)
                    .into(),
                Menu::Bottom(BarPanel::TextStyle) => self
                    .reader
                    .controls
                    .interface_language
                    .text_style_english(&item.title)
                    .into(),
                Menu::Top(TopGroup::Appearance | TopGroup::Settings | TopGroup::Tools) => self
                    .reader
                    .controls
                    .interface_language
                    .top_control_english(&item.title)
                    .into(),
                Menu::Top(TopGroup::Library) => self
                    .reader
                    .controls
                    .interface_language
                    .library_english(&item.title)
                    .into(),
                Menu::Top(TopGroup::Search) => self
                    .reader
                    .controls
                    .interface_language
                    .search_english(&item.title)
                    .into(),
                Menu::Top(TopGroup::Navigation) => self
                    .reader
                    .controls
                    .interface_language
                    .navigation_english(&item.title)
                    .into(),
                Menu::Bottom(BarPanel::TextScreen | BarPanel::ViewScreen) => self
                    .reader
                    .controls
                    .interface_language
                    .screen_row_english(&item.title)
                    .into(),
                _ => item.title.as_str().into(),
            };
            if item.title.to_lowercase().contains(self.term)
                || english_alias.to_lowercase().contains(self.term)
            {
                self.hits.push(Hit {
                    destination: Destination { menu, ordinal },
                    title: item.title,
                    breadcrumb: path.clone(),
                });
            }
            if let Some(screen) = child_menu(&item.action)
                && visited.len() < 9
                && !visited.contains(&screen)
            {
                visited.push(screen);
                self.walk(Menu::List(screen), &path, visited);
                visited.pop();
            }
        }
    }
}

impl Reader {
    pub(super) fn begin_menu_search(&mut self) -> Result<()> {
        let book = self
            .open_book_id()
            .ok_or("Open a book to search its reading menus")?;
        self.menu_search.origin = Some(Origin {
            book,
            toolbar: self.tools_screen(),
            top: self.reading_top,
            bottom: self.reading_bottom,
            group: self.top_group,
            panel: self.reading_panel,
        });
        self.menu_search.hits.clear();
        self.edit_menu_search();
        Ok(())
    }

    pub(super) fn edit_menu_search(&mut self) {
        self.show(Screen::MenuSearch);
        self.editor = Editor::new(QUERY_LIMIT);
        self.editor.text = self.menu_search.query.chars().collect();
        self.editor.caret = self.editor.text.len();
    }

    pub(super) fn leave_menu_search(&mut self) {
        let origin = self
            .menu_search
            .origin
            .take()
            .filter(|origin| self.open_book_id() == Some(origin.book));
        let toolbar = origin.map_or_else(|| self.tools_screen(), |origin| origin.toolbar);
        self.show(toolbar);
        if let Some(origin) = origin {
            self.reading_top = origin.top;
            self.reading_bottom = origin.bottom;
            self.top_group = origin.group;
            self.reading_panel = origin.panel;
        }
        self.menu_search.hits.clear();
    }

    pub(super) fn submit_menu_search(&mut self, query: &str) -> Result<()> {
        let query = query.trim();
        if query.is_empty() {
            return Err("Enter a menu name".into());
        }
        if query.chars().any(char::is_control) || query.chars().count() > QUERY_LIMIT {
            return Err("Enter a short menu name on one line".into());
        }
        let term = query.to_lowercase();
        let language = self.controls.interface_language;
        let mut catalog = Catalog {
            reader: self,
            term: &term,
            hits: Vec::new(),
        };
        for group in TopGroup::ALL {
            catalog.walk(
                Menu::Top(group),
                language.text(group.label()),
                &mut Vec::new(),
            );
        }
        for (panel, label) in self.menu_search_panels() {
            catalog.walk(Menu::Bottom(panel), &label, &mut Vec::new());
        }
        self.menu_search.hits = catalog.hits;
        self.menu_search.query = query.to_owned();
        primitive_app::save(&self.menu_search.path, query.as_bytes())?;
        self.show(Screen::MenuSearchResults);
        Ok(())
    }

    fn open_menu_search_hit(&mut self, index: usize) -> Result<()> {
        if self.menu_search.origin.map(|origin| origin.book) != self.open_book_id() {
            return Err("The open book changed. Search its menus again.".into());
        }
        let hit = self
            .menu_search
            .hits
            .get(index)
            .ok_or("That menu entry is unavailable")?;
        let destination = hit.destination;
        let expected_title = hit.title.clone();
        if destination.menu == Menu::List(Screen::Dictionaries) {
            self.refresh_installed_dictionaries()?;
        }
        let rows = match destination.menu {
            Menu::Top(group) => self.top_config_items(group),
            Menu::Bottom(panel) => self.bottom_menu_items(panel),
            Menu::List(screen) => self.items_for(screen),
        };
        if rows
            .get(destination.ordinal)
            .is_none_or(|item| item.title != expected_title)
        {
            return Err("That menu entry changed. Search again.".into());
        }
        match destination.menu {
            Menu::Top(group) => {
                self.show(self.tools_screen());
                self.reading_top = true;
                self.reading_bottom = false;
                self.top_group = Some(group);
            }
            Menu::Bottom(panel) => self.open_menu_search_panel(panel),
            Menu::List(screen) => {
                if screen == Screen::Dictionaries {
                    self.dictionary_origin = self.tools_screen();
                }
                self.show(screen);
                self.focus_reading_list(destination.ordinal);
            }
        }
        self.menu_search.focus = Some(destination);
        self.menu_search.origin = None;
        self.menu_search.hits.clear();
        Ok(())
    }

    fn menu_search_sheet(&self) -> Option<Sheet<'_>> {
        let hit = self.menu_search.hits.get(self.menu_search.preview?)?;
        let viewport = self.navigation_layout();
        Some(Sheet::centered(
            Rect::new(0, 0, viewport.width, viewport.height),
            &hit.title,
            &hit.breadcrumb,
            "Open",
        ))
    }

    pub(super) fn draw_menu_search_preview(&self, canvas: &mut Canvas) {
        if let Some(sheet) = self.menu_search_sheet() {
            sheet.draw(canvas, None);
        }
    }

    pub(super) fn menu_search_preview_press(&self, input: Input) -> Option<Option<(f32, f32)>> {
        let sheet = self.menu_search_sheet()?;
        Some(match input {
            Input::Tap(x, y)
                if [SheetAction::Cancel, SheetAction::Confirm]
                    .into_iter()
                    .any(|action| sheet.button(action).contains(x, y)) =>
            {
                Some((x, y))
            }
            _ => None,
        })
    }

    pub(super) fn menu_search_input(&mut self, input: Input) -> Option<Change> {
        if self.screen != Screen::MenuSearchResults {
            return None;
        }
        let choose = if let Some(index) = self.menu_search.preview {
            match input {
                Input::PageBack => Some(Err(())),
                Input::Tap(x, y) => match self.menu_search_sheet()?.hit(x, y) {
                    Some(SheetAction::Confirm) => Some(Ok(index)),
                    Some(SheetAction::Cancel) => Some(Err(())),
                    None => return Some(Change::None),
                },
                Input::Sleep | Input::Wake => return None,
                _ => return Some(Change::None),
            }
        } else if let Input::LongTap(x, y) = input {
            let geometry = self.list_geometry();
            (0..geometry.rows)
                .find(|row| geometry.row(*row).contains(x, y))
                .map(|row| row + self.list_page * geometry.rows)
                .filter(|index| *index < self.menu_search.hits.len())
                .map(Ok)
        } else {
            None
        };
        match choose? {
            Ok(index) => {
                self.menu_search.preview = None;
                Some(match self.open_menu_search_hit(index) {
                    Ok(()) => Change::Partial,
                    Err(error) => self.report(error),
                })
            }
            Err(()) => {
                self.menu_search.preview = None;
                Some(Change::Partial)
            }
        }
    }
}

#[cfg(test)]
mod tests;
