//! Rewrap a scan's original word images using bounded OCR geometry.

use std::cmp::Reverse;
use std::fmt;
use std::ops::Range;

use primitive_app::{HEIGHT, WIDTH};
use primitive_ui::{GrayImage, Rect};
use unicode_bidi::{BidiClass, bidi_class};

use crate::fixed_text::{PdfPageText, PdfTextRect};
use crate::library::{
    FixedReflowAlignment, FixedReflowColumns, FixedReflowDirection, FixedReflowSpacing,
    FixedReflowWordGap, MAX_SCAN_REFLOW_TEXT_SCALE, MIN_SCAN_REFLOW_TEXT_SCALE,
    SCAN_REFLOW_TEXT_SCALE_STEP,
};
use crate::ocr::{MAX_OCR_PIXELS, MAX_OCR_SIDE};

const MAX_WORDS: usize = 8192;
const MAX_PAGES: usize = 1024;
const MIN_VIEW_SIDE: u32 = 48;
const MAX_VIEW_SIDE: u32 = if WIDTH > HEIGHT { WIDTH } else { HEIGHT };
const MAX_VIEW_PIXELS: u32 = WIDTH * HEIGHT;
const SAMPLE_BITS: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScanReflowError {
    Raster,
    Viewport,
    NoWords,
    OcrGeometry,
    Direction,
    Limit,
}

impl fmt::Display for ScanReflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Raster => "The scan raster is invalid or too large",
            Self::Viewport => "The scan reading view has unsupported dimensions",
            Self::NoWords => "This scan has no recognized word boxes",
            Self::OcrGeometry => "The scan needs complete OCR word positions",
            Self::Direction => {
                "Scan reflow needs OCR words ordered in the selected writing direction"
            }
            Self::Limit => "This scan exceeds Reader's reflow limits",
        })
    }
}

impl std::error::Error for ScanReflowError {}

type Result<T> = std::result::Result<T, ScanReflowError>;

pub(crate) struct ScanReflow {
    raster: GrayImage,
    width: u32,
    height: u32,
    placements: Vec<Placement>,
    vertical: bool,
    strip_clip: Rect,
    max_word_height: u32,
}

#[derive(Clone, Copy)]
struct Crop {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

struct Word {
    crop: Crop,
    paragraph: bool,
    source_index: usize,
    line: usize,
    line_top: u32,
    line_left: u32,
    column: usize,
    source_gap_after: u32,
    gap_after: u32,
    width: u32,
    height: u32,
}

struct Placement {
    crop: Crop,
    source_index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy)]
struct Viewport {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    text_height: u32,
}

impl ScanReflow {
    /// The OCR bounds must describe the supplied upright, uncropped raster.
    pub(crate) fn new(
        raster: GrayImage,
        text: &PdfPageText,
        width: u32,
        height: u32,
        text_scale: u16,
        spacing: FixedReflowSpacing,
        max_columns: FixedReflowColumns,
        word_gap: FixedReflowWordGap,
        alignment: FixedReflowAlignment,
        direction: FixedReflowDirection,
    ) -> Result<Self> {
        // Report an unusable caller viewport before inspecting OCR content.
        if !view_size_supported(width, height) {
            return Err(ScanReflowError::Viewport);
        }
        let pixels = raster.width.checked_mul(raster.height);
        if raster.width.min(raster.height) == 0
            || raster.width.max(raster.height) > MAX_OCR_SIDE
            || pixels
                .is_none_or(|count| count > MAX_OCR_PIXELS || count as usize != raster.pixels.len())
        {
            return Err(ScanReflowError::Raster);
        }
        if !(MIN_SCAN_REFLOW_TEXT_SCALE..=MAX_SCAN_REFLOW_TEXT_SCALE).contains(&text_scale)
            || (text_scale - MIN_SCAN_REFLOW_TEXT_SCALE) % SCAN_REFLOW_TEXT_SCALE_STEP != 0
        {
            return Err(ScanReflowError::Viewport);
        }
        let (mut words, direction) = word_crops(
            text,
            raster.width,
            raster.height,
            &raster.pixels,
            max_columns,
            direction,
        )?;
        let vertical = direction == Direction::TopToBottomRightToLeft;
        let mut source_sizes: Vec<_> = words
            .iter()
            .map(|word| {
                if vertical {
                    word.crop.width
                } else {
                    word.crop.height
                }
            })
            .collect();
        let middle = source_sizes.len() / 2;
        let (_, median_size, _) = source_sizes.select_nth_unstable(middle);
        let left_margin = (width / 30).clamp(4, 36);
        let text_width = width.saturating_sub(2 * left_margin);
        let target_height = (f64::from(*median_size)
            * f64::from(text_width)
            * f64::from(text_scale)
            / f64::from(raster.width)
            / 100.0)
            .round();
        let viewport = Viewport::new(width, height, target_height.clamp(6.0, 128.0) as u32)?;
        viewport.size_words(&mut words, word_gap, vertical);
        let alignment = if vertical {
            FixedReflowAlignment::Auto
        } else if alignment == FixedReflowAlignment::Auto {
            infer_alignment(&text.lines, direction)
        } else {
            alignment
        };
        let placements = viewport.wrap(&words, spacing, direction, alignment)?;
        let max_word_height = placements.iter().map(|word| word.height).max().unwrap_or(0);
        Ok(Self {
            raster,
            width,
            height,
            placements,
            vertical,
            max_word_height,
            strip_clip: Rect::new(0, viewport.top as i32, width as i32, viewport.height as i32),
        })
    }

