//! Geometry shared by the in-book destination editor and navigation lists.
use super::{
    Action, Canvas, HEIGHT, Item, LIST_X, Layout, NEXT, PREV, Reader, Rect, Screen, WIDTH,
};
use primitive_app::App;

#[derive(Clone, Copy)]
pub(super) struct NavigationLayout {
    pub(super) width: i32,
    pub(super) height: i32,
}

impl NavigationLayout {
    pub(super) const PORTRAIT: Self = Self {
        width: WIDTH as i32,
        height: HEIGHT as i32,
    };

    pub(super) fn keyboard(self) -> Layout {
        Layout::new(self.width as u32, self.height as u32, 4, 5)
    }

    pub(super) fn back(self) -> Rect {
        Rect::new(self.width - 260, 40, 220, 100)
    }

    pub(super) fn header(self, canvas: &mut Canvas, title: &str) {
        self.header_action(canvas, title, "Back");
    }

    pub(super) fn header_action(self, canvas: &mut Canvas, title: &str, action: &str) {
        canvas.text(
            Rect::new(40, 47, self.width - 354, 110),
            title,
            primitive_ui::TITLE,
        );
        canvas.button(self.back(), action, true, false);
    }

    pub(super) fn body(self, y: i32, height: i32) -> Rect {
        Rect::new(LIST_X, y, self.width - 2 * LIST_X, height)
    }

    pub(super) fn previous(self) -> Rect {
        Rect::new(PREV.x, self.bottom_y(PREV.y), PREV.w, PREV.h)
    }

    pub(super) fn next(self) -> Rect {
        Rect::new(
            self.width - (WIDTH as i32 - NEXT.x),
            self.bottom_y(NEXT.y),
            NEXT.w,
            NEXT.h,
        )
    }

    pub(super) fn edge_footer(self, face: Rect) -> Rect {
        let x = if face.x >= Self::PORTRAIT.width / 2 {
            face.x + self.width - Self::PORTRAIT.width
        } else {
            face.x
        };
        Rect::new(x, self.bottom_y(face.y), face.w, face.h)
    }

    pub(super) fn centered(self, rect: Rect) -> Rect {
        Rect::new(
            rect.x + (self.width - Self::PORTRAIT.width) / 2,
            rect.y + (self.height - Self::PORTRAIT.height) / 2,
            rect.w,
            rect.h,
        )
    }

    pub(super) fn centered_footer(self, face: Rect) -> Rect {
        Rect::new(
            face.x + (self.width - WIDTH as i32) / 2,
            self.bottom_y(face.y),
            face.w,
            face.h,
        )
    }

    pub(super) fn footer_text(self, margin: i32, inset: i32, height: i32) -> Rect {
        Rect::new(margin, self.height - inset, self.width - 2 * margin, height)
    }

    fn bottom_y(self, portrait_y: i32) -> i32 {
        self.height - (HEIGHT as i32 - portrait_y)
    }
}

impl Screen {
    pub(super) fn uses_contents_list_viewport(self) -> bool {
        matches!(
            self,
            Self::CustomContents
                | Self::CustomEdits
                | Self::CustomBookmarks
                | Self::ContentsSettings
                | Self::ChapterNavigation
                | Self::GeneratedContents
        )
    }

    pub(super) fn uses_reading_list_viewport(self) -> bool {
        matches!(
            self,
            Self::Links
                | Self::NavigationSettings
                | Self::Gestures
                | Self::Scrolling
                | Self::GestureBinding(_)
                | Self::BackOptions(_)
                | Self::LocationHistory
                | Self::PublisherPages
                | Self::Dictionaries
                | Self::DictionaryButtons(_)
                | Self::DictionaryOptions(_)
                | Self::DictionaryPresets(
                    super::dictionary_presets::Page::List
                        | super::dictionary_presets::Page::Details(_)
                        | super::dictionary_presets::Page::Confirm(_, _)
                        | super::dictionary_presets::Page::Unavailable(..)
                )
                | Self::DictionaryHistory
                | Self::ClearDictionaryHistory
                | Self::OnlineHistory
                | Self::ClearOnlineHistory
                | Self::TranslationSettings
                | Self::WikipediaSettings
                | Self::ReadingProfiles
                | Self::ReadingProfile(_)
                | Self::ReadingProfileFields(_)
                | Self::ReadingProfileValues(_)
                | Self::ReadingProfileMargins(_)
                | Self::ReadingProfileFontRendering(_)
                | Self::ReadingProfileBookRendering(_)
                | Self::ReadingProfileChoice(_, _)
                | Self::ReadingProfileOpening(_, _)
                | Self::ReadingProfileClosing(..)
                | Self::ConfirmProfile(_, _)
                | Self::ReadingHistory(
                    super::reading_history::Page::ImportKoReader
                        | super::reading_history::Page::ChooseKoReaderDatabase
                )
                | Self::ReadingFonts
                | Self::Typography
                | Self::TypographyRules
                | Self::TypographyLanguages(_)
                | Self::FontRendering(_)
                | Self::StyleTweaks
                | Self::StylePreset
                | Self::StyleFiles
                | Self::StyleReview
                | Self::StyleClear
        )
    }

