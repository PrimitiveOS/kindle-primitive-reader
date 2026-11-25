//! The page editor addresses source labels, percentages and rendered pages.
use super::*;
use crate::session::screen_pages::background::wants_background;

#[derive(Clone, Copy)]
enum PageJump {
    Absolute(usize),
    Relative(isize),
}

#[derive(Clone, Copy)]
pub(super) struct PendingJump {
    jump: PageJump,
    book: u64,
    settings: Settings,
}

impl PageJump {
    fn resolve(self, current: usize, count: usize) -> Result<usize> {
        match self {
            Self::Absolute(page) if (1..=count).contains(&page) => Ok(page),
            Self::Absolute(_) => Err(format!("Enter a page from 1 to {count}").into()),
            Self::Relative(delta) => Ok(current.saturating_add_signed(delta).clamp(1, count)),
        }
    }
}

impl Reader {
    pub(super) fn draw_page_jump_context(&self, canvas: &mut Canvas) {
        let bounds = self.navigation_layout().body(LIST_Y + 218, 145);
        let help = if let Some(fixed) = &self.fixed {
            let reading = &fixed.reading;
            let mut text = format!(
                "{} of {}",
                fixed_page_label(reading, reading.page()),
                reading.count()
            );
            if reading
                .hidden_flow_map()
                .is_some_and(|flow| flow.has_hidden())
            {
                text.push_str("\nUse [page] for the main flow or [page]flow for a hidden flow.");
            }
            text
        } else if let Some(book) = &self.active {
            let reading = &book.reading;
            reading.whole_book_screen_pages().map_or_else(
                || {
                    format!(
                        "Chapter {} - Page {} of {}\nUse chapter:page to jump to a known chapter.",
                        reading.chapter + 1,
                        reading.page + 1,
                        reading.probe.page_count()
                    )
                },
                |(page, total)| format!("Document page {page} of {total}"),
            )
        } else {
            return;
        };
        canvas.text(bounds, &help, CAPTION.lines(3));
    }

    pub(super) fn jump_reflow_number(&mut self) -> Result<()> {
        let input = self.editor.string();
        let input = input.trim();
        if let Some(percent) = input.strip_suffix('%') {
            let active = self.active.as_mut().ok_or("No open book")?;
            let previous = position(&active.reading);
            active.reading.jump_progress(percent_fraction(percent)?)?;
            self.finish_reflow_jump(previous);
            return Ok(());
        }
        if let Some(label) = input.strip_prefix("label ") {
            return self.jump_reflow_label(label.trim());
        }
        if let Some((chapter, page)) = input.split_once(':') {
            let chapter = positive(chapter).ok_or("Enter a valid chapter number")?;
            let page = positive(page).ok_or("Enter a valid page number")?;
            return self.follow_reflow_page(chapter - 1, page - 1);
        }
        if let Some(delta) = relative(input) {
            return self.jump_reflow_screen_page(PageJump::Relative(delta?));
        }
        let page = positive(input).ok_or("Enter a page number, chapter:page, or a percentage")?;
        self.jump_reflow_screen_page(PageJump::Absolute(page))
    }

    fn jump_reflow_label(&mut self, label: &str) -> Result<()> {
        let active = self.active.as_ref().ok_or("No open book")?;
        if let Some(map) = &active.numbering {
            let row = positive(label)
                .map(|n| n - 1)
                .filter(|row| *row < map.count)
                .ok_or("Enter a stable page number from the list")?;
            return self.follow_character_page(row);
        }
        let mut matches = active
            .reading
            .probe
            .printed_pages
            .iter()
            .filter_map(|entry| {
                (entry.label == label)
                    .then_some(entry.destination.as_ref())
                    .flatten()
            });
        let target = matches
            .next()
            .ok_or("Enter a printed page label from the book")?;
        if matches.next().is_some() {
            return Err("That printed page label occurs more than once".into());
        }
        self.follow(target.clone())
    }

    fn jump_reflow_screen_page(&mut self, jump: PageJump) -> Result<()> {
        let active = self.active.as_mut().ok_or("No open book")?;
        let reading = &mut active.reading;
        if reading.has_local_screen_pages()
            && let PageJump::Relative(delta) = jump
            && let Some(page) = reading.page.checked_add_signed(delta)
            && page < reading.probe.page_count()
        {
            if page == reading.page {
                self.show(Screen::Book);
                return Ok(());
            }
            let before = position(reading);
            reading.go_to_laid_out_page(reading.chapter, page, None)?;
            self.finish_reflow_jump(before);
            return Ok(());
        }
        if reading.whole_book_screen_pages().is_some() {
            return self.follow_screen_ordinal(jump);
        }
        if active.entry.format == DocumentFormat::Epub && reading.has_exact_local_screen_pages() {
            reading.request_screen_page_navigation();
            if wants_background(reading) {
                self.screen_page_jump = Some(PendingJump {
                    jump,
                    book: active.id,
                    settings: reading.config.settings,
                });
                self.notice = Some("Counting document pages…".into());
                return Ok(());
            }
        }
        Err(unavailable())
    }

