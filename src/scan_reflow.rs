//! Rewrap a scan's original word images using bounded OCR geometry.

use std::fmt;
use std::ops::Range;

use primitive_app::{HEIGHT, WIDTH};
use primitive_ui::GrayImage;
use unicode_bidi::{BidiClass, bidi_class};

use crate::fixed_text::{PdfPageText, PdfTextRect};
use crate::library::FixedReflowSpacing;
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
    Page,
}

impl fmt::Display for ScanReflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Raster => "The scan raster is invalid or too large",
            Self::Viewport => "The scan reading view has unsupported dimensions",
            Self::NoWords => "This scan has no recognized word boxes",
            Self::OcrGeometry => "The scan needs complete OCR word positions",
            Self::Direction => "Scan reflow supports horizontal left-to-right words",
            Self::Limit => "This scan exceeds Reader's reflow limits",
            Self::Page => "That page is outside this scan view",
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
    pages: Vec<Range<usize>>,
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
    width: u32,
    height: u32,
}

struct Placement {
    crop: Crop,
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
        text_height: u32,
        spacing: FixedReflowSpacing,
    ) -> Result<Self> {
        let pixels = raster.width.checked_mul(raster.height);
        if raster.width.min(raster.height) == 0
            || raster.width.max(raster.height) > MAX_OCR_SIDE
            || pixels
                .is_none_or(|count| count > MAX_OCR_PIXELS || count as usize != raster.pixels.len())
        {
            return Err(ScanReflowError::Raster);
        }
        let viewport = Viewport::new(width, height, text_height)?;
        let mut words = word_crops(text, raster.width, raster.height)?;
        viewport.size_words(&mut words);
        let (placements, pages) = viewport.wrap(&words, spacing)?;
        Ok(Self {
            raster,
            width,
            height,
            placements,
            pages,
        })
    }

    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn page_first_word(&self, page: usize) -> Option<usize> {
        self.pages.get(page).map(|words| words.start)
    }

    pub(crate) fn page_for_word(&self, index: usize) -> Option<usize> {
        (index < self.placements.len())
            .then(|| self.pages.partition_point(|words| words.end <= index))
    }

    pub(crate) fn render(&self, page: usize) -> Result<GrayImage> {
        let range = self.pages.get(page).ok_or(ScanReflowError::Page)?;
        let mut frame = GrayImage {
            width: self.width,
            height: self.height,
            pixels: vec![255; (self.width * self.height) as usize],
        };
        for word in &self.placements[range.clone()] {
            blit_word(&self.raster, &mut frame, word);
        }
        Ok(frame)
    }
}