    pub(crate) fn scroll_clip(&self) -> Rect {
        self.strip_clip
    }

    pub(crate) fn scroll_end(&self) -> u32 {
        let end = if self.vertical {
            self.placements
                .iter()
                .map(|placement| placement.y + placement.height)
                .max()
                .unwrap_or(0)
        } else {
            self.placements
                .last()
                .map_or(0, |last| last.y + last.height)
        };
        end.saturating_sub(self.strip_clip.h as u32)
    }

    pub(crate) fn word_at_scroll_top(&self, top: u32) -> Option<usize> {
        self.scroll_words(top)
            .next()
            .map(|index| self.placements[index].source_index)
    }

    pub(crate) fn word_scroll_top(&self, word: usize) -> Option<u32> {
        self.placements
            .iter()
            .find(|placement| placement.source_index == word)
            .map(|placement| placement.y)
    }

    fn scroll_words(&self, top: u32) -> Box<dyn Iterator<Item = usize> + '_> {
        // Line baselines increase, while word tops within a line can differ.
        let last_baseline = top + self.strip_clip.h as u32 + self.max_word_height;
        if self.vertical {
            Box::new(self.placements.iter().enumerate().filter_map(
                move |(index, word)| {
                    (word.y + word.height > top && word.y < last_baseline).then_some(index)
                },
            ))
        } else {
            let start = self
                .placements
                .partition_point(|word| word.y + word.height <= top);
            Box::new((start..self.placements.len()).take_while(move |&index| {
                let word = &self.placements[index];
                word.y + word.height < last_baseline
            }))
        }
    }

    pub(crate) fn render_scroll(&self, top: u32) -> GrayImage {
        let mut frame = GrayImage {
            width: self.width,
            height: self.height,
            pixels: vec![255; (self.width * self.height) as usize],
        };
        let clip = self.strip_clip.y as u32..self.strip_clip.bottom() as u32;
        for index in self.scroll_words(top) {
            let word = &self.placements[index];
            let y = i64::from(word.y) - i64::from(top) + i64::from(self.strip_clip.y);
            blit_word(&self.raster, &mut frame, word, y, clip.clone());
        }
        frame
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    LeftToRight,
    RightToLeft,
    TopToBottomRightToLeft,
}

