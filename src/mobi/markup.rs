//! Source-backed additions to the historical normalized XHTML. Coordinates here
//! refer to the old XHTML so its chapter boundaries and saved text can be rebuilt.
use super::{Encoding, MobiError, escape_xml, safe_tag};
use crate::kf8::legacy_contents::Contents;
use html5ever::tokenizer::Tag;
use std::collections::{BTreeMap, BTreeSet};

const MAX_TARGETS: usize = 8192;
const MAX_EDITS: usize = 32_768;
const MAX_ASSETS: usize = 128;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_IMAGES: usize = 8 * 1024 * 1024;
type Result<T> = std::result::Result<T, MobiError>;

pub(super) fn attribute<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.trim())
}

fn number(text: &str) -> Option<usize> {
    (!text.is_empty() && text.len() <= 10 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Find candidate byte indices before tokenization. Only the HTML tokenizer can
/// confirm a destination or emit a link, so comments and unrelated attributes
/// cannot produce live links. Legacy filepos counts source bytes before decoding.
pub(super) fn destinations(
    text: &str,
    encoding: Encoding,
    contents: &Contents,
) -> Vec<(usize, usize)> {
    let raw = text.as_bytes();
    let mut targets = BTreeSet::new();
    for (index, _) in text.match_indices(['f', 'F']) {
        if !raw
            .get(index..index + 7)
            .is_some_and(|s| s.eq_ignore_ascii_case(b"filepos"))
        {
            continue;
        }
        let attr = text[index + 7..].trim_start_matches(|c: char| c.is_ascii_whitespace());
        let Some(value) = attr.strip_prefix('=') else {
            continue;
        };
        let value = value
            .trim_start_matches(|c: char| c.is_ascii_whitespace())
            .trim_start_matches(['\'', '"']);
        let end = value
            .bytes()
            .take(11)
            .take_while(u8::is_ascii_digit)
            .count();
        if let Some(n) = number(&value[..end]) {
            targets.insert(n);
            if targets.len() > MAX_TARGETS {
                return Vec::new();
            }
        }
    }
    targets.extend(contents.items.iter().map(|item| item.position));
    match encoding {
        Encoding::Cp1252 | Encoding::CodePage(_) => {
            let mut indices = targets.into_iter().peekable();
            text.char_indices()
                .enumerate()
                .filter_map(|(raw_at, (utf8_at, _))| {
                    if indices.next_if_eq(&raw_at).is_some() {
                        Some((raw_at, utf8_at))
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => targets.into_iter().map(|n| (n, n)).collect(),
    }
}

/// Confirm the byte belongs to a start tag in the markup, rather than a quoted
/// attribute, comment or raw-text body. Tokenizer emission validates the tag again.
pub(super) fn target_tags<'a>(
    text: &'a str,
    encoding: Encoding,
    contents: &Contents,
) -> Vec<(usize, usize, usize, &'a str)> {
    let requested = destinations(text, encoding, contents);
    if requested.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut cursor = 0;
    let mut next = 0;
    while let Some(relative) = text[cursor..].find('<') {
        let start = cursor + relative;
        while next < requested.len() && requested[next].1 < start {
            next += 1;
        }
        if next == requested.len() {
            break;
        }
        let rest = &text[start..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            let Some(end) = comment.find("-->") else {
                break;
            };
            cursor = start + 4 + end + 3;
            continue;
        }
        let special = rest
            .as_bytes()
            .get(1)
            .is_some_and(|b| matches!(b, b'/' | b'!' | b'?'));
        let Some((name, end)) = opening(text, start) else {
            cursor = if special || rest.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) {
                rest.find('>').map_or(text.len(), |n| start + n + 1)
            } else {
                start + 1
            };
            continue;
        };
        if requested[next].1 == start {
            result.push((requested[next].0, start, end, name));
            next += 1;
        }
        cursor = end;
        if ["script", "style", "title", "iframe"]
            .iter()
            .any(|raw| name.eq_ignore_ascii_case(raw))
        {
            loop {
                let Some(relative) = text[cursor..].find("</") else {
                    cursor = text.len();
                    break;
                };
                cursor += relative + 2;
                let end = cursor + name.len();
                if text
                    .get(cursor..end)
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(name))
                    && text
                        .as_bytes()
                        .get(end)
                        .is_some_and(|v| v.is_ascii_whitespace() || *v == b'>')
                {
                    cursor = text[end..].find('>').map_or(text.len(), |n| end + n + 1);
                    break;
                }
            }
        }
    }
    result
}

/// Candidate source-byte positions in text, never inside tags or references.
pub(super) fn target_text(
    text: &str,
    encoding: Encoding,
    contents: &Contents,
) -> Vec<(usize, usize)> {
    let requested = destinations(text, encoding, contents);
    let mut result = Vec::new();
    let (mut cursor, mut next) = (0, 0);
    while cursor < text.len() && next < requested.len() {
        let start = text[cursor..].find('<').map_or(text.len(), |n| cursor + n);
        while next < requested.len() && requested[next].1 < start {
            let (source, at) = requested[next];
            if at >= cursor && text.is_char_boundary(at) && safe_text_boundary(text, cursor, at) {
                result.push((source, at));
            }
            next += 1;
        }
        if start == text.len() {
            break;
        }
        let rest = &text[start..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            let Some(end) = comment.find("-->") else {
                break;
            };
            cursor = start + 4 + end + 3;
            continue;
        }
        let Some((name, end)) = opening(text, start) else {
            let special = rest
                .as_bytes()
                .get(1)
                .is_some_and(|b| matches!(b, b'/' | b'!' | b'?') || b.is_ascii_alphabetic());
            cursor = if special {
                rest.find('>').map_or(text.len(), |n| start + n + 1)
            } else {
                start + 1
            };
            continue;
        };
        cursor = end;
        if ["script", "style", "title", "iframe"]
            .iter()
            .any(|raw| name.eq_ignore_ascii_case(raw))
        {
            loop {
                let Some(relative) = text[cursor..].find("</") else {
                    cursor = text.len();
                    break;
                };
                cursor += relative + 2;
                let end = cursor + name.len();
                if text
                    .get(cursor..end)
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(name))
                    && text
                        .as_bytes()
                        .get(end)
                        .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                {
                    cursor = text[end..].find('>').map_or(text.len(), |n| end + n + 1);
                    break;
                }
            }
        }
    }
    result
}

fn safe_text_boundary(text: &str, start: usize, at: usize) -> bool {
    // An ending '<' can leave the tokenizer in tag-open state even when its
    // input queue is empty; CR can leave pending newline preprocessing.
    if at > 0
        && text
            .as_bytes()
            .get(at - 1)
            .is_some_and(|&b| matches!(b, b'<' | b'\r'))
    {
        return false;
    }
    let preceding = &text[start..at];
    if let Some(amp) = preceding.rfind('&') {
        if preceding[amp + 1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'#')
        {
            return false;
        }
    }
    true
}

fn opening(text: &str, at: usize) -> Option<(&str, usize)> {
    let rest = text.get(at..)?.strip_prefix('<')?;
    let len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-'))
        .count();
    let name = rest.get(..len)?;
    if len == 0
        || !rest
            .as_bytes()
            .get(len)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'>' | b'/'))
    {
        return None;
    }
    let mut quote = None;
    for (index, byte) in rest
        .as_bytes()
        .iter()
        .copied()
        .enumerate()
        .take(8192)
        .skip(len)
    {
        match (quote, byte) {
            (Some(q), c) if q == c => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some((name, at + index + 2)),
            (None, b'<') => return None,
            _ => {}
        }
    }
    None
}