    pub(super) fn uses_navigation_viewport(self) -> bool {
        self.is_contents()
            || self.uses_contents_list_viewport()
            || self.uses_reading_list_viewport()
            || self.is_numeric_editor()
            || self.uses_numeric_settings_list()
            || self.uses_reading_text_editor()
            || self.is_bookmark_list()
            || matches!(
                self,
                Self::GoToPage
                    | Self::ContentsSearch
                    | Self::CustomTitle
                    | Self::LibraryFind
                    | Self::DictionaryEntry
                    | Self::ProfileName(_)
                    | Self::DictionaryPresets(super::dictionary_presets::Page::Name(_))
                    | Self::PdfPassword
                    | Self::Find
                    | Self::Matches
                    | Self::BookmarkFind
                    | Self::BookmarkMatches
                    | Self::BookMap
                    | Self::PageOverview
                    | Self::StyleEditor
                    | Self::BookmarkBrowser(_)
                    | Self::ReadingHistory(super::reading_history::Page::Progress)
            )
    }
}

impl Reader {
    pub(super) fn focus_reading_list(&mut self, ordinal: usize) {
        let page = ordinal / self.list_geometry().rows;
        self.list_page = page;
        self.reading_list_anchor = Some(ReadingListAnchor {
            page,
            ordinal,
            link: None,
        });
    }

    pub(super) fn navigation_layout(&self) -> NavigationLayout {
        let (width, height) = self.reading_viewport().dimensions();
        NavigationLayout {
            width: width as i32,
            height: height as i32,
        }
    }

    pub(super) fn reading_list_ordinal(&self) -> usize {
        self.reading_list_anchor
            .as_ref()
            .filter(|anchor| anchor.page == self.list_page)
            .map_or_else(|| self.list_page * self.list_geometry().rows, |a| a.ordinal)
    }

    pub(super) fn restore_reading_list_ordinal(&mut self, ordinal: usize) {
        let ordinal = ordinal.min(self.items().len().saturating_sub(1));
        self.list_page = ordinal / self.list_geometry().rows;
        self.reading_list_anchor = Some(ReadingListAnchor {
            page: self.list_page,
            ordinal,
            link: None,
        });
    }
}

pub(super) struct ReadingListAnchor {
    page: usize,
    ordinal: usize,
    link: Option<std::rc::Rc<Item>>,
}

// Keep the earliest visible destination while rebuilding a list at a new size.
// Page links can change with reflow, so match their original destination first.
pub(super) struct ReadingListStage {
    original_page: usize,
    first: usize,
    link: Option<std::rc::Rc<Item>>,
    presented: Option<(usize, Option<usize>)>,
}

impl ReadingListStage {
    pub(super) fn capture(reader: &Reader) -> Option<Self> {
        if !reader.screen.uses_reading_list_viewport()
            && !reader.screen.uses_numeric_settings_list()
        {
            return None;
        }
        let retained = reader
            .reading_list_anchor
            .as_ref()
            .filter(|anchor| anchor.page == reader.list_page);
        let first = reader.reading_list_ordinal();
        let link = retained.and_then(|anchor| anchor.link.clone()).or_else(|| {
            if reader.screen == Screen::Links {
                reader
                    .page_link_items()
                    .into_iter()
                    .nth(first)
                    .map(std::rc::Rc::new)
            } else {
                None
            }
        });
        Some(Self {
            original_page: reader.list_page,
            first,
            link,
            presented: None,
        })
    }

    pub(super) fn matches(&self, reader: &Reader) -> bool {
        self.original_page == reader.list_page
    }

    pub(super) fn draw(&mut self, reader: &mut Reader, canvas: &mut Canvas) {
        let first = if reader.screen == Screen::Links {
            let items = reader.page_link_items();
            self.link
                .as_ref()
                .and_then(|before| items.iter().position(|after| same_link(before, after)))
                .unwrap_or_else(|| self.first.min(items.len().saturating_sub(1)))
        } else {
            self.first
        };
        let page = first / reader.list_geometry().rows;
        let current = if reader.screen == Screen::PublisherPages {
            reader.visible_publisher_page()
        } else {
            reader.current_contents_row
        };
        let previous_page = std::mem::replace(&mut reader.list_page, page);
        let previous_current = std::mem::replace(&mut reader.current_contents_row, current);
        reader.draw(canvas);
        reader.list_page = previous_page;
        reader.current_contents_row = previous_current;
        self.presented = Some((page, current));
    }

    pub(super) fn commit(self, reader: &mut Reader) {
        if let Some((page, current)) = self.presented {
            reader.list_page = page;
            reader.current_contents_row = current;
            reader.reading_list_anchor = Some(ReadingListAnchor {
                page,
                ordinal: self.first,
                link: self.link,
            });
        }
    }
}

fn same_link(before: &Item, after: &Item) -> bool {
    before.title == after.title
        && match (&before.action, &after.action) {
            (Action::Jump(left), Action::Jump(right)) => left == right,
            (Action::OpenFixedLink(left), Action::OpenFixedLink(right)) => left == right,
            (Action::OpenLinkAddress(left), Action::OpenLinkAddress(right)) => left == right,
            _ => false,
        }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "navigation_layout/reading_lists_tests.rs"]
mod reading_lists_tests;

#[cfg(test)]
#[path = "navigation_layout/lookup_tests.rs"]
mod lookup_tests;
