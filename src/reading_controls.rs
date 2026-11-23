use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use primitive_app::{Change, WIDTH, with_storage};
use primitive_ui::Input;
use serde::{Deserialize, Serialize};

use crate::session::PAGE_HEIGHT;

const MAX_FILE_BYTES: u64 = 4096;
pub(crate) const MAX_AUTO_TURN_SECONDS: u32 = 86_400;
pub(crate) const MAX_AUTO_TURN_DISTANCE_TENTHS: i16 = 200;
pub(crate) const DEFAULT_VERY_LONG_PRESS_TENTHS: u16 = 30;
pub(crate) const MIN_VERY_LONG_PRESS_TENTHS: u16 = 9;
pub(crate) const MAX_VERY_LONG_PRESS_TENTHS: u16 = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ReadingControls {
    pub hide_nonlinear_default: bool,
    #[serde(skip_serializing_if = "crate::highlight_style::HighlightStyle::is_default")]
    pub highlight_style: crate::highlight_style::HighlightStyle,
    #[serde(skip_serializing_if = "crate::highlight_style::HighlightColor::is_default")]
    pub highlight_color: crate::highlight_style::HighlightColor,
    /// Stock's reader-wide opt-in; the current book supplies its saved color.
    pub selection_uses_highlight_color: bool,
    #[serde(skip_serializing_if = "crate::highlight_style::NoteMarker::is_default")]
    pub note_marker: crate::highlight_style::NoteMarker,
    #[serde(skip_serializing_if = "crate::highlight_style::Appearance::is_default")]
    pub highlight_appearance: crate::highlight_style::Appearance,
    #[serde(skip_serializing_if = "SelectionPopupPosition::is_default")]
    pub selection_popup_position: SelectionPopupPosition,
    #[serde(skip_serializing_if = "SelectionLongPressAction::is_default")]
    pub selection_long_press_action: SelectionLongPressAction,
    #[serde(skip_serializing_if = "HighlightPrompt::is_default")]
    pub highlight_prompt: HighlightPrompt,
    /// `None` follows KOReader's default: open a dictionary for a held single word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dictionary_on_single_word: Option<bool>,
    /// `None` follows KOReader's default: auto-scroll selections at reading corners.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_corner_scroll: Option<bool>,
    /// Seconds in tenths. `None` retains KOReader's three-second default in legacy settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub very_long_press_tenths: Option<u16>,
    pub night_pages: bool,
    pub footnote_popups: bool,
    pub prefer_footnotes: bool,
    pub left_touch: bool,
    pub large_toolbar: bool,
    pub disable_page_taps: bool,
    pub page_refresh: PageRefresh,
    /// Persistent KOReader-style page-turn timeout, retained when turns are disabled.
    pub auto_turn_seconds: u32,
    /// Enabled separately so turning automatic movement off keeps the last timeout.
    pub auto_turn_enabled: bool,
    /// Signed movement distance in tenths of a page. Zero is the legacy 100% default.
    pub auto_turn_distance_tenths: i16,
    pub both_toolbars: bool,
    pub orientation: ReadingOrientation,
    pub palette: ScreenPalette,
    pub status_bar: StatusBar,
    /// Sort in-book bookmark rows by creation time instead of document location.
    pub bookmark_sort_by_date: bool,
    /// Reverse the selected in-book bookmark ordering.
    pub bookmark_reverse: bool,
    /// Text shown for each saved highlight in the in-book annotations list.
    pub annotation_list_text: AnnotationListText,
    /// Bitset of reading metrics shown when `status_bar` is `Custom`.
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub footer_items: u16,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub footer_order: u64,
    #[serde(skip_serializing_if = "FooterSeparator::is_default")]
    pub footer_separator: FooterSeparator,
    #[serde(skip_serializing_if = "FooterSymbolStyle::is_default")]
    pub footer_symbol_style: FooterSymbolStyle,
    #[serde(skip_serializing_if = "FooterAlignment::is_default")]
    pub footer_alignment: FooterAlignment,
    #[serde(skip_serializing_if = "is_false")]
    pub footer_pages_left_include_current: bool,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub footer_percentage_digits: u8,
    #[serde(skip_serializing_if = "FooterTrackPosition::is_default")]
    pub footer_track_position: FooterTrackPosition,
    #[serde(skip_serializing_if = "FooterTrackThickness::is_default")]
    pub footer_track_thickness: FooterTrackThickness,
    /// Minimum share reserved for the progress track; `None` keeps KOReader's 20% default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_track_min_width_pct: Option<u8>,
    /// Anchor the Alongside track to the minimum width instead of using all remaining space.
    #[serde(skip_serializing_if = "is_false")]
    pub footer_track_lock_min_width: bool,
    /// Horizontal progress-track inset; `None` keeps KOReader's 10 px default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_track_margin_width: Option<u8>,
    /// Show the reading session's starting location on the progress track.
    #[serde(skip_serializing_if = "is_false")]
    pub footer_initial_marker: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub footer_auto_refresh: bool,
    /// Hide the custom footer battery item when the charge is above this level; absent keeps 100%.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_battery_hide_threshold: Option<u8>,
    /// KOReader-compatible logical footer text size; absent keeps 14 (28 px in Reader chrome).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer_item_font_size: Option<u8>,
    #[serde(skip_serializing_if = "TopHeader::is_default")]
    pub top_header: TopHeader,
    #[serde(skip_serializing_if = "ChapterMarkerWidth::is_default")]
    pub chapter_marker_width: ChapterMarkerWidth,
    pub overlap_footer: bool,
    pub footer_scope: ProgressScope,
    pub hide_chapter_markers: bool,
    pub footer_all_at_once: bool,
    pub footer_combined_time: Option<ProgressScope>,
    pub footer_tap_locked: bool,
    pub skim_on_footer_hold: bool,
    pub menu_activation: MenuActivation,
    pub numbering: NumberingDefaults,
    pub end_action: EndAction,
    pub finish_at_end: bool,
    #[serde(default = "legacy_metadata_save")]
    pub metadata_save: MetadataSave,
}

