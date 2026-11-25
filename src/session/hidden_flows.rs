//! Resolve reader-authored flow boundaries against the active pagination.
use super::*;
use crate::library::hidden_flows::{HiddenFlows, PageFlows, ResolvedPoint};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Resolved {
    points: Vec<(usize, ResolvedPoint)>,
    counts: BTreeMap<usize, usize>,
    pub(super) unresolved: Vec<usize>,
    notes: BTreeMap<usize, Vec<(Range<usize>, usize)>>,
    spine_flows: BTreeMap<usize, usize>,
}

impl Resolved {
    pub(super) fn new(config: &BookConfig, current: &Probe, chapter: usize) -> Result<Self> {
        let mut result = Self::default();
        if !config.hidden_flows.enabled {
            if config.hide_nonlinear && config.can_separate_note_flows() {
                let layouts = automatic_note_layouts(config, current, chapter)?;
                let mut id = 0;
                let mut spine_flow = None;
                if !layouts.is_empty() {
                    for index in 0..current.chapter_count() {
                        if current.chapter_is_nonlinear(index) {
                            let flow = *spine_flow.get_or_insert_with(|| {
                                id += 1;
                                id
                            });
                            result.spine_flows.insert(index, 2 * flow + 1);
                        } else {
                            spine_flow = None;
                            if let Some((count, ranges)) = layouts.get(&index) {
                                result.counts.insert(index, *count);
                                let ranges_and_ids = ranges
                                    .iter()
                                    .map(|range| {
                                        id += 1;
                                        (range.clone(), id)
                                    })
                                    .collect();
                                result.notes.insert(index, ranges_and_ids);
                            }
                        }
                    }
                }
            }
            return Ok(result);
        }
        let mut groups = BTreeMap::<usize, Vec<_>>::new();
        for (index, point) in config.hidden_flows.points().iter().enumerate() {
            let position = &point.position;
            let valid = position.chapter < current.chapter_count()
                && position
                    .anchor
                    .as_ref()
                    .map_or(position.page == 0, |anchor| {
                        current.chapter_index(&anchor.chapter) == Some(position.chapter)
                    });
            if valid {
                groups
                    .entry(position.chapter)
                    .or_default()
                    .push((index, point));
            } else {
                result.unresolved.push(index);
            }
        }
        for (index, points) in groups {
            let loaded;
            let probe = if index == chapter {
                current
            } else {
                loaded = match config.chapter(index) {
                    Ok(probe) => probe,
                    Err(_) => {
                        result
                            .unresolved
                            .extend(points.into_iter().map(|(index, _)| index));
                        continue;
                    }
                };
                &loaded
            };
            result.counts.insert(index, probe.page_count());
            for (key, point) in points {
                let page = point
                    .position
                    .anchor
                    .as_ref()
                    .map_or(Some(0), |anchor| probe.flow_boundary_page(anchor));
                if let Some(page) = page {
                    result.points.push((
                        index,
                        ResolvedPoint {
                            page,
                            hidden: point.hidden,
                            index: key,
                        },
                    ));
                } else {
                    result.unresolved.push(key);
                }
            }
        }
        result
            .points
            .sort_by_key(|&(chapter, point)| (chapter, point.page, point.index));
        Ok(result)
    }

    fn inherited(&self, chapter: usize) -> usize {
        let mut flow = 0;
        for &(_, point) in self.points.iter().take_while(|&&(ch, _)| ch < chapter) {
            if !point.hidden {
                flow = 0;
            } else if flow == 0 {
                flow = point.index + 1;
            }
        }
        flow
    }

    pub(super) fn map(&self, chapter: usize, count: usize) -> PageFlows {
        if self.has_automatic_notes() {
            let spine = self.spine_flows.get(&chapter).copied().unwrap_or(0);
            let points = self
                .notes
                .get(&chapter)
                .into_iter()
                .flatten()
                .filter(|(range, _)| range.end <= count)
                .flat_map(|(range, id)| {
                    [
                        ResolvedPoint {
                            page: range.start,
                            hidden: true,
                            index: 2 * id,
                        },
                        ResolvedPoint {
                            page: range.end,
                            hidden: false,
                            index: 2 * id + 1,
                        },
                    ]
                })
                .collect();
            return PageFlows::new(count, spine, points);
        }
        PageFlows::new(
            count,
            self.inherited(chapter),
            self.points
                .iter()
                .filter(|&&(ch, _)| ch == chapter)
                .map(|&(_, point)| point)
                .collect(),
        )
    }

