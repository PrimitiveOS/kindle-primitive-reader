//! Import Tesseract TSV for one upright grayscale page.
//! https://tesseract-ocr.github.io/tessdoc/Command-Line-Usage.html#tsv-output
//!
//! The Kindle worker streams a bounded upright raster to the installed engine. The reader
//! maps normalized TSV word rectangles through its current page transform for selection.

use std::borrow::Cow;
use std::fmt;

use crate::fixed_text::{PdfPageText, PdfTextRect, PdfTextSpan};

pub const MAX_TSV_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OCR_PIXELS: u32 = 4 * 1024 * 1024;
pub const MAX_OCR_SIDE: u32 = 4096;
const MAX_ROWS: usize = 32_768;
const MAX_WORDS: usize = 8192;
const MAX_WORD_BYTES: usize = 512;
const MAX_TEXT_BYTES: usize = 96 * 1024;
const HEADER: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

pub(crate) fn valid_language_name(name: &str) -> bool {
    name.len() <= 32
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Tesseract accepts ordered language combinations separated by `+`. Keep the
/// set small so recognition and private model staging stay bounded on Kindle.
pub(crate) fn valid_language_spec(spec: &str) -> bool {
    let mut languages = spec.split('+');
    let Some(first) = languages.next() else {
        return false;
    };
    if !valid_language_name(first) {
        return false;
    }
    match (languages.next(), languages.next()) {
        (None, None) => true,
        (Some(second), None) => second != first && valid_language_name(second),
        _ => false,
    }
}

#[derive(Debug, PartialEq)]
pub struct OcrPage {
    pub text: PdfPageText,
    /// Tesseract's 0–100 score for each word in `text.words`.
    pub word_confidences: Vec<f32>,
    /// Malformed rows omitted from the result.
    pub skipped_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrError {
    InvalidRaster,
    OutputTooLarge,
    InvalidTsv,
    MultiplePages,
    RasterMismatch,
}

impl fmt::Display for OcrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRaster => "The OCR input image exceeds Reader's size limits.",
            Self::OutputTooLarge => "The OCR results exceed Reader's text input limit.",
            Self::InvalidTsv => "The OCR engine returned invalid text output.",
            Self::MultiplePages => "The OCR results contain more than one page.",
            Self::RasterMismatch => {
                "The OCR results have different dimensions from the input image."
            }
        })
    }
}

impl std::error::Error for OcrError {}

/// Convert word rows to UTF-8 text and top-left-origin unit rectangles. The
/// supplied dimensions are those of the exact raster fed to the engine.
pub fn parse_tsv(bytes: &[u8], width: u32, height: u32) -> Result<OcrPage, OcrError> {
    let raster = Raster::new(width, height)?;
    if bytes.len() > MAX_TSV_BYTES {
        return Err(OcrError::OutputTooLarge);
    }
    let data = std::str::from_utf8(bytes).map_err(|_| OcrError::InvalidTsv)?;
    let mut rows = data.strip_prefix('\u{feff}').unwrap_or(data).lines();
    if rows.next() != Some(HEADER) {
        return Err(OcrError::InvalidTsv);
    }
    let mut result = Collector::default();
    let mut page_seen = false;
    for (index, row) in rows.enumerate() {
        if index == MAX_ROWS {
            result.page.truncated = true;
            break;
        }
        if row.is_empty() {
            continue;
        }
        let mut fields = row.splitn(12, '\t');
        let level = match fields.next() {
            Some("1") => Level::Page,
            Some("2" | "3" | "4") => continue,
            Some("5") => Level::Word,
            _ => {
                result.skipped_rows += 1;
                continue;
            }
        };
        let page = fields.next().and_then(|field| field.parse::<u32>().ok());
        match page {
            Some(1) => {}
            Some(page) if page > 1 => return Err(OcrError::MultiplePages),
            _ => {
                result.skipped_rows += 1;
                continue;
            }
        }
        match level {
            Level::Page => {
                if page_seen {
                    return Err(OcrError::MultiplePages);
                }
                let dimensions = fields.nth(6).and_then(|width| {
                    Some((
                        width.parse::<u32>().ok()?,
                        fields.next()?.parse::<u32>().ok()?,
                    ))
                });
                if let Some(dimensions) = dimensions {
                    if dimensions != (width, height) {
                        return Err(OcrError::RasterMismatch);
                    }
                    page_seen = true;
                } else {
                    result.skipped_rows += 1;
                }
            }
            Level::Word => match Word::parse(fields, raster) {
                Ok(Some(word)) => {
                    if !result.push(word, raster) {
                        break;
                    }
                }
                Ok(None) => {}
                Err(()) => result.skipped_rows += 1,
            },
        }
    }
    Ok(result.finish())
}