fn word_crops(text: &PdfPageText, width: u32, height: u32) -> Result<Vec<Word>> {
    if text.words.is_empty() {
        return Err(ScanReflowError::NoWords);
    }
    if text.words.len() > MAX_WORDS {
        return Err(ScanReflowError::Limit);
    }
    if text.truncated || !text.valid_ocr_cache() {
        return Err(ScanReflowError::OcrGeometry);
    }
    if text.text.chars().any(unsupported_direction) {
        return Err(ScanReflowError::Direction);
    }

    let mut words = Vec::with_capacity(text.words.len());
    let mut previous: Option<Crop> = None;
    let mut previous_end = 0;
    let mut tall_words = 0;
    for word in &text.words {
        let separator = &text.text[previous_end..word.bytes.start];
        if !separator.chars().all(char::is_whitespace)
            || text.text[word.bytes.clone()].trim().is_empty()
        {
            return Err(ScanReflowError::OcrGeometry);
        }
        let crop = Crop::from_bounds(word.bounds, width, height)?;
        if let Some(last) = previous
            && !separator.contains('\n')
            && !last.followed_by(crop)
        {
            return Err(ScanReflowError::Direction);
        }
        tall_words += usize::from(crop.height > 3 * crop.width);
        words.push(Word {
            crop,
            paragraph: separator
                .bytes()
                .filter(|&byte| byte == b'\n')
                .take(2)
                .count()
                == 2,
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
    Ok(words)
}

fn unsupported_direction(ch: char) -> bool {
    use BidiClass::{AL, AN, FSI, LRE, LRI, LRO, PDF, PDI, R, RLE, RLI, RLO};
    matches!(
        bidi_class(ch),
        AL | AN | R | RLE | RLO | RLI | LRE | LRO | LRI | FSI | PDF | PDI
    )
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

    fn followed_by(self, next: Self) -> bool {
        let gap = self
            .top
            .max(next.top)
            .saturating_sub((self.top + self.height).min(next.top + next.height));
        next.left + 3 >= self.left && gap <= self.height.min(next.height).max(6) / 2
    }
}

impl Viewport {
    fn new(width: u32, height: u32, text_height: u32) -> Result<Self> {
        if width.min(height) < MIN_VIEW_SIDE
            || width.max(height) > MAX_VIEW_SIDE
            || width
                .checked_mul(height)
                .is_none_or(|area| area > MAX_VIEW_PIXELS)
        {
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

    fn size_words(self, words: &mut [Word]) {
        let mut heights: Vec<_> = words.iter().map(|word| word.crop.height).collect();
        let middle = heights.len() / 2;
        let (_, median, _) = heights.select_nth_unstable(middle);
        let scale = (f64::from(self.text_height) / f64::from(*median)).min(8.0);
        let max_height = self.height.min(self.text_height * 3);
        for word in words {
            let width = f64::from(word.crop.width);
            let height = f64::from(word.crop.height);
            let chosen = scale
                .min(f64::from(self.width) / width)
                .min(f64::from(max_height) / height);
            word.width = (width * chosen).round().max(1.0) as u32;
            word.height = (height * chosen).round().max(1.0) as u32;
        }
    }

    fn wrap(
        self,
        words: &[Word],
        spacing: FixedReflowSpacing,
    ) -> Result<(Vec<Placement>, Vec<Range<usize>>)> {
        let mut placements = Vec::with_capacity(words.len());
        let mut pages = Vec::new();
        let mut page_start = 0;
        let mut line_start = 0;
        let mut y = self.top;
        let word_space = (self.text_height / 5).max(2);
        let line_space = (self.text_height / 3).max(2);
        let paragraph_space = (self.text_height / 2).max(4);
        while line_start < words.len() {
            let mut line_end = line_start;
            let mut line_width = 0;
            let mut line_height = 0;
            for word in &words[line_start..] {
                if line_end != line_start
                    && (word.paragraph || line_width + word.width > self.width)
                {
                    break;
                }
                line_end += 1;
                line_width += word.width + word_space;
                line_height = line_height.max(word.height);
            }
            let paragraph = if words[line_start].paragraph && y != self.top {
                paragraph_space
            } else {
                0
            };
            if y + paragraph + line_height > self.top + self.height {
                pages.push(page_start..placements.len());
                if pages.len() == MAX_PAGES {
                    return Err(ScanReflowError::Limit);
                }
                page_start = placements.len();
                y = self.top;
            } else {
                y += paragraph;
            }
            let mut x = self.left;
            for word in &words[line_start..line_end] {
                placements.push(Placement {
                    crop: word.crop,
                    x,
                    y: y + line_height - word.height,
                    width: word.width,
                    height: word.height,
                });
                x += word.width + word_space;
            }
            y += spacing
                .scaled_pitch(line_height + line_space)
                .max(line_height);
            line_start = line_end;
        }
        pages.push(page_start..placements.len());
        Ok((placements, pages))
    }
}

fn blit_word(source: &GrayImage, frame: &mut GrayImage, word: &Placement) {
    let x_step = (word.crop.width << SAMPLE_BITS) / word.width;
    let y_step = (word.crop.height << SAMPLE_BITS) / word.height;
    let mut source_y = y_step / 2;
    for y in word.y..word.y + word.height {
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

        fn finish(self, width: u32, height: u32, text_height: u32) -> Result<ScanReflow> {
            ScanReflow::new(
                self.raster,
                &self.text,
                width,
                height,
                text_height,
                FixedReflowSpacing::Medium,
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

    fn pixel(frame: &GrayImage, x: u32, y: u32) -> u8 {
        frame.pixels[(y * frame.width + x) as usize]
    }

    #[test]
    fn wraps_original_gray_pixels_in_ocr_order_across_lines_and_viewport_pages() {
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
        let reflow = fixture.finish(64, 48, 10).unwrap();
        assert_eq!(reflow.page_count(), 2);
        assert_eq!(reflow.page_first_word(0), Some(0));
        assert_eq!(reflow.page_first_word(1), Some(6));
        assert_eq!(reflow.page_for_word(5), Some(0));
        assert_eq!(reflow.page_for_word(6), Some(1));
        assert_eq!(reflow.page_for_word(8), None);
        let first = reflow.render(0).unwrap();
        assert_eq!((first.width, first.height), (64, 48));
        assert_eq!(pixel(&first, 4, 4), 32);
        assert_eq!(pixel(&first, 9, 6), 201);
        assert_eq!(pixel(&first, 30, 4), 48);
        assert_eq!(pixel(&first, 28, 4), 255);
        assert_eq!(pixel(&first, 4, 17), 64);
        assert_eq!(pixel(&first, 30, 17), 80);
        assert_eq!(pixel(&first, 4, 30), 96);
        assert_eq!(pixel(&first, 30, 30), 112);

        let second = reflow.render(1).unwrap();
        assert_eq!(pixel(&second, 4, 4), 128);
        assert_eq!(pixel(&second, 30, 4), 144);
        assert!(matches!(reflow.render(2), Err(ScanReflowError::Page)));
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
            (FixedReflowSpacing::Small, 44),
            (FixedReflowSpacing::Medium, 53),
            (FixedReflowSpacing::Large, 62),
        ] {
            let reflow =
                ScanReflow::new(fixture.raster.clone(), &fixture.text, 230, 260, 40, spacing)
                    .unwrap();
            assert_eq!(reflow.placements[2].y - reflow.placements[0].y, expected);
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
            let frame = reflow.render(0).unwrap();
            for (index, word) in reflow.placements.iter().enumerate() {
                assert_eq!(pixel(&frame, word.x, word.y), 40 + index as u8 * 20);
            }
        }
    }

    #[test]
    fn keeps_paragraph_breaks_and_fits_an_oversize_word_without_clipping() {
        let mut fixture = Fixture::new();
        fixture.word("", "First", crop(4, 4), 30);
        fixture.word("\n\n", "Second", crop(4, 20), 110);
        let reflow = fixture.finish(100, 60, 10).unwrap();
        let frame = reflow.render(0).unwrap();
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
        let reflow = fixture.finish(64, 48, 24).unwrap();
        let frame = reflow.render(0).unwrap();
        assert_eq!(pixel(&frame, 4, 4), 50);
        assert_eq!(pixel(&frame, 59, 4), 50);
        assert_eq!(pixel(&frame, 60, 4), 255);
    }

    #[test]
    fn rejects_rtl_characters_and_reverse_or_vertical_intraline_geometry() {
        let mut rtl = Fixture::new();
        rtl.word("", "مرحبا", crop(50, 4), 30);
        assert!(matches!(
            rtl.finish(64, 48, 10),
            Err(ScanReflowError::Direction)
        ));

        let mut backwards = Fixture::new();
        backwards.word("", "A", crop(60, 4), 30);
        backwards.word(" ", "B", crop(30, 4), 60);
        assert!(matches!(
            backwards.finish(64, 48, 10),
            Err(ScanReflowError::Direction)
        ));

        let mut vertical = Fixture::new();
        vertical.word("", "A", crop(24, 4), 30);
        vertical.word(" ", "B", crop(24, 24), 60);
        assert!(matches!(
            vertical.finish(64, 48, 10),
            Err(ScanReflowError::Direction)
        ));
    }

    #[test]
    fn rejects_bad_raster_viewport_and_incomplete_ocr() {
        let mut mismatch = Fixture::new();
        mismatch.raster.pixels.pop();
        assert!(matches!(
            mismatch.finish(64, 48, 10),
            Err(ScanReflowError::Raster)
        ));
        let oversized = Fixture::new();
        assert!(matches!(
            oversized.finish(u32::MAX, u32::MAX, 10),
            Err(ScanReflowError::Viewport)
        ));

        let mut truncated = Fixture::new();
        truncated.word("", "A", crop(24, 4), 30);
        truncated.text.truncated = true;
        assert!(matches!(
            truncated.finish(64, 48, 10),
            Err(ScanReflowError::OcrGeometry)
        ));
        let mut malformed = Fixture::new();
        malformed.word("", "A", crop(24, 4), 30);
        malformed.text.words[0].bounds.x1 = f32::NAN;
        assert!(matches!(
            malformed.finish(64, 48, 10),
            Err(ScanReflowError::OcrGeometry)
        ));
    }
}
