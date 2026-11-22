//! Reuse parsed fonts while extracting adjacent pages of the same open PDF.
use crate::fixed::{Pdf, PdfError};
use crate::fixed_text::PdfPageText;
use hayro_interpret::InterpreterCache;
use std::cell::RefCell;
use std::ops::Deref;

/// Limit retention independently of the book's length. The next read after a
/// jump starts with a new cache. An extraction error or a truncated text layer
/// also releases the retained resources.
const PAGE_WINDOW: usize = 16;

#[derive(Default)]
struct Window<'a> {
    cache: InterpreterCache<'a>,
    previous: Option<usize>,
    reads: usize,
}

type PageWindow<'a> = RefCell<Window<'a>>;

self_cell::self_cell!(
    pub(super) struct ReadingPdf {
        owner: Pdf,
        #[not_covariant]
        dependent: PageWindow,
    }
);

impl From<Pdf> for ReadingPdf {
    fn from(pdf: Pdf) -> Self {
        Self::new(pdf, |_| PageWindow::default())
    }
}

impl Deref for ReadingPdf {
    type Target = Pdf;

    fn deref(&self) -> &Pdf {
        self.borrow_owner()
    }
}

impl ReadingPdf {
    pub(super) fn page_text(&self, page: usize) -> Result<PdfPageText, PdfError> {
        self.with_dependent(|pdf, state| {
            let mut window = state.borrow_mut();
            // The complete-book acceptance test runs the same executable twice to
            // compare timings with and without resource reuse on the original PDF.
            #[cfg(test)]
            if std::env::var_os("READER_TEST_UNCACHED_PDF_TEXT").is_some() {
                return pdf.page_text(page);
            }
            if window.reads >= PAGE_WINDOW
                || window.previous.is_some_and(|prev| prev.abs_diff(page) > 1)
            {
                *window = Window::default();
            }
            let result = pdf.page_text_with_cache(page, &window.cache);
            if result.as_ref().is_ok_and(|text| !text.truncated) {
                window.previous = Some(page);
                window.reads += 1;
            } else {
                *window = Window::default();
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(character: char) -> Pdf {
        let pages = PAGE_WINDOW + 2;
        let children = (0..pages)
            .map(|i| format!("{} 0 R", 6 + i))
            .collect::<Vec<_>>()
            .join(" ");
        let mapping = format!(
            "1 begincodespacerange <00> <FF> endcodespacerange\n1 beginbfchar <41> <{:04X}> endbfchar",
            character as u32
        );
        let stream =
            |text: &str| format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len());
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!(
                "<< /Type /Pages /Count {pages} /Kids [{children}] /Resources << /Font << /F1 3 0 R >> >> >>"
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 4 0 R >>".to_owned(),
            stream(&mapping),
            stream("BT /F1 14 Tf 10 90 Td (A) Tj ET"),
        ];
        for _ in 0..pages {
            objects.push(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 180] /Contents 5 0 R >>".into(),
            );
        }
        Pdf::open(crate::fixed::tests::pdf_objects(&objects)).unwrap()
    }

    #[test]
    fn cache_follows_its_pdf_and_resets_at_the_window_or_after_a_jump() {
        let left = ReadingPdf::from(document('L'));
        let right = ReadingPdf::from(document('R'));
        let reads = |pdf: &ReadingPdf| pdf.with_dependent(|_, window| window.borrow().reads);
        for page in 0..=PAGE_WINDOW {
            for (pdf, expected) in [(&left, "L"), (&right, "R")] {
                let layer = pdf.page_text(page).unwrap();
                assert_eq!(layer.text, expected);
                assert_eq!(layer, pdf.borrow_owner().page_text(page).unwrap());
                assert_eq!(reads(pdf), page % PAGE_WINDOW + 1);
            }
        }
        left.page_text(1).unwrap();
        assert_eq!(reads(&left), 1);
        assert_eq!(left.page_text(100), Err(PdfError::MissingPage));
        assert_eq!(reads(&left), 0);
        assert_eq!(right.page_text(1).unwrap().text, "R");
    }
}
