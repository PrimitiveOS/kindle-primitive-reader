//! Reading status shares the existing compact page footer with document controls.
use super::character_page_settings::NumberField;
use super::progress_markers::MarkerPages;
use super::{Action, ActiveFixed, Item, Reader, Screen};
use crate::Probe;
use crate::fixed_session::FixedReading;
use crate::library::Bookmark;
use crate::library::hidden_flows::PageFlows;
use crate::reading_controls::{
    FooterAlignment, FooterItem, FooterSymbolStyle, FooterTrackPosition, HeaderBattery,
    ProgressScope, ReadingControls, StatusBar, TopHeader,
};
use crate::session::{DONE, PAGE_HEIGHT, SHOW_STATUS};
use primitive_app::{Change, HEIGHT, WIDTH};
use primitive_ui::{Canvas, HAIRLINE, Icon, Rect, SECONDARY, TextStyle};
use std::io;
use std::time::{Duration, Instant};

const FOOTER_TRACK_GAP: i32 = 20;
const FOOTER_CAPTION_LEFT: i32 = 38;
const FOOTER_ICON_SIZE: i32 = 22;
const FOOTER_ICON_GAP: i32 = 5;
const FOOTER_PROGRESS_ICON: Icon = Icon {
    stroke: &["M3 6 H21 V18 H3 Z", "M6 14 L10 10 L13 13 L18 8"],
    fill: &[],
};
const FOOTER_AUTHOR_ICON: Icon = Icon {
    stroke: &[
        "M12 12 A4 4 0 1 0 12 4 A4 4 0 0 0 12 12 Z",
        "M4 21 C4 17 7 15 12 15 C17 15 20 17 20 21",
    ],
    fill: &[],
};
const FOOTER_BATTERY_ICON: Icon = Icon {
    stroke: &["M4 7 H20 V18 H4 Z", "M20 10 H22 V15 H20"],
    fill: &["M6 9 H18 V16 H6 Z"],
};
const PAGE_TURN_REGULAR_ICON: Icon = Icon {
    stroke: &[
        "M3 7 H20 M16 3 L20 7 L16 11",
        "M3 16 H20 M16 12 L20 16 L16 20",
    ],
    fill: &[],
};
const PAGE_TURN_INVERTED_ICON: Icon = Icon {
    stroke: &["M3 7 H20 M16 3 L20 7 L16 11", "M20 16 H3 M7 12 L3 16 L7 20"],
    fill: &[],
};

fn footer_item_icon(item: FooterItem, inverted: bool) -> Option<Icon> {
    Some(match item {
        FooterItem::CurrentPage => super::bar_icons::PAGE,
        FooterItem::ChapterTitle | FooterItem::BookPagesLeft | FooterItem::BookTitle => {
            super::bar_icons::BOOK
        }
        FooterItem::BookProgress => FOOTER_PROGRESS_ICON,
        FooterItem::ChapterPagesLeft => super::bar_icons::NEXT_PAGE,
        FooterItem::ChapterTimeLeft | FooterItem::BookTimeLeft | FooterItem::CurrentTime => {
            super::bar_icons::CLOCK
        }
        FooterItem::Author => FOOTER_AUTHOR_ICON,
        FooterItem::BookmarkCount => super::bar_icons::BOOKMARK,
        FooterItem::BatteryPercentage => FOOTER_BATTERY_ICON,
        FooterItem::PageTurningInverted => {
            if inverted {
                PAGE_TURN_INVERTED_ICON
            } else {
                PAGE_TURN_REGULAR_ICON
            }
        }
        FooterItem::CustomText => return None,
    })
}

pub(super) fn fixed_chapters(reading: &FixedReading) -> Vec<(usize, String)> {
    let mut chapters: Vec<_> = reading
        .chapter_outline()
        .filter_map(|item| {
            let page = item.page.filter(|&page| page < reading.count())?;
            Some((page, item.title.clone()))
        })
        .collect();
    sort_chapters(&mut chapters);
    chapters
}

fn sort_chapters(chapters: &mut Vec<(usize, String)>) {
    chapters.sort_by_key(|(page, _)| *page);
    chapters.dedup_by_key(|(page, _)| *page);
}