fn word_crops(
    text: &PdfPageText,
    width: u32,
    height: u32,
    pixels: &[u8],
    max_columns: FixedReflowColumns,
    preferred_direction: FixedReflowDirection,
) -> Result<(Vec<Word>, Direction)> {
    if text.words.is_empty() {
        return Err(ScanReflowError::NoWords);
    }
    if text.words.len() > MAX_WORDS {
        return Err(ScanReflowError::Limit);
    }
    if text.truncated || !text.valid_ocr_cache() {
        return Err(ScanReflowError::OcrGeometry);
    }
    let direction = text_direction(&text.text, preferred_direction)?;
    let vertical = direction == Direction::TopToBottomRightToLeft;
    let columns = crate::scan_columns::detect_reading_columns_with_limit(
        pixels,
        width,
        height,
        max_columns.count(),
    );
    let is_multicolumn = columns.as_ref().is_some_and(|columns| columns.len() > 1);

    let mut words = Vec::with_capacity(text.words.len());
    let mut previous: Option<Crop> = None;
    let mut previous_end = 0;
    let mut fallback_line = 0;
    let mut tall_words = 0;
    for (source_index, word) in text.words.iter().enumerate() {
        let separator = &text.text[previous_end..word.bytes.start];
        if !separator.chars().all(char::is_whitespace)
            || text.text[word.bytes.clone()].trim().is_empty()
        {
            return Err(ScanReflowError::OcrGeometry);
        }
        let crop = Crop::from_bounds(word.bounds, width, height)?;
        if !vertical
            && !is_multicolumn
            && let Some(last) = previous
            && !separator.contains('\n')
            && !last.followed_by(crop, direction)
        {
            return Err(ScanReflowError::Direction);
        }
        let line_index = text
            .lines
            .partition_point(|line| line.bytes.end <= word.bytes.start);
        let (line, line_top, line_left) = text
            .lines
            .get(line_index)
            .filter(|line| line.bytes.start <= word.bytes.start && line.bytes.end >= word.bytes.end)
            .map_or_else(
                || {
                    let line = fallback_line;
                    fallback_line += separator.bytes().filter(|&byte| byte == b'\n').count();
                    (line, crop.top, crop.left)
                },
                |line| {
                    let top = (line.bounds.y0 * height as f32).round() as u32;
                    let left = (line.bounds.x0 * width as f32).round() as u32;
                    (line_index, top, left)
                },
            );
        let column = columns
            .as_ref()
            .map(|columns| column_for(crop, width, columns))
            .unwrap_or(0);
        tall_words += usize::from(if vertical {
            crop.width > 3 * crop.height
        } else {
            crop.height > 3 * crop.width
        });
        words.push(Word {
            crop,
            paragraph: separator
                .bytes()
                .filter(|&byte| byte == b'\n')
                .take(2)
                .count()
                == 2,
            source_index,
            line,
            line_top,
            line_left,
            column,
            source_gap_after: 0,
            gap_after: 0,
            width: 0,
            height: 0,
        });
        previous = Some(crop);
        previous_end = word.bytes.end;
    }
    if !text.text[previous_end..].trim().is_empty() {
        return Err(ScanReflowError::OcrGeometry);
    }
    if tall_words >= 4 && tall_words * 5 > words.len() * 4 {
        return Err(ScanReflowError::Direction);
    }
    if vertical {
        assign_vertical_lines(&mut words);
    }
    if vertical || is_multicolumn {
        let column_count = columns.as_ref().map_or(1, Vec::len);
        words.sort_by(|left, right| {
            let page_column_order = reading_column(left.column, column_count, direction)
                .cmp(&reading_column(right.column, column_count, direction));
            if vertical {
                page_column_order
                    .then_with(|| right.line_left.cmp(&left.line_left))
                    .then_with(|| left.crop.top.cmp(&right.crop.top))
                    .then_with(|| left.crop.left.cmp(&right.crop.left))
            } else {
                page_column_order
                    .then_with(|| left.line_top.cmp(&right.line_top))
                    .then_with(|| left.line.cmp(&right.line))
                    .then_with(|| match direction {
                        Direction::LeftToRight => left.crop.left.cmp(&right.crop.left),
                        Direction::RightToLeft => right.crop.left.cmp(&left.crop.left),
                        Direction::TopToBottomRightToLeft => left.crop.top.cmp(&right.crop.top),
                    })
            }
        });
    }
    for index in 0..words.len() {
        let Some(next) = words.get(index + 1) else {
            continue;
        };
        let word = &words[index];
        if word.line != next.line || word.column != next.column {
            continue;
        }
        let source_gap_after = match direction {
            Direction::LeftToRight => next
                .crop
                .left
                .saturating_sub(word.crop.left.saturating_add(word.crop.width)),
            Direction::RightToLeft => word
                .crop
                .left
                .saturating_sub(next.crop.left.saturating_add(next.crop.width)),
            Direction::TopToBottomRightToLeft => next
                .crop
                .top
                .saturating_sub(word.crop.top.saturating_add(word.crop.height)),
        };
        words[index].source_gap_after = source_gap_after;
    }
    Ok((words, direction))
}

fn assign_vertical_lines(words: &mut [Word]) {
    let mut order: Vec<_> = (0..words.len()).collect();
    order.sort_by_key(|&index| Reverse(words[index].crop.left + words[index].crop.width / 2));
    let mut clusters: Vec<(u32, u32)> = Vec::new();
    for index in order {
        let crop = words[index].crop;
        let center = crop.left + crop.width / 2;
        let matching_line = clusters.len().checked_sub(1).filter(|&line| {
            center.abs_diff(clusters[line].0) <= ((crop.width + clusters[line].1) / 4).max(3)
        });
        let line = if let Some(line) = matching_line {
            let (cluster_center, cluster_width) = &mut clusters[line];
            *cluster_center = (*cluster_center + center) / 2;
            *cluster_width = (*cluster_width).max(crop.width);
            line
        } else {
            clusters.push((center, crop.width));
            clusters.len() - 1
        };
        words[index].line = line;
        words[index].line_left = clusters[line].0;
    }
}

fn column_for(crop: Crop, width: u32, columns: &[PdfTextRect]) -> usize {
    let center = crop.left + crop.width / 2;
    columns
        .iter()
        .position(|column| {
            let left = (column.x0 * width as f32).round() as u32;
            let right = (column.x1 * width as f32).round() as u32;
            center >= left && center <= right
        })
        .unwrap_or_else(|| {
            columns
                .iter()
                .enumerate()
                .min_by_key(|(_, column)| {
                    let center_x = ((column.x0 + column.x1) * 0.5 * width as f32).round() as u32;
                    center.abs_diff(center_x)
                })
                .map_or(0, |(index, _)| index)
        })
}

fn reading_column(index: usize, count: usize, direction: Direction) -> usize {
    match direction {
        Direction::LeftToRight => index,
        Direction::RightToLeft | Direction::TopToBottomRightToLeft => {
            count.saturating_sub(index + 1)
        }
    }
}

fn text_direction(text: &str, preferred: FixedReflowDirection) -> Result<Direction> {
    use BidiClass::{FSI, LRE, LRI, LRO, PDF, PDI, RLE, RLI, RLO};
    let direction = match preferred {
        FixedReflowDirection::LeftToRight => Direction::LeftToRight,
        FixedReflowDirection::RightToLeft => Direction::RightToLeft,
        FixedReflowDirection::TopToBottomRightToLeft => Direction::TopToBottomRightToLeft,
    };
    for ch in text.chars() {
        let class = bidi_class(ch);
        if matches!(class, RLE | RLO | RLI | LRE | LRO | LRI | FSI | PDF | PDI) {
            // Raster crops cannot apply embedded Unicode bidi overrides safely.
            return Err(ScanReflowError::Direction);
        }
    }
    Ok(direction)
}

