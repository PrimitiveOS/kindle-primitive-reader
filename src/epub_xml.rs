//! Read declarations common in EPUB 2 without loading an external DTD or expanding declared entities.
use crate::Result;

pub(crate) fn prepare(mut xml: String, limit: usize) -> Result<String> {
    if xml.len() > limit {
        return Err("The EPUB XML exceeds its size limit".into());
    }
    let Some((start, end, html)) = declaration(&xml, false) else {
        return Ok(xml);
    };
    // The chapter parser already accepted this exact form. Keep its revision and anchors.
    if &xml[start..end] != "<!DOCTYPE html>" {
        blank(&mut xml, start, end);
    }
    if html && xml[end..].contains('&') {
        expand_html_names(&mut xml, end, limit, |_, _| Ok(()))?;
    }
    Ok(xml)
}

/// Preserve all source offsets while parsing the EPUB's conventional HTML declaration.
pub(crate) fn without_plain_doctype(xml: &str) -> std::borrow::Cow<'_, str> {
    if let Some((start, end, _)) = declaration(xml, false)
        && &xml[start..end] == "<!DOCTYPE html>"
    {
        let mut masked = xml.to_owned();
        blank(&mut masked, start, end);
        masked.into()
    } else {
        xml.into()
    }
}

#[derive(Default)]
pub(crate) struct SourceOffsets {
    changes: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>,
}

impl SourceOffsets {
    /// Resolve a source byte inside a character reference to the start of that character.
    pub(crate) fn resolve(&self, offset: usize) -> usize {
        let i = self
            .changes
            .partition_point(|(source, _)| source.end <= offset);
        if let Some((source, normalized)) = self.changes.get(i)
            && source.contains(&offset)
        {
            return normalized.start;
        }
        self.changes
            .get(..i)
            .and_then(|before| before.last())
            .map_or(offset, |(source, normalized)| {
                normalized.end + (offset - source.end)
            })
    }
}

/// Kindle links use offsets in the original, unexpanded UTF-8 chapter.
pub(crate) fn prepare_kf8(xml: &mut String, limit: usize) -> Result<SourceOffsets> {
    let mut positions = SourceOffsets::default();
    if let Some((start, end, html)) = declaration(xml, true) {
        blank(xml, start, end);
        if html && xml[end..].contains('&') {
            expand_html_names(xml, end, limit, |source, normalized| {
                if source.len() != normalized.len() {
                    if positions.changes.len() >= 64 * 1024 {
                        return Err("Too many source offset changes in this Kindle chapter".into());
                    }
                    positions.changes.push((source, normalized));
                }
                Ok(())
            })?;
        }
    }
    Ok(positions)
}

fn blank(xml: &mut String, start: usize, end: usize) {
    // Preserve offsets and line numbers when only the declaration changes.
    let padding: String = xml.as_bytes()[start..end]
        .iter()
        .map(|&byte| match byte {
            b'\n' => '\n',
            b'\r' => '\r',
            _ => ' ',
        })
        .collect();
    xml.replace_range(start..end, &padding);
}

/// Accept a simple root name and an optional quoted external identifier. Anything containing
/// custom declarations or an internal subset stays with the ordinary DTD-disabled XML parser.
fn declaration(xml: &str, kindle_html: bool) -> Option<(usize, usize, bool)> {
    let mut rest = xml.trim_start_matches(xml_space);
    loop {
        if rest.starts_with("<?") {
            rest = rest
                .get(rest.find("?>")? + 2..)?
                .trim_start_matches(xml_space);
        } else if rest.starts_with("<!--") {
            rest = rest
                .get(rest.find("-->")? + 3..)?
                .trim_start_matches(xml_space);
        } else {
            break;
        }
    }
    let start = xml.len() - rest.len();
    let token = rest.get(..9)?;
    let html_declaration = kindle_html && token.eq_ignore_ascii_case("<!doctype");
    if token != "<!DOCTYPE" && !html_declaration {
        return None;
    }
    rest = &rest[9..];
    if !rest.starts_with(xml_space) {
        return None;
    }
    rest = rest.trim_start_matches(xml_space);
    let end_name = rest.find(|c| xml_space(c) || c == '>')?;
    let name = &rest[..end_name];
    if token != "<!DOCTYPE"
        && (!name.eq_ignore_ascii_case("html")
            || !rest[end_name..]
                .trim_start_matches(xml_space)
                .starts_with('>'))
    {
        return None;
    }
    if !matches!(name, "html" | "ncx" | "package" | "container" | "svg") {
        return None;
    }
    rest = rest[end_name..].trim_start_matches(xml_space);
    if !rest.starts_with('>') {
        let literals = if let Some(after) = rest.strip_prefix("PUBLIC") {
            rest = after;
            2
        } else {
            rest = rest.strip_prefix("SYSTEM")?;
            1
        };
        for _ in 0..literals {
            if !rest.starts_with(xml_space) {
                return None;
            }
            rest = rest.trim_start_matches(xml_space);
            let quote = rest.chars().next().filter(|ch| matches!(ch, '\'' | '"'))?;
            rest = &rest[1..];
            rest = rest.get(rest.find(quote)? + 1..)?;
        }
        rest = rest.trim_start_matches(xml_space);
    }
    rest = rest.strip_prefix('>')?;
    Some((start, xml.len() - rest.len(), name == "html"))
}

fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\r' | '\t')
}