    pub(super) fn has_automatic_notes(&self) -> bool {
        !self.notes.is_empty()
    }

    pub(super) fn marker_at(&self, chapter: usize, page: usize) -> Option<usize> {
        self.points
            .iter()
            .find(|&&(ch, point)| ch == chapter && point.page == page)
            .map(|&(_, point)| point.index)
    }

    pub(super) fn inactive(&self) -> Vec<usize> {
        self.counts
            .iter()
            .flat_map(|(&chapter, &count)| self.map(chapter, count).inactive)
            .collect()
    }

    pub(super) fn weights(&self, source: &[u64], flow: usize) -> Vec<u64> {
        let mut total = 0;
        let mut result = vec![0];
        for (chapter, span) in source.windows(2).enumerate() {
            let count = self.counts.get(&chapter).copied().unwrap_or(1);
            let visible = self.map(chapter, count).count_in(flow, 0..count);
            let weight = span[1] - span[0];
            total += ((weight as f64 * visible as f64 / count as f64).round() as u64)
                .max(u64::from(visible > 0));
            result.push(total);
        }
        result
    }
}

fn automatic_note_layouts(
    config: &BookConfig,
    current: &Probe,
    chapter: usize,
) -> Result<BTreeMap<usize, (usize, Vec<Range<usize>>)>> {
    let result = with_storage(|| {
        let mut book = Some(Book::open(&config.book).map_err(io::Error::other)?);
        if book.as_ref().is_none_or(|book| !book.is_reflowable_epub()) {
            return Ok(BTreeMap::new());
        }
        let styles = config.user_styles.clone().for_nonlinear_notes(true);
        let mut fonts = None::<parley::FontContext>;
        let mut layouts = BTreeMap::new();
        for index in 0..current.chapter_count() {
            if current.chapter_is_nonlinear(index) {
                continue;
            }
            if index == chapter {
                let ranges = current.canonical_note_flows();
                if !ranges.is_empty() {
                    layouts.insert(index, (current.page_count(), ranges));
                }
                continue;
            }
            let Ok(xhtml) = book
                .as_mut()
                .ok_or_else(|| io::Error::other("Reading source is unavailable"))?
                .chapter(index)
            else {
                continue;
            };
            // Composed notes need a same-chapter fragment href. Entities can
            // also encode the separator, so leave those inputs to the parser.
            if !xhtml.contains(['#', '&']) || !xhtml.contains("href") {
                continue;
            }
            if fonts.is_none() {
                fonts = Some(crate::font_context(&config.fonts).map_err(io::Error::other)?);
            }
            let font_context = fonts
                .as_ref()
                .ok_or_else(|| io::Error::other("The reading font is unavailable"))?;
            let probe = Probe::layout_opened_book(
                &mut book,
                config.engine,
                config.settings,
                index,
                None,
                &config.user_hyphenation,
                &styles,
                || Ok(font_context.clone()),
            );
            if let Ok(probe) = probe {
                let ranges = probe.canonical_note_flows();
                if !ranges.is_empty() {
                    layouts.insert(index, (probe.page_count(), ranges));
                }
            }
        }
        Ok(layouts)
    })?;
    Ok(result)
}

impl BookConfig {
    pub(super) fn can_separate_note_flows(&self) -> bool {
        self.engine == Engine::Parley
            && !self.scroll
            && !self.facing_pages
            && !self.hidden_flows.enabled
            && (self.settings.in_page_footnotes || self.user_styles.in_page_epub_footnotes())
            && self.user_styles.combines_nonlinear_notes()
    }
}

impl Reading {
    pub(super) fn uses_page_flows(&self) -> bool {
        self.config.hidden_flows.enabled || self.custom_flows.has_automatic_notes()
    }

    pub(crate) fn has_nonlinear_fragments(&self) -> bool {
        self.probe.has_nonlinear_chapters()
            || self.custom_flows.has_automatic_notes()
            || (self.config.can_separate_note_flows() && self.probe.in_page_notes.is_some())
    }

    /// Zero-based position and page count of the current chapter in the active flow.
    pub(crate) fn visible_progress_pages(&self, page: usize) -> (usize, usize) {
        let count = self.probe.page_count();
        let page = page.min(count.saturating_sub(1));
        self.probe.flow_pages.as_ref().map_or((page, count), |map| {
            let flow = map.flow(page);
            (
                map.count_in(flow, 0..page),
                map.count_in(flow, 0..count).max(1),
            )
        })
    }

