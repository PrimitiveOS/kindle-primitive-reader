//! Optional content diagnostics. Reports counts and zero-based indices, never document text or paths.
use crate::{Engine, Probe, Renderer, Result, Settings, epub::Book, fixed_epub::FixedEpub};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Default, Serialize)]
pub(super) struct Audit {
    chapters: usize,
    checked: usize,
    failed_indices: Vec<usize>,
    genuine_reflow_sections: usize,
    empty_body_indices: Vec<usize>,
    empty_layout_with_markup_indices: Vec<usize>,
    recovered_placeholder_indices: Vec<usize>,
    fixed_pages: usize,
    pages_in_checked_sections: u64,
    painted_pages: u64,
    painted_pages_with_ink: u64,
    source_chars: u64,
    mapped_chars: u64,
    missing_glyphs: u64,
    source_graphics: u64,
    decoded_graphics: u64,
    placeholder_graphics: u64,
    source_fragments: u64,
    located_fragments: u64,
    inspected_page_anchors: u64,
    exact_page_anchors: u64,
    issues: Vec<SectionIssues>,
}

#[derive(Default, PartialEq, Serialize)]
struct SectionIssues {
    index: usize,
    unmapped_chars: usize,
    missing_glyphs: usize,
    undecoded_graphics: usize,
    unresolved_fragments: usize,
    blank_painted_pages: usize,
    ink_pages_without_anchor: usize,
    page_anchor_discrepancies: usize,
    blank_page_examples: Vec<usize>,
    anchor_page_examples: Vec<(usize, Option<usize>)>,
}

fn indices(n: usize, all: bool) -> Vec<usize> {
    if all {
        return (0..n).collect();
    }
    if n == 0 {
        return Vec::new();
    }
    let mut chosen = vec![0, n / 2, n - 1];
    chosen.dedup();
    chosen
}

impl Audit {
    fn reflow(
        &mut self,
        chapter: &mut Probe,
        index: usize,
        recovered: bool,
        literal_empty_body: bool,
        all: bool,
    ) -> Result<()> {
        let mut issue = SectionIssues {
            index,
            ..Default::default()
        };
        let empty = !recovered && chapter.source.blocks.is_empty();
        let genuine = !recovered && !empty;
        if recovered {
            self.recovered_placeholder_indices.push(index);
        } else if empty {
            if literal_empty_body {
                self.empty_body_indices.push(index);
            } else {
                self.empty_layout_with_markup_indices.push(index);
            }
        } else {
            self.genuine_reflow_sections += 1;
        }
        if genuine {
            let (mapped, total) = chapter.coverage();
            self.source_chars += total as u64;
            self.mapped_chars += mapped as u64;
            issue.unmapped_chars = total - mapped;
            issue.missing_glyphs = chapter.missing_glyphs();
            self.missing_glyphs += issue.missing_glyphs as u64;
            let graphics: usize = chapter.source.blocks.iter().map(|b| b.images.len()).sum();
            let (decoded, placeholders) = match &chapter.renderer {
                Renderer::Parley(layout) => layout.illustration_counts(),
                _ => (0, 0), // Vertical layout is only selected when there are no images.
            };
            self.source_graphics += graphics as u64;
            self.decoded_graphics += decoded as u64;
            self.placeholder_graphics += placeholders as u64;
            issue.undecoded_graphics = graphics.saturating_sub(decoded);
            self.source_fragments += chapter.source.fragments.len() as u64;
            let resolved = chapter
                .source
                .fragments
                .keys()
                .filter(|id| {
                    chapter
                        .fragment(id)
                        .and_then(|a| chapter.page_for(&a))
                        .is_some()
                })
                .count();
            self.located_fragments += resolved as u64;
            issue.unresolved_fragments = chapter.source.fragments.len() - resolved;
        }
        let pages = chapter.page_count();
        self.pages_in_checked_sections += pages as u64;
        for page in indices(pages, all) {
            let pixels = chapter.capture(page)?;
            self.painted_pages += 1;
            let ink = pixels.iter().any(|&v| v < 255);
            self.painted_pages_with_ink += u64::from(ink);
            if genuine {
                issue.blank_painted_pages += usize::from(!ink);
                if !ink && issue.blank_page_examples.len() < 8 {
                    issue.blank_page_examples.push(page);
                }
                match chapter.page_anchor(page) {
                    Some(anchor) => {
                        self.inspected_page_anchors += 1;
                        let mapped = chapter.page_for(&anchor);
                        let same = mapped == Some(page);
                        if !same && issue.anchor_page_examples.len() < 8 {
                            issue.anchor_page_examples.push((page, mapped));
                        }

                        self.exact_page_anchors += u64::from(same);
                        issue.page_anchor_discrepancies += usize::from(!same);
                    }
                    None => issue.ink_pages_without_anchor += usize::from(ink),
                }
            }
        }
        if issue
            != (SectionIssues {
                index,
                ..Default::default()
            })
        {
            self.issues.push(issue);
        }
        Ok(())
    }
}