#[derive(Default)]
pub(super) struct Features {
    pub presentation: super::Presentation,
    pub next: Option<(usize, String)>,
    ids: BTreeMap<String, usize>,
    destinations: BTreeMap<usize, (usize, Option<String>, bool)>,
    links: Vec<(usize, usize)>,
    images: Vec<(usize, usize, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct Addition {
    pub at: usize,
    pub xml: String,
}

pub(crate) fn apply(
    raw: &str,
    edits: &[Addition],
    source_start: usize,
    prefix: usize,
) -> Result<String> {
    let mut length = raw.len();
    for edit in edits {
        length = length
            .checked_add(edit.xml.len())
            .filter(|n| *n <= super::MAX_XHTML_BYTES)
            .ok_or(MobiError::TextTooLarge)?;
    }
    let mut xhtml = String::with_capacity(length);
    let mut cursor = 0;
    for edit in edits {
        let at = prefix + (edit.at - source_start);
        xhtml.push_str(raw.get(cursor..at).ok_or(MobiError::InvalidText)?);
        xhtml.push_str(&edit.xml);
        cursor = at;
    }
    xhtml.push_str(&raw[cursor..]);
    Ok(xhtml)
}

impl Features {
    pub(super) fn has_images(&self) -> bool {
        !self.images.is_empty()
    }
    pub fn record(&mut self, tag: &Tag, name: &str, at: usize) {
        let id = attribute(tag, "id")
            .filter(|v| !v.is_empty() && v.len() <= 512)
            .or_else(|| {
                (name == "a")
                    .then(|| attribute(tag, "name"))
                    .flatten()
                    .filter(|v| !v.is_empty() && v.len() <= 512)
            });
        if let Some(id) = id {
            *self.ids.entry(id.to_owned()).or_default() += 1;
        }
        if let Some((position, expected)) = self.next.take() {
            if expected.eq_ignore_ascii_case(tag.name.as_ref())
                && safe_tag(tag.name.as_ref()).is_some()
            {
                self.destinations
                    .insert(position, (at, id.map(str::to_owned), false));
            }
        }
        if name == "a"
            && !attribute(tag, "href").is_some_and(|v| v.starts_with('#'))
            && let Some(target) = attribute(tag, "filepos").and_then(number)
            && self.links.len() < MAX_EDITS
        {
            self.links.push((at, target));
        }
    }
    pub(super) fn text_destination(&mut self, source: usize, at: usize) {
        self.destinations.insert(source, (at, None, true));
    }
    pub fn image(&mut self, tag: &Tag, at: usize) -> Result<()> {
        let Some(n) = attribute(tag, "recindex")
            .and_then(number)
            .filter(|v| *v != 0)
        else {
            return Ok(());
        };
        if self.images.len() >= MAX_EDITS {
            return Ok(());
        }
        let mut attrs = String::new();
        for name in ["alt", "width", "height"] {
            if let Some(value) = attribute(tag, name).filter(|v| !v.is_empty() && v.len() <= 512)
                && (name == "alt" || number(value).is_some_and(|n| (1..=32768).contains(&n)))
            {
                attrs.push_str(&format!(" {name}=\""));
                escape_xml(&mut attrs, value)?;
                attrs.push('"');
            }
        }
        self.images.push((at, n, attrs));
        Ok(())
    }
    pub fn finish(
        self,
        bytes: &[u8],
        records: &[usize],
        first: Option<usize>,
        contents: &mut Contents,
    ) -> Result<(Vec<Addition>, BTreeMap<String, Vec<u8>>, BTreeSet<String>)> {
        let mut edits = Vec::new();
        let mut names = BTreeMap::new();
        let mut shadow_ids = BTreeSet::new();
        let used: BTreeSet<_> = self.links.iter().map(|&(_, n)| n).collect();
        let navigation: BTreeSet<_> = contents.items.iter().map(|entry| entry.position).collect();
        for (source, (at, id, text_target)) in self.destinations {
            if !used.contains(&source) && !navigation.contains(&source) {
                continue;
            }
            let id = if let Some(id) = id {
                if self.ids.get(&id) != Some(&1) {
                    continue;
                }
                id
            } else {
                let mut id = if text_target {
                    format!("mobi-shadow-text-{source}")
                } else {
                    format!("mobi-filepos-{source}")
                };
                while self.ids.contains_key(&id) {
                    id.push('_');
                }
                let edit = Addition {
                    at,
                    xml: if text_target {
                        format!("<span id=\"{id}\"></span>")
                    } else {
                        format!(" id=\"{id}\"")
                    },
                };
                if used.contains(&source) {
                    edits.push(edit)
                } else {
                    contents.insertions.push(edit)
                }
                id
            };
            if text_target {
                shadow_ids.insert(id.clone());
            }
            names.insert(source, id);
        }
        for item in &mut contents.items {
            item.fragment = names.get(&item.position).cloned();
        }
        contents.insertions.sort_by_key(|item| item.at);
        for (at, target) in self.links {
            if let Some(name) = names.get(&target) {
                let mut xml = " href=\"#".to_owned();
                escape_xml(&mut xml, name)?;
                xml.push('"');
                edits.push(Addition { at, xml });
            }
        }
        let mut resources = BTreeMap::<String, Vec<u8>>::new();
        let mut total = 0usize;
        if let Some(first) = first {
            for (at, n, attrs) in self.images {
                let Some(index) = first.checked_add(n - 1) else {
                    continue;
                };
                let Some((&start, &end)) = records
                    .get(index)
                    .zip(index.checked_add(1).and_then(|i| records.get(i)))
                else {
                    continue;
                };
                let record = &bytes[start..end];
                if record.len() > MAX_IMAGE_BYTES {
                    continue;
                }
                let ext = if record.starts_with(b"\xff\xd8\xff") {
                    "jpg"
                } else if record.starts_with(b"\x89PNG\r\n\x1a\n") {
                    "png"
                } else if record.starts_with(b"GIF87a") || record.starts_with(b"GIF89a") {
                    "gif"
                } else {
                    continue;
                };
                let path = format!("assets/mobi-{n}.{ext}");
                if !resources.contains_key(&path) {
                    if resources.len() >= MAX_ASSETS || record.len() > MAX_TOTAL_IMAGES - total {
                        continue;
                    }
                    resources.insert(path.clone(), record.to_vec());
                    total += record.len();
                }
                edits.push(Addition {
                    at,
                    xml: format!("<img src=\"{path}\"{attrs} />"),
                });
            }
        }
        edits.sort_by_key(|edit| edit.at);
        Ok((edits, resources, shadow_ids))
    }
}
