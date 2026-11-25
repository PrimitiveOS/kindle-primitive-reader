//! Alternate status header for paged reflow.
use std::borrow::Cow;

use primitive_device::BatteryStatus;

use super::{Canvas, HeaderBattery, Reader, Rect, TextStyle, TopHeader, draw_battery_icon};
use crate::app::ActiveBook;
use crate::app::footer_custom_text::{ClockTokens, local_clock};
use crate::library::hidden_flows::PageFlows;

struct Pagination {
    current: usize,
    count: usize,
    flow: Option<usize>,
}

impl Pagination {
    fn from_book(book: &ActiveBook, flows: Option<&PageFlows>) -> Self {
        let reading = &book.reading;
        if let Some(flows) = flows {
            let page = reading.page;
            let flow = flows.flow(page);
            return Self {
                current: flows.count_in(flow, 0..page) + 1,
                count: flows.count_in(flow, 0..reading.probe.page_count()).max(1),
                flow: Some(reading.hidden_flow_ordinal(flow).unwrap_or(0)),
            };
        }
        let (current, count) = reading
            .whole_book_screen_pages()
            .unwrap_or_else(|| (reading.page + 1, reading.probe.page_count().max(1)));
        Self {
            current,
            count,
            flow: None,
        }
    }

    fn info(&self, header: TopHeader, labels: Option<(String, String)>) -> Option<String> {
        if !header.page_number && !header.page_count {
            return None;
        }
        let (current, total, flow) = labels.map_or_else(
            || (self.current.to_string(), self.count.to_string(), self.flow),
            |(current, total)| (current, total, None),
        );
        let value = match (header.page_number, header.page_count) {
            (true, true) => format!(
                "{current}{}{total}",
                if flow == Some(0) { " // " } else { " / " }
            ),
            (true, false) => current,
            _ => total,
        };
        Some(if let Some(flow) = flow.filter(|&flow| flow != 0) {
            format!("[{value}]{flow}")
        } else {
            value
        })
    }

    fn percentage(&self, digits: u8) -> String {
        let value = format!(
            "{:.precision$}%",
            self.current as f64 / self.count as f64 * 100.,
            precision = digits.min(2) as usize
        );
        if self.flow.is_some_and(|flow| flow != 0) {
            format!("[{value}]")
        } else {
            value
        }
    }
}

struct HeaderParts<'a> {
    title: Cow<'a, str>,
    author: Cow<'a, str>,
    status: String,
}

impl HeaderParts<'_> {
    fn full_identity(&self) -> Cow<'_, str> {
        match (self.title.is_empty(), self.author.is_empty()) {
            (true, _) => Cow::Borrowed(&self.author),
            (_, true) => Cow::Borrowed(&self.title),
            _ => Cow::Owned(format!(
                "{}.  {}",
                self.author.trim_end_matches('.'),
                self.title
            )),
        }
    }

    fn fitted_identity(
        &self,
        width: i32,
        page: usize,
        mut measure: impl FnMut(&str) -> i32,
    ) -> String {
        let full = self.full_identity();
        let text = if !self.title.is_empty() && !self.author.is_empty() && measure(&full) > width {
            // CRE alternates author on odd pages and title on even pages.
            if page % 2 == 0 {
                &self.title
            } else {
                &self.author
            }
        } else {
            &full
        };
        super::custom_line::fit_item(text, width, measure).into_owned()
    }
}

fn single_line(text: &str) -> Cow<'_, str> {
    if text.contains(['\n', '\r']) {
        Cow::Owned(text.replace(['\n', '\r'], " "))
    } else {
        Cow::Borrowed(text)
    }
}

fn header_flows(book: &ActiveBook) -> Option<PageFlows> {
    book.reading.custom_flow_map().filter(PageFlows::has_hidden)
}