// Check the supplied chapter rather than treating an all-white frame as proof of a blank leaf.
fn literal_empty_body(markup: &str) -> bool {
    let xml = crate::loose::without_plain_html_doctype(markup);
    roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .ok()
    .and_then(|document| {
        document
            .root_element()
            .children()
            .find(|node| node.has_tag_name("body"))
            .map(|body| {
                body.attributes().len() == 0
                    && body.children().all(|child| {
                        !child.is_element()
                            && (!child.is_text()
                                || child.text().unwrap_or_default().trim().is_empty())
                    })
            })
    })
    .unwrap_or(false)
}

pub(super) fn verify_spine(path: &Path, fonts: &[PathBuf], all: bool) -> Result<Audit> {
    let package = Book::open(path)?;
    let count = package.chapters.len();
    let is_fixed = package.is_fixed_layout();
    let placeholders: Vec<_> = package
        .chapters
        .iter()
        .map(|entry| package.recovered_placeholder(entry))
        .collect();
    drop(package);
    let fixed = if is_fixed {
        FixedEpub::open(path, fonts)?
    } else {
        None
    };
    let settings = Settings {
        width: primitive_app::WIDTH,
        height: crate::session::PAGE_HEIGHT,
        ..Default::default()
    };
    let mut audit = Audit {
        chapters: count,
        ..Default::default()
    };
    for index in indices(count, all) {
        audit.checked += 1;
        let result = (|| -> Result<()> {
            if let Some(book) = &fixed {
                let pixels = book
                    .decode_page(index)?
                    .fit(settings.width, settings.height, 0)?;
                audit.fixed_pages += 1;
                audit.pages_in_checked_sections += 1;
                audit.painted_pages += 1;
                audit.painted_pages_with_ink += u64::from(pixels.iter().any(|&v| v < 255));
            } else {
                let mut chapter = Probe::open(path, Engine::Parley, fonts, settings, index)?;
                let literal_empty = chapter.source.blocks.is_empty()
                    && !placeholders[index]
                    && literal_empty_body(&Book::open(path)?.chapter(index)?);
                audit.reflow(&mut chapter, index, placeholders[index], literal_empty, all)?;
            }
            Ok(())
        })();
        if result.is_err() {
            audit.failed_indices.push(index);
        }
    }
    Ok(audit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_white_leaf_requires_an_empty_source_body() {
        assert!(literal_empty_body(
            "<html><head/><body>  <!-- printer leaf --> </body></html>"
        ));
        for markup in [
            "<html><body><img src='missing.png'/></body></html>",
            "<html><body><p> </p></body></html>",
            "<html><body style='background: black'/></html>",
            "<html><body>Chapter unavailable</body></html>",
        ] {
            assert!(!literal_empty_body(markup));
        }
    }

    #[test]
    fn a_recovery_notice_does_not_inflate_source_text_or_anchor_coverage() {
        let fixture = crate::test_fixture("field-notes-epub2.epub");
        let fonts = [crate::test_fixture("fonts/NotoSans-Regular.ttf")];
        let package = Book::open(&fixture).unwrap();
        let mut bytes = std::fs::read(&fixture).unwrap();
        let lost = {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
            let entry = zip.by_name(&package.chapters[0].href).unwrap();
            entry.header_start() as usize..(entry.data_start() + entry.compressed_size()) as usize
        };
        bytes.drain(lost);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file =
            std::env::temp_dir().join(format!("reader-spine-{}-{nonce}.epub", std::process::id()));
        std::fs::write(&file, &bytes).unwrap();
        let result = verify_spine(&file, &fonts, true);
        std::fs::remove_file(&file).unwrap();
        let audit = result.unwrap();
        assert!(audit.failed_indices.is_empty());
        assert_eq!(audit.recovered_placeholder_indices, [0]);
        assert_eq!(audit.genuine_reflow_sections, 1);
        let surviving =
            Probe::open(&fixture, Engine::Parley, &fonts, Settings::default(), 1).unwrap();
        let (covered, total) = surviving.coverage();
        assert_eq!(
            (audit.mapped_chars, audit.source_chars),
            (covered as u64, total as u64)
        );
        assert_eq!(audit.painted_pages, audit.pages_in_checked_sections);
        assert!(audit.painted_pages_with_ink > 0);
    }
}
