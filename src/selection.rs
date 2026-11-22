//! Text selection and page-local highlight geometry for reflowable books.

use std::ops::Range;

use icu_segmenter::{WordSegmenter, options::WordBreakInvariantOptions};
use parley::{Affinity, BoundingBox, Cluster, Cursor, Layout, Selection};
use serde::{Deserialize, Serialize};

use crate::flow_projection::Affinity as SourceAffinity;
use crate::{Anchor, Probe, Renderer, Settings};

/// A half-open range of source text, ordered in publication reading order.
/// `text` preserves the source wording with the same whitespace normalization
/// used by the renderer. Paragraphs in a selection are separated by newlines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub start: Anchor,
    pub end: Anchor,
    pub text: String,
}

/// A highlight rectangle in the upright page image, including its margins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HighlightRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Probe {
    /// Up to `words` source words on either side of a selected passage.
    /// It uses the selection anchors so repeated words elsewhere on the page
    /// cannot attach their context to a vocabulary entry.
    pub(crate) fn word_context(
        &self,
        selection: &TextSelection,
        words: usize,
    ) -> Option<(String, String)> {
        let spans = self.selection_spans(&selection.start, &selection.end)?;
        let mut before = String::new();
        for (index, block) in self
            .source
            .blocks
            .iter()
            .enumerate()
            .take(spans.first + 1)
            .rev()
        {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let end = if index == spans.first {
                view.project_nearest(spans.first_byte, SourceAffinity::Before)?
            } else {
                view.text().len()
            };
            let text = view.text().get(..end)?;
            before = bounded_context(
                &format!("{} {before}", bounded_context(text, words, true)),
                words,
                true,
            );
            if word_ranges(&before).count() >= words || before.chars().count() == 512 {
                break;
            }
        }
        let mut after = String::new();
        for (index, block) in self.source.blocks.iter().enumerate().skip(spans.last) {
            let view = self.reading_flow.view(&self.source, block).ok()?;
            let start = if index == spans.last {
                view.project_nearest(spans.last_byte, SourceAffinity::After)?
            } else {
                0
            };
            let text = view.text().get(start..)?;
            after = bounded_context(
                &format!("{after} {}", bounded_context(text, words, false)),
                words,
                false,
            );
            if word_ranges(&after).count() >= words || after.chars().count() == 512 {
                break;
            }
        }
        Some((before, after))
    }

    /// A bounded automatic-word scan from the original source start. It stops
    /// at punctuation, inline block boundaries and 20 Unicode scalar values.
    /// The returned endpoint is an actual source anchor, never a lookup form.
    pub(crate) fn dictionary_scan(&self, selection: &TextSelection) -> Option<TextSelection> {
        let spans = self.selection_spans(&selection.start, &selection.end)?;
        if spans.first != spans.last {
            return None;
        }
        let block = &self.source.blocks[spans.first];
        let view = self.reading_flow.view(&self.source, block).ok()?;
        if !view.source_is_present(spans.first_byte) {
            return None;
        }
        let start = view.project_offset(spans.first_byte, SourceAffinity::After)?;
        let tail = view.text().get(start..)?;
        let length = tail
            .char_indices()
            .take(crate::dictionary::MAX_SCAN_CHARACTERS)
            .take_while(|(_, c)| crate::dictionary::cjk_word_character(*c))
            .map(|(i, c)| i + c.len_utf8())
            .last()?;
        let length = icu_segmenter::GraphemeClusterSegmenter::new()
            .segment_str(tail)
            .take_while(|end| *end <= length)
            .last()
            .filter(|end| *end > 0)?;
        self.select_range(
            &self.anchor(block.key, view.source_offset(start, SourceAffinity::After)?),
            &self.anchor(
                block.key,
                view.source_offset(start + length, SourceAffinity::Before)?,
            ),
        )
    }

    /// Select the Unicode word directly under a page coordinate.
    pub fn word_at(&self, page: usize, x: f32, y: f32) -> Option<TextSelection> {
        self.word_in(self.pages.range(page)?, x, y)
    }

    pub(crate) fn word_in(&self, range: Range<f32>, x: f32, y: f32) -> Option<TextSelection> {
        if !self.is_vertical() {
            self.selection_point(range.clone(), x, y)?;
        }
        let (key, offset) = self.source_at(range, x, y)?;
        let block = &self.source.blocks[self.selection_block(key)?];
        let view = self.reading_flow.view(&self.source, block).ok()?;
        if !view.source_is_present(offset) {
            return None;
        }
        let offset = view.project_offset(offset, SourceAffinity::After)?;
        let word = word_range(view.text(), offset)?;
        self.select_range(
            &self.anchor(key, view.source_offset(word.start, SourceAffinity::After)?),
            &self.anchor(key, view.source_offset(word.end, SourceAffinity::Before)?),
        )
    }

    /// Find the caret nearest the point within a visible paragraph.
    pub fn text_position_at(&self, page: usize, x: f32, y: f32) -> Option<Anchor> {
        let range = self.pages.range(page)?;
        if let Renderer::Vertical(renderer) = &self.renderer {
            let (key, offset) = renderer.caret_at(range, x, y)?;
            return Some(self.anchor(key, offset));
        }
        let point = self.selection_point(range, x, y)?;
        let Renderer::Parley(renderer) = &self.renderer else {
            return None;
        };
        for (key, left, origin, column, layout) in renderer.selection_layouts(point.page) {
            let y = point.y - origin;
            if column.contains(&point.x)
                && (0.0..layout.height()).contains(&y)
                && Cluster::from_point(layout, point.x - left, y).is_some()
            {
                let cursor = Cursor::from_point(layout, point.x - left, y);
                let offset = match renderer.mapping(key) {
                    Some(mapping) => mapping.source_offset(
                        cursor.index(),
                        match cursor.affinity() {
                            Affinity::Upstream => SourceAffinity::Before,
                            Affinity::Downstream => SourceAffinity::After,
                        },
                    )?,
                    None => cursor.index(),
                };
                return Some(self.anchor(key, offset));
            }
        }
        None
    }

    /// Select an anchored range across adjacent blocks and pages of the loaded
    /// chapter. The caller may supply the endpoints in either direction.
    pub fn select_range(&self, start: &Anchor, end: &Anchor) -> Option<TextSelection> {
        self.select_quote(start, end, false)
    }

    /// Historical quote recovery must not become corrupt when current CSS
    /// intentionally omits unchanged canonical text. Never rewrite saved quotes.
    pub(crate) fn select_source_range(
        &self,
        start: &Anchor,
        end: &Anchor,
    ) -> Option<TextSelection> {
        self.select_quote(start, end, true)
    }

    fn select_quote(&self, start: &Anchor, end: &Anchor, canonical: bool) -> Option<TextSelection> {
        let spans = self.selection_spans(start, end)?;
        let mut quote = String::new();
        for (offset, block) in self.source.blocks[spans.first..=spans.last]
            .iter()
            .enumerate()
        {
            let index = spans.first + offset;
            let Some(span) = spans.in_block(index, block.text.len()) else {
                continue;
            };
            let part = if canonical {
                block.text.get(span)?.to_owned()
            } else {
                let view = self.reading_flow.view(&self.source, block).ok()?;
                let mut part = String::new();
                for range in view.layout_spans(span)? {
                    part.push_str(view.text().get(range)?);
                }
                part
            };
            if part.is_empty() {
                continue;
            }
            if !quote.is_empty() {
                quote.push('\n');
            }
            quote.push_str(&part);
        }
        if quote.trim().is_empty() {
            return None;
        }
        Some(TextSelection {
            start: self.anchor(self.source.blocks[spans.first].key, spans.first_byte),
            end: self.anchor(self.source.blocks[spans.last].key, spans.last_byte),
            text: quote,
        })
    }

    /// Return the portions of a selection visible on this page. Geometry is
    /// derived anew from source anchors after font, page, or viewport changes.
    pub fn selection_rects(&self, page: usize, selection: &TextSelection) -> Vec<HighlightRect> {
        let Some(range) = self.pages.range(page) else {
            return Vec::new();
        };
        self.selection_rects_in(range, selection)
    }

    pub(crate) fn selection_rects_in(
        &self,
        page: Range<f32>,
        selection: &TextSelection,
    ) -> Vec<HighlightRect> {
        let Some(spans) = self.selection_spans(&selection.start, &selection.end) else {
            return Vec::new();
        };
        if let Renderer::Vertical(renderer) = &self.renderer {
            return (spans.first..=spans.last)
                .flat_map(|index| {
                    let block = &self.source.blocks[index];
                    spans
                        .in_block(index, block.text.len())
                        .map_or_else(Vec::new, |bytes| {
                            let Ok(view) = self.reading_flow.view(&self.source, block) else {
                                return Vec::new();
                            };
                            view.layout_spans(bytes)
                                .unwrap_or_default()
                                .into_iter()
                                .flat_map(|bytes| {
                                    renderer.selection_rects(page.clone(), block.key, bytes)
                                })
                                .collect()
                        })
                })
                .collect();
        }
        let Renderer::Parley(renderer) = &self.renderer else {
            return Vec::new();
        };
        let mut rects = Vec::new();
        for (key, left, origin, _, layout) in renderer.selection_layouts(page.clone()) {
            let Some(index) = self.selection_block(key) else {
                continue;
            };
            let Some(span) = spans.in_block(index, self.source.blocks[index].text.len()) else {
                continue;
            };
            let Ok(view) = self
                .reading_flow
                .view(&self.source, &self.source.blocks[index])
            else {
                continue;
            };
            let Some(layout_spans) = view.layout_spans(span) else {
                continue;
            };
            for span in layout_spans {
                let Some(span) = visible_span(layout, origin, &page, span) else {
                    continue;
                };
                let start = Cursor::from_byte_index(layout, span.start, Affinity::Downstream);
                let boundary = Cluster::from_byte_index(layout, span.end - 1)
                    .map_or(span.end, |cluster| cluster.text_range().end);
                let end = Cursor::from_byte_index(layout, boundary, Affinity::Upstream);
                Selection::new(start, end).geometry_with(layout, |mut rect, row| {
                    rect.x0 += f64::from(left);
                    rect.x1 += f64::from(left);
                    // Parley includes replaced elements at the same caret position
                    // in selection rectangles. Highlight the surrounding text runs.
                    if !self.source.blocks[index].images.is_empty()
                        && let Some(line) = layout.get(row)
                    {
                        for item in line.items() {
                            let parley::PositionedLayoutItem::InlineBox(image) = item else {
                                continue;
                            };
                            let right = f64::from(left + image.x + image.width);
                            let left = f64::from(left + image.x);
                            if left >= rect.x1 || right <= rect.x0 {
                                continue;
                            }
                            if rect.x0 < left
                                && let Some(left_run) = page_rect(
                                    BoundingBox { x1: left, ..rect },
                                    origin,
                                    &page,
                                    self.settings,
                                )
                            {
                                rects.push(left_run);
                            }
                            rect.x0 = right;
                            if rect.x0 >= rect.x1 {
                                return;
                            }
                        }
                    }
                    if let Some(rect) = page_rect(rect, origin, &page, self.settings) {
                        rects.push(rect);
                    }
                });
            }
        }
        rects
    }

    fn selection_point(&self, page: Range<f32>, x: f32, y: f32) -> Option<PagePoint> {
        let left = self.settings.left_margin() as f32;
        if !(left..(self.settings.width - self.settings.right_margin()) as f32).contains(&x) {
            return None;
        }
        let local_y = y - self.settings.top_margin() as f32;
        let y = local_y + page.start;
        ((0.0..self.settings.text_height() as f32).contains(&local_y) && page.contains(&y))
            .then_some(PagePoint {
                page,
                x: x - left,
                y,
            })
    }

    fn selection_block(&self, key: usize) -> Option<usize> {
        self.source
            .blocks
            .binary_search_by_key(&key, |block| block.key)
            .ok()
    }

    fn selection_spans(&self, start: &Anchor, end: &Anchor) -> Option<Spans> {
        if start.illustration.is_some()
            || end.illustration.is_some()
            || start.boundary.is_some()
            || end.boundary.is_some()
        {
            return None;
        }
        let (a, b) = if start.chapter == end.chapter {
            (self.project_anchor(start)?, self.project_anchor(end)?)
        } else {
            if start.book != self.book || end.book != self.book {
                return None;
            }
            let start_chapter = self.chapter_index(&start.chapter)?;
            let end_chapter = self.chapter_index(&end.chapter)?;
            let current = self.chapter_index(&self.chapter)?;
            if !(start_chapter..=end_chapter).contains(&current) {
                return None;
            }
            let local_start = if current == start_chapter {
                self.project_anchor(start)?
            } else {
                (
                    self.source
                        .blocks
                        .iter()
                        .position(|block| !block.text.is_empty())?,
                    0,
                )
            };
            let local_end = if current == end_chapter {
                self.project_anchor(end)?
            } else {
                let last = self
                    .source
                    .blocks
                    .iter()
                    .rposition(|block| !block.text.is_empty())?;
                (last, self.source.blocks[last].text.len())
            };
            (local_start, local_end)
        };
        if a == b {
            return None;
        }
        let ((first, first_byte), (last, last_byte)) = if a < b { (a, b) } else { (b, a) };
        Some(Spans {
            first,
            last,
            first_byte,
            last_byte,
        })
    }
}