    pub(crate) fn custom_flow_map(&self) -> Option<PageFlows> {
        self.custom_flow_map_for_chapter(self.chapter, self.probe.page_count())
    }

    pub(crate) fn custom_flow_map_for_chapter(
        &self,
        chapter: usize,
        count: usize,
    ) -> Option<PageFlows> {
        self.uses_page_flows()
            .then(|| self.custom_flows.map(chapter, count))
    }

    pub(crate) fn custom_flow_for_page(
        &self,
        chapter: usize,
        page: usize,
        count: usize,
    ) -> Option<usize> {
        self.custom_flow_map_for_chapter(chapter, count)
            .map(|flows| flows.flow(page))
    }

    /// `true` means the whole source chapter is hidden; `false` is a partial range.
    pub(crate) fn hidden_chapter(&self, chapter: usize) -> Option<bool> {
        if !self.uses_page_flows() {
            return None;
        }
        let count = if chapter == self.chapter {
            self.probe.page_count()
        } else {
            self.custom_flows.counts.get(&chapter).copied().unwrap_or(1)
        };
        let map = self.custom_flows.map(chapter, count);
        map.has_hidden().then(|| map.count_in(0, 0..count) == 0)
    }

    pub(crate) fn hidden_flow_ordinal(&self, flow: usize) -> Option<usize> {
        let mut ids = std::collections::BTreeSet::new();
        for (&chapter, &count) in &self.custom_flows.counts {
            ids.extend(self.custom_flows.map(chapter, count).hidden_ids());
        }
        ids.extend(
            self.custom_flows
                .map(self.chapter, self.probe.page_count())
                .hidden_ids(),
        );
        ids.extend(self.custom_flows.spine_flows.values().copied());
        ids.into_iter()
            .position(|id| id == flow)
            .map(|index| index + 1)
    }

    pub(crate) fn flow_marker_at(&self, chapter: usize, page: usize) -> Option<usize> {
        self.custom_flows.marker_at(chapter, page)
    }

    pub(crate) fn unresolved_flow_markers(&self) -> &[usize] {
        &self.custom_flows.unresolved
    }

    pub(crate) fn inactive_flow_markers(&self) -> Vec<usize> {
        self.custom_flows.inactive()
    }

    pub(crate) fn set_hidden_flows(&mut self, flows: HiddenFlows) -> Result<()> {
        if flows.enabled == self.config.hidden_flows.enabled
            && flows.points() == self.config.hidden_flows.points()
        {
            self.config.hidden_flows = std::rc::Rc::new(flows);
            return Ok(());
        }
        let mut config = self.config.clone();
        config.hidden_flows = std::rc::Rc::new(flows);
        if config.hide_nonlinear
            && (config.can_separate_note_flows() || self.config.can_separate_note_flows())
            && config.hidden_flows.enabled != self.config.hidden_flows.enabled
        {
            config.eager_layouts = Default::default();
            config.screen_pages = Default::default();
            *self = Self::restore(config, self.chapter, self.page, self.anchor().as_ref())?;
            return Ok(());
        }
        // A boundary changes which existing pages are shown, not their typography.
        // Reuse the current chapter's layout; only other marker-bearing chapters
        // need resolving. In particular, editing a long single-chapter book must
        // not shape the entire chapter again for each marker.
        let resolved = Resolved::new(&config, &self.probe, self.chapter)?;
        let next = config
            .hidden_flows
            .enabled
            .then(|| resolved.map(self.chapter, self.probe.page_count()));
        let anchor = self.anchor();
        let top = self.scroll_top.map(|top| {
            if config.hidden_flows.enabled {
                top
            } else {
                top.clamp(0.0, self.probe.scroll_end())
            }
        });
        let page = top.map_or(self.page, |top| self.probe.page_at(top));
        let previous = std::mem::replace(&mut self.probe.flow_pages, next);
        let frame = match capture(&config, &mut self.probe, self.chapter, page, top) {
            Ok(frame) => frame,
            Err(error) => {
                self.probe.flow_pages = previous;
                return Err(error);
            }
        };
        self.config = config;
        self.custom_flows = resolved;
        self.frame = frame;
        self.page = page;
        self.scroll_top = top;
        self.focus_anchor = anchor;
        self.adjacent.clear();
        self.error = None;
        self.pause_turns();
        self.refresh_flow_weights();
        self.schedule_prefetch();
        Ok(())
    }