/// Only replace names from the built-in HTML vocabulary. Numeric references remain XML-safe
/// in both text and attributes. Contents of comments, PIs and CDATA are literal.
fn expand_html_names(
    xml: &mut String,
    start: usize,
    limit: usize,
    mut record: impl FnMut(std::ops::Range<usize>, std::ops::Range<usize>) -> Result<()>,
) -> Result<()> {
    use std::fmt::Write;
    let mut rest = &xml[start..];
    let mut output = String::new();
    let mut copied = 0;
    while let Some(next) = rest.find(['<', '&']) {
        rest = &rest[next..];
        if let Some(terminator) = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")]
            .into_iter()
            .find_map(|(open, close)| rest.starts_with(open).then_some(close))
        {
            let Some(end) = rest.find(terminator) else {
                break;
            };
            rest = &rest[end + terminator.len()..];
            continue;
        }
        if !rest.starts_with('&') {
            rest = &rest[1..];
            continue;
        }
        let offset = xml.len() - rest.len();
        rest = &rest[1..];
        let length = rest
            .bytes()
            .take(33)
            .take_while(u8::is_ascii_alphanumeric)
            .count();
        if length == 0 || rest.as_bytes().get(length) != Some(&b';') {
            continue;
        }
        let name = &rest[..=length];
        if matches!(name, "amp;" | "apos;" | "lt;" | "gt;" | "quot;") {
            continue;
        }
        let Some(&(first, second)) = html5ever::data::NAMED_ENTITIES.get(name) else {
            continue;
        };
        output.push_str(&xml[copied..offset]);
        let normalized = output.len();
        write!(&mut output, "&#{first};")?;
        if second != 0 {
            write!(&mut output, "&#{second};")?;
        }
        rest = &rest[name.len()..];
        copied = xml.len() - rest.len();
        record(offset..copied, normalized..output.len())?;
        if output.len() > limit {
            return Err("The EPUB XML exceeds its size limit".into());
        }
    }
    if copied != 0 {
        if output.len() + xml.len() - copied > limit {
            return Err("The EPUB XML exceeds its size limit".into());
        }
        output.push_str(&xml[copied..]);
        *xml = output;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document;

    #[test]
    fn kindle_positions_follow_shorter_longer_and_two_codepoint_names() {
        let original = "<!DOCTYPE html><html><p>&shy; É &RightArrow; &NotEqualTilde; &nbsp;</p><h1 id='next'>Next</h1></html>";
        let mut normalized = original.to_owned();
        let offsets = prepare_kf8(&mut normalized, 2048).unwrap();
        assert_eq!(
            offsets.resolve(original.find("<h1").unwrap()),
            normalized.find("<h1").unwrap()
        );
        assert_eq!(
            offsets.resolve(original.find("&RightArrow;").unwrap() + 3),
            normalized.find("&#8594;").unwrap()
        );
        let doc = Document::parse(&normalized).unwrap();
        assert!(
            doc.descendants()
                .find(|n| n.has_tag_name("p"))
                .unwrap()
                .text()
                .unwrap()
                .contains('→')
        );
    }

    #[test]
    fn keeps_character_data_and_offsets_when_skipping_an_external_declaration() {
        let start = "<?xml version='1.0'?>\n<!-- footnotes -->\n<!DOCTYPE html PUBLIC 'a'\n'b'>\n";
        let xml = format!("{start}<html><p>a &amp; b</p></html>");
        let result = prepare(xml.clone(), 5000).unwrap();
        assert_eq!(result.find("<html>"), xml.find("<html>"));
        assert_eq!(result.lines().count(), xml.lines().count());
        assert_eq!(
            Document::parse(&result)
                .unwrap()
                .root_element()
                .first_element_child()
                .unwrap()
                .text(),
            Some("a & b")
        );
        let short = "<!DOCTYPE html><html/>";
        assert_eq!(prepare(short.to_owned(), 100).unwrap(), short);
    }

    #[test]
    fn handles_standard_names_in_text_and_attributes_and_leaves_literal_sections_intact() {
        let raw = "<!DOCTYPE html SYSTEM 'reference.dtd'><html a='&nbsp;'><!-- &copy; --><?note &ndash;?><p><![CDATA[&copy;]]></p><p>&ndash;&lt;&NotEqualTilde;</p></html>";
        let clean = prepare(raw.to_owned(), 2000).unwrap();
        assert!(clean.contains("<!-- &copy; --><?note &ndash;?>"));
        let doc = Document::parse(&clean).unwrap();
        let root = doc.root_element();
        assert_eq!(root.attribute("a"), Some("\u{00a0}"));
        assert_eq!(root.first_element_child().unwrap().text(), Some("&copy;"));
        assert_eq!(
            root.last_element_child().unwrap().text(),
            Some("–<≂\u{0338}")
        );
        let unknown = "<!DOCTYPE html SYSTEM 'reference.dtd'><html>&Unlisted;</html>";
        assert!(Document::parse(&prepare(unknown.into(), 500).unwrap()).is_err());
    }

    #[test]
    fn document_defined_entities_still_require_a_dtd_and_are_rejected() {
        let raw = "<!DOCTYPE html [<!ENTITY custom 'term'>]><html>&custom;</html>";
        assert_eq!(prepare(raw.to_owned(), 500).unwrap(), raw);
        assert!(matches!(
            Document::parse(raw),
            Err(roxmltree::Error::DtdDetected)
        ));
    }
}