impl Reader {
    fn header_parts<'a>(
        &'a self,
        book: &'a ActiveBook,
        pagination: &Pagination,
        clock: Option<&ClockTokens>,
        battery: BatteryStatus,
    ) -> HeaderParts<'a> {
        let header = self.controls.top_header;
        let (custom_title, custom_author) = self.reading_identity(book);
        let author = if header.author {
            custom_author
                .or(book.reading.probe.author.as_deref())
                .unwrap_or_default()
                .trim()
        } else {
            ""
        };
        let title = if header.title {
            let value = custom_title.unwrap_or(&book.reading.probe.title).trim();
            if value.is_empty() && author.is_empty() {
                book.entry
                    .library_path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
            } else {
                value
            }
        } else {
            ""
        };
        let mut status = Vec::new();
        if let Some(time) = clock.filter(|_| header.clock) {
            status.push(time.header_time(self.controls.twelve_hour_clock));
        }
        if let Some(page_info) = pagination.info(header, self.top_header_page_labels()) {
            status.push(page_info);
        }
        if header.reading_percent {
            let digits = self.controls.footer_percentage_digits;
            let value = if pagination.flow.is_some()
                || book.reading.probe.chapter_count() == 1
                || book.reading.whole_book_screen_pages().is_some()
            {
                pagination.percentage(digits)
            } else {
                self.footer_progress().map_or_else(
                    || pagination.percentage(digits),
                    |progress| progress.percentage_with_digits(digits),
                )
            };
            status.push(value);
        }
        if header.battery == HeaderBattery::Percentage
            && let Some(percent) = battery.percent
        {
            // CRE displays a plus when the status font lacks the charging bolt.
            let charge = if battery.charging { "+" } else { "" };
            status.push(format!("[{charge}{:>2}%]", percent.min(100)));
        }
        HeaderParts {
            title: single_line(title),
            author: single_line(author),
            status: status.join("  "),
        }
    }

    fn header_readings(&self) -> (Option<ClockTokens>, BatteryStatus) {
        let header = self.controls.top_header;
        let clock = header.clock.then(local_clock).flatten();
        let battery = if header.battery == HeaderBattery::Off {
            BatteryStatus::default()
        } else {
            primitive_device::battery_status()
        };
        (clock, battery)
    }

    #[cfg(test)]
    pub(super) fn header_status_caption(&self) -> Option<String> {
        self.header_caption_parts().map(|(_, status)| status)
    }

    #[cfg(test)]
    pub(in crate::app) fn top_header_caption(&self) -> Option<String> {
        let (identity, status) = self.header_caption_parts()?;
        Some(match (identity.is_empty(), status.is_empty()) {
            (true, _) => status,
            (_, true) => identity,
            _ => format!("{identity}  ·  {status}"),
        })
    }

    #[cfg(test)]
    fn header_caption_parts(&self) -> Option<(String, String)> {
        if !self.top_header_visible() {
            return None;
        }
        let book = self.active.as_ref()?;
        let (clock, battery) = self.header_readings();
        let flows = header_flows(book);
        let pagination = Pagination::from_book(book, flows.as_ref());
        let parts = self.header_parts(book, &pagination, clock.as_ref(), battery);
        Some((parts.full_identity().into_owned(), parts.status))
    }

    pub(in crate::app) fn draw_top_header(&self, canvas: &mut Canvas) {
        if !self.top_header_visible() {
            return;
        }
        let (clock, battery) = self.header_readings();
        self.draw_top_header_with_readings(canvas, clock.as_ref(), battery);
    }

    fn draw_top_header_with_readings(
        &self,
        canvas: &mut Canvas,
        clock: Option<&ClockTokens>,
        battery: BatteryStatus,
    ) {
        let Some(book) = &self.active else { return };
        let header = self.controls.top_header;
        let flows = header_flows(book);
        let pagination = Pagination::from_book(book, flows.as_ref());
        let parts = self.header_parts(book, &pagination, clock, battery);
        let width = self.reading_viewport().dimensions().0 as i32;
        let (ink, paper) = if self.uses_night_pages() {
            (255, 0)
        } else {
            (0, 255)
        };
        canvas.fill(Rect::new(0, 0, width, TopHeader::HEIGHT as i32), paper);
        let font_size = i32::from(header.font_size());
        let row = Rect::new(20, 2, width - 40, 38);
        let icon = battery
            .percent
            .filter(|_| header.battery == HeaderBattery::Icon);
        let right = row.right() - if icon.is_some() { 42 } else { 0 };
        let status = super::custom_line::fit_item(&parts.status, right - row.x, |text| {
            canvas.measure(text, font_size)
        });
        let status_width = canvas.measure(&status, font_size);
        let right_x = right - status_width;
        let identity_width = (right_x - row.x - 20).max(0);
        let identity = parts.fitted_identity(identity_width, pagination.current, |text| {
            canvas.measure(text, font_size)
        });
        let style = TextStyle::new(font_size).ink(ink);
        if !identity.is_empty() {
            canvas.text(
                Rect::new(row.x, row.y, identity_width, row.h),
                &identity,
                style,
            );
        }
        if status_width > 0 {
            canvas.text(
                Rect::new(right_x, row.y, status_width, row.h),
                &status,
                style,
            );
        }
        if let Some(level) = icon {
            draw_battery_icon(
                canvas,
                row.right() - 34,
                11,
                level,
                battery.charging,
                ink,
                paper,
            );
        }

        let reading = &book.reading;
        let local_count = reading.probe.page_count();
        let (starts, count) = if header.chapter_marks {
            self.alternative_header_marker_pages(book)
                .or_else(|| self.publisher_header_marker_pages(book))
                .map_or_else(
                    || {
                        let starts =
                            self.customized_header_marker_pages(book)
                                .unwrap_or_else(|| {
                                    reading
                                        .probe
                                        .footer_sections()
                                        .iter()
                                        .map(|(page, _)| *page)
                                        .collect()
                                });
                        (starts, local_count)
                    },
                    |markers| (markers.starts, markers.count),
                )
        } else {
            (Vec::new(), pagination.count)
        };
        let starts = starts
            .into_iter()
            .filter_map(|start| {
                if let Some(flows) = &flows {
                    let flow = flows.flow(reading.page);
                    (flows.flow(start) == flow)
                        .then(|| (flows.count_in(flow, 0..start), pagination.count))
                } else {
                    Some((start, count))
                }
            })
            .map(|(page, count)| page as f64 / count.max(1) as f64);
        // While only local marker coordinates are available, keep them on the
        // same scale as the filled part of the track.
        let (current, total) = if flows.is_some() || count == pagination.count {
            (pagination.current, pagination.count)
        } else {
            (reading.page + 1, local_count)
        };
        draw_header_track(
            canvas,
            width,
            current.saturating_sub(1) as f64 / total.max(1) as f64,
            header.chapter_marks,
            starts,
            ink,
            paper,
        );
    }
}