struct PagePoint {
    page: Range<f32>,
    x: f32,
    y: f32,
}

struct Spans {
    first: usize,
    last: usize,
    first_byte: usize,
    last_byte: usize,
}

impl Spans {
    fn in_block(&self, index: usize, length: usize) -> Option<Range<usize>> {
        if !(self.first..=self.last).contains(&index) {
            return None;
        }
        let start = if index == self.first {
            self.first_byte
        } else {
            0
        };
        let end = if index == self.last {
            self.last_byte
        } else {
            length
        };
        (start < end).then_some(start..end)
    }
}

pub(crate) fn word_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let words = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    let mut start = 0;
    words
        .segment_str(text)
        .iter_with_word_type()
        .filter_map(move |(end, word_type)| {
            let range = start..end;
            start = end;
            // Combining marks can end a word segment that ICU does not tag as word-like.
            (word_type.is_word_like() || text[range.clone()].chars().any(char::is_alphanumeric))
                .then_some(range)
        })
}

/// Unicode word context with a scalar cap for space-free text and long tokens.
pub(crate) fn bounded_context(text: &str, words: usize, from_end: bool) -> String {
    if words == 0 {
        return String::new();
    }
    let text = if from_end {
        let start = text.char_indices().rev().nth(2047).map_or(0, |(i, _)| i);
        &text[start..]
    } else {
        &text[..text.char_indices().nth(2048).map_or(text.len(), |(i, _)| i)]
    };
    let ranges: Vec<_> = word_ranges(text).collect();
    let Some(first) = ranges.first() else {
        return String::new();
    };
    let last = ranges.last().unwrap();
    let range = if from_end {
        ranges[ranges.len().saturating_sub(words)].start..last.end
    } else {
        first.start..ranges[(words - 1).min(ranges.len() - 1)].end
    };
    let selected = &text[range];
    if from_end {
        selected[selected.char_indices().rev().nth(511).map_or(0, |(i, _)| i)..].to_owned()
    } else {
        selected.chars().take(512).collect()
    }
}

