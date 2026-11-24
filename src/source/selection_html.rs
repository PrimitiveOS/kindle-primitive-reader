//! Raw publisher byte boundaries for the bounded selection inspector.
use super::*;

const MAX_INSPECT_BYTES: usize = 1024 * 1024;
const MAX_HTML_BYTES: usize = 48 * 1024;
const MAX_DEPTH: usize = 96;

#[derive(Clone, Copy)]
struct Cursor<'a, 'input> {
    byte: usize,
    parent: Node<'a, 'input>,
}

struct Boundaries<'a, 'input> {
    selected: Range<usize>,
    start: Option<Cursor<'a, 'input>>,
    end: Option<Cursor<'a, 'input>>,
}

impl<'a, 'input> Boundaries<'a, 'input> {
    fn accept(
        &mut self,
        canonical: Range<usize>,
        start: Cursor<'a, 'input>,
        end: Cursor<'a, 'input>,
    ) {
        if canonical.start == self.selected.start {
            self.start = Some(start);
        }
        if canonical.end == self.selected.end {
            self.end = Some(end);
        }
    }
}

impl Source {
    pub(crate) fn can_inspect_publisher_html(&self) -> bool {
        // Anonymous wrappers and legacy numeric-reference normalization move
        // block keys away from publisher offsets. Never treat those keys as raw.
        self.normalized_xhtml.is_none()
    }

    /// The publisher byte for a canonical text boundary. At inline edges, the
    /// range end belongs to the preceding run and the range start to the next.
    pub(crate) fn publisher_text_boundary(
        &self,
        document: &Document<'_>,
        xhtml: &str,
        block_key: usize,
        offset: usize,
        end: bool,
    ) -> Option<usize> {
        if !self.can_inspect_publisher_html()
            || xhtml.len() > MAX_INSPECT_BYTES
            || !self.matches_input(xhtml)
        {
            return None;
        }
        let block = self.blocks.get(
            self.blocks
                .binary_search_by_key(&block_key, |b| b.key)
                .ok()?,
        )?;
        let node = document
            .descendants()
            .find(|node| node.is_element() && node.range().start == block_key)?;
        if end && offset > 0 || offset == block.text.len() {
            let start = block.text.get(..offset)?.char_indices().next_back()?.0;
            return map_block(node, xhtml, &block.text, start..offset).map(|(_, end)| end.byte);
        }
        let length = block.text.get(offset..)?.chars().next()?.len_utf8();
        map_block(node, xhtml, &block.text, offset..offset + length).map(|(start, _)| start.byte)
    }

    pub(crate) fn publisher_selection_html(
        &self,
        xhtml: &str,
        first: (usize, Range<usize>),
        last: (usize, Range<usize>),
    ) -> Option<String> {
        if !self.can_inspect_publisher_html()
            || xhtml.len() > MAX_INSPECT_BYTES
            || !self.matches_input(xhtml)
        {
            return None;
        }
        let options = || ParsingOptions {
            nodes_limit: 100_000,
            ..ParsingOptions::default()
        };
        let without_doctype;
        let document = match Document::parse_with_options(xhtml, options()) {
            Err(roxmltree::Error::DtdDetected) => {
                let plain = "<!DOCTYPE html>";
                let start = xhtml.find(plain)?;
                without_doctype = format!(
                    "{}{}{}",
                    &xhtml[..start],
                    " ".repeat(plain.len()),
                    &xhtml[start + plain.len()..]
                );
                Document::parse_with_options(&without_doctype, options()).ok()?
            }
            outcome => outcome.ok()?,
        };
        let first_block = self.blocks.get(first.0)?;
        let last_block = self.blocks.get(last.0)?;
        let node = |key| {
            document
                .descendants()
                .find(|node| node.is_element() && node.range().start == key)
        };
        let (start, first_end) =
            map_block(node(first_block.key)?, xhtml, &first_block.text, first.1)?;
        let end = if first.0 == last.0 {
            first_end
        } else {
            map_block(node(last_block.key)?, xhtml, &last_block.text, last.1)?.1
        };
        bounded_fragment(xhtml, start, end)
    }
}

