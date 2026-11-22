//! Reading status shares the existing compact page footer with document controls.
use super::{Action, ActiveFixed, Item, Reader, Screen};
use crate::Probe;
use crate::fixed_session::FixedReading;
use crate::library::Bookmark;
use crate::reading_controls::{ProgressScope, ReadingControls, StatusBar};
use crate::session::{DONE, PAGE_HEIGHT, SHOW_STATUS};
use primitive_app::{Change, HEIGHT};
use primitive_ui::{Canvas, Rect};

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
                .toc
                .iter()
                .filter_map(|entry| {
                    let target = entry.destination.as_ref()?;
                    if target.path != self.chapter {
                        return None;
                    }
                    let page = if let Some(id) = &target.fragment {
                        self.page_for(&self.fragment(id)?)?
                    } else {
                        0
                    };
                    Some((page, entry.label.clone()))
                })
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
}

struct Progress<'a> {
    section_range: std::ops::Range<usize>,
    section_name: &'a str,
    section_page: usize,
    section_len: usize,
    book_pages_left: usize,
    future_pages_estimated: bool,
    // Fixed documents use physical pages. Reflow estimates weight spine items by source size.
    book_fraction: f64,
    estimated: bool,
    book_ticks: Ticks<'a>,
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
    fn draw(&self, mut mark: impl FnMut(f64)) {
        match self {
            Self::Pages(sections, count) => {
                for (page, _) in (0..sections.len()).filter_map(|index| sections.at(index)) {
                    mark(page as f64 / *count as f64);
                }
            }
            Self::FlowPages(sections, flows, flow) => {
                let total = flows.count_in(*flow, 0..usize::MAX).max(1);
                for (page, _) in (0..sections.len()).filter_map(|index| sections.at(index)) {
                    mark(flows.count_in(*flow, 0..page) as f64 / total as f64);
                }
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
        Self {
            section_range: start..end,
            section_name: name,
            section_page: page - start + 1,
            section_len: end - start,
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
        let total = flows.count_in(flow, 0..count).max(1);
        self.section_page = flows
            .count_in(flow, self.section_range.start..last + 1)
            .max(1);
        self.section_len = flows.count_in(flow, self.section_range.clone()).max(1);
        self.book_pages_left = flows.count_in(flow, last + 1..count);
        self.book_fraction = flows.count_in(flow, 0..last + 1) as f64 / total as f64;
        self.book_ticks = Ticks::FlowPages(sections, flows, flow);
    }

    fn percentage(&self) -> String {
        let percent = self.book_fraction * 100.0;
        if self.estimated && percent < 100.0 {
            format!("About {percent:.0}%")
        } else {
            format!("{percent:.0}%")
        }
    }

    fn pages_left(&self) -> usize {
        self.section_len - self.section_page
    }
}

impl Reader {
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
        }
        Some(progress)
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
                    format!("Custom · {caption}")
                } else {
                    format!("Hidden {flow} · {caption}")
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
        Some(match page {
            Some(label) => format!("{label}  ·  Timer {remaining}"),
            None => format!("Timer {remaining}"),
        })
    }

    pub(super) fn alarm_progress(&self, token: char) -> Option<String> {
        let progress = self.footer_progress()?;
        match token {
            'l' => Some(progress.pages_left().to_string()),
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
                    progress.pages_left()
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
        let time = |scope| {
            let (pages, unit) = if scope == ProgressScope::Book {
                (p.book_pages_left, "book")
            } else {
                (p.pages_left(), "chapter")
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
                return Some(format!("{} · {}", p.percentage(), time(scope)?));
            }
            return Some(format!(
                "{} of book  ·  {}  ·  {} left",
                p.percentage(),
                printed().unwrap_or_else(|| format!("{} / {}", p.section_page, p.section_len)),
                p.pages_left()
            ));
        }
        match mode {
            StatusBar::ChapterTime => time(ProgressScope::Chapter),
            StatusBar::BookTime => time(ProgressScope::Book),
            StatusBar::Progress => Some(format!("{} of book", p.percentage())),
            StatusBar::Chapter => Some(format!(
                "{}  ·  Page {} of {}",
                p.section_name, p.section_page, p.section_len
            )),
            StatusBar::Remaining => Some(format!(
                "{}  ·  {} {} left",
                p.section_name,
                p.pages_left(),
                if p.pages_left() == 1 { "page" } else { "pages" }
            )),
            _ => None,
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

    pub(super) fn draw_footer_track(&self, canvas: &mut Canvas) {
        let open_above = matches!(self.screen, Screen::BookTools | Screen::FixedTools)
            && self.reading_top
            && !self.reading_bottom
            && self.reading_panel.is_none();
        let scope = self.controls.footer_scope;
        if scope == ProgressScope::Off
            || self.controls.status_bar == StatusBar::Minimal
            || self.touch_locked
            || self.selection.is_some()
            || self.pdf_selection.is_some()
            || self.notice.is_some()
            || self.search_result.is_some()
            || self.ocr.is_some()
            || !(open_above || matches!(self.screen, Screen::Book | Screen::Fixed))
        {
            return;
        }
        let Some(p) = self.footer_progress() else {
            return;
        };
        let right = if self.next_comic().is_some() {
            super::NEXT_COMIC.x
        } else {
            self.footer_right()
        };
        let bar = Rect::new(38, HEIGHT as i32 - 12, right - 76, 5);
        let fraction = match scope {
            ProgressScope::Chapter => p.section_page as f64 / p.section_len as f64,
            _ => p.book_fraction,
        };
        let fill = (bar.w as f64 * fraction).round() as i32;
        canvas.fill(bar, 200);
        if fill > 0 {
            canvas.fill(Rect::new(bar.x, bar.y, fill, bar.h), 0);
        }
        if scope == ProgressScope::Book && !self.controls.hide_chapter_markers {
            let mut last = -3;
            p.book_ticks.draw(|tick| {
                let x = (tick * bar.w as f64).round() as i32;
                if x > last + 2 && x > 0 && x < bar.w - 1 {
                    canvas.fill(Rect::new(bar.x + x, bar.y - 3, 2, bar.h + 6), 0);
                    last = x;
                }
            });
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
        let next = if c.footer_all_at_once && c.status_bar != StatusBar::Minimal {
            StatusBar::Minimal
        } else {
            c.status_bar.next()
        };
        let mut items = vec![
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
            setting(
                "Show items together",
                on_off(c.footer_all_at_once),
                ReadingControls {
                    footer_all_at_once: !c.footer_all_at_once,
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
        ];
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
        if c.footer_all_at_once {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(regular.percentage(), "57%");
        let mut ticks = Vec::new();
        regular.book_ticks.draw(|position| ticks.push(position));
        assert_eq!(ticks, [0., 2. / 7., 5. / 7.]);
        let mut note = Progress::from_pages(3, 12, sections, "Book");
        note.apply_flows(flows, 3, 3, 12, sections);
        assert_eq!(
            (note.section_page, note.section_len, note.book_pages_left),
            (2, 3, 1)
        );
        assert_eq!(note.percentage(), "67%");
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
        Ticks::Pages(PageSections::Publisher(&sections), 7).draw(|pos| ticks.push(pos));
        assert_eq!(ticks, [2.0 / 7.0, 5.0 / 7.0]);
    }
}

#[cfg(test)]
mod pace_replay;

#[cfg(test)]
#[path = "reading_footer/overlap_tests.rs"]
mod overlap_tests;