pub(crate) fn word_range(text: &str, offset: usize) -> Option<Range<usize>> {
    word_ranges(text)
        .find(|range| range.end > offset)
        .filter(|range| range.start <= offset)
}

fn visible_span(
    layout: &Layout<impl parley::style::Brush>,
    origin: f32,
    page: &Range<f32>,
    span: Range<usize>,
) -> Option<Range<usize>> {
    let first = layout
        .lines()
        .find(|line| origin + line.metrics().content_block_max_coord > page.start)?;
    let last = layout
        .lines()
        .rev()
        .find(|line| origin + line.metrics().content_block_min_coord < page.end)?;
    let start = span.start.max(first.text_range().start);
    let end = span.end.min(last.text_range().end);
    (start < end).then_some(start..end)
}

fn page_rect(
    rect: BoundingBox,
    origin: f32,
    page: &Range<f32>,
    settings: Settings,
) -> Option<HighlightRect> {
    if ![rect.x0, rect.y0, rect.x1, rect.y1]
        .iter()
        .all(|edge| edge.is_finite())
    {
        return None;
    }
    let left_margin = f64::from(settings.left_margin());
    let top_margin = f64::from(settings.top_margin());
    let right = f64::from(settings.width - settings.right_margin());
    let bottom = top_margin
        + f64::from(
            (page.end - page.start)
                .ceil()
                .min(settings.text_height() as f32),
        );
    let y = top_margin + f64::from(origin - page.start);
    let left = (rect.x0 + left_margin).floor().clamp(left_margin, right) as i32;
    let top = (rect.y0 + y).floor().clamp(top_margin, bottom) as i32;
    let right = (rect.x1 + left_margin).ceil().clamp(left_margin, right) as i32;
    let bottom = (rect.y1 + y).ceil().clamp(top_margin, bottom) as i32;
    (right > left && bottom > top).then_some(HighlightRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests {

    use crate::Engine;

    use super::*;

    #[test]
    fn selects_unicode_words_and_skips_punctuation() {
        let text = "A cafe\u{301}, العربية. こんにちは世界";
        for expected in ["cafe\u{301}", "العربية", "こんにちは", "世界"] {
            let position = text.find(expected).unwrap();
            let word = word_range(text, position).unwrap();
            assert_eq!(&text[word], expected);
        }
        assert!(word_range(text, text.find(',').unwrap()).is_none());
    }

    #[test]
    fn independent_margins_keep_words_and_highlights_in_the_painted_viewport() {
        let fonts = [crate::test_fixture("fonts/Silkscreen-Regular.ttf")];
        let book = crate::test_fixture("field-notes-epub3.epub");
        let settings = Settings {
            width: 460,
            height: 700,
            margin_sides: Some(68),
            margin_top: Some(86),
            margin_bottom: Some(33),
            ..Settings::default()
        };
        let mut probe = Probe::open(&book, Engine::Parley, &fonts, settings, 0).unwrap();
        let start = probe.matches("The tide keeps its own time.")[0].clone();
        let mut end = start.clone();
        end.offset += "The".len();
        let selection = probe.select_range(&start, &end).unwrap();
        let page = probe.page_for(&start).unwrap();
        let rect = probe.selection_rects(page, &selection)[0];
        assert!(rect.x >= 68 && rect.x + rect.width <= 392);
        assert!(rect.y >= 86 && rect.y + rect.height <= 667);
        assert_eq!(
            probe
                .word_at(
                    page,
                    rect.x as f32 + 5.0,
                    rect.y as f32 + rect.height as f32 / 2.0
                )
                .unwrap()
                .text,
            "The",
        );
        let pixels = probe.capture(page).unwrap();
        let stride = settings.width as usize;
        assert!(pixels[..86 * stride].iter().all(|&pixel| pixel == 255));
        assert!(pixels[667 * stride..].iter().all(|&pixel| pixel == 255));
    }

    #[test]
    fn locates_and_draws_a_highlight_across_blocks_and_pages() {
        let font = crate::test_fixture("fonts/Silkscreen-Regular.ttf");
        let book = crate::test_fixture("field-notes-epub3.epub");
        let settings = Settings {
            width: 360,
            height: 420,
            font_size: 32.0,
            margin: 18,
            line_height: 1.4,
            ..Settings::default()
        };
        let probe = Probe::open(
            &book,
            Engine::Parley,
            std::slice::from_ref(&font),
            settings,
            0,
        )
        .unwrap();
        let repeats = probe.matches("The tide keeps its own time.");
        let first = &repeats[0];
        let finish = probe.fragment("lantern").unwrap();
        let span = probe.select_range(&finish, first).unwrap();
        assert_eq!(&span.start, first);
        assert_eq!(span.end, finish);
        assert!(span.text.contains("Morning observation"));
        assert!(span.text.matches("The tide keeps its own time.").count() >= 2);
        let serialized = serde_json::to_vec(&span).unwrap();
        let restored: TextSelection = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, span);

        let second_layout = Settings {
            width: 420,
            height: 360,
            font_size: 37.0,
            ..settings
        };
        for layout in [settings, second_layout] {
            let probe = Probe::open(
                &book,
                Engine::Parley,
                std::slice::from_ref(&font),
                layout,
                0,
            )
            .unwrap();
            let pages: Vec<_> = (0..probe.page_count())
                .map(|page| probe.selection_rects(page, &span))
                .collect();
            assert!(pages.iter().filter(|rects| !rects.is_empty()).count() >= 2);
            assert_eq!(
                probe.select_range(&span.start, &span.end).as_ref(),
                Some(&span)
            );
            let rectangle = probe.selection_rects(probe.page_for(&span.start).unwrap(), &span)[0];
            let x = rectangle.x as f32 + 5.0;
            let y = rectangle.y as f32 + rectangle.height as f32 / 2.0;
            let page = probe.page_for(&span.start).unwrap();
            assert_eq!(probe.word_at(page, x, y).unwrap().text, "The");
            assert_eq!(
                probe.text_position_at(page, x, y).unwrap().block,
                span.start.block
            );
        }

        let mut stale = span.clone();
        stale.start.revision ^= 1;
        assert!(probe.selection_rects(0, &stale).is_empty());
    }
}