/// KOReader's optional top status line for reflowable documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct TopHeader {
    pub enabled: bool,
    pub auto_refresh: bool,
    pub title: bool,
    pub author: bool,
    pub clock: bool,
    pub page_number: bool,
    pub page_count: bool,
    pub reading_percent: bool,
    pub chapter_marks: bool,
    pub battery: HeaderBattery,
    pub font_size: u8,
}

impl Default for TopHeader {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_refresh: true,
            title: true,
            author: true,
            clock: true,
            page_number: true,
            page_count: true,
            reading_percent: false,
            chapter_marks: true,
            battery: HeaderBattery::Icon,
            font_size: 20,
        }
    }
}

impl TopHeader {
    pub(crate) const HEIGHT: u32 = 48;

    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn font_size(self) -> u8 {
        self.font_size.clamp(8, 36)
    }

    pub(crate) fn next_font_size(self) -> Self {
        let current = self.font_size();
        Self {
            font_size: if current >= 36 { 8 } else { current + 1 },
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HeaderBattery {
    Off,
    #[default]
    Icon,
    Percentage,
}

impl HeaderBattery {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Icon => "Battery icon",
            Self::Percentage => "Battery percentage",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Off => Self::Icon,
            Self::Icon => Self::Percentage,
            Self::Percentage => Self::Off,
        }
    }
}

/// Where the selected-text action panel appears over the reading page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SelectionPopupPosition {
    Top,
    #[default]
    Center,
    Bottom,
    Highlight,
}

/// The excerpt and note presentation used for saved highlights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnnotationListText {
    HighlightedText,
    HighlightAndNote,
    #[default]
    NoteIfSet,
}

impl AnnotationListText {
    pub(crate) const ALL: [Self; 3] = [
        Self::HighlightedText,
        Self::HighlightAndNote,
        Self::NoteIfSet,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::HighlightedText => "Highlighted text",
            Self::HighlightAndNote => "Highlighted text and note",
            Self::NoteIfSet => "Note if set, otherwise highlighted text",
        }
    }
}

impl SelectionPopupPosition {
    pub(crate) const ALL: [Self; 4] = [Self::Top, Self::Center, Self::Bottom, Self::Highlight];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Center => "Center",
            Self::Bottom => "Bottom",
            Self::Highlight => "By selected text",
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Default action for a held word when it is not handled by the single-word dictionary rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SelectionLongPressAction {
    #[default]
    Ask,
    Nothing,
    Highlight,
    Select,
    Note,
    Translate,
    Wikipedia,
    Dictionary,
    Search,
}

impl SelectionLongPressAction {
    pub(crate) const ALL: [Self; 9] = [
        Self::Ask,
        Self::Nothing,
        Self::Highlight,
        Self::Select,
        Self::Note,
        Self::Translate,
        Self::Wikipedia,
        Self::Dictionary,
        Self::Search,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask with popup dialog",
            Self::Nothing => "Do nothing",
            Self::Highlight => "Highlight",
            Self::Select => "Select and highlight",
            Self::Note => "Add note",
            Self::Translate => "Translate",
            Self::Wikipedia => "Wikipedia",
            Self::Dictionary => "Dictionary",
            Self::Search => "Full-text search",
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Ask for a highlight's appearance before saving it, matching KOReader's
/// optional highlight prompt while keeping the reader's default one-tap flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HighlightPrompt {
    #[default]
    None,
    Style,
    Color,
    StyleAndColor,
}

impl HighlightPrompt {
    pub(crate) const ALL: [Self; 4] = [Self::None, Self::Style, Self::Color, Self::StyleAndColor];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "Off",
            Self::Style => "Style",
            Self::Color => "Color",
            Self::StyleAndColor => "Style and color",
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MetadataSave {
    CloseOrSleep,
    FiveMinutes,
    #[default]
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    AfterChange,
}

fn legacy_metadata_save() -> MetadataSave {
    MetadataSave::AfterChange
}

impl MetadataSave {
    pub const ALL: [Self; 6] = [
        Self::CloseOrSleep,
        Self::FiveMinutes,
        Self::FifteenMinutes,
        Self::ThirtyMinutes,
        Self::OneHour,
        Self::AfterChange,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CloseOrSleep => "On close or sleep",
            Self::FiveMinutes => "Every 5 minutes",
            Self::FifteenMinutes => "Every 15 minutes",
            Self::ThirtyMinutes => "Every 30 minutes",
            Self::OneHour => "Every 60 minutes",
            Self::AfterChange => "After each change",
        }
    }