    pub(super) fn custom_weights(&self) -> Vec<u64> {
        let source = self
            .config
            .chapter_offsets
            .get()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let flow = self
            .custom_flows
            .map(self.chapter, self.probe.page_count())
            .flow(self.page);
        self.custom_flows.weights(source, flow)
    }

    pub(super) fn jump_custom_progress(&mut self, fraction: f64) -> Result<()> {
        let source = self
            .config
            .chapter_offsets
            .get()
            .ok_or("Book progress is unavailable")?;
        let weights = self.custom_flows.weights(source, 0);
        let total = *weights.last().ok_or("Book progress is unavailable")?;
        if total == 0 {
            return Err(
                "All pages are in hidden flows. Show them or restart the regular flow first."
                    .into(),
            );
        }
        let target = fraction * total.saturating_sub(1) as f64;
        let chapter = weights
            .partition_point(|&weight| weight as f64 <= target)
            .saturating_sub(1)
            .min(weights.len().saturating_sub(2));
        let weight = weights[chapter + 1] - weights[chapter];
        let within = if fraction >= 1.0 {
            1.0
        } else {
            (target - weights[chapter] as f64) / weight as f64
        };
        let probe = self.config.chapter(chapter)?;
        let map = self.custom_flows.map(chapter, probe.page_count());
        let visible = map.count_in(0, 0..probe.page_count());
        let index = ((visible.saturating_sub(1)) as f64 * within).round() as usize;
        let page = map
            .nth(0, index)
            .ok_or("That regular-flow page is unavailable")?;
        self.use_chapter(probe, chapter, None, page)
    }

    pub(super) fn turn_in_custom_flow(&mut self, direction: isize) -> Result<Change> {
        let map = self.custom_flow_map().ok_or("Custom flows are disabled")?;
        let current_flow = map.flow(self.page);
        let span = frame_span(&self.config, &self.probe);
        let candidate = if direction > 0 {
            self.last_visible_page()
                .checked_add(1)
                .and_then(|page| map.allowed_page(page, direction, current_flow))
        } else {
            let previous = self
                .page
                .checked_sub(1)
                .and_then(|page| map.allowed_page(page, direction, current_flow));
            if span == 2 {
                previous
                    .and_then(|page| page.checked_sub(1))
                    .and_then(|page| map.allowed_page(page, direction, current_flow))
                    .or(previous)
            } else {
                previous
            }
        };
        if let Some(page) = candidate {
            let frame = capture(&self.config, &mut self.probe, self.chapter, page, None)?;
            self.frame = frame;
            self.page = page;
            self.scroll_top = None;
            self.focus_anchor = None;
            self.adjacent.clear();
            self.error = None;
            self.refresh_flow_weights();
            return Ok(Change::Partial);
        }
        let mut chapter = self.chapter;
        while let Some(next) = chapter
            .checked_add_signed(direction)
            .filter(|&ch| ch < self.probe.chapter_count())
        {
            chapter = next;
            let probe = self.config.chapter(chapter)?;
            let map = self.custom_flows.map(chapter, probe.page_count());
            let first = if direction < 0 {
                probe.page_count() - 1
            } else {
                0
            };
            if let Some(mut page) = map.allowed_page(first, direction, current_flow) {
                if direction < 0 && frame_span(&self.config, &probe) == 2 {
                    page = page
                        .checked_sub(1)
                        .and_then(|previous| map.allowed_page(previous, direction, current_flow))
                        .unwrap_or(page);
                }
                self.use_chapter(probe, chapter, None, page)?;
                return Ok(Change::Partial);
            }
        }
        Ok(Change::None)
    }

