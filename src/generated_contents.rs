use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result};

const MAX_CHAPTERS: usize = 512;
const MAX_ENTRIES: usize = 1024;
const MAX_CHAPTER_HEADINGS: usize = 64;
const MAX_LABEL_CHARS: usize = 96;
const MAX_ANCHOR_PATH_BYTES: usize = 4096;

#[derive(Clone)]
pub(crate) enum OutlineTarget {
    Chapter(usize),
    Heading(usize, Anchor),
}

pub(crate) struct OutlineEntry {
    pub(crate) title: String,
    pub(crate) depth: usize,
    pub(crate) target: OutlineTarget,
}

#[derive(Default)]
pub(crate) struct GeneratedContents {
    publication: Option<(u64, usize)>,
    next_chapter: usize,
    pub(crate) entries: Vec<OutlineEntry>,
    pub(crate) skipped: usize,
    pub(crate) truncated: bool,
}

impl GeneratedContents {
    /// Read at most one bounded XHTML spine item. Only anchors and labels survive the step.
    pub(crate) fn step(&mut self, book: &mut Book) -> Result<bool> {
        let publication = (book.revision(), book.chapters.len());
        if *self.publication.get_or_insert(publication) != publication {
            return Err("The book changed while building the outline".into());
        }
        let chapter = self.next_chapter;
        self.next_chapter += 1;
        if let Some(href) = book.chapters.get(chapter).map(|entry| entry.href.clone()) {
            match book
                .chapter(chapter)
                .and_then(|xhtml| Source::parse(&xhtml))
            {
                Ok(source) => self.add_chapter(chapter, publication.0, &href, &source),
                Err(_) => self.skipped += 1,
            }
        }
        let chapters_remain = self.next_chapter < publication.1;
        let reached_limit = self.next_chapter >= MAX_CHAPTERS || self.entries.len() >= MAX_ENTRIES;
        self.truncated |= reached_limit && chapters_remain;
        Ok(!chapters_remain || reached_limit)
    }

    fn add_chapter(&mut self, chapter: usize, book: u64, href: &str, source: &Source) {
        let available = (MAX_ENTRIES - self.entries.len()).min(MAX_CHAPTER_HEADINGS);
        let mut headings = source
            .blocks
            .iter()
            .filter(|_| href.len() <= MAX_ANCHOR_PATH_BYTES)
            .filter_map(|block| {
                let depth = match source.elements[block.element].tag.as_str() {
                    "h1" => 0,
                    "h2" => 1,
                    "h3" => 2,
                    "h4" => 3,
                    "h5" => 4,
                    "h6" => 5,
                    _ => return None,
                };
                let title = outline_label(&block.text);
                (!title.is_empty()).then_some((depth, title, block.key))
            });
        let previous = self.entries.len();
        self.entries.extend(
            headings
                .by_ref()
                .take(available)
                .map(|(depth, title, block)| OutlineEntry {
                    title,
                    depth,
                    target: OutlineTarget::Heading(
                        chapter,
                        Anchor {
                            book,
                            chapter: href.to_owned(),
                            revision: source.revision,
                            block,
                            offset: 0,
                        },
                    ),
                }),
        );
        self.truncated |= headings.next().is_some();
        if self.entries.len() == previous && available > 0 {
            self.entries.push(OutlineEntry {
                title: format!("Chapter {}", chapter + 1),
                depth: 0,
                target: OutlineTarget::Chapter(chapter),
            });
        }
    }
}

fn outline_label(text: &str) -> String {
    let mut label = String::new();
    let mut pending_space = false;
    for character in text
        .trim()
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .take(MAX_LABEL_CHARS)
    {
        if character.is_whitespace() {
            pending_space = !label.is_empty();
        } else {
            if pending_space {
                label.push(' ');
                pending_space = false;
            }
            label.push(character);
        }
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_outline_limits_each_chapter_and_preserves_heading_depth() {
        let headings = "<h2>Section <em>heading</em></h2>".repeat(MAX_CHAPTER_HEADINGS + 1);
        let source = Source::parse(&format!("<html><body>{headings}</body></html>")).unwrap();
        let mut outline = GeneratedContents::default();
        outline.add_chapter(3, 5, "chapter.xhtml", &source);
        assert_eq!(outline.entries.len(), MAX_CHAPTER_HEADINGS);
        assert!(outline.truncated);
        let first = &outline.entries[0];
        assert_eq!(first.title, "Section heading");
        assert_eq!(first.depth, 1);
        let OutlineTarget::Heading(3, anchor) = &first.target else {
            panic!("Expected a heading target")
        };
        assert_eq!(anchor.book, 5);
        assert_eq!(anchor.block, source.blocks[0].key);
        assert_eq!(anchor.revision, source.revision);
    }
}