fn draw_header_track(
    canvas: &mut Canvas,
    width: i32,
    fraction: f64,
    chapter_marks: bool,
    markers: impl Iterator<Item = f64>,
    ink: u8,
    paper: u8,
) {
    let track = Rect::new(12, TopHeader::HEIGHT as i32 - 4, width - 24, 1);
    let read = (fraction.clamp(0., 1.) * f64::from(track.w)).floor() as i32;
    canvas.fill(track, ink);
    if read > 0 {
        canvas.fill(Rect::new(track.x, track.y - 1, read, 3), ink);
    }
    if !chapter_marks {
        return;
    }
    let marks: Vec<_> = std::iter::once(0.)
        .chain(markers)
        .chain(std::iter::once(1.))
        .take((track.w / 5).max(0) as usize)
        .collect();
    if marks.len() >= (track.w / 5) as usize {
        return;
    }
    let mut previous = None;
    for fraction in marks {
        let offset = (f64::from(track.w - 2) * fraction.clamp(0., 1.)).floor() as i32;
        if previous.is_some_and(|prior| offset <= prior + 1) {
            continue;
        }
        previous = Some(offset);
        let (y, height, color) = if offset < read {
            (track.y - 1, 3, paper)
        } else {
            (track.y - 3, 7, ink)
        };
        canvas.fill(Rect::new(track.x + offset, y, 2, height), color);
    }
}

#[cfg(test)]
mod tests;