/// Replay the canonical text extractor and verify the entire result. This
/// disambiguates repeated quotes and fails closed on unsupported XML text runs.
fn map_block<'a, 'input>(
    node: Node<'a, 'input>,
    xhtml: &str,
    saved: &str,
    selected: Range<usize>,
) -> Option<(Cursor<'a, 'input>, Cursor<'a, 'input>)> {
    // Use the book extractor’s exact skip ranges from this revision.
    let mut available_ruby = MAX_RUBY_GROUPS;
    let (_, exclusions) = ruby_markup(node, &mut available_ruby);
    let mut boundaries = Boundaries {
        selected,
        start: None,
        end: None,
    };
    let mut text = String::new();
    let mut spaces = false;
    let mut whitespace: Option<(Cursor<'a, 'input>, Cursor<'a, 'input>)> = None;
    for descendant in node.descendants().filter(|child| !hidden(*child)) {
        let raw = descendant.range();
        let excluded = exclusions.partition_point(|range| range.end <= raw.start);
        if exclusions
            .get(excluded)
            .is_some_and(|range| range.contains(&raw.start))
        {
            continue;
        }
        if descendant.has_tag_name("br") {
            let parent = descendant.parent_element()?;
            let offset = text.len();
            text.push('\n');
            boundaries.accept(
                offset..offset + 1,
                Cursor {
                    byte: raw.start,
                    parent,
                },
                Cursor {
                    byte: raw.end,
                    parent,
                },
            );
            spaces = false;
            whitespace = None;
        }
        if !descendant.is_text() {
            continue;
        }
        let parent = descendant.parent_element()?;
        let raw_text = xhtml.get(raw.clone())?;
        let mut cursor = 0;
        for expected in descendant.text()?.chars() {
            let (actual, bytes) = xml_char(raw_text.get(cursor..)?)?;
            if actual != expected {
                return None;
            }
            let begin = Cursor {
                byte: raw.start + cursor,
                parent,
            };
            cursor += bytes;
            let end = Cursor {
                byte: raw.start + cursor,
                parent,
            };
            let output = emit(&mut text, &mut spaces, actual);
            if let Some(span) = output {
                let character = span.end - actual.len_utf8();
                if span.start < character {
                    let (space_start, space_end) = whitespace?;
                    boundaries.accept(span.start..character, space_start, space_end);
                }
                whitespace = None;
                boundaries.accept(character..span.end, begin, end);
            } else {
                let space_start = whitespace.map_or(begin, |(start, _)| start);
                whitespace = Some((space_start, end));
            }
        }
        if cursor != raw_text.len() {
            return None;
        }
    }
    if text != saved {
        return None;
    }
    Some((boundaries.start?, boundaries.end?))
}

/// XML was parsed before this lexical step. Accept only independently
/// verifiable text and XML’s predefined or numeric references.
fn xml_char(input: &str) -> Option<(char, usize)> {
    let first = input.chars().next()?;
    match first {
        '\r' => Some(('\n', if input.starts_with("\r\n") { 2 } else { 1 })),
        '<' => None, // includes CDATA and mixed raw runs, whose Node ranges can be merged
        '&' => {
            let end = input
                .as_bytes()
                .iter()
                .take(32)
                .position(|byte| *byte == b';')?;
            let name = input.get(1..end)?;
            let character = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let value = if let Some(hex) = name.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16).ok()?
                    } else {
                        name.strip_prefix('#')?.parse().ok()?
                    };
                    char::from_u32(value)?
                }
            };
            Some((character, end + 1))
        }
        _ => Some((first, first.len_utf8())),
    }
}

fn bounded_fragment(xhtml: &str, start: Cursor<'_, '_>, end: Cursor<'_, '_>) -> Option<String> {
    let selected = xhtml.get(start.byte..end.byte)?;
    if selected.is_empty() || selected.len() > MAX_HTML_BYTES {
        return None;
    }
    let mut parents: Vec<_> = start
        .parent
        .ancestors()
        .filter(Node::is_element)
        .take(MAX_DEPTH + 1)
        .collect();
    if parents.len() > MAX_DEPTH {
        return None;
    }
    let mut html = String::new();
    let mut push = |part: &str| {
        if html.len() + part.len() > MAX_HTML_BYTES {
            return None;
        }
        html.push_str(part);
        Some(())
    };
    parents.reverse();
    for parent in parents {
        push(opening_tag(xhtml.get(parent.range())?)?)?;
    }
    push(selected)?;
    for (depth, parent) in end.parent.ancestors().filter(Node::is_element).enumerate() {
        if depth >= MAX_DEPTH {
            return None;
        }
        let raw = xhtml.get(parent.range())?;
        push(raw.get(raw.rfind("</")?..)?)?;
    }
    Document::parse(&html).ok()?;
    Some(html)
}