fn infer_alignment(
    lines: &[crate::fixed_text::PdfTextSpan],
    direction: Direction,
) -> FixedReflowAlignment {
    let fallback = match direction {
        Direction::LeftToRight => FixedReflowAlignment::Left,
        Direction::RightToLeft => FixedReflowAlignment::Right,
        Direction::TopToBottomRightToLeft => FixedReflowAlignment::Auto,
    };
    let geometry: Vec<_> = lines
        .iter()
        .map(|line| line.bounds)
        .filter(|bounds| bounds.x1 - bounds.x0 >= 0.45)
        .collect();
    if geometry.len() < 3 {
        return fallback;
    }
    let lefts: Vec<_> = geometry.iter().map(|bounds| bounds.x0).collect();
    let rights: Vec<_> = geometry.iter().map(|bounds| 1.0 - bounds.x1).collect();
    let widths: Vec<_> = geometry
        .iter()
        .map(|bounds| bounds.x1 - bounds.x0)
        .collect();
    let left_median = median_f32(lefts.clone());
    let right_median = median_f32(rights.clone());
    let width_median = median_f32(widths.clone());
    let center_offset = median_f32(
        lefts
            .iter()
            .zip(&rights)
            .map(|(left, right)| (left - right).abs())
            .collect(),
    );
    let left_spread = median_f32(lefts.iter().map(|left| (left - left_median).abs()).collect());
    let right_spread = median_f32(
        rights
            .iter()
            .map(|right| (right - right_median).abs())
            .collect(),
    );
    let width_spread = median_f32(
        widths
            .iter()
            .map(|width| (width - width_median).abs())
            .collect(),
    );
    let left_is_stable = left_spread <= 0.025;
    let right_is_stable = right_spread <= 0.025;
    if left_is_stable
        && right_is_stable
        && width_median >= 0.45
        && width_spread <= 0.025
    {
        FixedReflowAlignment::Justify
    } else if center_offset <= 0.035 {
        FixedReflowAlignment::Center
    } else if right_is_stable && !left_is_stable {
        FixedReflowAlignment::Right
    } else if left_is_stable {
        FixedReflowAlignment::Left
    } else {
        fallback
    }
}

fn median_f32(mut values: Vec<f32>) -> f32 {
    let middle = values.len() / 2;
    let (_, value, _) = values.select_nth_unstable_by(middle, f32::total_cmp);
    *value
}