    pub fn interval(self) -> Option<Duration> {
        let minutes = match self {
            Self::FiveMinutes => 5,
            Self::FifteenMinutes => 15,
            Self::ThirtyMinutes => 30,
            Self::OneHour => 60,
            Self::CloseOrSleep | Self::AfterChange => return None,
        };
        Some(Duration::from_secs(60 * minutes))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EndAction {
    #[default]
    Ask,
    Stay,
    Status,
    Next,
    Beginning,
    Library,
    Finish,
    StatusThenLibrary,
    Delete,
}

impl EndAction {
    pub const ALL: [Self; 9] = [
        Self::Ask,
        Self::Stay,
        Self::Status,
        Self::Next,
        Self::Beginning,
        Self::Library,
        Self::Finish,
        Self::StatusThenLibrary,
        Self::Delete,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Show actions",
            Self::Stay => "Stay on the last page",
            Self::Status => "Show book status",
            Self::Next => "Open next file",
            Self::Beginning => "Go to beginning",
            Self::Library => "Return to Library",
            Self::Finish => "Mark as finished",
            Self::StatusThenLibrary => "Show book status, then Library",
            Self::Delete => "Ask to delete the file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct NumberingDefaults {
    pub stride: Option<u16>,
    pub override_publisher: bool,
    pub prompt_publisher: bool,
    pub footer: bool,
    pub margins: bool,
    pub label_size: u8,
}

impl Default for NumberingDefaults {
    fn default() -> Self {
        Self {
            stride: None,
            override_publisher: false,
            prompt_publisher: false,
            footer: false,
            margins: false,
            label_size: 14,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MenuActivation {
    #[default]
    TapAndSwipe,
    Tap,
    Swipe,
}

impl MenuActivation {
    pub fn label(self) -> &'static str {
        match self {
            Self::TapAndSwipe => "Tap and swipe",
            Self::Tap => "Tap",
            Self::Swipe => "Swipe",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::TapAndSwipe => Self::Tap,
            Self::Tap => Self::Swipe,
            Self::Swipe => Self::TapAndSwipe,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StatusBar {
    PageNumbers,
    Progress,
    #[default]
    Chapter,
    Remaining,
    ChapterTime,
    BookTime,
    Custom,
    Minimal,
}

impl StatusBar {
    pub fn label(self) -> &'static str {
        match self {
            Self::PageNumbers => "Page numbers",
            Self::Progress => "Book progress",
            Self::Chapter => "Chapter and page",
            Self::Remaining => "Chapter pages left",
            Self::ChapterTime => "Chapter time left",
            Self::BookTime => "Book time left",
            Self::Custom => "Custom items",
            Self::Minimal => "Minimal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::PageNumbers => Self::Progress,
            Self::Progress => Self::Chapter,
            Self::Chapter => Self::Remaining,
            Self::Remaining => Self::ChapterTime,
            Self::ChapterTime => Self::BookTime,
            Self::BookTime => Self::Custom,
            Self::Custom => Self::Minimal,
            Self::Minimal => Self::PageNumbers,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FooterItem {
    CurrentPage,
    ChapterTitle,
    BookProgress,
    ChapterPagesLeft,
    BookPagesLeft,
    ChapterTimeLeft,
    BookTimeLeft,
    BookTitle,
    Author,
    BookmarkCount,
    CurrentTime,
    BatteryPercentage,
}

impl FooterItem {
    pub(crate) const ALL: [Self; 12] = [
        Self::CurrentPage,
        Self::ChapterTitle,
        Self::BookProgress,
        Self::ChapterPagesLeft,
        Self::BookPagesLeft,
        Self::ChapterTimeLeft,
        Self::BookTimeLeft,
        Self::BookTitle,
        Self::Author,
        Self::BookmarkCount,
        Self::CurrentTime,
        Self::BatteryPercentage,
    ];

    pub(crate) const DEFAULT_MASK: u16 =
        Self::CurrentPage.bit() | Self::BookProgress.bit() | Self::ChapterPagesLeft.bit();

    pub(crate) const fn bit(self) -> u16 {
        1 << self as u16
    }

    pub(crate) fn ordered(order: u64) -> Vec<Self> {
        let mut items = Vec::with_capacity(Self::ALL.len());
        let mut seen = 0u16;
        for position in 0..Self::ALL.len() {
            let value = ((order >> (position * 4)) & 0xf) as usize;
            if let Some(item) = Self::ALL.get(value)
                && seen & item.bit() == 0
            {
                items.push(*item);
                seen |= item.bit();
            }
        }
        for item in Self::ALL {
            if seen & item.bit() == 0 {
                items.push(item);
            }
        }
        items
    }

    pub(crate) fn move_order(order: u64, item: Self, direction: i8) -> u64 {
        let mut items = Self::ordered(order);
        let Some(index) = items.iter().position(|candidate| *candidate == item) else {
            return order;
        };
        let target = match direction {
            -1 => index.checked_sub(1),
            1 => (index + 1 < items.len()).then_some(index + 1),
            _ => None,
        };
        let Some(target) = target else {
            return order;
        };
        items.swap(index, target);
        items
            .iter()
            .enumerate()
            .fold(0u64, |packed, (position, item)| {
                packed | (u64::from(*item as u8) << (position * 4))
            })
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CurrentPage => "Current page",
            Self::ChapterTitle => "Chapter title",
            Self::BookProgress => "Book progress",
            Self::ChapterPagesLeft => "Pages left in chapter",
            Self::BookPagesLeft => "Pages left in book",
            Self::ChapterTimeLeft => "Time left in chapter",
            Self::BookTimeLeft => "Time left in book",
            Self::BookTitle => "Book title",
            Self::Author => "Author",
            Self::BookmarkCount => "Bookmark count",
            Self::CurrentTime => "Current time",
            Self::BatteryPercentage => "Battery percentage",
        }
    }
}

fn is_zero_u16(value: &u16) -> bool {
    *value == 0
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterSeparator {
    #[default]
    Hyphen,
    Bar,
    Bullet,
    Dot,
    None,
}

/// Prefixes for the selected custom status items, following KOReader's three styles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterSymbolStyle {
    #[default]
    Icons,
    Letters,
    Compact,
}

impl FooterSymbolStyle {
    const ALL: [Self; 3] = [Self::Icons, Self::Letters, Self::Compact];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Icons => "Icons",
            Self::Letters => "Letters",
            Self::Compact => "Compact",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub(crate) fn prefix(self, item: FooterItem) -> &'static str {
        match self {
            Self::Icons => match item {
                FooterItem::CurrentPage
                | FooterItem::ChapterTitle
                | FooterItem::BookProgress
                | FooterItem::ChapterPagesLeft
                | FooterItem::BookPagesLeft
                | FooterItem::ChapterTimeLeft
                | FooterItem::BookTimeLeft
                | FooterItem::BookTitle
                | FooterItem::Author
                | FooterItem::BookmarkCount
                | FooterItem::CurrentTime
                | FooterItem::BatteryPercentage => "",
            },
            Self::Letters => match item {
                FooterItem::CurrentPage => "P: ",
                FooterItem::ChapterTitle => "C: ",
                FooterItem::BookProgress => "R: ",
                FooterItem::ChapterPagesLeft => "=> ",
                FooterItem::BookPagesLeft => "-> ",
                FooterItem::ChapterTimeLeft => "TC: ",
                FooterItem::BookTimeLeft => "TB: ",
                FooterItem::BookTitle => "B: ",
                FooterItem::Author => "A: ",
                FooterItem::BookmarkCount => "BM: ",
                FooterItem::CurrentTime => "",
                FooterItem::BatteryPercentage => "B: ",
            },
            Self::Compact => match item {
                FooterItem::CurrentPage => "P ",
                FooterItem::ChapterTitle => "C ",
                FooterItem::BookProgress => "",
                FooterItem::ChapterPagesLeft => "> ",
                FooterItem::BookPagesLeft => ">> ",
                FooterItem::ChapterTimeLeft => "T ",
                FooterItem::BookTimeLeft => "",
                FooterItem::BookTitle | FooterItem::Author | FooterItem::CurrentTime => "",
                FooterItem::BookmarkCount => "BM ",
                FooterItem::BatteryPercentage => "B ",
            },
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Horizontal placement for the status text inside the reading footer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterAlignment {
    #[default]
    Left,
    Center,
    Right,
}

impl FooterAlignment {
    const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl FooterSeparator {
    const ALL: [Self; 5] = [Self::Hyphen, Self::Bar, Self::Bullet, Self::Dot, Self::None];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hyphen => "Hyphen (-)",
            Self::Bar => "Vertical bar (|)",
            Self::Bullet => "Bullet (•)",
            Self::Dot => "Dot (·)",
            Self::None => "No separator",
        }
    }

    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Hyphen => " - ",
            Self::Bar => " | ",
            Self::Bullet => " • ",
            Self::Dot => " · ",
            Self::None => "  ",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Where the reading progress track sits within the compact page footer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterTrackPosition {
    Above,
    Alongside,
    #[default]
    Below,
}

impl FooterTrackPosition {
    const ALL: [Self; 3] = [Self::Above, Self::Alongside, Self::Below];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Above => "Above status text",
            Self::Alongside => "Alongside status text",
            Self::Below => "Below status text",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Height of the visible progress track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FooterTrackThickness {
    Thin,
    #[default]
    Thick,
}

impl FooterTrackThickness {
    const ALL: [Self; 2] = [Self::Thin, Self::Thick];

    pub(crate) const fn height(self) -> i32 {
        match self {
            Self::Thin => 3,
            Self::Thick => 7,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Thin => "Thin",
            Self::Thick => "Thick",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Chapter tick thickness in the progress track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChapterMarkerWidth {
    Thin,
    #[default]
    Medium,
    Thick,
}

impl ChapterMarkerWidth {
    const ALL: [Self; 3] = [Self::Thin, Self::Medium, Self::Thick];

    pub(crate) const fn width(self) -> i32 {
        match self {
            Self::Thin => 1,
            Self::Medium => 2,
            Self::Thick => 3,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Thin => "Thin",
            Self::Medium => "Medium",
            Self::Thick => "Thick",
        }
    }

    pub(crate) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProgressScope {
    #[default]
    Book,
    Chapter,
    Off,
}

impl ProgressScope {
    pub fn next(self) -> Self {
        match self {
            Self::Book => Self::Chapter,
            Self::Chapter => Self::Off,
            Self::Off => Self::Book,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Book => "Whole book",
            Self::Chapter => "Current chapter",
            Self::Off => "Off",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadingOrientation {
    #[default]
    FollowLauncher,
    Upright,
    UpsideDown,
}

impl ReadingOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::FollowLauncher => "Follow Launcher",
            Self::Upright => "Upright",
            Self::UpsideDown => "Upside down",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::FollowLauncher => Self::Upright,
            Self::Upright => Self::UpsideDown,
            Self::UpsideDown => Self::FollowLauncher,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScreenPalette {
    #[default]
    FollowLauncher,
    Light,
    Dark,
}

impl ScreenPalette {
    pub fn label(self) -> &'static str {
        match self {
            Self::FollowLauncher => "Follow Launcher",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::FollowLauncher => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::FollowLauncher,
        }
    }

    pub fn inverted(self) -> Option<bool> {
        match self {
            Self::FollowLauncher => None,
            Self::Light => Some(false),
            Self::Dark => Some(true),
        }
    }
}

/// The theme of Primitive Launcher, the system layer. Opened from the launcher (`PRIMITIVE_LAUNCHER=1`), the whole
/// Reader follows the launcher's dark mode, its menus and its pages, and has no light or dark switch of its own.
/// The launcher asks for a redraw (SIGUSR1) when its theme changes; the app kit reads the theme again for the menus,
/// and Reader reads it again for the pages each time it draws.
#[derive(Debug)]
pub(crate) struct SystemTheme {
    settings: PathBuf,
    dark: Cell<bool>,
}

impl SystemTheme {
    /// The launcher's theme, or `None` when Reader was not opened from the launcher.
    pub fn from_env() -> Option<Self> {
        if std::env::var("PRIMITIVE_LAUNCHER").as_deref() != Ok("1") {
            return None;
        }
        let launcher = std::env::var_os("KL_DIR")
            .map_or_else(|| PathBuf::from("/mnt/us/launcher"), PathBuf::from);
        let theme = Self::new(launcher.join("settings.txt"));
        theme.refresh();
        Some(theme)
    }

    pub fn new(settings: PathBuf) -> Self {
        Self {
            settings,
            dark: Cell::new(false),
        }
    }

    /// Read the launcher's dark mode again.
    pub fn refresh(&self) {
        let text = fs::read_to_string(&self.settings).unwrap_or_default();
        self.dark.set(launcher_dark_mode(&text));
    }

    pub fn dark(&self) -> bool {
        self.dark.get()
    }
}

/// Whether the launcher's settings turn dark mode on (`dark_mode=on`).
pub(crate) fn launcher_dark_mode(settings: &str) -> bool {
    settings.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(key, value)| key.trim() == "dark_mode" && value.trim() == "on")
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PageRefresh {
    #[default]
    Default,
    EveryPage,
    EveryFivePages,
    EveryTenPages,
}

impl PageRefresh {
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::EveryPage,
            Self::EveryPage => Self::EveryFivePages,
            Self::EveryFivePages => Self::EveryTenPages,
            Self::EveryTenPages => Self::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::EveryPage => "Every page",
            Self::EveryFivePages => "Every 5 pages",
            Self::EveryTenPages => "Every 10 pages",
        }
    }

    pub fn after_page(self, since_last: &mut u8) -> Change {
        let interval = match self {
            Self::Default => return Change::Partial,
            Self::EveryPage => 1,
            Self::EveryFivePages => 5,
            Self::EveryTenPages => 10,
        };
        *since_last += 1;
        if *since_last >= interval {
            *since_last = 0;
            Change::Full
        } else {
            Change::Partial
        }
    }
}

impl ReadingControls {
    pub(crate) fn selection_corner_scroll_enabled(self) -> bool {
        self.selection_corner_scroll.unwrap_or(true)
    }

    pub(crate) fn very_long_press_tenths(self) -> u16 {
        self.very_long_press_tenths
            .unwrap_or(DEFAULT_VERY_LONG_PRESS_TENTHS)
            .clamp(MIN_VERY_LONG_PRESS_TENTHS, MAX_VERY_LONG_PRESS_TENTHS)
    }

    pub(crate) fn very_long_press_millis(self) -> u32 {
        u32::from(self.very_long_press_tenths()) * 100
    }

    pub(crate) fn very_long_press_label(self) -> String {
        let tenths = self.very_long_press_tenths();
        format!("{}.{:01} seconds", tenths / 10, tenths % 10)
    }

    pub(crate) fn auto_turn_distance_tenths(self) -> i16 {
        if self.auto_turn_distance_tenths == 0
            || self.auto_turn_distance_tenths.unsigned_abs() > MAX_AUTO_TURN_DISTANCE_TENTHS as u16
        {
            10
        } else {
            self.auto_turn_distance_tenths
        }
    }

    pub(crate) fn auto_turn_interval(self) -> Option<Duration> {
        (self.auto_turn_enabled && (1..=MAX_AUTO_TURN_SECONDS).contains(&self.auto_turn_seconds))
            .then(|| Duration::from_secs(u64::from(self.auto_turn_seconds)))
    }

    pub fn load(path: &Path, existing_library: bool) -> Self {
        with_storage(|| {
            let file = fs::File::open(path)?;
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(io::Error::other("Reading controls settings are too large"));
            }
            let mut controls: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if controls.auto_turn_seconds > MAX_AUTO_TURN_SECONDS {
                controls.auto_turn_seconds = 0;
                controls.auto_turn_enabled = false;
            }
            if controls.auto_turn_distance_tenths.unsigned_abs()
                > MAX_AUTO_TURN_DISTANCE_TENTHS as u16
            {
                controls.auto_turn_distance_tenths = 0;
            }
            Ok(controls)
        })
        .unwrap_or_else(|_| {
            // Fresh profiles match the stock note-preview behavior. Existing
            // valid files (including an explicit false or sparse legacy file)
            // were deserialized above and retain their saved choice.
            let mut controls = Self {
                footnote_popups: true,
                ..Self::default()
            };
            if existing_library {
                controls.metadata_save = MetadataSave::AfterChange;
            }
            controls
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        primitive_app::save(path, &bytes)
    }

    /// Accept only inputs already routed to the page-turn handler.
    pub fn page_input(self, input: Input) -> Option<Input> {
        match input {
            Input::Tap(_, y) if self.disable_page_taps && y < PAGE_HEIGHT as f32 => None,
            Input::Tap(x, y) if self.left_touch && y < PAGE_HEIGHT as f32 => {
                Some(Input::Tap(WIDTH as f32 - x, y))
            }
            _ => Some(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_note_previews_are_on_and_saved_or_sparse_legacy_choices_are_preserved() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "primitive-note-default-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("reading-controls.json");
        for existing in [false, true] {
            assert!(ReadingControls::load(&path, existing).footnote_popups);
        }
        for enabled in [false, true] {
            let controls = ReadingControls {
                footnote_popups: enabled,
                ..Default::default()
            };
            controls.save(&path).unwrap();
            let before = std::fs::read(&path).unwrap();
            for existing in [false, true] {
                assert_eq!(
                    ReadingControls::load(&path, existing).footnote_popups,
                    enabled
                );
            }
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        std::fs::write(&path, b"{}").unwrap();
        assert!(!ReadingControls::load(&path, true).footnote_popups);
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_system_theme_is_the_launchers_dark_mode() {
        assert!(launcher_dark_mode("rotation=upright\ndark_mode=on\n"));
        assert!(launcher_dark_mode(" dark_mode = on "));
        assert!(!launcher_dark_mode("dark_mode=off"));
        assert!(!launcher_dark_mode(""), "no setting is light");

        let path = std::env::temp_dir().join(format!("reader-theme-{}.txt", std::process::id()));
        let theme = SystemTheme::new(path.clone());
        fs::write(&path, "dark_mode=on\n").unwrap();
        theme.refresh();
        assert!(theme.dark());
        fs::write(&path, "dark_mode=off\n").unwrap();
        theme.refresh();
        assert!(!theme.dark(), "a change is read again");
        fs::remove_file(&path).unwrap();
        theme.refresh();
        assert!(!theme.dark(), "no settings file is light");
    }

    #[test]
    fn old_settings_keep_the_default_partial_page_refresh() {
        let controls: ReadingControls = serde_json::from_str(r#"{"night_pages":true}"#).unwrap();
        assert!(controls.night_pages);
        assert_eq!(
            controls.selection_popup_position,
            SelectionPopupPosition::Center
        );
        assert_eq!(controls.auto_turn_seconds, 0);
        assert_eq!(controls.auto_turn_interval(), None);
        assert_eq!(controls.page_refresh, PageRefresh::Default);
        let mut since_last = 0;
        assert_eq!(
            controls.page_refresh.after_page(&mut since_last),
            Change::Partial
        );
        assert_eq!(since_last, 0);
    }

    #[test]
    fn selection_popup_position_defaults_to_stock_center_and_round_trips() {
        for position in SelectionPopupPosition::ALL {
            let controls = ReadingControls {
                selection_popup_position: position,
                ..ReadingControls::default()
            };
            let bytes = serde_json::to_vec(&controls).unwrap();
            let reopened: ReadingControls = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reopened.selection_popup_position, position);
        }
        assert_eq!(
            serde_json::from_str::<ReadingControls>(r#"{"selection_popup_position":"bogus"}"#)
                .unwrap_err()
                .classify(),
            serde_json::error::Category::Data
        );
    }

    #[test]
    fn highlight_prompt_defaults_to_one_tap_and_persists_each_choice() {
        assert_eq!(
            ReadingControls::default().highlight_prompt,
            HighlightPrompt::None
        );
        for prompt in HighlightPrompt::ALL {
            let controls = ReadingControls {
                highlight_prompt: prompt,
                ..ReadingControls::default()
            };
            let reopened: ReadingControls =
                serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
            assert_eq!(reopened.highlight_prompt, prompt);
        }
        let legacy: ReadingControls =
            serde_json::from_str(r#"{"selection_long_press_action":"highlight"}"#).unwrap();
        assert_eq!(legacy.highlight_prompt, HighlightPrompt::None);
    }

    #[test]
    fn long_press_actions_round_trip_and_keep_stock_single_word_dictionary_default() {
        let defaults = ReadingControls::default();
        assert_eq!(
            defaults.selection_long_press_action,
            SelectionLongPressAction::Ask
        );
        assert_eq!(defaults.dictionary_on_single_word, None);
        assert_eq!(defaults.selection_corner_scroll, None);
        assert!(defaults.selection_corner_scroll_enabled());
        for action in SelectionLongPressAction::ALL {
            let controls = ReadingControls {
                selection_long_press_action: action,
                dictionary_on_single_word: Some(false),
                selection_corner_scroll: Some(false),
                ..defaults
            };
            let bytes = serde_json::to_vec(&controls).unwrap();
            let reopened: ReadingControls = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reopened.selection_long_press_action, action);
            assert_eq!(reopened.dictionary_on_single_word, Some(false));
            assert_eq!(reopened.selection_corner_scroll, Some(false));
            assert!(!reopened.selection_corner_scroll_enabled());
        }
        let legacy: ReadingControls = serde_json::from_str(r#"{"night_pages":true}"#).unwrap();
        assert_eq!(
            legacy.selection_long_press_action,
            SelectionLongPressAction::Ask
        );
        assert_eq!(legacy.dictionary_on_single_word, None);
        assert_eq!(legacy.selection_corner_scroll, None);
        assert!(legacy.selection_corner_scroll_enabled());
    }

    #[test]
    fn very_long_press_interval_defaults_to_three_seconds_and_is_bounded() {
        let mut controls = ReadingControls::default();
        assert_eq!(controls.very_long_press_tenths(), 30);
        assert_eq!(controls.very_long_press_millis(), 3_000);
        assert_eq!(controls.very_long_press_label(), "3.0 seconds");

        controls.very_long_press_tenths = Some(9);
        assert_eq!(controls.very_long_press_tenths(), 9);
        controls.very_long_press_tenths = Some(201);
        assert_eq!(controls.very_long_press_tenths(), 200);

        controls.very_long_press_tenths = Some(45);
        let reopened: ReadingControls =
            serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
        assert_eq!(reopened.very_long_press_tenths(), 45);
        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.very_long_press_tenths(), 30);
    }

    #[test]
    fn custom_footer_items_round_trip_and_old_settings_keep_the_legacy_mode() {
        let controls = ReadingControls {
            status_bar: StatusBar::Custom,
            footer_items: FooterItem::DEFAULT_MASK
                | FooterItem::BookmarkCount.bit()
                | FooterItem::BatteryPercentage.bit(),
            footer_battery_hide_threshold: Some(80),
            footer_item_font_size: Some(18),
            footer_symbol_style: FooterSymbolStyle::Letters,
            ..ReadingControls::default()
        };
        let bytes = serde_json::to_vec(&controls).unwrap();
        let reopened: ReadingControls = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reopened.status_bar, StatusBar::Custom);
        assert_eq!(reopened.footer_items, controls.footer_items);
        assert_eq!(reopened.footer_battery_hide_threshold, Some(80));
        assert_eq!(reopened.footer_item_font_size, Some(18));
        assert_eq!(reopened.footer_symbol_style, FooterSymbolStyle::Letters);
        assert_eq!(FooterItem::BatteryPercentage.label(), "Battery percentage");
        assert!(FooterItem::ordered(0).contains(&FooterItem::BatteryPercentage));

        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.status_bar, StatusBar::Chapter);
        assert_eq!(legacy.footer_items, 0);
        assert_eq!(legacy.footer_symbol_style, FooterSymbolStyle::Icons);
        assert_eq!(StatusBar::BookTime.next(), StatusBar::Custom);
        assert_eq!(StatusBar::Custom.next(), StatusBar::Minimal);
    }

    #[test]
    fn custom_footer_symbol_styles_match_stock_modes_and_cycle() {
        assert_eq!(FooterSymbolStyle::Icons.label(), "Icons");
        assert_eq!(FooterSymbolStyle::Letters.label(), "Letters");
        assert_eq!(FooterSymbolStyle::Compact.label(), "Compact");
        assert_eq!(FooterSymbolStyle::Icons.next(), FooterSymbolStyle::Letters);
        assert_eq!(
            FooterSymbolStyle::Letters.next(),
            FooterSymbolStyle::Compact
        );
        assert_eq!(FooterSymbolStyle::Compact.next(), FooterSymbolStyle::Icons);

        assert_eq!(
            FooterSymbolStyle::Icons.prefix(FooterItem::BookProgress),
            ""
        );
        assert_eq!(
            FooterSymbolStyle::Letters.prefix(FooterItem::BookProgress),
            "R: "
        );
        assert_eq!(
            FooterSymbolStyle::Compact.prefix(FooterItem::BookProgress),
            ""
        );
        assert_eq!(
            FooterSymbolStyle::Letters.prefix(FooterItem::ChapterTimeLeft),
            "TC: "
        );
        assert_eq!(
            FooterSymbolStyle::Compact.prefix(FooterItem::ChapterPagesLeft),
            "> "
        );
        for item in FooterItem::ALL {
            assert!(FooterSymbolStyle::Letters.prefix(item).is_ascii());
            assert!(FooterSymbolStyle::Compact.prefix(item).is_ascii());
        }
    }

    #[test]
    fn footer_alignment_matches_stock_options_and_preserves_the_left_default() {
        use FooterAlignment::{Center, Left, Right};

        assert_eq!(Left.label(), "Left");
        assert_eq!(Center.label(), "Center");
        assert_eq!(Right.label(), "Right");
        assert_eq!(Left.next(), Center);
        assert_eq!(Center.next(), Right);
        assert_eq!(Right.next(), Left);

        let controls = ReadingControls {
            footer_alignment: Center,
            ..ReadingControls::default()
        };
        let reopened: ReadingControls =
            serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
        assert_eq!(reopened.footer_alignment, Center);
        assert_eq!(
            serde_json::from_str::<ReadingControls>("{}")
                .unwrap()
                .footer_alignment,
            Left
        );
    }

    #[test]
    fn top_header_matches_stock_item_defaults_and_persists_configuration() {
        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert!(!legacy.top_header.enabled);
        assert!(legacy.top_header.auto_refresh);
        assert!(legacy.top_header.title);
        assert!(legacy.top_header.author);
        assert!(legacy.top_header.clock);
        assert!(legacy.top_header.page_number);
        assert!(legacy.top_header.page_count);
        assert!(!legacy.top_header.reading_percent);
        assert!(legacy.top_header.chapter_marks);
        assert_eq!(legacy.top_header.battery, HeaderBattery::Icon);
        assert_eq!(legacy.top_header.font_size(), 20);

        let controls = ReadingControls {
            top_header: TopHeader {
                enabled: true,
                auto_refresh: false,
                title: false,
                author: true,
                clock: false,
                page_number: true,
                page_count: false,
                reading_percent: true,
                chapter_marks: false,
                battery: HeaderBattery::Percentage,
                font_size: 34,
            },
            ..ReadingControls::default()
        };
        let bytes = serde_json::to_vec(&controls).unwrap();
        let reopened: ReadingControls = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reopened.top_header, controls.top_header);
        assert!(
            !serde_json::to_string(&ReadingControls::default())
                .unwrap()
                .contains("top_header")
        );
    }

    #[test]
    fn top_header_font_and_battery_options_cycle_with_bounded_values() {
        let mut header = TopHeader::default();
        assert_eq!(header.next_font_size().font_size(), 21);
        header.font_size = 36;
        assert_eq!(header.next_font_size().font_size(), 8);
        header.font_size = 255;
        assert_eq!(header.font_size(), 36);
        assert_eq!(HeaderBattery::Off.next(), HeaderBattery::Icon);
        assert_eq!(HeaderBattery::Icon.next(), HeaderBattery::Percentage);
        assert_eq!(HeaderBattery::Percentage.next(), HeaderBattery::Off);
    }

    #[test]
    fn footer_presentation_preferences_round_trip_and_keep_stock_defaults() {
        let controls = ReadingControls {
            footer_separator: FooterSeparator::Bullet,
            footer_pages_left_include_current: true,
            footer_percentage_digits: 2,
            footer_auto_refresh: true,
            ..ReadingControls::default()
        };
        let reopened: ReadingControls =
            serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
        assert_eq!(reopened.footer_separator, FooterSeparator::Bullet);
        assert!(reopened.footer_pages_left_include_current);
        assert_eq!(reopened.footer_percentage_digits, 2);
        assert!(reopened.footer_auto_refresh);

        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.footer_separator, FooterSeparator::Hyphen);
        assert!(!legacy.footer_pages_left_include_current);
        assert_eq!(legacy.footer_percentage_digits, 0);
        assert!(!legacy.footer_auto_refresh);

        assert_eq!(FooterSeparator::Hyphen.label(), "Hyphen (-)");
        assert_eq!(FooterSeparator::Hyphen.text(), " - ");
        assert_eq!(FooterSeparator::Hyphen.next(), FooterSeparator::Bar);
        assert_eq!(FooterSeparator::Bar.next(), FooterSeparator::Bullet);
        assert_eq!(FooterSeparator::None.text(), "  ");
        assert!(
            !serde_json::to_string(&ReadingControls::default())
                .unwrap()
                .contains("footer_symbol_style")
        );
    }

    #[test]
    fn footer_track_preferences_round_trip_and_keep_existing_layout() {
        let controls = ReadingControls {
            footer_track_position: FooterTrackPosition::Above,
            footer_track_thickness: FooterTrackThickness::Thin,
            footer_track_min_width_pct: Some(35),
            footer_track_margin_width: Some(24),
            chapter_marker_width: ChapterMarkerWidth::Thick,
            ..ReadingControls::default()
        };
        let reopened: ReadingControls =
            serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
        assert_eq!(reopened.footer_track_position, FooterTrackPosition::Above);
        assert_eq!(reopened.footer_track_thickness, FooterTrackThickness::Thin);
        assert_eq!(reopened.footer_track_min_width_pct, Some(35));
        assert_eq!(reopened.footer_track_margin_width, Some(24));
        assert_eq!(reopened.chapter_marker_width, ChapterMarkerWidth::Thick);

        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.footer_track_position, FooterTrackPosition::Below);
        assert_eq!(legacy.footer_track_thickness, FooterTrackThickness::Thick);
        assert_eq!(legacy.footer_track_min_width_pct, None);
        assert_eq!(legacy.footer_track_margin_width, None);
        assert_eq!(legacy.chapter_marker_width, ChapterMarkerWidth::Medium);
    }

    #[test]
    fn footer_item_order_moves_metrics_and_repairs_partial_or_duplicate_orders() {
        let mut order = 0;
        for _ in 0..9 {
            order = FooterItem::move_order(order, FooterItem::BookmarkCount, -1);
        }
        let ordered = FooterItem::ordered(order);
        assert_eq!(ordered.len(), FooterItem::ALL.len());
        assert_eq!(ordered[0], FooterItem::BookmarkCount);
        assert_eq!(ordered[1], FooterItem::CurrentPage);
        assert_eq!(
            FooterItem::move_order(order, FooterItem::BookmarkCount, -1),
            order
        );

        let controls = ReadingControls {
            footer_order: order,
            ..ReadingControls::default()
        };
        let reopened: ReadingControls =
            serde_json::from_slice(&serde_json::to_vec(&controls).unwrap()).unwrap();
        assert_eq!(FooterItem::ordered(reopened.footer_order), ordered);

        let legacy: ReadingControls = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.footer_order, 0);
        assert_eq!(FooterItem::ordered(0), FooterItem::ALL);
    }

    #[test]
    fn auto_turn_timeout_round_trips_and_rejects_out_of_range_values() {
        let mut controls = ReadingControls {
            auto_turn_seconds: 75,
            auto_turn_enabled: true,
            ..ReadingControls::default()
        };
        assert_eq!(controls.auto_turn_interval(), Some(Duration::from_secs(75)));
        let bytes = serde_json::to_vec(&controls).unwrap();
        controls = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(controls.auto_turn_seconds, 75);
        assert_eq!(controls.auto_turn_interval(), Some(Duration::from_secs(75)));

        controls.auto_turn_enabled = false;
        assert_eq!(controls.auto_turn_seconds, 75);
        assert_eq!(controls.auto_turn_interval(), None);
        controls.auto_turn_enabled = true;
        controls.auto_turn_seconds = MAX_AUTO_TURN_SECONDS + 1;
        assert_eq!(controls.auto_turn_interval(), None);
    }

    #[test]
    fn automatic_scroll_distance_defaults_to_one_page_and_preserves_signed_tenths() {
        let mut controls = ReadingControls::default();
        assert_eq!(controls.auto_turn_distance_tenths(), 10);
        controls.auto_turn_distance_tenths = -17;
        let bytes = serde_json::to_vec(&controls).unwrap();
        controls = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(controls.auto_turn_distance_tenths(), -17);

        controls.auto_turn_distance_tenths = i16::MIN;
        assert_eq!(controls.auto_turn_distance_tenths(), 10);
    }
}