    fn follow_screen_ordinal(&mut self, jump: PageJump) -> Result<()> {
        let reading = &mut self.active.as_mut().ok_or("No open book")?.reading;
        let (current, count) = reading.whole_book_screen_pages().ok_or_else(unavailable)?;
        let page = jump.resolve(current, count)?;
        if page == current {
            self.show(Screen::Book);
            return Ok(());
        }
        let before = position(reading);
        reading.jump_to_screen_ordinal(page)?;
        let provisional = reading.quote_marker().is_some();
        self.finish_reflow_jump(before);
        if provisional {
            self.notice = Some(super::quote_repair::APPROXIMATE_PAGE_NOTICE.into());
        }
        Ok(())
    }

    pub(super) fn poll_screen_page_jump(&mut self) -> Option<Change> {
        let pending = self.screen_page_jump?;
        if !self.stats_awake || self.screen != Screen::GoToPage {
            return None;
        }
        let Some(active) = self.active.as_ref().filter(|active| {
            active.id == pending.book && active.reading.config.settings == pending.settings
        }) else {
            self.screen_page_worker.cancel();
            self.screen_page_jump = None;
            return Some(self.report("The book layout changed. Submit again to retry."));
        };
        if active.reading.whole_book_screen_pages().is_some() {
            self.screen_page_jump = None;
            return Some(match self.follow_screen_ordinal(pending.jump) {
                Ok(()) => Change::Partial,
                Err(error) => self.report(error),
            });
        }
        if !wants_background(&active.reading) {
            self.screen_page_jump = None;
            return Some(self.report(unavailable()));
        }
        None
    }

    pub(super) fn cancel_screen_page_jump(&mut self) {
        if self.screen_page_jump.take().is_some() {
            self.notice = Some("Page count cancelled. Submit again to retry.".into());
        }
    }

    pub(super) fn jump_fixed_number(&mut self, target: &str) -> Result<()> {
        let fixed = self.fixed.as_ref().ok_or("No open document")?;
        let reading = &fixed.reading;
        let count = reading.count();
        if count == 0 {
            return Err("This document has no pages".into());
        }
        let page = if let Some(label) = target.strip_prefix("label ") {
            reading.find_page_label(label.trim())
        } else if let Some(percent) = target.strip_suffix('%') {
            Some(page_at_percent(percent, count)?)
        } else if let Some(delta) = relative(target) {
            let delta = delta?;
            let current = reading.page();
            let page = reading
                .hidden_flow_map()
                .and_then(|map| map.relative_page(current, delta))
                .unwrap_or_else(|| current.saturating_add_signed(delta).min(count - 1));
            if page == current {
                self.show(Screen::Fixed);
                return Ok(());
            }
            Some(page)
        } else if target.starts_with('[')
            && reading
                .hidden_flow_map()
                .is_some_and(|map| map.has_hidden())
        {
            let (number, flow) = flow_page(target).ok_or("Enter [page] or [page]flow")?;
            let map = reading.hidden_flow_map().unwrap();
            let id = if flow == 0 {
                Some(0)
            } else {
                map.hidden_ids().nth(flow - 1)
            };
            Some(
                id.and_then(|id| map.nth(id, number - 1))
                    .ok_or("That page is outside the selected flow")?,
            )
        } else if let Ok(number) = target.parse::<usize>() {
            (1..=count).contains(&number).then(|| number - 1)
        } else {
            reading.find_page_label(target)
        };
        let page = page
            .ok_or_else(|| format!("Enter a page from 1 to {count} or a unique printed label"))?;
        self.follow_fixed(page, None)
    }
}

fn unavailable() -> Box<dyn std::error::Error + Send + Sync> {
    "Document page numbers are unavailable here. Enter chapter:page or a percentage.".into()
}

fn positive(input: &str) -> Option<usize> {
    let input = input.trim();
    (!input.is_empty() && input.bytes().all(|c| c.is_ascii_digit()))
        .then(|| input.parse::<usize>().ok())
        .flatten()
        .filter(|page| *page > 0)
}

fn relative(input: &str) -> Option<Result<isize>> {
    let digits = input.strip_prefix(['+', '-'])?;
    (!digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())).then(|| {
        input
            .parse()
            .map_err(|_| "That relative page number is too large".into())
    })
}

fn flow_page(input: &str) -> Option<(usize, usize)> {
    let (number, flow) = input.strip_prefix('[')?.split_once(']')?;
    let number = positive(number)?;
    let flow = if flow.is_empty() {
        0
    } else if flow.bytes().all(|c| c.is_ascii_digit()) {
        flow.parse().ok()?
    } else {
        return None;
    };
    Some((number, flow))
}

#[cfg(test)]
mod tests;