impl Crop {
    fn from_bounds(bounds: PdfTextRect, width: u32, height: u32) -> Result<Self> {
        // Tesseract's unit coordinates originated on integer pixel boundaries.
        let project = |coordinate: f32, size| (coordinate * size as f32).round() as u32;
        let left = project(bounds.x0, width);
        let top = project(bounds.y0, height);
        let right = project(bounds.x1, width);
        let bottom = project(bounds.y1, height);
        if left >= right || top >= bottom {
            return Err(ScanReflowError::OcrGeometry);
        }
        Ok(Self {
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }

    fn followed_by(self, next: Self, direction: Direction) -> bool {
        let ordered = match direction {
            Direction::LeftToRight => next.left + 3 >= self.left,
            Direction::RightToLeft => next.left <= self.left + 3,
            Direction::TopToBottomRightToLeft => next.top + 3 >= self.top,
        };
        let gap = if direction == Direction::TopToBottomRightToLeft {
            self.left
                .max(next.left)
                .saturating_sub((self.left + self.width).min(next.left + next.width))
        } else {
            self.top
                .max(next.top)
                .saturating_sub((self.top + self.height).min(next.top + next.height))
        };
        let size = if direction == Direction::TopToBottomRightToLeft {
            self.width.min(next.width)
        } else {
            self.height.min(next.height)
        };
        ordered && gap <= size.max(6) / 2
    }
}

/// Whether a reading viewport is within the bounds this reflow supports.
fn view_size_supported(width: u32, height: u32) -> bool {
    width.min(height) >= MIN_VIEW_SIDE
        && width.max(height) <= MAX_VIEW_SIDE
        && width
            .checked_mul(height)
            .is_some_and(|area| area <= MAX_VIEW_PIXELS)
}

impl Viewport {
    fn new(width: u32, height: u32, text_height: u32) -> Result<Self> {
        if !view_size_supported(width, height) {
            return Err(ScanReflowError::Viewport);
        }
        let left = (width / 30).clamp(4, 36);
        let top = (height / 30).clamp(4, 32);
        let height = height - 2 * top;
        if !(6..=height.min(128)).contains(&text_height) {
            return Err(ScanReflowError::Viewport);
        }
        Ok(Self {
            left,
            top,
            width: width - 2 * left,
            height,
            text_height,
        })
    }

    fn size_words(self, words: &mut [Word], word_gap: FixedReflowWordGap, vertical: bool) {
        let mut primary_sizes: Vec<_> = words
            .iter()
            .map(|word| {
                if vertical {
                    word.crop.width
                } else {
                    word.crop.height
                }
            })
            .collect();
        let middle = primary_sizes.len() / 2;
        let (_, median, _) = primary_sizes.select_nth_unstable(middle);
        let median_size = *median;
        let scale = (f64::from(self.text_height) / f64::from(median_size)).min(8.0);
        let max_primary = if vertical {
            self.width.min(self.text_height * 3)
        } else {
            self.height.min(self.text_height * 3)
        };
        let max_cross = if vertical { self.height } else { self.width };
        let mut measured_gaps: Vec<_> = words
            .iter()
            .map(|word| word.source_gap_after)
            .filter(|gap| *gap > 0)
            .collect();
        let fallback_gap = if measured_gaps.is_empty() {
            median_size / 5
        } else {
            let middle = measured_gaps.len() / 2;
            let (_, gap, _) = measured_gaps.select_nth_unstable(middle);
            *gap
        };
        let max_gap = (self.text_height / 2).max(4);
        for word in words {
            let width = f64::from(word.crop.width);
            let height = f64::from(word.crop.height);
            let primary = if vertical { width } else { height };
            let cross = if vertical { height } else { width };
            let chosen = scale
                .min(f64::from(max_cross) / cross)
                .min(f64::from(max_primary) / primary);
            word.width = (width * chosen).round().max(1.0) as u32;
            word.height = (height * chosen).round().max(1.0) as u32;
            word.gap_after = if let Some(ratio) = word_gap.manual_ratio() {
                (self.text_height as f32 * ratio).round().max(2.0) as u32
            } else {
                let source_gap = if word.source_gap_after > 0 {
                    word.source_gap_after
                } else {
                    fallback_gap
                };
                (f64::from(source_gap) * scale)
                    .round()
                    .clamp(2.0, f64::from(max_gap)) as u32
            };
        }
    }

    fn wrap(
        self,
        words: &[Word],
        spacing: FixedReflowSpacing,
        direction: Direction,
        alignment: FixedReflowAlignment,
    ) -> Result<Vec<Placement>> {
        if direction == Direction::TopToBottomRightToLeft {
            return self.wrap_vertical(words, spacing);
        }
        let mut placements = Vec::with_capacity(words.len());
        let mut line_start = 0;
        let mut y = 0;
        let line_space = (self.text_height / 3).max(2);
        let paragraph_space = (self.text_height / 2).max(4);
        while line_start < words.len() {
            let mut line_end = line_start;
            let mut line_width = 0;
            let mut line_height = 0;
            for word in &words[line_start..] {
                let word_start = if line_end == line_start {
                    line_width
                } else {
                    line_width + words[line_end - 1].gap_after
                };
                if line_end != line_start
                    && (word.paragraph || word_start + word.width > self.width)
                {
                    break;
                }
                line_end += 1;
                line_width = word_start + word.width;
                line_height = line_height.max(word.height);
            }
            if line_start != 0 && words[line_start].paragraph {
                y += paragraph_space;
            }
            if y + line_height > self.height * MAX_PAGES as u32 {
                return Err(ScanReflowError::Limit);
            }
            let line_is_paragraph_end = line_end == words.len() || words[line_end].paragraph;
            let justify_slots = line_end.saturating_sub(line_start + 1);
            let justify_extra = if alignment == FixedReflowAlignment::Justify
                && !line_is_paragraph_end
                && justify_slots > 0
            {
                self.width.saturating_sub(line_width)
            } else {
                0
            };
            let extra_per_gap = if justify_slots == 0 {
                0
            } else {
                justify_extra / justify_slots as u32
            };
            let extra_remainder = if justify_slots == 0 {
                0
            } else {
                justify_extra % justify_slots as u32
            };
            let remaining = self.width.saturating_sub(line_width);
            let alignment_offset = match (alignment, direction) {
                (FixedReflowAlignment::Left, Direction::RightToLeft)
                | (FixedReflowAlignment::Right, Direction::LeftToRight) => remaining,
                (FixedReflowAlignment::Center, _) => remaining / 2,
                _ => 0,
            };
            let mut x = match direction {
                Direction::LeftToRight => self.left + alignment_offset,
                Direction::RightToLeft => self.left + self.width - alignment_offset,
                Direction::TopToBottomRightToLeft => self.left,
            };
            for (offset, word) in words[line_start..line_end].iter().enumerate() {
                let gap_after = if offset + 1 < line_end - line_start {
                    word.gap_after
                        + extra_per_gap
                        + if (offset as u32) < extra_remainder {
                            1
                        } else {
                            0
                        }
                } else {
                    0
                };
                if direction == Direction::RightToLeft {
                    x -= word.width;
                }
                placements.push(Placement {
                    crop: word.crop,
                    source_index: word.source_index,
                    x,
                    y: y + line_height - word.height,
                    width: word.width,
                    height: word.height,
                });
                match direction {
                    Direction::LeftToRight => x += word.width + gap_after,
                    Direction::RightToLeft => x = x.saturating_sub(gap_after),
                    Direction::TopToBottomRightToLeft => x += word.width + gap_after,
                }
            }
            let pitch = spacing
                .scaled_pitch(line_height + line_space)
                .max(line_height);
            y += pitch;
            line_start = line_end;
        }
        Ok(placements)
    }

    fn wrap_vertical(
        self,
        words: &[Word],
        spacing: FixedReflowSpacing,
    ) -> Result<Vec<Placement>> {
        let mut placements = Vec::with_capacity(words.len());
        let right_edge = self.left + self.width;
        let line_space = (self.text_height / 3).max(2);
        let paragraph_space = (self.text_height / 2).max(4);
        let mut page = 0_u32;
        let mut page_top = 0_u32;
        let mut column_right = right_edge;
        let mut has_column = false;
        let mut line_start = 0;

        while line_start < words.len() {
            let first = &words[line_start];
            let mut line_end = line_start + 1;
            while words.get(line_end).is_some_and(|next| {
                next.line == first.line && next.column == first.column
            }) {
                line_end += 1;
            }
            let mut segment_start = line_start;
            let mut first_segment = true;
            while segment_start < line_end {
                let mut segment_end = segment_start;
                let mut segment_height = 0_u32;
                for word in &words[segment_start..line_end] {
                    let next_height = if segment_end == segment_start {
                        word.height
                    } else {
                        segment_height
                            .saturating_add(words[segment_end - 1].gap_after)
                            .saturating_add(word.height)
                    };
                    if segment_end != segment_start && next_height > self.height {
                        break;
                    }
                    segment_height = next_height;
                    segment_end += 1;
                }
                if segment_end == segment_start {
                    return Err(ScanReflowError::Viewport);
                }
                let column_width = words[segment_start..segment_end]
                    .iter()
                    .map(|word| word.width)
                    .max()
                    .unwrap_or(0);
                if column_width == 0 || column_width > self.width {
                    return Err(ScanReflowError::Viewport);
                }
                let column_gap = spacing
                    .scaled_pitch(column_width + line_space)
                    .max(column_width)
                    .saturating_sub(column_width);
                let paragraph_extra = if first_segment && first.paragraph {
                    paragraph_space
                } else {
                    0
                };
                if has_column
                    && column_right.saturating_sub(column_width + paragraph_extra) < self.left
                {
                    page += 1;
                    if page >= MAX_PAGES as u32 {
                        return Err(ScanReflowError::Limit);
                    }
                    page_top = page.saturating_mul(self.height);
                    column_right = right_edge;
                    has_column = false;
                } else if has_column {
                    column_right = column_right.saturating_sub(paragraph_extra);
                }

                let column_left = column_right - column_width;
                let mut y = page_top;
                for word in &words[segment_start..segment_end] {
                    placements.push(Placement {
                        crop: word.crop,
                        source_index: word.source_index,
                        x: column_left + (column_width - word.width) / 2,
                        y,
                        width: word.width,
                        height: word.height,
                    });
                    y = y.saturating_add(word.height).saturating_add(word.gap_after);
                }
                column_right = column_left.saturating_sub(column_gap);
                has_column = true;
                segment_start = segment_end;
                first_segment = false;
            }
            line_start = line_end;
        }
        Ok(placements)
    }
}

fn blit_word(
    source: &GrayImage,
    frame: &mut GrayImage,
    word: &Placement,
    y: i64,
    clip: Range<u32>,
) {
    let first = y.max(i64::from(clip.start));
    let last = (y + i64::from(word.height)).min(i64::from(clip.end));
    if first >= last {
        return;
    }
    let x_step = (word.crop.width << SAMPLE_BITS) / word.width;
    let y_step = (word.crop.height << SAMPLE_BITS) / word.height;
    let mut source_y = y_step / 2 + y_step * (first - y) as u32;
    for y in first as u32..last as u32 {
        let source_start =
            (word.crop.top + (source_y >> SAMPLE_BITS)) * source.width + word.crop.left;
        let frame_start = y * frame.width + word.x;
        let row = &mut frame.pixels[frame_start as usize..(frame_start + word.width) as usize];
        let mut source_x = x_step / 2;
        for pixel in row {
            *pixel = source.pixels[(source_start + (source_x >> SAMPLE_BITS)) as usize];
            source_x += x_step;
        }
        source_y += y_step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed_text::PdfTextSpan;

    struct Fixture {
        raster: GrayImage,
        text: PdfPageText,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                raster: GrayImage {
                    width: 100,
                    height: 60,
                    pixels: vec![255; 100 * 60],
                },
                text: PdfPageText::default(),
            }
        }

        fn word(&mut self, separator: &str, word: &str, crop: Crop, tone: u8) {
            self.text.text.push_str(separator);
            let start = self.text.text.len();
            self.text.text.push_str(word);
            let width = self.raster.width as f32;
            let height = self.raster.height as f32;
            for y in crop.top..crop.top + crop.height {
                let offset = (y * self.raster.width + crop.left) as usize;
                self.raster.pixels[offset..offset + crop.width as usize].fill(tone);
            }
            self.text.words.push(PdfTextSpan {
                bytes: start..self.text.text.len(),
                bounds: PdfTextRect {
                    x0: crop.left as f32 / width,
                    y0: crop.top as f32 / height,
                    x1: (crop.left + crop.width) as f32 / width,
                    y1: (crop.top + crop.height) as f32 / height,
                },
            });
        }

        fn finish(self, width: u32, height: u32, text_scale: u16) -> Result<ScanReflow> {
            self.finish_direction(
                width,
                height,
                text_scale,
                FixedReflowDirection::LeftToRight,
            )
        }

        fn finish_direction(
            self,
            width: u32,
            height: u32,
            text_scale: u16,
            direction: FixedReflowDirection,
        ) -> Result<ScanReflow> {
            ScanReflow::new(
                self.raster,
                &self.text,
                width,
                height,
                text_scale,
                FixedReflowSpacing::Medium,
                FixedReflowColumns::Two,
                FixedReflowWordGap::Auto,
                FixedReflowAlignment::Auto,
                direction,
            )
        }
    }

    fn crop(left: u32, top: u32) -> Crop {
        Crop {
            left,
            top,
            width: 24,
            height: 10,
        }
    }

    fn vertical_crop(left: u32, top: u32) -> Crop {
        Crop {
            left,
            top,
            width: 8,
            height: 10,
        }
    }

    fn pixel(frame: &GrayImage, x: u32, y: u32) -> u8 {
        frame.pixels[(y * frame.width + x) as usize]
    }

    #[test]
    fn wraps_original_gray_pixels_in_ocr_order_across_overlapping_viewports() {
        let mut fixture = Fixture::new();
        for index in 0..8 {
            let separator = if index == 0 {
                ""
            } else if index % 3 == 0 {
                "\n"
            } else {
                " "
            };
            fixture.word(
                separator,
                "Mưa",
                crop(4 + index % 3 * 27, 4 + index / 3 * 16),
                32 + index as u8 * 16,
            );
        }
        fixture.raster.pixels[6 * 100 + 9] = 201;
        // 200% text scale makes the three-column, four-row grid taller than the 40 px text area.
        let reflow = fixture.finish(64, 48, 200).unwrap();
        assert_eq!(reflow.scroll_end(), 13);
        assert_eq!(reflow.word_at_scroll_top(0), Some(0));
        assert_eq!(reflow.word_scroll_top(6), Some(42));
        assert_eq!(reflow.word_scroll_top(8), None);
        let first = reflow.render_scroll(0);
        assert_eq!((first.width, first.height), (64, 48));
        assert_eq!(pixel(&first, 4, 4), 32);
        assert_eq!(pixel(&first, 9, 6), 201);
        assert_eq!(pixel(&first, 35, 6), 48);
        assert_eq!(pixel(&first, 31, 4), 255);
        assert_eq!(pixel(&first, 6, 20), 64);
        assert_eq!(pixel(&first, 35, 20), 80);
        assert_eq!(pixel(&first, 6, 34), 96);
        assert_eq!(pixel(&first, 35, 34), 112);

        let second = reflow.render_scroll(reflow.scroll_end());
        assert_eq!(pixel(&second, 6, 35), 128);
        assert_eq!(pixel(&second, 35, 35), 144);
        // Rows outside the clipped text area stay blank at the scroll limit.
        assert_eq!(pixel(&second, 4, 3), 255);
        assert_eq!(pixel(&second, 4, 4), 255);
    }

    #[test]
    fn fixed_reflow_spacing_changes_vertical_pitch_without_changing_word_sizes_or_order() {
        let mut fixture = Fixture::new();
        for index in 0..6 {
            let separator = match index {
                0 => "",
                3 => "\n",
                _ => " ",
            };
            fixture.word(
                separator,
                "Ink",
                crop(4 + index % 3 * 27, 4 + index / 3 * 15),
                40 + index as u8 * 20,
            );
        }
        let mut reference = None;
        for (spacing, expected) in [
            (FixedReflowSpacing::Small, 24),
            (FixedReflowSpacing::Medium, 29),
            (FixedReflowSpacing::Large, 34),
        ] {
            let reflow = ScanReflow::new(
                fixture.raster.clone(),
                &fixture.text,
                230,
                260,
                100,
                spacing,
                FixedReflowColumns::Two,
                FixedReflowWordGap::Auto,
                FixedReflowAlignment::Auto,
                FixedReflowDirection::LeftToRight,
            )
            .unwrap();
            // Word 3 starts the next line, so its distance from word 0 is the line pitch.
            assert_eq!(reflow.placements[3].y - reflow.placements[0].y, expected);
            let geometry = reflow
                .placements
                .iter()
                .map(|word| (word.x, word.width, word.height))
                .collect::<Vec<_>>();
            if let Some(reference) = &reference {
                assert_eq!(&geometry, reference);
            } else {
                reference = Some(geometry);
            }
            let frame = reflow.render_scroll(0);
            for (index, word) in reflow.placements.iter().enumerate() {
                assert_eq!(
                    pixel(&frame, word.x, word.y + reflow.strip_clip.y as u32),
                    40 + index as u8 * 20
                );
            }
        }
    }

    #[test]
    fn keeps_paragraph_breaks_and_fits_an_oversize_word_without_clipping() {
        let mut fixture = Fixture::new();
        fixture.word("", "First", crop(4, 4), 30);
        fixture.word("\n\n", "Second", crop(4, 20), 110);
        let reflow = fixture.finish(100, 60, 100).unwrap();
        let frame = reflow.render_scroll(0);
        assert_eq!(pixel(&frame, 4, 4), 30);
        assert_eq!(pixel(&frame, 30, 4), 255);
        assert_eq!(pixel(&frame, 4, 17), 255);
        assert_eq!(pixel(&frame, 4, 22), 110);

        let mut fixture = Fixture::new();
        fixture.word(
            "",
            "Longer",
            Crop {
                width: 96,
                ..crop(2, 4)
            },
            50,
        );
        let reflow = fixture.finish(64, 48, 200).unwrap();
        let frame = reflow.render_scroll(0);
        assert_eq!(pixel(&frame, 4, 4), 50);
        assert_eq!(pixel(&frame, 59, 4), 50);
        assert_eq!(pixel(&frame, 60, 4), 255);
    }

    #[test]
    fn selected_direction_controls_rtl_order_and_rejects_vertical_intraline_geometry() {
        let mut rtl = Fixture::new();
        rtl.word("", "مرحبا", crop(50, 4), 30);
        rtl.word(" ", "world", crop(20, 4), 60);
        let reflow = rtl
            .finish_direction(
                64,
                48,
                100,
                FixedReflowDirection::RightToLeft,
            )
            .unwrap();
        assert!(reflow.placements[0].x > reflow.placements[1].x);

        let mut backwards = Fixture::new();
        backwards.word("", "A", crop(60, 4), 30);
        backwards.word(" ", "B", crop(30, 4), 60);
        assert!(matches!(
            backwards.finish_direction(
                64,
                48,
                100,
                FixedReflowDirection::LeftToRight,
            ),
            Err(ScanReflowError::Direction)
        ));

        let mut vertical = Fixture::new();
        vertical.word("", "A", crop(24, 4), 30);
        vertical.word(" ", "B", crop(24, 24), 60);
        assert!(matches!(
            vertical.finish(64, 48, 100),
            Err(ScanReflowError::Direction)
        ));
    }

    #[test]
    fn vertical_scan_reflow_orders_columns_right_to_left_and_words_top_to_bottom() {
        let mut fixture = Fixture::new();
        // OCR extracted the left column first and the words bottom-to-top.
        fixture.word("", "C", vertical_crop(38, 24), 30);
        fixture.word(" ", "D", vertical_crop(38, 4), 50);
        fixture.word("\n", "A", vertical_crop(68, 24), 70);
        fixture.word(" ", "B", vertical_crop(68, 4), 90);

        let reflow = fixture
            .finish_direction(
                64,
                48,
                100,
                FixedReflowDirection::TopToBottomRightToLeft,
            )
            .unwrap();

        assert!(reflow.vertical);
        assert_eq!(
            reflow
                .placements
                .iter()
                .map(|word| word.source_index)
                .collect::<Vec<_>>(),
            vec![3, 2, 1, 0]
        );
        assert!(reflow.placements[0].x > reflow.placements[2].x);
        assert!(reflow.placements[0].y < reflow.placements[1].y);
        assert!(reflow.placements[2].y < reflow.placements[3].y);
        assert_eq!(reflow.word_at_scroll_top(0), Some(3));
        assert!(reflow.word_scroll_top(2).is_some());
    }

    #[test]
    fn vertical_scan_reflow_continues_an_overlong_column_in_the_next_column() {
        let mut fixture = Fixture::new();
        for index in 0..7 {
            fixture.word(
                if index == 0 { "" } else { " " },
                "字",
                vertical_crop(68, 2 + index * 7),
                40 + index as u8 * 20,
            );
        }

        let reflow = fixture
            .finish_direction(
                64,
                48,
                100,
                FixedReflowDirection::TopToBottomRightToLeft,
            )
            .unwrap();

        assert_eq!(
            reflow
                .placements
                .iter()
                .map(|word| word.source_index)
                .collect::<Vec<_>>(),
            (0..7).collect::<Vec<_>>()
        );
        assert!(reflow.placements[0].x > reflow.placements[4].x);
        assert_eq!(reflow.placements[0].y, reflow.placements[4].y);
        assert_eq!(reflow.scroll_end(), 0);
    }

    #[test]
    fn rejects_bad_raster_viewport_and_incomplete_ocr() {
        let mut mismatch = Fixture::new();
        mismatch.raster.pixels.pop();
        assert!(matches!(
            mismatch.finish(64, 48, 100),
            Err(ScanReflowError::Raster)
        ));
        let oversized = Fixture::new();
        assert!(matches!(
            oversized.finish(u32::MAX, u32::MAX, 100),
            Err(ScanReflowError::Viewport)
        ));

        let mut truncated = Fixture::new();
        truncated.word("", "A", crop(24, 4), 30);
        truncated.text.truncated = true;
        assert!(matches!(
            truncated.finish(64, 48, 100),
            Err(ScanReflowError::OcrGeometry)
        ));
        let mut malformed = Fixture::new();
        malformed.word("", "A", crop(24, 4), 30);
        malformed.text.words[0].bounds.x1 = f32::NAN;
        assert!(matches!(
            malformed.finish(64, 48, 100),
            Err(ScanReflowError::OcrGeometry)
        ));
    }
}