    pub(super) fn scroll_in_custom_flow(&mut self, top: f32, distance: f32) -> Result<Change> {
        if distance == 0.0 {
            return Ok(Change::None);
        }
        let map = self.custom_flow_map().ok_or("Custom flows are disabled")?;
        let flow = map.flow(self.page);
        let ranges = allowed_intervals(&self.probe, flow);
        let total: f32 = ranges.iter().map(|range| range.end - range.start).sum();
        let current = virtual_position(&ranges, top);
        let target = (current + distance).clamp(
            0.0,
            (total - self.config.settings.text_height() as f32).max(0.0),
        );
        let next = source_position(&ranges, target).unwrap_or(top);
        let advances = if distance > 0.0 {
            next > top + 0.01
        } else {
            next < top - 0.01
        };
        if advances {
            let page = self.probe.page_at(next);
            self.frame = capture(
                &self.config,
                &mut self.probe,
                self.chapter,
                page,
                Some(next),
            )?;
            self.page = page;
            self.scroll_top = Some(next);
            self.focus_anchor = None;
            self.refresh_flow_weights();
            return Ok(Change::Partial);
        }
        let direction = if distance > 0.0 { 1 } else { -1 };
        let mut chapter = self.chapter;
        while let Some(next) = chapter
            .checked_add_signed(direction)
            .filter(|&ch| ch < self.probe.chapter_count())
        {
            chapter = next;
            let mut probe = self.config.chapter(chapter)?;
            let map = self.custom_flows.map(chapter, probe.page_count());
            let first = if direction < 0 {
                probe.page_count() - 1
            } else {
                0
            };
            let Some(page) = map.allowed_page(first, direction, flow) else {
                continue;
            };
            probe.flow_pages = Some(map);
            if probe.is_vertical() {
                self.use_chapter(probe, chapter, None, page)?;
            } else {
                let ranges = allowed_intervals(&probe, flow);
                let total: f32 = ranges.iter().map(|range| range.end - range.start).sum();
                let offset = if direction < 0 {
                    (total - probe.settings.text_height() as f32).max(0.0)
                } else {
                    0.0
                };
                let top =
                    source_position(&ranges, offset).ok_or("That flow has no visible content")?;
                self.use_chapter(probe, chapter, None, page)?;
                self.page = self.probe.page_at(top);
                self.frame = capture(&self.config, &mut self.probe, chapter, self.page, Some(top))?;
                self.scroll_top = Some(top);
                self.refresh_flow_weights();
            }
            return Ok(Change::Partial);
        }
        Ok(Change::None)
    }
}

fn allowed_intervals(probe: &Probe, flow: usize) -> Vec<Range<f32>> {
    let Some(map) = &probe.flow_pages else {
        return vec![0.0..probe.page_range(probe.page_count() - 1).end];
    };
    map.allowed_ranges(flow)
        .into_iter()
        .filter(|range| !range.is_empty())
        .map(|range| probe.page_range(range.start).start..probe.page_range(range.end - 1).end)
        .collect()
}

fn virtual_position(ranges: &[Range<f32>], source: f32) -> f32 {
    let mut offset = 0.0;
    for range in ranges {
        if source < range.end {
            return offset + (source - range.start).max(0.0);
        }
        offset += range.end - range.start;
    }
    offset
}

fn source_position(ranges: &[Range<f32>], mut offset: f32) -> Option<f32> {
    for range in ranges {
        let length = range.end - range.start;
        if offset < length {
            return Some(range.start + offset);
        }
        offset -= length;
    }
    ranges.last().map(|range| range.end)
}

pub(super) fn scroll_parts(probe: &Probe, top: f32) -> Vec<VisiblePart> {
    let Some(map) = &probe.flow_pages else {
        return Vec::new();
    };
    let flow = map.flow(probe.page_at(top));
    let mut parts = Vec::new();
    let mut used = 0.0;
    let height = probe.settings.text_height() as f32;
    for range in allowed_intervals(probe, flow)
        .into_iter()
        .filter(|range| range.end > top)
    {
        if used >= height {
            break;
        }
        let start = range.start.max(top);
        let end = range.end.min(start + height - used);
        parts.push(VisiblePart {
            range: start..end,
            x: 0,
            y: used.round() as u32,
        });
        used += end - start;
    }
    parts
}

pub(super) fn capture_scroll(probe: &mut Probe, top: f32) -> Result<Vec<u8>> {
    let mut pixels = vec![255; (probe.settings.width * probe.settings.height) as usize];
    let stride = probe.settings.width as usize;
    let margin = probe.settings.top_margin() as usize;
    let text_height = probe.settings.text_height();
    for part in scroll_parts(probe, top) {
        let rows = ((part.range.end - part.range.start).ceil() as u32)
            .min(text_height.saturating_sub(part.y));
        let source = probe.capture_range(part.range)?;
        for row in 0..rows as usize {
            let from = (margin + row) * stride;
            let to = (margin + part.y as usize + row) * stride;
            pixels[to..to + stride].copy_from_slice(&source[from..from + stride]);
        }
    }
    Ok(pixels)
}
