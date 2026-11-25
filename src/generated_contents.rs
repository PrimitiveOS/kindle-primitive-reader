use crate::epub::Book;
use crate::source::Source;
use crate::{Anchor, Result};

const MAX_CHAPTERS: usize = 512;
const MAX_ENTRIES: usize = 1024;
pub(crate) const MAX_CHAPTER_HEADINGS: usize = 64;
const MAX_LABEL_CHARS: usize = 96;
const MAX_ANCHOR_PATH_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OutlineTarget {
    Chapter(usize),
    Heading(usize, Anchor),
}

pub(crate) struct OutlineEntry {
    pub(crate) title: String,
    pub(crate) depth: usize,
    pub(crate) target: OutlineTarget,
}

impl OutlineEntry {
    pub(crate) fn display_title(&self) -> &str {
        if self.title.is_empty() {
            "–"
        } else {
            &self.title
        }
    }
}

#[derive(Default)]
pub(crate) struct GeneratedContents {
    publication: Option<(u64, usize)>,
    next_chapter: usize,
    alternative: bool,
    fragments: Vec<usize>,
    pub(crate) entries: Vec<OutlineEntry>,
    pub(crate) skipped: usize,
    pub(crate) truncated: bool,
}

impl GeneratedContents {
    pub(crate) fn alternative() -> Self {
        Self {
            alternative: true,
            ..Self::default()
        }
    }

    /// Read at most one bounded XHTML spine item. Only anchors and labels survive the step.
    pub(crate) fn step(&mut self, book: &mut Book) -> Result<bool> {
        let publication = (book.revision(), book.chapters.len());
        if *self.publication.get_or_insert(publication) != publication {
            return Err("The book changed while building the outline".into());
        }
        let chapter = self.next_chapter;
        self.next_chapter += 1;
        if let Some(href) = book.chapters.get(chapter).map(|entry| entry.href.clone()) {
            match book.chapter(chapter).and_then(|xhtml| {
                if self.alternative {
                    book.chapter_source(&href, &xhtml)
                } else {
                    Source::parse(&xhtml)
                }
            }) {
                Ok(source) => self.add_chapter(chapter, publication.0, &href, &source),
                Err(_) => self.skipped += 1,
            }
        }
        let chapters_remain = self.next_chapter < publication.1;
        let reached_limit = self.next_chapter >= MAX_CHAPTERS || self.entries.len() >= MAX_ENTRIES;
        self.truncated |= reached_limit && chapters_remain;
        let done = !chapters_remain || reached_limit;
        if done && self.alternative {
            if self.entries.is_empty() {
                self.entries
                    .extend(self.fragments.drain(..).map(|chapter| OutlineEntry {
                        title: String::new(),
                        depth: 0,
                        target: OutlineTarget::Chapter(chapter),
                    }));
            }
            self.fragments.clear();
        }
        Ok(done)
    }

    fn add_chapter(&mut self, chapter: usize, book: u64, href: &str, source: &Source) {
        let available = (MAX_ENTRIES - self.entries.len()).min(MAX_CHAPTER_HEADINGS);
        let mut headings = source
            .blocks
            .iter()
            .filter(|_| href.len() <= MAX_ANCHOR_PATH_BYTES)
            .filter_map(|block| {
                let depth = heading_depth(&source.elements[block.element].tag)?;
                let title = outline_label(&block.text);
                (!title.is_empty() || self.alternative).then_some((depth, title, block.key))
            });
        if self.alternative {
            if self.entries.is_empty() {
                self.fragments.push(chapter);
            }
            let mut available = available;
            for (depth, title, block) in headings {
                let target = OutlineTarget::Heading(
                    chapter,
                    Anchor {
                        raw_caret: None,
                        illustration: None,
                        book,
                        chapter: href.to_owned(),
                        revision: source.revision,
                        block,
                        offset: 0,
                    },
                );
                let first_depth = self
                    .entries
                    .last()
                    .map_or(0, |row| row.depth + 1)
                    .min(depth);
                for level in first_depth..=depth {
                    if available == 0 {
                        self.truncated = true;
                        return;
                    }
                    available -= 1;
                    let title = if level == depth {
                        title.clone()
                    } else {
                        String::new()
                    };
                    self.entries.push(OutlineEntry {
                        title,
                        depth: level,
                        target: target.clone(),
                    });
                }
            }
            return;
        }
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
                            raw_caret: None,
                            illustration: None,
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

pub(crate) fn heading_depth(tag: &str) -> Option<usize> {
    match tag {
        "h1" => Some(0),
        "h2" => Some(1),
        "h3" => Some(2),
        "h4" => Some(3),
        "h5" => Some(4),
        "h6" => Some(5),
        _ => None,
    }
}

fn outline_label(text: &str) -> String {
    let mut label = String::new();
    let mut pending_space = false;
    for character in text
        .trim()
        .chars()
        .filter(|c| *c != '\u{00ad}' && (!c.is_control() || c.is_whitespace()))
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
