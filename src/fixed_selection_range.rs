//! Reconstruct a saved annotation in document reading order using upright word geometry.
use super::{FixedReading, PdfSelection, Result};
use crate::fixed_text::{PdfPageText, PdfTextRect};
use crate::library::{
    HighlightPage, MAX_HIGHLIGHT_TEXT_BYTES, MAX_PDF_HIGHLIGHT_BOUNDS, PdfHighlight,
};

fn endpoint(text: &PdfPageText, regions: &[PdfTextRect], last: bool) -> Option<usize> {
    let mut indices = text.words.iter().enumerate().filter_map(|(index, word)| {
        let x = (word.bounds.x0 + word.bounds.x1) / 2.0;
        let y = (word.bounds.y0 + word.bounds.y1) / 2.0;
        regions
            .iter()
            .any(|rect| rect.x0 <= x && x <= rect.x1 && rect.y0 <= y && y <= rect.y1)
            .then_some(index)
    });
    if last {
        indices.next_back()
    } else {
        indices.next()
    }
}

pub(super) fn line_regions(
    document: &PdfPageText,
    first: usize,
    last: usize,
) -> Result<Vec<PdfTextRect>> {
    let words = &document.words[first..=last];
    if words.len() <= MAX_PDF_HIGHLIGHT_BOUNDS {
        return Ok(words.iter().map(|word| word.bounds).collect());
    }
    let mut regions: Vec<PdfTextRect> = Vec::new();
    let mut lines = document.lines.iter().peekable();
    let mut previous = None;
    for word in words {
        while lines
            .peek()
            .is_some_and(|line| line.bytes.end <= word.bytes.start)
        {
            lines.next();
        }
        let current = lines
            .peek()
            .filter(|line| line.bytes.start <= word.bytes.start && word.bytes.end <= line.bytes.end)
            .map(|line| line.bytes.start);
        if current.is_some()
            && current == previous
            && let Some(rect) = regions.last_mut()
        {
            rect.x0 = rect.x0.min(word.bounds.x0);
            rect.x1 = rect.x1.max(word.bounds.x1);
            rect.y0 = rect.y0.min(word.bounds.y0);
            rect.y1 = rect.y1.max(word.bounds.y1);
        } else {
            if regions.len() == MAX_PDF_HIGHLIGHT_BOUNDS {
                return Err("This page exceeds the highlight geometry limit".into());
            }
            regions.push(word.bounds);
        }
        previous = current;
    }
    Ok(regions)
}

impl FixedReading {
    pub(crate) fn extend_saved_mark(
        &self,
        saved: &PdfHighlight,
        word: &PdfSelection,
    ) -> Result<PdfHighlight> {
        let (selected_first, selected_last) = word.endpoints();
        let read = |page| -> Result<PdfPageText> {
            if page == selected_first.page {
                return Ok(selected_first.document.clone());
            }
            if page == selected_last.page {
                return Ok(selected_last.document.clone());
            }
            if self.view.force_ocr {
                return self
                    .cached_ocr_for(page, self.ocr_language.as_deref())
                    .ok_or_else(|| format!("Run OCR on page {} first", page + 1).into());
            }
            self.page_text(page)
        };
        let (last_page, last_bounds, _) =
            saved.parts().last().ok_or("The selection is unavailable")?;
        if saved
            .page
            .min(selected_first.page)
            .abs_diff(last_page.max(selected_last.page))
            >= PdfHighlight::MAX_PAGES
        {
            return Err("This range exceeds the saved highlight page limit".into());
        }
        let first_source = read(saved.page)?;
        let first_word = endpoint(&first_source, &saved.bounds, false)
            .ok_or("The start of the saved selection no longer matches this text layer")?;
        let last_source;
        let last_text = if last_page == saved.page {
            &first_source
        } else {
            last_source = read(last_page)?;
            &last_source
        };
        let last_word = endpoint(last_text, last_bounds, true)
            .ok_or("The end of the saved selection no longer matches this text layer")?;
        let first = (saved.page, first_word).min((selected_first.page, selected_first.word));
        let last = (last_page, last_word).max((selected_last.page, selected_last.word));
        if first == (saved.page, first_word) && last == (last_page, last_word) {
            return Ok(saved.clone());
        }
        let mut result = PdfHighlight {
            chapter_title: None,
            color: saved.color,
            created_at: saved.created_at,
            style: saved.style,
            page: first.0,
            bounds: Vec::new(),
            text: String::new(),
            note: saved.note.clone(),
            continuation: Vec::new(),
        };
        for page in first.0..=last.0 {
            let loaded;
            let source = if page == saved.page {
                &first_source
            } else if page == last_page {
                last_text
            } else if page == selected_first.page {
                &selected_first.document
            } else if page == selected_last.page {
                &selected_last.document
            } else {
                loaded = read(page)?;
                &loaded
            };
            if source.words.is_empty() {
                return Err(format!(
                    "Page {} has no selectable words. Run OCR on that page first",
                    page + 1
                )
                .into());
            }
            if source.truncated && page != last.0 {
                return Err("The text layer on an intervening page is incomplete".into());
            }
            let from = if page == first.0 { first.1 } else { 0 };
            let to = if page == last.0 {
                last.1
            } else {
                source.words.len() - 1
            };
            let quote = source
                .text
                .get(source.words[from].bytes.start..source.words[to].bytes.end)
                .ok_or("The word offsets are unavailable")?;
            let separator = usize::from(page != first.0);
            if result.text.len() + separator + quote.len() > MAX_HIGHLIGHT_TEXT_BYTES {
                return Err("This selection exceeds the saved highlight size limit".into());
            }
            let bounds = line_regions(source, from, to)?;
            if page == first.0 {
                result.bounds = bounds;
            } else {
                result.text.push('\n');
                result.continuation.push(HighlightPage {
                    page,
                    bounds,
                    text_offset: result.text.len(),
                });
            }
            result.text.push_str(quote);
        }
        if !result.is_valid() {
            return Err("The resulting highlight is unavailable".into());
        }
        Ok(result)
    }
}