impl Probe {
    fn footer_sections(&self) -> &[(usize, String)] {
        self.footer_chapters.get_or_init(|| {
            let mut sections: Vec<_> = self
                .local_contents_pages()
                .iter()
                .map(|&(index, page)| (page, self.toc[index].label.clone()))
                .collect();
            sort_chapters(&mut sections);
            sections
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum PageSections<'a> {
    Publisher(&'a [(usize, String)]),
    Custom(&'a [Bookmark]),
}

impl<'a> PageSections<'a> {
    fn len(self) -> usize {
        match self {
            Self::Publisher(entries) => entries.len(),
            Self::Custom(entries) => entries.len(),
        }
    }

    pub(super) fn before(self, page: usize) -> usize {
        match self {
            Self::Publisher(entries) => entries.partition_point(|(p, _)| *p < page),
            Self::Custom(entries) => entries.partition_point(|entry| entry.position.page < page),
        }
    }

    pub(super) fn at(self, index: usize) -> Option<(usize, &'a str)> {
        match self {
            Self::Publisher(entries) => entries
                .get(index)
                .map(|(page, title)| (*page, title.as_str())),
            Self::Custom(entries) => entries
                .get(index)
                .map(|entry| (entry.position.page, entry.label.as_str())),
        }
    }

    fn for_marker_pages(self, selected: Option<&[usize]>, mut visit: impl FnMut(usize)) {
        if let Some(selected) = selected {
            selected.iter().copied().for_each(visit);
        } else {
            for (page, _) in (0..self.len()).filter_map(|index| self.at(index)) {
                visit(page);
            }
        }
    }

    fn position(
        self,
        page: usize,
        flow: Option<(&crate::library::hidden_flows::PageFlows, usize)>,
    ) -> (usize, usize) {
        let mut current = 0;
        let mut count = 0;
        for index in 0..self.len() {
            let Some((start, _)) = self.at(index) else {
                continue;
            };
            if flow.is_some_and(|(flows, active)| flows.flow(start) != active) {
                continue;
            }
            count += 1;
            if start <= page {
                current = count;
            }
        }
        (current.max(1), count.max(1))
    }
}

struct Progress<'a> {
    section_range: std::ops::Range<usize>,
    section_index: usize,
    section_count: usize,
    section_name: &'a str,
    section_page: usize,
    section_len: usize,
    bound_screen_pages_left: Option<usize>,
    book_pages_left: usize,
    future_pages_estimated: bool,
    // Fixed documents use physical pages. Reflow estimates weight spine items by source size.
    book_fraction: f64,
    estimated: bool,
    book_ticks: Ticks<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FooterLocation {
    book_id: u64,
    chapter: usize,
    flow: usize,
    section: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct InitialPosition {
    location: FooterLocation,
    book_fraction: f64,
    chapter_fraction: f64,
}

enum Ticks<'a> {
    Pages(PageSections<'a>, usize),
    FlowPages(
        PageSections<'a>,
        crate::library::hidden_flows::PageFlows,
        usize,
    ),
    Bytes(&'a [u64]),
}

impl Ticks<'_> {
    fn draw(&self, selected: Option<&[usize]>, mut mark: impl FnMut(f64)) {
        match self {
            Self::Pages(sections, count) => {
                sections.for_marker_pages(selected, |page| {
                    mark(page as f64 / *count as f64);
                });
            }
            Self::FlowPages(sections, flows, flow) => {
                let total = flows.count_in(*flow, 0..usize::MAX).max(1);
                sections.for_marker_pages(selected, |page| {
                    if selected.is_none() || flows.flow(page) == *flow {
                        mark(flows.count_in(*flow, 0..page) as f64 / total as f64);
                    }
                });
            }
            Self::Bytes(offsets) => {
                if let Some(&length) = offsets.last().filter(|&&n| n > 0) {
                    for &offset in *offsets {
                        mark(offset as f64 / length as f64);
                    }
                }
            }
        }
    }
}

impl<'a> Progress<'a> {
    fn from_pages(
        page: usize,
        count: usize,
        sections: PageSections<'a>,
        fallback: &'a str,
    ) -> Self {
        let count = count.max(1);
        let page = page.min(count - 1);
        let next = sections.before(page.saturating_add(1));
        let (start, name) = next
            .checked_sub(1)
            .and_then(|index| sections.at(index))
            .unwrap_or((0, fallback));
        let end = sections.at(next).map_or(count, |(end, _)| end);
        let (section_index, section_count) = sections.position(page, None);
        Self {
            section_range: start..end,
            section_index,
            section_count,
            section_name: name,
            section_page: page - start + 1,
            section_len: end - start,
            bound_screen_pages_left: None,
            book_pages_left: count - page - 1,
            future_pages_estimated: false,
            book_fraction: (page + 1) as f64 / count as f64,
            estimated: false,
            book_ticks: Ticks::Pages(sections, count),
        }
    }

    fn apply_flows(
        &mut self,
        flows: crate::library::hidden_flows::PageFlows,
        first: usize,
        last: usize,
        count: usize,
        sections: PageSections<'a>,
    ) {
        let flow = flows.flow(first);
        (self.section_index, self.section_count) = sections.position(first, Some((&flows, flow)));
        let total = flows.count_in(flow, 0..count).max(1);
        self.section_page = flows
            .count_in(flow, self.section_range.start..last + 1)
            .max(1);
        self.section_len = flows.count_in(flow, self.section_range.clone()).max(1);
        self.book_pages_left = flows.count_in(flow, last + 1..count);
        self.book_fraction = flows.count_in(flow, 0..last + 1) as f64 / total as f64;
        self.book_ticks = Ticks::FlowPages(sections, flows, flow);
    }

    fn percentage_with_digits(&self, digits: u8) -> String {
        let percent = self.book_fraction * 100.0;
        let percent_text = format!("{percent:.precision$}", precision = digits.min(2) as usize);
        if self.estimated && percent < 100.0 {
            format!("About {percent_text}%")
        } else {
            format!("{percent_text}%")
        }
    }

    fn pages_left(&self) -> usize {
        self.section_len - self.section_page
    }

    fn screen_pages_left(&self) -> usize {
        self.bound_screen_pages_left
            .unwrap_or_else(|| self.pages_left())
    }
}

fn bound_chapter_pages_left(
    markers: &MarkerPages,
    page: usize,
    flows: Option<&PageFlows>,
) -> usize {
    let next = markers
        .starts
        .get(markers.starts.partition_point(|&at| at <= page));
    let unread = page.saturating_add(1)..next.copied().unwrap_or(markers.count);
    let Some(flows) = flows else {
        return unread.len();
    };
    let flow = flows.flow(page);
    if next.is_some() {
        // A real next chapter counts only regular pages when reading the
        // regular flow. Inside a hidden flow, use the full screen-page range.
        return if flow == 0 {
            flows.count_in(0, unread)
        } else {
            unread.len()
        };
    }
    // With no subsequent marker, use the document's remaining reading flow.
    // The current hidden section can be finished before rejoining regular pages.
    let in_current = flows.count_in(flow, unread.clone());
    if flow == 0 {
        in_current
    } else {
        in_current + flows.count_in(0, unread)
    }
}

fn pages_left_display(pages_left: usize, include_current: bool) -> usize {
    pages_left + usize::from(include_current)
}

fn time_left_for(seconds_per_page: f64, pages: usize) -> String {
    let seconds = (seconds_per_page * pages as f64).round() as u64;
    match seconds {
        0 => "0 min".into(),
        1..60 => "Under 1 min".into(),
        _ => super::reading_time(seconds),
    }
}

fn custom_text_preview(text: &str, max_chars: usize) -> String {
    let mut characters = text.chars();
    let preview = characters.by_ref().take(max_chars).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn draw_initial_marker(canvas: &mut Canvas, bar: Rect, fraction: f64) {
    if bar.w <= 0 || bar.h <= 0 {
        return;
    }
    let marker = (bar.x as f64 + (bar.w as f64 * fraction.clamp(0.0, 1.0)).round()) as i32;
    let marker = marker.clamp(bar.x, bar.x + bar.w - 1);
    for (row, half_width) in [0, 1, 2].into_iter().enumerate() {
        canvas.fill(
            Rect::new(
                marker - half_width,
                bar.y - 3 + row as i32,
                half_width * 2 + 1,
                1,
            ),
            0,
        );
    }
    canvas.fill(Rect::new(marker, bar.y - 1, 1, bar.h + 1), 0);
}

fn ellipsis_to_fit(canvas: &mut Canvas, text: &str, max_width: u32, font_size: i32) -> String {
    let mut value = text.replace(['\n', '\r'], " ");
    if canvas.measure(&value, font_size) <= max_width as i32 {
        return value;
    }
    while !value.is_empty() {
        value.pop();
        let candidate = format!("{value}…");
        if canvas.measure(&candidate, font_size) <= max_width as i32 {
            return candidate;
        }
    }
    String::new()
}

fn draw_battery_icon(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    level: u32,
    charging: bool,
    ink: u8,
    background: u8,
) {
    canvas.fill(Rect::new(x, y, 25, 18), ink);
    canvas.fill(Rect::new(x + 2, y + 2, 21, 14), background);
    let filled = (19 * level.min(100) / 100) as i32;
    if filled > 0 {
        canvas.fill(Rect::new(x + 3, y + 4, filled, 10), ink);
    }
    canvas.fill(Rect::new(x + 25, y + 5, 3, 8), ink);
    if charging {
        for (dx, dy, width, height) in [
            (14, 3, 6, 2),
            (12, 5, 5, 2),
            (10, 7, 5, 2),
            (8, 9, 5, 2),
            (8, 11, 4, 2),
        ] {
            let bolt_center = x + dx + width / 2;
            let filled_end = x + 3 + filled;
            canvas.fill(
                Rect::new(x + dx, y + dy, width, height),
                if bolt_center < filled_end {
                    background
                } else {
                    ink
                },
            );
        }
    }
}

fn battery_footer_text(level: u32, charging: bool, hide_above: u8) -> Option<String> {
    let level = level.min(100);
    if level > u32::from(hide_above.min(100)) {
        return None;
    }
    let label = if charging { "Charging" } else { "Battery" };
    Some(format!("{label} {level}%"))
}

fn battery_symbol(level: u32, charging: bool, fully_charged: bool) -> String {
    let codepoint = if fully_charged {
        0xEDA3 // KOReader's fully-charged battery glyph.
    } else if charging {
        0xE783 // KOReader's charging battery glyph in its bundled symbol font.
    } else {
        match level {
            100.. => 0xE778,
            90..=99 => 0xE781,
            80..=89 => 0xE780,
            70..=79 => 0xE77F,
            60..=69 => 0xE77E,
            50..=59 => 0xE77D,
            40..=49 => 0xE77C,
            30..=39 => 0xE77B,
            20..=29 => 0xE77A,
            10..=19 => 0xE779,
            _ => 0xE782,
        }
    };
    char::from_u32(codepoint)
        .expect("battery symbol is a valid Unicode scalar")
        .to_string()
}

impl Reader {
    pub(super) fn set_footer_font_file(&mut self, file: Option<String>) -> io::Result<()> {
        if let Some(file) = file.as_deref() {
            self.fonts_for(Some(file))?;
        }
        super::footer_font_face::save(
            &self.footer_font_path,
            &self.fonts_directory,
            file.as_deref(),
        )?;
        self.footer_font_render_path = file
            .as_deref()
            .map(|name| crate::reading_fonts::resolve(&self.fonts_directory, name))
            .transpose()?;
        self.footer_font_file = file;
        Ok(())
    }

    pub(super) fn footer_font_render_file(&self) -> Option<String> {
        self.footer_font_render_path
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
    }

    pub(super) fn footer_caption_font_px(&self) -> i32 {
        let size = i32::from(
            self.controls
                .footer_item_font_size
                .unwrap_or(14)
                .clamp(8, 36),
        );
        if size <= 14 {
            size * 2
        } else {
            28 + ((size - 14) * 12 + 11) / 22
        }
    }

    pub(super) fn footer_caption_style(&self) -> TextStyle {
        let style = TextStyle::new(self.footer_caption_font_px()).ink(SECONDARY);
        if self.controls.footer_item_bold {
            style.bold()
        } else {
            style
        }
    }

    fn reading_status_visible(&self) -> bool {
        self.stats_awake
            && matches!(
                self.screen,
                Screen::Book
                    | Screen::BookTools
                    | Screen::Fixed
                    | Screen::FixedTools
                    | Screen::Footnote
                    | Screen::OnlineLookup
                    | Screen::Skim
            )
    }

    pub(super) fn top_header_visible(&self) -> bool {
        self.active.as_ref().is_some_and(|book| {
            book.reading.config.settings.status_header_height != 0 && !book.reading.is_continuous()
        }) && (matches!(
            self.screen,
            Screen::Book
                | Screen::BookTools
                | Screen::Footnote
                | Screen::OnlineLookup
                | Screen::Skim
        ) || self.definition_over_book())
    }

    fn footer_time_visible(&self) -> bool {
        self.reading_status_visible()
            && self.controls.footer_auto_refresh
            && self.controls.status_bar == StatusBar::Custom
            && (self.controls.footer_items & FooterItem::CurrentTime.bit() != 0
                || (self.custom_footer_text_enabled()
                    && ['D', 'd', 'm', 'M']
                        .into_iter()
                        .any(|token| self.footer_custom_text.contains_token(token))))
    }

    fn top_header_time_visible(&self) -> bool {
        self.reading_status_visible()
            && self.top_header_visible()
            && self.controls.top_header.auto_refresh
            && self.controls.top_header.clock
    }

    fn top_header_battery_visible(&self) -> bool {
        self.reading_status_visible()
            && self.top_header_visible()
            && self.controls.top_header.auto_refresh
            && self.controls.top_header.battery != HeaderBattery::Off
    }

    fn footer_battery_visible(&self) -> bool {
        self.reading_status_visible()
            && self.controls.footer_auto_refresh
            && self.controls.status_bar == StatusBar::Custom
            && (self.controls.footer_items & FooterItem::BatteryPercentage.bit() != 0
                || (self.custom_footer_text_enabled()
                    && ['b', 'B']
                        .into_iter()
                        .any(|token| self.footer_custom_text.contains_token(token))))
    }

    fn custom_footer_text_enabled(&self) -> bool {
        self.controls.status_bar == StatusBar::Custom
            && self.controls.footer_items & FooterItem::CustomText.bit() != 0
    }

    fn footer_clock_active(&self) -> bool {
        self.footer_time_visible()
            || self.top_header_time_visible()
            || self.top_header_battery_visible()
            || self.footer_battery_visible()
    }

    pub(super) fn clock_refresh_visible(&self) -> bool {
        self.footer_time_visible() || self.top_header_time_visible()
    }

    fn status_battery_reading(&self) -> Option<(u32, bool)> {
        if !self.top_header_battery_visible() && !self.footer_battery_visible() {
            return None;
        }
        primitive_device::battery().map(|percent| (percent.min(100), primitive_device::charging()))
    }

    pub(super) fn refresh_status_battery_changed(&mut self) -> bool {
        let current = self.status_battery_reading();
        let changed = current != self.status_battery_last;
        self.status_battery_last = current;
        changed
    }

    pub(super) fn update_footer_clock(&mut self, now: Instant) {
        self.footer_refresh_at = if self.footer_clock_active() {
            current_unix_seconds().and_then(|seconds| {
                let delay = 61 - seconds.rem_euclid(60) as u64;
                now.checked_add(Duration::from_secs(delay))
            })
        } else {
            None
        };
        self.status_battery_last = self.status_battery_reading();
    }

    pub(super) fn reset_footer_initial_position(&mut self) {
        let initial = self.footer_progress().and_then(|progress| {
            let location = self.footer_location(&progress)?;
            Some(InitialPosition {
                location,
                book_fraction: progress.book_fraction,
                chapter_fraction: progress.section_page as f64 / progress.section_len.max(1) as f64,
            })
        });
        self.footer_initial_position = initial;
    }

    #[cfg(test)]
    pub(super) fn footer_initial_fraction(&self) -> Option<f64> {
        let progress = self.footer_progress()?;
        self.initial_marker_fraction(&progress)
    }

    fn footer_location(&self, progress: &Progress<'_>) -> Option<FooterLocation> {
        if let Some(active) = &self.active {
            let flow = active
                .reading
                .custom_flow_map()
                .map_or(0, |flows| flows.flow(active.reading.page));
            return Some(FooterLocation {
                book_id: active.id,
                chapter: active.reading.chapter,
                flow,
                section: progress.section_index,
            });
        }
        let fixed = self.fixed.as_ref()?;
        let flow = fixed
            .reading
            .hidden_flow_map()
            .map_or(0, |flows| flows.flow(fixed.reading.page()));
        Some(FooterLocation {
            book_id: fixed.id,
            chapter: 0,
            flow,
            section: progress.section_index,
        })
    }

    fn initial_marker_fraction(&self, progress: &Progress<'_>) -> Option<f64> {
        if !self.controls.footer_initial_marker {
            return None;
        }
        let initial = self.footer_initial_position?;
        let current = self.footer_location(progress)?;
        if initial.location.book_id != current.book_id || initial.location.flow != current.flow {
            return None;
        }
        match self.controls.footer_scope {
            ProgressScope::Book => Some(initial.book_fraction),
            ProgressScope::Chapter
                if initial.location.chapter == current.chapter
                    && initial.location.section == current.section =>
            {
                Some(initial.chapter_fraction)
            }
            ProgressScope::Chapter | ProgressScope::Off => None,
        }
    }

    pub(super) fn device_reading_settings(&self) -> crate::Settings {
        crate::Settings {
            height: if self.controls.overlap_footer {
                HEIGHT
            } else {
                PAGE_HEIGHT
            },
            ..super::device_settings()
        }
    }

    pub(super) fn overlapping_book_page(&self) -> bool {
        self.controls.overlap_footer
            && match self.screen {
                Screen::Book | Screen::BookTools => self.active.is_some(),
                Screen::Fixed | Screen::FixedTools => self.fixed.is_some(),
                _ => false,
            }
    }

    pub(super) fn hidden_overlap_footer(&self) -> bool {
        self.overlapping_book_page()
            && self.controls.status_bar == StatusBar::Minimal
            && self.notice.is_none()
            && self.search_result.is_none()
            && self.selection.is_none()
            && !self.touch_locked
            && self.pdf_selection.is_none()
            && self.ocr.is_none()
            && self.next_comic().is_none()
            && self
                .active
                .as_ref()
                .is_none_or(|b| !b.reading.has_page_error())
    }

    pub(super) fn overlay_footer_input(&mut self, input: primitive_ui::Input) -> Option<Change> {
        use primitive_ui::Input;
        if !self.overlapping_book_page()
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.search_result.is_some()
            || self.ocr.is_some()
            || self.next_comic().is_some()
        {
            return None;
        }
        let (Input::Tap(x, y) | Input::LongTap(x, y)) = input else {
            return None;
        };
        if y < PAGE_HEIGHT as f32 {
            return None;
        }
        if DONE.contains(x, y) {
            return Some(if matches!(input, Input::Tap(..)) {
                self.exit()
            } else {
                Change::None
            });
        }
        if self.fixed.is_some() && !self.hidden_overlap_footer() && x >= self.footer_right() as f32
        {
            return None;
        }
        if self.hidden_overlap_footer() {
            if !SHOW_STATUS.contains(x, y) {
                return None;
            }
            if matches!(input, Input::LongTap(..)) || self.controls.footer_tap_locked {
                self.show(Screen::FooterSettings);
                return Some(Change::Partial);
            }
        }
        Some(
            match input {
                Input::Tap(..) => self.tap_footer(x, y),
                Input::LongTap(..) => self.hold_footer(x, y),
                _ => None,
            }
            .unwrap_or(Change::None),
        )
    }

    pub(super) fn fixed_sections<'a>(&'a self, fixed: &'a ActiveFixed) -> PageSections<'a> {
        let custom = self
            .library
            .state(fixed.id)
            .map(|state| &state.custom_contents);
        if let Some(chapters) = custom.filter(|chapters| chapters.enabled) {
            let entries = chapters.entries();
            // CustomContents orders fixed-page positions before reflow anchors in each chapter.
            // A restored entry beyond this edition's last page cannot define a chapter here.
            let end = entries.partition_point(|entry| {
                let p = &entry.position;
                p.chapter == 0 && p.anchor.is_none() && p.page < fixed.reading.count()
            });
            PageSections::Custom(&entries[..end])
        } else {
            PageSections::Publisher(&fixed.footer_chapters)
        }
    }

    fn footer_progress(&self) -> Option<Progress<'_>> {
        if let Some(fixed) = &self.fixed {
            let reading = &fixed.reading;
            let last = if reading.is_spread() {
                reading
                    .visible_pages()
                    .max()
                    .unwrap_or_else(|| reading.page())
            } else {
                reading.page()
            };
            let sections = self.fixed_sections(fixed);
            let mut progress = Progress::from_pages(last, reading.count(), sections, "Document");
            if let Some(flows) = reading.hidden_flow_map() {
                progress.apply_flows(
                    flows.clone(),
                    reading.page(),
                    last,
                    reading.count(),
                    sections,
                );
            }
            progress.bound_screen_pages_left = self.selected_chapter_pages_left(&progress, last);
            return Some(progress);
        }
        let reading = &self.active.as_ref()?.reading;
        let probe = &reading.probe;
        let mut progress = Progress::from_pages(
            reading.last_visible_page(),
            probe.page_count(),
            PageSections::Publisher(probe.footer_sections()),
            &probe.chapter_label,
        );
        if let Some(flows) = reading.custom_flow_map() {
            progress.apply_flows(
                flows,
                reading.page,
                reading.last_visible_page(),
                probe.page_count(),
                PageSections::Publisher(probe.footer_sections()),
            );
        }
        if probe.chapter_count() > 1 {
            let ch = reading.chapter;
            let weights = reading.progress_weights();
            // A publication with an unreadable later item can still display the current chapter.
            // An index based on chapter count is the fallback when sizes were unavailable.
            progress.estimated = true;
            progress.book_ticks = Ticks::Bytes(weights);
            progress.future_pages_estimated = match (weights.get(ch + 1), weights.last()) {
                (Some(end), Some(total)) => end < total,
                _ => ch + 1 < probe.chapter_count(),
            };
            let later_pages = match (weights.get(ch..=ch + 1), weights.last()) {
                (Some([start, end]), Some(&total)) if end > start => {
                    ((total - end) as f64 * probe.page_count() as f64 / (end - start) as f64)
                        .round() as usize
                }
                _ => probe
                    .chapter_count()
                    .saturating_sub(ch + 1)
                    .saturating_mul(probe.page_count()),
            };
            progress.book_pages_left = progress.book_pages_left.saturating_add(later_pages);
            progress.book_fraction = match (weights.get(ch..=ch + 1), weights.last()) {
                (Some([start, end]), Some(&length)) if length > 0 => {
                    (*start as f64 + progress.book_fraction * (end - start) as f64) / length as f64
                }
                _ => (ch as f64 + progress.book_fraction) / probe.chapter_count() as f64,
            };
        } else {
            progress.bound_screen_pages_left =
                self.selected_chapter_pages_left(&progress, reading.last_visible_page());
        }
        Some(progress)
    }

    fn selected_chapter_pages_left(&self, progress: &Progress<'_>, page: usize) -> Option<usize> {
        let state = self.library.state(self.open_book_id()?)?;
        let levels = &state.contents_depths;
        if !levels.bind_navigation || !levels.has_ignored() {
            return None;
        }
        let markers = self.whole_book_marker_pages(true)?;
        let flows = match &progress.book_ticks {
            Ticks::FlowPages(_, flows, _) => Some(flows),
            _ => None,
        };
        Some(bound_chapter_pages_left(&markers, page, flows))
    }

    pub(super) fn chapter_pages_use_edition_labels(&self) -> bool {
        self.publisher_footer_enabled()
            && self
                .active
                .as_ref()
                .is_some_and(|book| book.page_labels() > 0)
    }

    fn displayed_chapter_pages_left(&self, progress: &Progress<'_>) -> usize {
        // Printed and character-count indices need their own section mapping.
        // Duration still uses the exactly resolved screen-page count.
        if self.chapter_pages_use_edition_labels() {
            progress.pages_left()
        } else {
            progress.screen_pages_left()
        }
    }

    pub(super) fn status_caption(&self) -> Option<String> {
        let flow = self
            .active
            .as_ref()
            .and_then(|book| {
                book.reading.custom_flow_map().map(|map| {
                    book.reading
                        .hidden_flow_ordinal(map.flow(book.reading.page))
                        .unwrap_or(0)
                })
            })
            .or_else(|| {
                self.fixed.as_ref().and_then(|book| {
                    book.reading
                        .hidden_flow_map()
                        .map(|map| map.flow_ordinal(map.flow(book.reading.page())).unwrap_or(0))
                })
            });
        let base = self.progress_caption().map(|caption| {
            if caption.is_empty() {
                return caption;
            }
            flow.map_or(caption.clone(), |flow| {
                if flow == 0 {
                    format!("Custom{}{}", self.controls.footer_separator.text(), caption)
                } else {
                    format!(
                        "Hidden {flow}{}{}",
                        self.controls.footer_separator.text(),
                        caption
                    )
                }
            })
        });
        if self.controls.status_bar == StatusBar::Minimal {
            return base;
        }
        let Some(remaining) = self.read_alarm.footer() else {
            return base;
        };
        let page = base
            .filter(|s| !s.is_empty())
            .or_else(|| self.fixed.as_ref().map(|f| f.reading.page_status()))
            .or_else(|| self.active.as_ref().map(|a| a.reading.page_status()));
        let separator = self.controls.footer_separator.text();
        Some(match page {
            Some(label) => format!("{label}{separator}Timer {remaining}"),
            None => format!("Timer {remaining}"),
        })
    }

    fn top_header_parts(&self) -> Option<(String, String)> {
        let header = self.controls.top_header;
        if !self.top_header_visible() {
            return None;
        }
        let active = self.active.as_ref()?;
        let progress = self.footer_progress();
        let mut identity = Vec::new();
        if header.title {
            let title = active.reading.probe.title.trim();
            if !title.is_empty() {
                identity.push(title.to_owned());
            }
        }
        if header.author {
            if let Some(author) = active
                .reading
                .probe
                .author
                .as_deref()
                .map(str::trim)
                .filter(|author| !author.is_empty())
            {
                identity.push(author.to_owned());
            }
        }

        let mut status = Vec::new();
        if header.clock {
            if let Some(time) = local_footer_time() {
                status.push(time);
            }
        }
        if header.page_number || header.page_count {
            if let Some((current, last)) = self.top_header_page_labels() {
                status.push(match (header.page_number, header.page_count) {
                    (true, true) => format!("{current} / {last}"),
                    (true, false) => current,
                    (false, _) => last,
                });
            } else {
                let (page, count) = active.reading.whole_book_screen_pages().unwrap_or_else(|| {
                    (active.reading.page + 1, active.reading.probe.page_count())
                });
                status.push(match (header.page_number, header.page_count) {
                    (true, true) => format!("{page}/{count}"),
                    (true, false) => page.to_string(),
                    (false, _) => format!("of {count}"),
                });
            }
        }
        if header.reading_percent
            && let Some(progress) = &progress
        {
            status.push(progress.percentage_with_digits(self.controls.footer_percentage_digits));
        }
        if header.battery == HeaderBattery::Percentage
            && let Some(level) = primitive_device::battery()
        {
            status.push(format!("{level}%"));
        }
        Some((identity.join("  ·  "), status.join("  ·  ")))
    }

    pub(super) fn top_header_caption(&self) -> Option<String> {
        let (identity, status) = self.top_header_parts()?;
        Some(match (identity.is_empty(), status.is_empty()) {
            (true, true) => String::new(),
            (false, true) => identity,
            (true, false) => status,
            (false, false) => format!("{identity}  ·  {status}"),
        })
    }

    pub(super) fn draw_top_header(&self, canvas: &mut Canvas) {
        let header = self.controls.top_header;
        let Some((identity, status)) = self.top_header_parts() else {
            return;
        };
        let night = self.uses_night_pages();
        let background = if night { 0 } else { 255 };
        let ink = if night { 255 } else { 0 };
        canvas.fill(
            Rect::new(0, 0, WIDTH as i32, TopHeader::HEIGHT as i32),
            background,
        );
        let font_size = i32::from(header.font_size());
        let baseline = Rect::new(20, 4, WIDTH as i32 - 40, 38);
        let battery_width = if header.battery == HeaderBattery::Icon {
            42
        } else {
            0
        };
        let right_width = canvas.measure(&status, font_size).min(540) as i32;
        let right_edge = baseline.right() - battery_width;
        let right_x = (right_edge - right_width).max(baseline.x + 180);
        let left_width = (right_x - baseline.x - 20).max(80);
        let left = ellipsis_to_fit(canvas, &identity, left_width as u32, font_size);
        let right = ellipsis_to_fit(canvas, &status, (right_edge - right_x) as u32, font_size);
        let text_style = TextStyle::new(font_size).ink(ink);
        canvas.text(
            Rect::new(baseline.x, baseline.y, left_width, baseline.h),
            &left,
            text_style,
        );
        canvas.text(
            Rect::new(right_x, baseline.y, right_edge - right_x, baseline.h),
            &right,
            text_style,
        );
        if header.battery == HeaderBattery::Icon
            && let Some(level) = primitive_device::battery()
        {
            draw_battery_icon(
                canvas,
                baseline.right() - 34,
                11,
                level,
                primitive_device::charging(),
                ink,
                background,
            );
        }
        if header.chapter_marks {
            canvas.fill(
                Rect::new(12, TopHeader::HEIGHT as i32 - 2, WIDTH as i32 - 24, 1),
                HAIRLINE,
            );
            let count = self
                .active
                .as_ref()
                .map_or(0, |book| book.reading.probe.page_count())
                .max(1);
            if let Some(book) = &self.active {
                let mut mark = |page: usize| {
                    let x = 12 + ((WIDTH - 24) as f64 * page as f64 / count as f64).round() as i32;
                    canvas.fill(Rect::new(x, TopHeader::HEIGHT as i32 - 5, 2, 4), ink);
                };
                if let Some(selected) = self.customized_header_marker_pages(book) {
                    selected.into_iter().for_each(mark);
                } else {
                    for (page, _) in book.reading.probe.footer_sections() {
                        mark(*page);
                    }
                }
            }
        } else {
            canvas.fill(
                Rect::new(0, TopHeader::HEIGHT as i32 - 1, WIDTH as i32, 1),
                HAIRLINE,
            );
        }
    }

    pub(super) fn alarm_progress(&self, token: char) -> Option<String> {
        let progress = self.footer_progress()?;
        match token {
            'l' => Some(self.displayed_chapter_pages_left(&progress).to_string()),
            'C' => Some(progress.section_name.to_owned()),
            'P' => Some(format!(
                "{:.0}",
                progress.section_page as f64 * 100. / progress.section_len.max(1) as f64
            )),
            'p' => Some(format!("{:.0}", progress.book_fraction * 100.)),
            'H' | 'h' => {
                let id = self
                    .fixed
                    .as_ref()
                    .map(|f| f.id)
                    .or_else(|| self.active.as_ref().map(|a| a.id))?;
                let pages = if token == 'H' {
                    progress.book_pages_left
                } else {
                    progress.screen_pages_left()
                };
                Some(super::reading_time(
                    (self.pace.rate(id) * pages as f64).round() as u64,
                ))
            }
            _ => None,
        }
    }

    fn progress_caption(&self) -> Option<String> {
        let mode = self.controls.status_bar;
        if mode == StatusBar::Minimal {
            return Some(String::new());
        }
        let printed = || {
            let active = self.active.as_ref()?;
            self.publisher_footer_enabled()
                .then(|| self.current_publisher_page())
                .flatten()
                .and_then(|row| active.page_label(row))
                .map(|label| {
                    format!(
                        "{} page {}",
                        if active.numbering.is_some() {
                            "Stable"
                        } else {
                            "Printed"
                        },
                        super::publisher_pages::compact_label(&label)
                    )
                })
        };
        if !self.controls.footer_all_at_once && mode == StatusBar::PageNumbers {
            return printed().or_else(|| self.custom_page_status());
        }
        let p = self.footer_progress()?;
        let separator = self.controls.footer_separator.text();
        let percentage = p.percentage_with_digits(self.controls.footer_percentage_digits);
        if mode == StatusBar::Custom {
            return Some(self.custom_footer_caption(&p));
        }
        let time = |scope| {
            let (pages, unit) = if scope == ProgressScope::Book {
                (p.book_pages_left, "book")
            } else {
                (p.screen_pages_left(), "chapter")
            };
            let id = self
                .fixed
                .as_ref()
                .map(|b| b.id)
                .or_else(|| self.active.as_ref().map(|b| b.id))?;
            let seconds = (self.pace.rate(id) * pages as f64).round() as u64;
            let duration = match seconds {
                0 => "0 min".to_owned(),
                1..60 => "Under 1 min".to_owned(),
                _ => super::reading_time(seconds),
            };
            let approximate = if scope == ProgressScope::Book
                && p.future_pages_estimated
                && pages > 0
                && seconds >= 60
            {
                "About "
            } else {
                ""
            };
            Some(format!("{approximate}{duration} left in {unit}"))
        };
        if self.controls.footer_all_at_once {
            if let Some(scope @ (ProgressScope::Book | ProgressScope::Chapter)) =
                self.controls.footer_combined_time
            {
                return Some(format!("{percentage}{separator}{}", time(scope)?));
            }
            return Some(format!(
                "{percentage} of book{separator}{}{separator}{} left",
                printed().unwrap_or_else(|| format!("{} / {}", p.section_page, p.section_len)),
                pages_left_display(
                    self.displayed_chapter_pages_left(&p),
                    self.controls.footer_pages_left_include_current,
                )
            ));
        }
        match mode {
            StatusBar::ChapterTime => time(ProgressScope::Chapter),
            StatusBar::BookTime => time(ProgressScope::Book),
            StatusBar::Progress => Some(format!("{percentage} of book")),
            StatusBar::Chapter => Some(format!(
                "Chapter {} of {} - Page {} of {}",
                p.section_index, p.section_count, p.section_page, p.section_len,
            )),
            StatusBar::Remaining => {
                let pages_left = pages_left_display(
                    self.displayed_chapter_pages_left(&p),
                    self.controls.footer_pages_left_include_current,
                );
                let unit = if pages_left == 1 { "page" } else { "pages" };
                Some(match self.bound_footer_chapter_title() {
                    Some("") => format!("{pages_left} {unit} left"),
                    title => format!(
                        "{}{separator}{pages_left} {unit} left",
                        title.unwrap_or(p.section_name)
                    ),
                })
            }
            _ => None,
        }
    }

    fn custom_footer_parts(&self, progress: &Progress<'_>) -> Vec<(FooterItem, String, bool)> {
        let (entry, id) = if let Some(book) = &self.fixed {
            (&book.entry, book.id)
        } else if let Some(book) = &self.active {
            (&book.entry, book.id)
        } else {
            return Vec::new();
        };
        let time_left = |scope| {
            let pages = if scope == ProgressScope::Book {
                progress.book_pages_left
            } else {
                progress.screen_pages_left()
            };
            let seconds = (self.pace.rate(id) * pages as f64).round() as u64;
            super::reading_time(seconds)
        };
        let left_in_chapter = pages_left_display(
            self.displayed_chapter_pages_left(progress),
            self.controls.footer_pages_left_include_current,
        );
        let left_in_book = pages_left_display(
            progress.book_pages_left,
            self.controls.footer_pages_left_include_current,
        );
        let percentage = progress.percentage_with_digits(self.controls.footer_percentage_digits);
        let page_count = self
            .fixed
            .as_ref()
            .map(|book| book.reading.count())
            .or_else(|| {
                self.active
                    .as_ref()
                    .map(|book| book.reading.probe.page_count())
            })
            .unwrap_or(0);
        let page_count_label = self
            .fixed
            .as_ref()
            .and_then(|book| book.reading.page_label(page_count.saturating_sub(1)))
            .or_else(|| {
                self.active.as_ref().and_then(|book| {
                    book.page_label(book.page_labels().saturating_sub(1))
                        .map(|label| label.into_owned())
                })
            })
            .unwrap_or_else(|| page_count.to_string());
        let current_page_label = self
            .fixed
            .as_ref()
            .and_then(|book| book.reading.page_label(book.reading.page()))
            .or_else(|| {
                self.active.as_ref().and_then(|book| {
                    book.page_label(book.reading.page)
                        .map(|label| label.into_owned())
                })
            })
            .unwrap_or_else(|| {
                self.fixed
                    .as_ref()
                    .map(|book| book.reading.page() + 1)
                    .or_else(|| self.active.as_ref().map(|book| book.reading.page + 1))
                    .unwrap_or(0)
                    .to_string()
            });
        let book_time_left = time_left_for(self.pace.rate(id), progress.book_pages_left);
        let chapter_time_left = time_left_for(self.pace.rate(id), progress.screen_pages_left());
        let chapter_title = std::cell::OnceCell::new();
        let chapter_title = || {
            *chapter_title.get_or_init(|| {
                self.bound_footer_chapter_title()
                    .unwrap_or(progress.section_name)
            })
        };
        let battery = std::cell::OnceCell::new();
        let battery = || battery.get_or_init(primitive_device::battery_status);
        let clock = std::cell::OnceCell::new();
        let clock = || {
            clock
                .get_or_init(super::footer_custom_text::local_clock)
                .as_ref()
        };
        let custom_text = || {
            self.footer_custom_text.expand(|token| match token {
                'T' => Some(entry.title.clone()),
                'A' => entry.author.clone(),
                'S' => self.active.as_ref().and_then(|book| {
                    book.reading.probe.first_series.as_ref().map(|series| {
                        series.position.as_ref().map_or_else(
                            || series.name.clone(),
                            |position| format!("{} #{position}", series.name),
                        )
                    })
                }),
                't' => Some(page_count_label.clone()),
                'c' => Some(current_page_label.clone()),
                'l' => Some(left_in_chapter.to_string()),
                'p' => Some((progress.book_fraction * 100.0).round().to_string()),
                'H' => Some(book_time_left.clone()),
                'C' => Some(chapter_title().to_owned()),
                'P' => Some(
                    (progress.section_page as f64 * 100.0 / progress.section_len.max(1) as f64)
                        .round()
                        .to_string(),
                ),
                'h' => Some(chapter_time_left.clone()),
                'F' => Some(entry.path.to_string_lossy().into_owned()),
                'f' => entry
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
                'b' => battery().percent.map(|percent| percent.to_string()),
                'B' => {
                    let status = battery();
                    status.percent.map(|percent| {
                        battery_symbol(percent, status.charging, status.fully_charged)
                    })
                }
                'r' => Some(self.controls.footer_separator.text().to_owned()),
                'D' => clock().map(|clock| clock.date.clone()),
                'd' => clock().map(|clock| clock.short_date.clone()),
                'm' => clock().map(|clock| clock.time.clone()),
                'M' => clock().map(|clock| clock.time_with_seconds.clone()),
                _ => None,
            })
        };
        let current_page = || {
            if let Some(book) = &self.fixed {
                let pages: Vec<_> = book.reading.visible_pages().collect();
                let first = pages
                    .first()
                    .copied()
                    .unwrap_or_else(|| book.reading.page())
                    + 1;
                let last = pages.last().copied().unwrap_or(first - 1) + 1;
                let range = if first == last {
                    first.to_string()
                } else {
                    format!("{first}–{last}")
                };
                Some(format!("Page {range} / {}", book.reading.count()))
            } else {
                let book = self.active.as_ref()?;
                let first = book.reading.page + 1;
                let last = book.reading.last_visible_page() + 1;
                let range = if first == last {
                    first.to_string()
                } else {
                    format!("{first}–{last}")
                };
                Some(format!(
                    "Page {range} / {}",
                    book.reading.probe.page_count()
                ))
            }
        };
        let mut items = Vec::new();
        for item in FooterItem::ordered(self.controls.footer_order) {
            if self.controls.footer_items & item.bit() == 0 {
                continue;
            }
            let text = match item {
                FooterItem::CurrentPage => current_page(),
                FooterItem::ChapterTitle => {
                    let title = chapter_title().trim();
                    (!title.is_empty()).then(|| title.to_owned())
                }
                FooterItem::BookProgress => Some(percentage.clone()),
                FooterItem::ChapterPagesLeft => Some(format!(
                    "{} {} left in chapter",
                    left_in_chapter,
                    if left_in_chapter == 1 {
                        "page"
                    } else {
                        "pages"
                    }
                )),
                FooterItem::BookPagesLeft => Some(format!(
                    "{} {} left in book",
                    left_in_book,
                    if left_in_book == 1 { "page" } else { "pages" }
                )),
                FooterItem::ChapterTimeLeft => Some(format!(
                    "{} left in chapter",
                    time_left(ProgressScope::Chapter)
                )),
                FooterItem::BookTimeLeft => {
                    Some(format!("{} left in book", time_left(ProgressScope::Book)))
                }
                FooterItem::BookTitle => {
                    (!entry.title.trim().is_empty()).then(|| entry.title.trim().to_owned())
                }
                FooterItem::Author => entry
                    .author
                    .as_deref()
                    .map(str::trim)
                    .filter(|author| !author.is_empty())
                    .map(str::to_owned),
                FooterItem::BookmarkCount => Some(format!(
                    "{} bookmarks",
                    self.library
                        .state(id)
                        .map_or(0, |state| state.bookmarks.len())
                )),
                FooterItem::CurrentTime => clock().map(|time| format!("Time {}", time.time)),
                FooterItem::BatteryPercentage => {
                    let status = battery();
                    status.percent.and_then(|percent| {
                        battery_footer_text(
                            percent,
                            status.charging,
                            self.controls.footer_battery_hide_threshold.unwrap_or(100),
                        )
                    })
                }
                FooterItem::PageTurningInverted => match self.controls.footer_symbol_style {
                    FooterSymbolStyle::Icons | FooterSymbolStyle::Compact => Some(String::new()),
                    FooterSymbolStyle::Letters => Some(if self.page_turning_inverted() {
                        "On".to_owned()
                    } else {
                        "Off".to_owned()
                    }),
                },
                FooterItem::CustomText => Some(custom_text()),
            };
            if let Some(text) = text {
                items.push((
                    item,
                    text,
                    item == FooterItem::CustomText && self.footer_custom_text.is_merge_separator(),
                ));
            }
        }
        items
    }

    fn page_turning_inverted(&self) -> bool {
        self.controls.invert_page_turn_buttons
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.reading.probe.pages_right_to_left())
            || self
                .fixed
                .as_ref()
                .is_some_and(|fixed| fixed.reading.pages_right_to_left())
    }

    fn custom_footer_caption(&self, progress: &Progress<'_>) -> String {
        let items = self.custom_footer_parts(progress);
        if items.is_empty() {
            "No items selected".to_owned()
        } else {
            let separator = self.controls.footer_separator.text();
            items.iter().enumerate().fold(
                String::new(),
                |mut caption, (index, (item, text, merge))| {
                    let prior_merge = index > 0 && items[index - 1].2;
                    if index > 0 && !prior_merge && !merge {
                        caption.push_str(separator);
                    }
                    caption.push_str(
                        self.controls
                            .footer_symbol_style
                            .prefix_for(*item, self.page_turning_inverted()),
                    );
                    caption.push_str(text);
                    caption
                },
            )
        }
    }

    pub(super) fn custom_footer_icons_enabled(&self) -> bool {
        self.controls.status_bar == StatusBar::Custom
            && self.controls.footer_symbol_style == FooterSymbolStyle::Icons
            && self.footer_progress().is_some_and(|progress| {
                self.custom_footer_parts(&progress)
                    .iter()
                    .any(|(item, _, _)| {
                        footer_item_icon(*item, self.page_turning_inverted()).is_some()
                    })
            })
    }

    pub(super) fn custom_footer_icon_width(&self) -> i32 {
        let Some(progress) = self.footer_progress() else {
            return 0;
        };
        self.custom_footer_parts(&progress)
            .iter()
            .filter(|(item, _, _)| footer_item_icon(*item, self.page_turning_inverted()).is_some())
            .count() as i32
            * (FOOTER_ICON_SIZE + FOOTER_ICON_GAP)
    }

    pub(super) fn draw_custom_footer_icons(
        &self,
        canvas: &mut Canvas,
        status_rect: Rect,
        status_style: TextStyle,
        font_size: i32,
    ) {
        let Some(progress) = self.footer_progress() else {
            return;
        };
        let items = self.custom_footer_parts(&progress);
        let separator = self.controls.footer_separator.text();
        let mut x = status_rect.x;
        for (index, (item, text, merge)) in items.iter().enumerate() {
            let prior_merge = index > 0 && items[index - 1].2;
            if index != 0 && !prior_merge && !merge {
                let width = canvas.measure(separator, font_size);
                canvas.text(
                    Rect::new(x, status_rect.y, width, status_rect.h),
                    separator,
                    status_style,
                );
                x += width;
            }
            if let Some(icon) = footer_item_icon(*item, self.page_turning_inverted()) {
                let top = status_rect.y + (status_rect.h - FOOTER_ICON_SIZE) / 2;
                canvas.icon(
                    Rect::new(x, top, FOOTER_ICON_SIZE, FOOTER_ICON_SIZE),
                    icon,
                    0,
                );
                x += FOOTER_ICON_SIZE + FOOTER_ICON_GAP;
            }
            let width = canvas.measure(text, font_size);
            canvas.text(
                Rect::new(x, status_rect.y, width.max(1), status_rect.h),
                text,
                status_style,
            );
            x += width;
        }
    }

    fn footer_right(&self) -> i32 {
        // Zoom and Done keep their own touch targets in the same row.
        if self
            .fixed
            .as_ref()
            .is_some_and(|fixed| !fixed.reading.is_spread())
        {
            crate::fixed_session::SMALLER.x
        } else {
            DONE.x
        }
    }

    fn footer_track_layout(&self, right: i32, status_text_width: i32) -> (i32, i32, i32) {
        let margin = i32::from(
            self.controls
                .footer_track_margin_width
                .unwrap_or(10)
                .min(140),
        );
        let track_right = (right - margin).max(margin);
        let minimum_pct = i32::from(
            self.controls
                .footer_track_min_width_pct
                .unwrap_or(20)
                .clamp(5, 90),
        );
        let minimum_track_width = (WIDTH as i32 * minimum_pct + 99) / 100;
        let first_track_left = FOOTER_CAPTION_LEFT + FOOTER_TRACK_GAP;
        if self.controls.footer_track_lock_min_width {
            let locked_width = minimum_track_width.min((track_right - first_track_left).max(0));
            let track_left = track_right - locked_width;
            let caption_width = status_text_width
                .max(0)
                .min((track_left - first_track_left).max(0));
            (caption_width, track_left, locked_width)
        } else {
            let caption_width = status_text_width
                .max(0)
                .min((track_right - first_track_left - minimum_track_width).max(0));
            let track_left = first_track_left + caption_width;
            (caption_width, track_left, (track_right - track_left).max(0))
        }
    }

    fn footer_bottom_margin_shift(&self) -> i32 {
        (i32::from(
            self.controls
                .footer_container_bottom_margin
                .unwrap_or(1)
                .min(49),
        ) - 1)
            * 2
    }

    pub(super) fn footer_cover_top(&self, status_rect: Rect, status_text_width: i32) -> i32 {
        self.footer_track_rect(status_text_width)
            .map_or(status_rect.y, |track| status_rect.y.min(track.y))
            .max(0)
    }

    pub(super) fn footer_track_rect(&self, status_text_width: i32) -> Option<Rect> {
        let open_above = matches!(self.screen, Screen::BookTools | Screen::FixedTools)
            && self.reading_top
            && !self.reading_bottom
            && self.reading_panel.is_none();
        if self.controls.footer_scope == ProgressScope::Off
            || self.controls.status_bar == StatusBar::Minimal
            || self.touch_locked
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.notice.is_some()
            || self.search_result.is_some()
            || self.ocr.is_some()
            || !(open_above || matches!(self.screen, Screen::Book | Screen::Fixed))
        {
            return None;
        }
        let right = if self.next_comic().is_some() {
            super::NEXT_COMIC.x
        } else {
            self.footer_right()
        };
        let margin = i32::from(
            self.controls
                .footer_track_margin_width
                .unwrap_or(10)
                .min(140),
        );
        let track_width = (right - margin - margin).max(0);
        let thickness = self.controls.footer_track_thickness.height();
        let bottom_shift = self.footer_bottom_margin_shift();
        Some(match self.controls.footer_track_position {
            FooterTrackPosition::Above => Rect::new(
                margin,
                PAGE_HEIGHT as i32 + 2 - bottom_shift,
                track_width,
                thickness,
            ),
            FooterTrackPosition::Alongside => {
                let (_, track_left, bar_width) = self.footer_track_layout(right, status_text_width);
                Rect::new(
                    track_left,
                    PAGE_HEIGHT as i32 + 32 - bottom_shift,
                    bar_width,
                    thickness,
                )
            }
            FooterTrackPosition::Below => Rect::new(
                margin,
                HEIGHT as i32 - 12 - bottom_shift,
                track_width,
                thickness,
            ),
        })
    }

    pub(super) fn footer_caption_width(&self, default: u32, status_text_width: i32) -> u32 {
        if self.controls.footer_track_position == FooterTrackPosition::Alongside {
            let right = if self.next_comic().is_some() {
                super::NEXT_COMIC.x
            } else {
                self.footer_right()
            };
            return self.footer_track_layout(right, status_text_width).0 as u32;
        }
        default
    }

    pub(super) fn footer_caption_rect(
        &self,
        slot_width: u32,
        content_width: i32,
        font_size: i32,
    ) -> Rect {
        let mut rect = crate::session::footer_caption_rect(slot_width as i32, font_size);
        let center_y = rect.y + rect.h / 2;
        rect.h = i32::from(
            self.controls
                .footer_container_height
                .unwrap_or(14)
                .clamp(7, 98),
        ) * 2;
        rect.y = center_y - rect.h / 2 - self.footer_bottom_margin_shift();
        let width = content_width.clamp(0, rect.w);
        let remaining = rect.w - width;
        rect.x += match self.controls.footer_alignment {
            FooterAlignment::Left => 0,
            FooterAlignment::Center => remaining / 2,
            FooterAlignment::Right => remaining,
        };
        rect.w = width;
        rect
    }

    pub(super) fn draw_footer_track(&self, canvas: &mut Canvas, status_text_width: i32) {
        let Some(bar) = self.footer_track_rect(status_text_width) else {
            return;
        };
        let scope = self.controls.footer_scope;
        let Some(p) = self.footer_progress() else {
            return;
        };
        let fraction = match scope {
            ProgressScope::Chapter => p.section_page as f64 / p.section_len as f64,
            _ => p.book_fraction,
        };
        let fill = (bar.w as f64 * fraction).round() as i32;
        canvas.fill(bar, 200);
        if fill > 0 {
            canvas.fill(Rect::new(bar.x, bar.y, fill, bar.h), 0);
        }
        if let Some(fraction) = self.initial_marker_fraction(&p) {
            draw_initial_marker(canvas, bar, fraction);
        }
        if scope == ProgressScope::Book && !self.controls.hide_chapter_markers {
            let marker_width = self.controls.chapter_marker_width.width();
            let mut last = -marker_width;
            let selected = self.whole_book_marker_pages(true);
            p.book_ticks.draw(
                selected.as_ref().map(|markers| markers.starts.as_slice()),
                |tick| {
                    let x = (tick * bar.w as f64).round() as i32;
                    let marker_x = x - marker_width / 2;
                    if marker_x > last + marker_width
                        && marker_x > 0
                        && marker_x < bar.w - marker_width
                    {
                        canvas.fill(
                            Rect::new(bar.x + marker_x, bar.y - 1, marker_width, bar.h + 2),
                            0,
                        );
                        last = marker_x;
                    }
                },
            );
        }
    }

    pub(super) fn tap_footer(&mut self, x: f32, y: f32) -> Option<Change> {
        if x < 0.0
            || x >= self.footer_right() as f32
            || !(HEIGHT as f32 * 12.0 / 13.0..HEIGHT as f32).contains(&y)
        {
            return None;
        }
        if self.controls.footer_tap_locked {
            return Some(Change::None);
        }
        if self.hidden_overlap_footer() && !SHOW_STATUS.contains(x, y) {
            return None;
        }
        let status_bar = if self.controls.footer_all_at_once {
            if self.controls.status_bar == StatusBar::Minimal {
                StatusBar::PageNumbers
            } else {
                StatusBar::Minimal
            }
        } else {
            self.controls.status_bar.next()
        };
        let controls = ReadingControls {
            status_bar,
            ..self.controls
        };
        Some(match self.activate(Action::SetReadingControls(controls)) {
            Ok(()) => {
                self.notice = None;
                Change::Partial
            }
            Err(error) => self.report(error),
        })
    }

    pub(super) fn hold_footer(&mut self, x: f32, y: f32) -> Option<Change> {
        if x < 0.0
            || x >= self.footer_right() as f32
            || !(PAGE_HEIGHT as f32..HEIGHT as f32).contains(&y)
        {
            return None;
        }
        if self.hidden_overlap_footer() && !SHOW_STATUS.contains(x, y) {
            return None;
        }
        if !self.controls.skim_on_footer_hold
            || self.controls.status_bar == StatusBar::Minimal
            || self.search_result.is_some()
            || self.ocr.is_some()
            || (super::NEXT_COMIC.contains(x, y) && self.next_comic().is_some())
        {
            return Some(Change::None);
        }
        Some(match self.activate(Action::Skim) {
            Ok(()) => Change::Partial,
            Err(error) => self.report(error),
        })
    }

    pub(super) fn footer_items(&self) -> Vec<Item> {
        let c = self.controls;
        let setting = |name, value, controls| {
            Item::new(name, Action::SetReadingControls(controls)).detail(value)
        };
        let on_off = |on| if on { "On" } else { "Off" };
        let next = if c.footer_all_at_once
            && c.status_bar != StatusBar::Minimal
            && c.status_bar != StatusBar::Custom
        {
            StatusBar::Minimal
        } else {
            c.status_bar.next()
        };
        let mut items = vec![
            Item::new("Status bar presets", Action::Show(Screen::FooterPresets))
                .detail(format!("{} saved", self.footer_presets.len())),
            Item::new("Item font", Action::ShowFooterFonts)
                .detail(self.footer_font_file.as_deref().unwrap_or("Device default")),
            setting(
                "Status text",
                if c.footer_all_at_once && c.status_bar != StatusBar::Minimal {
                    "Combined"
                } else {
                    c.status_bar.label()
                },
                ReadingControls {
                    status_bar: next,
                    ..c
                },
            ),
            Item::new(
                "Item font size",
                Action::Show(Screen::NumberingValue(NumberField::FooterItemFontSize)),
            )
            .detail(format!(
                "{} · 8–36",
                c.footer_item_font_size.unwrap_or(14).clamp(8, 36)
            )),
            Item::new(
                "Container height",
                Action::Show(Screen::NumberingValue(NumberField::FooterContainerHeight)),
            )
            .detail(format!(
                "{} · 7–98",
                c.footer_container_height.unwrap_or(14).clamp(7, 98)
            )),
            Item::new(
                "Container bottom margin",
                Action::Show(Screen::NumberingValue(
                    NumberField::FooterContainerBottomMargin,
                )),
            )
            .detail(format!(
                "{} · 0–49",
                c.footer_container_bottom_margin.unwrap_or(1).min(49)
            )),
            setting(
                "Bold text",
                on_off(c.footer_item_bold),
                ReadingControls {
                    footer_item_bold: !c.footer_item_bold,
                    ..c
                },
            ),
            setting(
                "Progress bar",
                c.footer_scope.label(),
                ReadingControls {
                    footer_scope: c.footer_scope.next(),
                    ..c
                },
            ),
            setting(
                "Progress bar position",
                c.footer_track_position.label(),
                ReadingControls {
                    footer_track_position: c.footer_track_position.next(),
                    ..c
                },
            ),
            setting(
                "Progress bar thickness",
                c.footer_track_thickness.label(),
                ReadingControls {
                    footer_track_thickness: c.footer_track_thickness.next(),
                    ..c
                },
            ),
            Item::new(
                "Progress bar minimum width",
                Action::Show(Screen::NumberingValue(NumberField::FooterTrackMinimumWidth)),
            )
            .detail(format!("{}%", c.footer_track_min_width_pct.unwrap_or(20))),
            setting(
                "Lock progress bar width at minimum",
                on_off(c.footer_track_lock_min_width),
                ReadingControls {
                    footer_track_lock_min_width: !c.footer_track_lock_min_width,
                    ..c
                },
            ),
            Item::new(
                "Progress bar margins",
                Action::Show(Screen::NumberingValue(NumberField::FooterTrackMarginWidth)),
            )
            .detail(format!("{} px", c.footer_track_margin_width.unwrap_or(10))),
            setting(
                "Show initial position marker",
                on_off(c.footer_initial_marker),
                ReadingControls {
                    footer_initial_marker: !c.footer_initial_marker,
                    ..c
                },
            ),
            setting(
                "Status item separator",
                c.footer_separator.label(),
                ReadingControls {
                    footer_separator: c.footer_separator.next(),
                    ..c
                },
            ),
            setting(
                "Show footer separator",
                on_off(c.footer_horizontal_separator),
                ReadingControls {
                    footer_horizontal_separator: !c.footer_horizontal_separator,
                    ..c
                },
            ),
            setting(
                "Status item symbols",
                c.footer_symbol_style.label(),
                ReadingControls {
                    footer_symbol_style: c.footer_symbol_style.next(),
                    ..c
                },
            ),
            setting(
                "Pages left includes current page",
                on_off(c.footer_pages_left_include_current),
                ReadingControls {
                    footer_pages_left_include_current: !c.footer_pages_left_include_current,
                    ..c
                },
            ),
            setting(
                "Progress percentage digits",
                match c.footer_percentage_digits.min(2) {
                    0 => "0 decimal places",
                    1 => "1 decimal place",
                    _ => "2 decimal places",
                },
                ReadingControls {
                    footer_percentage_digits: (c.footer_percentage_digits.min(2) + 1) % 3,
                    ..c
                },
            ),
        ];
        if c.status_bar != StatusBar::Minimal {
            items.insert(
                1,
                setting(
                    "Footer alignment",
                    c.footer_alignment.label(),
                    ReadingControls {
                        footer_alignment: c.footer_alignment.next(),
                        ..c
                    },
                ),
            );
        }
        let header = c.top_header;
        items.extend([
            Item::new("Top status header", Action::Heading),
            Item::new("About the top status header", Action::AboutTopHeader),
            setting(
                "Refresh header clock automatically",
                on_off(header.auto_refresh),
                ReadingControls {
                    top_header: TopHeader {
                        auto_refresh: !header.auto_refresh,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header book title",
                on_off(header.title),
                ReadingControls {
                    top_header: TopHeader {
                        title: !header.title,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header book author",
                on_off(header.author),
                ReadingControls {
                    top_header: TopHeader {
                        author: !header.author,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header current time",
                on_off(header.clock),
                ReadingControls {
                    top_header: TopHeader {
                        clock: !header.clock,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header current page",
                on_off(header.page_number),
                ReadingControls {
                    top_header: TopHeader {
                        page_number: !header.page_number,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header total pages",
                on_off(header.page_count),
                ReadingControls {
                    top_header: TopHeader {
                        page_count: !header.page_count,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header progress percentage",
                on_off(header.reading_percent),
                ReadingControls {
                    top_header: TopHeader {
                        reading_percent: !header.reading_percent,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header chapter marks",
                on_off(header.chapter_marks),
                ReadingControls {
                    top_header: TopHeader {
                        chapter_marks: !header.chapter_marks,
                        ..header
                    },
                    ..c
                },
            ),
            setting(
                "Header battery status",
                header.battery.label(),
                ReadingControls {
                    top_header: TopHeader {
                        battery: header.battery.next(),
                        ..header
                    },
                    ..c
                },
            ),
            Item::new("Header font size", Action::EditHeaderFontSize)
                .detail(format!("{} px", header.font_size())),
        ]);
        if c.status_bar != StatusBar::Custom {
            items.insert(
                1,
                setting(
                    "Show items together",
                    on_off(c.footer_all_at_once),
                    ReadingControls {
                        footer_all_at_once: !c.footer_all_at_once,
                        ..c
                    },
                ),
            );
        }
        if self.active.is_some() || self.fixed.is_some() {
            items.push(setting(
                "Overlap status bar",
                on_off(c.overlap_footer),
                ReadingControls {
                    overlap_footer: !c.overlap_footer,
                    ..c
                },
            ));
        }
        if c.footer_scope == ProgressScope::Book {
            items.push(setting(
                "Chapter markers",
                on_off(!c.hide_chapter_markers),
                ReadingControls {
                    hide_chapter_markers: !c.hide_chapter_markers,
                    ..c
                },
            ));
            items.push(setting(
                "Chapter marker thickness",
                c.chapter_marker_width.label(),
                ReadingControls {
                    chapter_marker_width: c.chapter_marker_width.next(),
                    ..c
                },
            ));
        }
        items.extend([
            setting(
                "Lock status bar taps",
                on_off(c.footer_tap_locked),
                ReadingControls {
                    footer_tap_locked: !c.footer_tap_locked,
                    ..c
                },
            ),
            setting(
                "Hold status bar to skim",
                on_off(c.skim_on_footer_hold),
                ReadingControls {
                    skim_on_footer_hold: !c.skim_on_footer_hold,
                    ..c
                },
            ),
        ]);
        if c.status_bar == StatusBar::Custom {
            items.push(Item::new("Visible status items", Action::Heading));
            items.push(setting(
                "Auto refresh items",
                on_off(c.footer_auto_refresh),
                ReadingControls {
                    footer_auto_refresh: !c.footer_auto_refresh,
                    ..c
                },
            ));
            for item in FooterItem::ordered(c.footer_order) {
                let enabled = c.footer_items & item.bit() != 0;
                items.push(setting(
                    item.label(),
                    on_off(enabled),
                    ReadingControls {
                        footer_items: c.footer_items ^ item.bit(),
                        ..c
                    },
                ));
            }
            if c.footer_items & FooterItem::BatteryPercentage.bit() != 0 {
                items.push(
                    Item::new(
                        "Hide battery when above",
                        Action::Show(Screen::NumberingValue(
                            NumberField::FooterBatteryHideThreshold,
                        )),
                    )
                    .detail(format!(
                        "{}% · 0–100%",
                        c.footer_battery_hide_threshold.unwrap_or(100)
                    )),
                );
            }
            items.push(Item::new("Custom text", Action::Heading));
            items.push(
                Item::new("Edit custom text", Action::Show(Screen::FooterCustomText))
                    .detail(custom_text_preview(&self.footer_custom_text.text, 42)),
            );
            items.push(
                Item::new(
                    "Number of repetitions",
                    Action::Show(Screen::NumberingValue(
                        NumberField::FooterCustomTextRepetitions,
                    )),
                )
                .detail(format!("{} · 1–99", self.footer_custom_text.repetitions)),
            );
            items.push(
                Item::new(
                    "Custom text tokens",
                    Action::Show(Screen::FooterCustomTextGuide),
                )
                .detail("Show supported book, page, time and battery values"),
            );
            items.push(Item::new("Status item order", Action::Heading));
            let ordered = FooterItem::ordered(c.footer_order);
            for (index, item) in ordered.iter().copied().enumerate() {
                if index > 0 {
                    items.push(
                        Item::new(
                            format!("Move up: {}", item.label()),
                            Action::SetReadingControls(ReadingControls {
                                footer_order: FooterItem::move_order(c.footer_order, item, -1),
                                ..c
                            }),
                        )
                        .detail(format!(
                            "Position {} to {}",
                            index + 1,
                            index
                        )),
                    );
                }
                if index + 1 < ordered.len() {
                    items.push(
                        Item::new(
                            format!("Move down: {}", item.label()),
                            Action::SetReadingControls(ReadingControls {
                                footer_order: FooterItem::move_order(c.footer_order, item, 1),
                                ..c
                            }),
                        )
                        .detail(format!(
                            "Position {} to {}",
                            index + 1,
                            index + 2
                        )),
                    );
                }
            }
        }
        if c.footer_all_at_once && c.status_bar != StatusBar::Custom {
            let (label, next) = match c.footer_combined_time {
                Some(ProgressScope::Chapter) => ("Chapter", Some(ProgressScope::Book)),
                Some(ProgressScope::Book) => ("Book", None),
                _ => ("Off", Some(ProgressScope::Chapter)),
            };
            items.push(setting(
                "Time estimate in combined status",
                label,
                ReadingControls {
                    footer_combined_time: next,
                    ..c
                },
            ));
        }
        items
    }
}

#[cfg(feature = "kindle")]
fn current_unix_seconds() -> Option<i64> {
    crate::mark_time::now()
}

#[cfg(not(feature = "kindle"))]
fn current_unix_seconds() -> Option<i64> {
    None
}

#[cfg(feature = "kindle")]
fn local_footer_time() -> Option<String> {
    let time = primitive_device::local_time_at(current_unix_seconds()?)?;
    Some(format!("Time {:02}:{:02}", time.hour, time.minute))
}

#[cfg(not(feature = "kindle"))]
fn local_footer_time() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_footer_text_reports_level_and_charging_state() {
        assert_eq!(
            battery_footer_text(47, false, 100),
            Some("Battery 47%".to_owned())
        );
        assert_eq!(
            battery_footer_text(88, true, 100),
            Some("Charging 88%".to_owned())
        );
        assert_eq!(
            battery_footer_text(140, false, 100),
            Some("Battery 100%".to_owned())
        );
        assert_eq!(
            battery_footer_text(80, false, 80),
            Some("Battery 80%".to_owned())
        );
        assert_eq!(battery_footer_text(81, false, 80), None);
        assert_eq!(
            battery_footer_text(0, false, 0),
            Some("Battery 0%".to_owned())
        );
    }

    #[test]
    fn custom_footer_battery_symbol_uses_koreader_glyph_steps() {
        assert_eq!(battery_symbol(100, false, false), "");
        assert_eq!(battery_symbol(100, false, true), "");
        assert_eq!(battery_symbol(90, false, false), "");
        assert_eq!(battery_symbol(80, false, false), "");
        assert_eq!(battery_symbol(70, false, false), "");
        assert_eq!(battery_symbol(60, false, false), "");
        assert_eq!(battery_symbol(50, false, false), "");
        assert_eq!(battery_symbol(40, false, false), "");
        assert_eq!(battery_symbol(30, false, false), "");
        assert_eq!(battery_symbol(20, false, false), "");
        assert_eq!(battery_symbol(10, false, false), "");
        assert_eq!(battery_symbol(9, false, false), "");
        assert_eq!(battery_symbol(47, true, false), "");
    }

    #[test]
    fn top_header_battery_icon_changes_when_charging() {
        let fonts = vec![
            crate::test_fixture("fonts/NotoSans-Regular.ttf")
                .display()
                .to_string(),
        ];
        let render = |charging| {
            let mut canvas = Canvas::new(WIDTH, 48, &fonts).unwrap();
            canvas.begin();
            draw_battery_icon(&mut canvas, 100, 10, 50, charging, 0, 255);
            canvas.finish(false)
        };

        assert_ne!(render(false), render(true));
    }

    #[test]
    fn custom_flow_footer_counts_only_the_opened_flow_and_keeps_main_page_ticks() {
        use crate::library::hidden_flows::{PageFlows, ResolvedPoint};
        let flows = PageFlows::new(
            12,
            0,
            vec![
                ResolvedPoint {
                    page: 2,
                    hidden: true,
                    index: 0,
                },
                ResolvedPoint {
                    page: 5,
                    hidden: false,
                    index: 1,
                },
                ResolvedPoint {
                    page: 8,
                    hidden: true,
                    index: 2,
                },
                ResolvedPoint {
                    page: 10,
                    hidden: false,
                    index: 3,
                },
            ],
        );
        let chapters = [(0, "A".into()), (5, "B".into()), (10, "C".into())];
        let sections = PageSections::Publisher(&chapters);
        let mut regular = Progress::from_pages(6, 12, sections, "Book");
        regular.apply_flows(flows.clone(), 6, 6, 12, sections);
        assert_eq!(
            (
                regular.section_page,
                regular.section_len,
                regular.book_pages_left
            ),
            (2, 3, 3)
        );
        assert_eq!(regular.percentage_with_digits(0), "57%");
        assert_eq!(regular.percentage_with_digits(1), "57.1%");
        assert_eq!(regular.percentage_with_digits(2), "57.14%");
        assert_eq!(regular.percentage_with_digits(9), "57.14%");
        assert_eq!(pages_left_display(0, false), 0);
        assert_eq!(pages_left_display(0, true), 1);
        assert_eq!(pages_left_display(3, true), 4);
        let mut ticks = Vec::new();
        regular
            .book_ticks
            .draw(None, |position| ticks.push(position));
        assert_eq!(ticks, [0., 2. / 7., 5. / 7.]);
        ticks.clear();
        regular
            .book_ticks
            .draw(Some(&[3, 5, 9]), |position| ticks.push(position));
        assert_eq!(ticks, [2. / 7.]);
        let selected = MarkerPages {
            starts: vec![8],
            count: 12,
        };
        assert_eq!(bound_chapter_pages_left(&selected, 0, Some(&flows)), 4);
        assert_eq!(bound_chapter_pages_left(&selected, 3, Some(&flows)), 4);
        assert_eq!(bound_chapter_pages_left(&selected, 8, Some(&flows)), 3);
        assert_eq!(bound_chapter_pages_left(&selected, 11, Some(&flows)), 0);
        let mut note = Progress::from_pages(3, 12, sections, "Book");
        note.apply_flows(flows, 3, 3, 12, sections);
        assert_eq!(
            (note.section_page, note.section_len, note.book_pages_left),
            (2, 3, 1)
        );
        assert_eq!(note.percentage_with_digits(0), "67%");
    }

    #[test]
    fn chapter_ticks_ignore_duplicate_destinations_and_cover_front_matter_and_the_last_page() {
        let mut sections = vec![
            (5, "Part II".into()),
            (2, "Part I".into()),
            (2, "Subtitle".into()),
        ];
        sort_chapters(&mut sections);
        for (page, title, current, count) in [
            (0, "Front matter", 1, 2),
            (1, "Front matter", 2, 2),
            (2, "Part I", 1, 3),
            (4, "Part I", 3, 3),
            (5, "Part II", 1, 2),
            (6, "Part II", 2, 2),
        ] {
            let p =
                Progress::from_pages(page, 7, PageSections::Publisher(&sections), "Front matter");
            assert_eq!(
                (p.section_name, p.section_page, p.section_len),
                (title, current, count)
            );
            assert_eq!(p.pages_left(), count - current);
        }
        let mut ticks = Vec::new();
        Ticks::Pages(PageSections::Publisher(&sections), 7).draw(None, |pos| ticks.push(pos));
        assert_eq!(ticks, [2.0 / 7.0, 5.0 / 7.0]);
    }
}

#[cfg(test)]
mod pace_replay;

#[cfg(test)]
#[path = "reading_footer/overlap_tests.rs"]
mod overlap_tests;

#[cfg(test)]
#[path = "reading_footer/header_page_labels_tests.rs"]
mod header_page_labels_tests;