fn opening_tag(raw: &str) -> Option<&str> {
    let mut quote = None;
    for (index, byte) in raw.bytes().enumerate() {
        match (quote, byte) {
            (Some(current), closing) if current == closing => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return raw.get(..=index),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_selection_html_preserves_inline_markup_and_entities_between_exact_endpoints() {
        let xhtml = include_str!("../../tests/fixtures/selection-html.xhtml");
        let source = Source::parse(xhtml).unwrap();
        let phrase = "jade & blue";
        let positions = source
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                let start = block.text.find(phrase)?;
                Some((index, start..start + phrase.len()))
            })
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 2);
        for ((index, span), (id, markup)) in positions.into_iter().zip([
            ("first", r#"<em class="cargo">jade &amp; blue</em>"#),
            (
                "second",
                r#"<strong class="signal">jade &amp; blue</strong>"#,
            ),
        ]) {
            let args = (index, span);
            let html = source
                .publisher_selection_html(xhtml, args.clone(), args)
                .unwrap();
            assert!(html.contains(&format!("<p id=\"{id}\">{markup}</p>")));
            assert!(!html.contains("Outside") && !html.contains("ends here"));
            assert!(!html.contains("font-style"));
            let fragment = Document::parse(&html).unwrap();
            assert!(
                fragment
                    .descendants()
                    .any(|node| node.text() == Some(phrase))
            );
        }

        let first = source.blocks[1].text.find("blue").unwrap();
        let last = source.blocks[3].text.find(" chart").unwrap() + 6;
        let html = source
            .publisher_selection_html(xhtml, (1, first..source.blocks[1].text.len()), (3, 0..last))
            .unwrap();
        assert!(html.contains("<br/>"));
        assert!(html.contains(r##"<a href="#end" epub:type="noteref">tide chart</a>"##));
        assert!(!html.contains("carefully"));
        assert!(Document::parse(&html).is_ok());
    }

    #[test]
    fn publisher_selection_html_keeps_multibyte_characters_and_raw_numeric_references() {
        let xhtml =
            "<!DOCTYPE html><html><body><p>Left café &#x1F30A; &#101; right.</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        let start = "Left ".len();
        let end = start + "café 🌊 e".len();
        let html = source
            .publisher_selection_html(xhtml, (0, start..end), (0, start..end))
            .unwrap();
        assert_eq!(
            html,
            "<html><body><p>café &#x1F30A; &#101;</p></body></html>"
        );
    }

    #[test]
    fn publisher_selection_html_rejects_source_changes_normalization_and_malformed_offsets() {
        let xhtml = "<html><body><p>é &amp; f</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        assert!(
            source
                .publisher_selection_html(xhtml, (0, 1..3), (0, 1..3))
                .is_none()
        );
        assert!(
            source
                .publisher_selection_html(
                    "<html><body><p>é &amp; z</p></body></html>",
                    (0, 0..2),
                    (0, 0..2)
                )
                .is_none()
        );
        assert!(Source::parse("<html><body><p>bad &bogus;</p></body></html>").is_err());
        let xhtml = "<html><body><div>anonymous<p>paragraph</p>tail</div></body></html>";
        let source = Source::parse(xhtml).unwrap();
        assert!(!source.can_inspect_publisher_html());
        let xhtml = "<html><body><p>prefix<![CDATA[é & atom]]>suffix</p></body></html>";
        let source = Source::parse(xhtml).unwrap();
        assert!(
            source
                .publisher_selection_html(xhtml, (0, 0..12), (0, 0..12))
                .is_none()
        );
    }
}