enum Level {
    Page,
    Word,
}

#[derive(Clone, Copy)]
struct Raster {
    width: u32,
    height: u32,
}

impl Raster {
    fn new(width: u32, height: u32) -> Result<Self, OcrError> {
        if width == 0
            || height == 0
            || width.max(height) > MAX_OCR_SIDE
            || u64::from(width) * u64::from(height) > u64::from(MAX_OCR_PIXELS)
        {
            return Err(OcrError::InvalidRaster);
        }
        Ok(Self { width, height })
    }

    fn bounds(self, left: i64, top: i64, width: i64, height: i64) -> Option<PdfTextRect> {
        let xmax = i64::from(self.width);
        let ymax = i64::from(self.height);
        let right = left.checked_add(width)?.clamp(0, xmax);
        let bottom = top.checked_add(height)?.clamp(0, ymax);
        let left = left.clamp(0, xmax);
        let top = top.clamp(0, ymax);
        (left < right && top < bottom).then(|| PdfTextRect {
            x0: left as f32 / self.width as f32,
            y0: top as f32 / self.height as f32,
            x1: right as f32 / self.width as f32,
            y1: bottom as f32 / self.height as f32,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct LineKey {
    block: u32,
    paragraph: u32,
    line: u32,
}

struct Word<'a> {
    key: LineKey,
    bounds: PdfTextRect,
    confidence: f32,
    text: Cow<'a, str>,
}

impl<'a> Word<'a> {
    fn parse(
        mut fields: impl Iterator<Item = &'a str>,
        raster: Raster,
    ) -> Result<Option<Self>, ()> {
        let [
            Some(block),
            Some(paragraph),
            Some(line),
            Some(word),
            Some(left),
            Some(top),
            Some(width),
            Some(height),
            Some(confidence),
            Some(text),
        ] = std::array::from_fn(|_| fields.next())
        else {
            return Err(());
        };
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        if text.len() > MAX_WORD_BYTES
            || text
                .chars()
                .any(|ch| ch.is_control() && !ch.is_whitespace())
        {
            return Err(());
        }
        let id = |field: &str| field.parse::<u32>().ok().filter(|&id| id > 0).ok_or(());
        let key = LineKey {
            block: id(block)?,
            paragraph: id(paragraph)?,
            line: id(line)?,
        };
        id(word)?;
        let coord = |field: &str| field.parse::<i64>().map_err(|_| ());
        let bounds = raster
            .bounds(coord(left)?, coord(top)?, coord(width)?, coord(height)?)
            .ok_or(())?;
        let confidence = confidence.parse::<f32>().map_err(|_| ())?;
        if !(0.0..=100.0).contains(&confidence) {
            return Err(());
        }
        let text = if text.chars().any(char::is_whitespace) {
            Cow::Owned(text.split_whitespace().collect::<Vec<_>>().join(" "))
        } else {
            Cow::Borrowed(text)
        };
        Ok(Some(Self {
            key,
            bounds,
            confidence,
            text,
        }))
    }
}

#[derive(Default)]
struct Collector {
    page: PdfPageText,
    line: Option<(LineKey, PdfTextSpan)>,
    confidences: Vec<f32>,
    skipped_rows: usize,
}

impl Collector {
    fn push(&mut self, word: Word<'_>, raster: Raster) -> bool {
        let Word {
            key,
            bounds,
            confidence,
            text,
        } = word;
        let separator = match &self.line {
            None => "",
            Some((previous, _)) if *previous == key => {
                let join = self.page.words.last().is_some_and(|previous| {
                    let Some(left) = self.page.text.chars().next_back() else {
                        return false;
                    };
                    let Some(right) = text.chars().next() else {
                        return false;
                    };
                    let gap = (bounds.x0 - previous.bounds.x1) * raster.width as f32;
                    let height = ((bounds.y1 - bounds.y0)
                        .min(previous.bounds.y1 - previous.bounds.y0))
                        * raster.height as f32;
                    unspaced_cjk(left) && unspaced_cjk(right) && gap >= -1.0 && gap <= height * 0.5
                });
                if join { "" } else { " " }
            }
            Some((previous, _))
                if previous.block == key.block && previous.paragraph == key.paragraph =>
            {
                "\n"
            }
            Some(_) => "\n\n",
        };
        let start = self.page.text.len() + separator.len();
        let end = start + text.len();
        if self.page.words.len() == MAX_WORDS || end > MAX_TEXT_BYTES {
            self.page.truncated = true;
            return false;
        }
        let word = PdfTextSpan {
            bytes: start..end,
            bounds,
        };
        self.page.text.push_str(separator);
        self.page.text.push_str(&text);
        if let Some((previous, line)) = &mut self.line
            && *previous == key
        {
            line.bytes.end = end;
            line.bounds = union(line.bounds, bounds);
        } else {
            self.end_line();
            self.line = Some((key, word.clone()));
        }
        self.page.words.push(word);
        self.confidences.push(confidence);
        true
    }

    fn end_line(&mut self) {
        if let Some((_, line)) = self.line.take() {
            self.page.lines.push(line);
        }
    }

    fn finish(mut self) -> OcrPage {
        self.end_line();
        OcrPage {
            text: self.page,
            word_confidences: self.confidences,
            skipped_rows: self.skipped_rows,
        }
    }
}

/// Han and Japanese syllabaries normally form unspaced phrases. Korean is
/// intentionally excluded because spaces carry word boundaries in Hangul.
fn unspaced_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{3001}'..='\u{303f}' | '\u{3040}'..='\u{30ff}' |
        '\u{31f0}'..='\u{31ff}' | '\u{3400}'..='\u{4dbf}' |
        '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' |
        '\u{ff66}'..='\u{ff9f}' | '\u{20000}'..='\u{323af}')
}

fn union(a: PdfTextRect, b: PdfTextRect) -> PdfTextRect {
    PdfTextRect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tsv(rows: &[&str]) -> String {
        [HEADER, &rows.join("\n")].join("\n")
    }

    #[test]
    fn preserves_word_order_and_utf8_ranges_across_lines_and_blocks() {
        let input = tsv(&[
            "1\t1\t0\t0\t0\t0\t0\t0\t400\t200\t-1\t",
            "2\t1\t2\t0\t0\t0\t250\t28\t100\t155\t-1\t",
            "4\t1\t2\t1\t1\t0\t270\t40\t90\t20\t-1\t",
            "5\t1\t2\t1\t1\t1\t270\t40\t40\t20\t98.5\tCafé",
            "5\t1\t2\t1\t1\t2\t315\t42\t45\t16\t82.25\t波",
            "5\t1\t2\t1\t2\t1\t270\t63\t60\t20\t100\tمرحبا",
            "5\t1\t1\t1\t1\t1\t15\t10\t70\t20\t0\tTide",
            "5\t1\t1\t2\t1\t1\t15\t45\t60\t22\t90\tNotes",
        ]);
        let input = format!("\u{feff}{}\r\n", input.replace('\n', "\r\n"));
        let parsed = parse_tsv(input.as_bytes(), 400, 200).unwrap();
        assert_eq!(parsed.text.text, "Café 波\nمرحبا\n\nTide\n\nNotes");
        assert_eq!(parsed.word_confidences, [98.5, 82.25, 100.0, 0.0, 90.0]);
        assert_eq!(parsed.skipped_rows, 0);
        assert_eq!(parsed.text.words[0].bytes, 0..5);
        assert_eq!(parsed.text.words[1].bytes, 6..9);
        assert_eq!(
            &parsed.text.text[parsed.text.words[2].bytes.clone()],
            "مرحبا"
        );
        assert_eq!(
            parsed
                .text
                .lines
                .iter()
                .map(|span| &parsed.text.text[span.bytes.clone()])
                .collect::<Vec<_>>(),
            ["Café 波", "مرحبا", "Tide", "Notes"]
        );
        assert_eq!(
            parsed.text.lines[0].bounds,
            PdfTextRect {
                x0: 0.675,
                y0: 0.2,
                x1: 0.9,
                y1: 0.3
            }
        );
    }

    #[test]
    fn clips_words_and_skips_malformed_rows_without_shifting_valid_spans() {
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\t-5\t195\t20\t10\t97\tShore",
            "4\tpartial metadata rows are ignored",
            "5\t1\t1\t1\t1\t2\t370\t10\t0\t10\t90\tZero width",
            "5\t1\t1\t1\t1\t3\t400\t10\t5\t10\t90\tOffscreen",
            "5\t1\t1\t1\t1\t4\t0\t9\t20\t10\tNaN\tInvalid confidence",
            "5\t1\t1\t1\t1\t5\t0\t9\t20\t10\t101\tOutside confidence range",
            "5\t1\t1\t1\t1\t6\t0\t9\t20\t10\t-1\tInvalid word confidence",
            "5\t1\t1\t1\t1\t7\t9223372036854775807\t9\t20\t10\t70\tOverflow",
            "5\t1\tshort row",
            "5\t1\t1\t1\t1\t8\t0\t8\t20\t10\t84\t  ",
            "5\t1\t1\t1\t2\t1\t8\t17\t36\t10\t89\ttwo\t words",
        ]);
        let parsed = parse_tsv(input.as_bytes(), 400, 200).unwrap();
        assert_eq!(parsed.text.text, "Shore\ntwo words");
        assert_eq!(parsed.skipped_rows, 7);
        assert_eq!(parsed.word_confidences, [97.0, 89.0]);
        assert_eq!(
            parsed.text.words[0].bounds,
            PdfTextRect {
                x0: 0.0,
                y0: 0.975,
                x1: 0.0375,
                y1: 1.0
            }
        );
        assert_eq!(parsed.text.words[1].bytes, 6..15);
    }

    #[test]
    fn rejects_unrelated_output_multiple_pages_and_wrong_raster() {
        assert_eq!(
            parse_tsv(b"unexpected output", 400, 200),
            Err(OcrError::InvalidTsv)
        );
        assert_eq!(parse_tsv(b"\x80", 400, 200), Err(OcrError::InvalidTsv));
        assert_eq!(
            parse_tsv(HEADER.as_bytes(), 4097, 1),
            Err(OcrError::InvalidRaster)
        );
        assert_eq!(
            parse_tsv(HEADER.as_bytes(), 4096, 2048),
            Err(OcrError::InvalidRaster)
        );
        assert_eq!(
            parse_tsv(HEADER.as_bytes(), 0, 200),
            Err(OcrError::InvalidRaster)
        );
        let wrong_size = tsv(&["1\t1\t0\t0\t0\t0\t0\t0\t401\t200\t-1\t"]);
        assert_eq!(
            parse_tsv(wrong_size.as_bytes(), 400, 200),
            Err(OcrError::RasterMismatch)
        );
        let two_pages = tsv(&[
            "1\t1\t0\t0\t0\t0\t0\t0\t400\t200\t-1\t",
            "5\t2\t1\t1\t1\t1\t0\t0\t50\t20\t90\tOther page",
        ]);
        assert_eq!(
            parse_tsv(two_pages.as_bytes(), 400, 200),
            Err(OcrError::MultiplePages)
        );
    }

    #[test]
    fn reports_valid_empty_pages_and_truncates_on_complete_word_boundaries() {
        let blank = tsv(&["1\t1\t0\t0\t0\t0\t0\t0\t10\t10\t-1\t"]);
        let blank = parse_tsv(blank.as_bytes(), 10, 10).unwrap();
        assert_eq!(blank.text, PdfPageText::default());

        let oversized = vec![b'a'; MAX_TSV_BYTES + 1];
        assert_eq!(parse_tsv(&oversized, 10, 10), Err(OcrError::OutputTooLarge));
        let word = "波".repeat(160);
        let row = format!("5\t1\t1\t1\t1\t1\t0\t0\t5\t5\t90\t{word}");
        let input = tsv(&vec![row.as_str(); 256]);
        let page = parse_tsv(input.as_bytes(), 10, 10).unwrap();
        assert!(page.text.truncated);
        assert!(page.text.text.len() <= MAX_TEXT_BYTES);
        assert!(page.text.text.ends_with(&word));
        assert_eq!(
            page.text.words.last().unwrap().bytes.end,
            page.text.text.len()
        );
        assert_eq!(page.word_confidences.len(), page.text.words.len());
    }
}
