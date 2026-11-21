//! Compare every physical page with reference page geometry and text availability.
//! Output contains page numbers and counts. Document text stays in memory.
use reader::fixed::{MAX_PDF_BYTES, Pdf};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Deserialize)]
struct ReferencePage {
    width: f32,
    height: f32,
    has_readable_text: bool,
}

#[derive(Default, Serialize)]
struct Summary {
    pages: usize,
    reference_text_pages: usize,
    reader_text_pages: usize,
    readable_words: usize,
    intrinsic_links: usize,
    bounded_text_pages: Vec<usize>,
    different_geometry: Vec<usize>,
    reference_only_text: Vec<usize>,
    reader_only_text: Vec<usize>,
    invalid_word_spans: Vec<usize>,
}

fn main() -> reader::Result<()> {
    let mut args = env::args_os().skip(1).map(PathBuf::from);
    let usage = "Usage: verify_pdf_pages <PDF> <reference page JSON>";
    let input = args.next().ok_or(usage)?;
    let index = args.next().ok_or(usage)?;
    if args.next().is_some() || fs::metadata(&input)?.len() > MAX_PDF_BYTES as u64 {
        return Err(usage.into());
    }
    let expected: Vec<ReferencePage> = serde_json::from_slice(&fs::read(index)?)?;
    let pdf = Pdf::open(fs::read(input)?)?;
    if pdf.page_count() != expected.len() {
        return Err("Physical page count differs from the reference".into());
    }
    let mut result = Summary {
        pages: pdf.page_count(),
        ..Summary::default()
    };
    for (index, reference) in expected.iter().enumerate() {
        let physical = index + 1;
        let (width, height) = pdf.page_size(index)?;
        if (width - reference.width).abs() > 0.02 || (height - reference.height).abs() > 0.02 {
            result.different_geometry.push(physical);
        }
        let native = pdf.page_text(index)?;
        let has_text = native.text.chars().any(char::is_alphanumeric);
        result.reference_text_pages += usize::from(reference.has_readable_text);
        result.reader_text_pages += usize::from(has_text);
        if reference.has_readable_text && !has_text {
            result.reference_only_text.push(physical);
        }
        if has_text && !reference.has_readable_text {
            result.reader_only_text.push(physical);
        }
        if native.truncated {
            result.bounded_text_pages.push(physical);
        }
        result.readable_words += native.words.len();
        result.intrinsic_links += pdf.page_links(index)?.len();
        if native.words.iter().any(|span| {
            let rect = span.bounds;
            native
                .text
                .get(span.bytes.clone())
                .is_none_or(str::is_empty)
                || !(0.0 <= rect.x0
                    && rect.x0 <= rect.x1
                    && rect.x1 <= 1.0
                    && 0.0 <= rect.y0
                    && rect.y0 <= rect.y1
                    && rect.y1 <= 1.0)
        }) {
            result.invalid_word_spans.push(physical);
        }
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
