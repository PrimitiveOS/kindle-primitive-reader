//! Incremental pagination groundwork for a verified simple long chapter.
//!
//! This consumes the exact, globally positioned line stream a future layout
//! continuation produces. Work-batch boundaries have no pagination meaning.
//! It is deliberately not connected to Reader or to the existing paginator.

use super::{LongChapterPlan, cancelled};
use crate::publisher_breaks::BoxRules;
use crate::publisher_padding::fit_content::HeightTail;
use crate::source::Source;
use crate::{Line, Result, Settings};
use std::ops::Range;
use std::sync::atomic::AtomicBool;

const MAX_LINES: usize = 30_000;
const MAX_PAGES: usize = 16_384;

pub(crate) struct SimplePagination<'a> {
    source: &'a Source,
    cancel: &'a AtomicBool,
    height: f32,
    limit: f32,
    bounds: Vec<f32>,
    pending: Option<Range<f32>>,
    extent_end: Option<f32>,
    previous_top: Option<f32>,
    previous_source: Option<(usize, usize)>,
    lines: usize,
    failed: bool,
}

impl<'a> SimplePagination<'a> {
    /// Require the actual complete layout metadata to show that the ordinary
    /// paginator has no break rules, splittable spaces or fitted box tails.
    /// The borrowed Source cannot change while this accumulator is alive.
    pub(crate) fn new(
        plan: &LongChapterPlan,
        source: &'a Source,
        xhtml: &str,
        settings: Settings,
        breaks: &[BoxRules],
        split_spaces: &[Range<f32>],
        fitted_tails: &[HeightTail],
        cancel: &'a AtomicBool,
    ) -> Result<Self> {
        cancelled(cancel)?;
        settings.validate()?;
        if !plan.matches_source(source, xhtml)
            || !breaks.is_empty()
            || !split_spaces.is_empty()
            || !fitted_tails.is_empty()
        {
            return Err("The long chapter layout cannot be paginated safely".into());
        }
        cancelled(cancel)?;
        let height = settings.text_height() as f32;
        let limit = settings.chapter_height_limit();
        if !height.is_finite() || height <= 0.0 || !limit.is_finite() || limit <= 0.0 {
            return Err("The long chapter page geometry is invalid".into());
        }
        Ok(Self {
            source,
            cancel,
            height,
            limit,
            bounds: vec![0.0],
            pending: None,
            extent_end: None,
            previous_top: None,
            previous_source: None,
            lines: 0,
            failed: false,
        })
    }

    /// Lines must be the exact complete stream in the same top-sorted order
    /// used by the ordinary paginator. Original block keys and byte ranges
    /// remain unchanged; unsupported projections fail closed.
    pub(crate) fn push(&mut self, line: &Line) -> Result<()> {
        if self.failed {
            return Err("The long chapter pagination is unavailable".into());
        }
        let result = self.push_line(line);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn push_line(&mut self, line: &Line) -> Result<()> {
        cancelled(self.cancel)?;
        if self.lines >= MAX_LINES
            || line.raw_whitespace.is_some()
            || line.spans.is_some()
            || line.additional_sources.is_some()
            || line.page_extension != [0.0, 0.0]
            || !line.top.is_finite()
            || !line.bottom.is_finite()
            || line.top < 0.0
            || line.top > line.bottom
            || line.bottom > self.limit
            || self
                .previous_top
                .is_some_and(|top| line.top.total_cmp(&top).is_lt())
        {
            return Err("The long chapter line stream is unsupported".into());
        }
        let index = self
            .source
            .blocks
            .binary_search_by_key(&line.block, |block| block.key)
            .map_err(|_| "The long chapter line has no original block")?;
        let block = &self.source.blocks[index];
        if line.bytes.start > line.bytes.end
            || line.bytes.end > block.text.len()
            || !block.text.is_char_boundary(line.bytes.start)
            || !block.text.is_char_boundary(line.bytes.end)
            || self
                .previous_source
                .is_some_and(|previous| (index, line.bytes.start) < previous)
        {
            return Err("The long chapter line has invalid source boundaries".into());
        }
        self.lines += 1;
        self.previous_top = Some(line.top);
        self.previous_source = Some((index, line.bytes.start));
        let extent = line.page_extent();
        self.extent_end = Some(
            self.extent_end
                .map_or(extent.end, |end| end.max(extent.end)),
        );
        if let Some(last) = &mut self.pending {
            if extent.start < last.end {
                last.start = last.start.min(extent.start);
                last.end = last.end.max(extent.end);
                return Ok(());
            }
        }
        if let Some(last) = self.pending.take() {
            self.include_interval(last)?;
        }
        self.pending = Some(extent);
        Ok(())
    }

    fn append_bound(&mut self, value: f32) -> Result<()> {
        if self.bounds.len() > MAX_PAGES
            || !value.is_finite()
            || value <= *self.bounds.last().expect("initial page bound")
        {
            return Err("The long chapter exceeds the pagination limit".into());
        }
        self.bounds.push(value);
        Ok(())
    }

    /// Equivalent to the no-break, no-splittable-space branch of the ordinary
    /// paginator after all overlapping line extents have been merged.
    fn include_interval(&mut self, interval: Range<f32>) -> Result<()> {
        if interval.end - interval.start > self.height {
            return Err("The chapter contains content taller than one reading page".into());
        }
        let origin = *self.bounds.last().expect("initial page bound");
        if interval.end > origin + self.height && interval.start > origin {
            let next = interval.start;
            while next >= *self.bounds.last().expect("initial page bound") + 2.0 * self.height {
                cancelled(self.cancel)?;
                let following = *self.bounds.last().expect("initial page bound") + self.height;
                self.append_bound(following)?;
            }
            if next > *self.bounds.last().expect("initial page bound") {
                self.append_bound(next)?;
            }
        }
        Ok(())
    }

    /// Returns boundaries for the supplied complete line stream only. A future
    /// caller must separately prove the layout and font/source snapshot, that
    /// no lines were omitted, and that both extents came from that layout.
    pub(crate) fn finish(mut self, content_height: f32, painted_bottom: f32) -> Result<Vec<f32>> {
        if self.failed {
            return Err("The long chapter pagination is unavailable".into());
        }
        cancelled(self.cancel)?;
        if let Some(last) = self.pending.take() {
            self.include_interval(last)?;
        }
        if !content_height.is_finite() || !painted_bottom.is_finite() || painted_bottom < 0.0 {
            return Err("The chapter exceeds the layout height limit".into());
        }
        let content_height = self
            .extent_end
            .map_or(content_height, |end| content_height.max(end));
        if !(0.0..=self.limit).contains(&content_height) {
            return Err("The chapter exceeds the layout height limit".into());
        }
        while content_height > *self.bounds.last().expect("initial page bound") + self.height {
            cancelled(self.cancel)?;
            let following = *self.bounds.last().expect("initial page bound") + self.height;
            self.append_bound(following)?;
        }
        if self.bounds.len() > MAX_PAGES {
            return Err("The long chapter exceeds the pagination limit".into());
        }
        // The ordinary paginator permits a zero-length final page.
        self.bounds
            .push(content_height.max(*self.bounds.last().expect("initial page bound")));
        cancelled(self.cancel)?;
        // Match the existing Parley post-pagination removal of unpainted tail.
        let mut pages = crate::Pagination {
            bounds: self.bounds,
        };
        if pages.len() > 1 {
            pages.trim_unpainted_tail(painted_bottom);
        }
        Ok(pages.bounds)
    }
}
