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

/// Recover an otherwise XML-compatible HTML chapter whose publisher used
/// built-in HTML character names without declaring an HTML doctype. This is
/// deliberately separate from general XML and package-metadata parsing.
pub(crate) fn recover_html_names(xml: String, limit: usize) -> String {
    if !xml.contains('&') || xml.len() > limit {
        return xml;
    }
    let options = || roxmltree::ParsingOptions {
        nodes_limit: 100_000,
        ..Default::default()
    };
    if !matches!(
        roxmltree::Document::parse_with_options(&xml, options()),
        Err(roxmltree::Error::UnknownEntityReference(_, _))
    ) {
        return xml;
    }
    // Never turn document-defined entities into built-in HTML names. The
    // conservative whole-source check can also refuse declarations in comments.
    if xml
        .as_bytes()
        .windows(9)
        .any(|b| b.eq_ignore_ascii_case(b"<!doctype"))
        || xml
            .as_bytes()
            .windows(8)
            .any(|b| b.eq_ignore_ascii_case(b"<!entity"))
    {
        return xml;
    }
    let mut candidate = xml.clone();
    if expand_html_names(&mut candidate, 0, limit, |_, _| Ok(())).is_err() || candidate == xml {
        return xml;
    }
    let valid = match roxmltree::Document::parse_with_options(&candidate, options()) {
        Ok(document)
            if document.root_element().tag_name().name() == "html"
                && matches!(
                    document.root_element().tag_name().namespace(),
                    None | Some("http://www.w3.org/1999/xhtml")
                ) =>
        {
            true
        }
        _ => false,
    };
    if valid { candidate } else { xml }
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

/// Admit only these exact declarations for malformed-chapter recovery. The
/// ordinary XML preparation masks them; their URLs are never fetched.
pub(crate) fn known_xhtml_public_doctype(xml: &str) -> bool {
    let Some((start, end, true)) = declaration(xml, false) else {
        return false;
    };
    matches!(
        &xml[start..end],
        "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">"
            | "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\">"
            | "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">"
    )
}

/// Repair only unclosed HTML void start tags in an otherwise XML-compatible
/// chapter. Already parseable input is returned by the caller unchanged.
fn parse_recovery_xml(
    text: &str,
) -> std::result::Result<roxmltree::Document<'_>, roxmltree::Error> {
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
}

pub(crate) fn recover_html_voids(xml: &str, limit: usize) -> Option<String> {
    let repaired = html_void_candidate(xml, limit, false)?;
    finish_void_recovery(repaired, limit)
}

fn finish_void_recovery(repaired: String, limit: usize) -> Option<String> {
    let repaired = recover_html_names(repaired, limit);
    let checked = without_plain_doctype(&repaired);
    let doc = parse_recovery_xml(&checked).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "html"
        || !matches!(
            root.tag_name().namespace(),
            None | Some("http://www.w3.org/1999/xhtml")
        )
        || !root.children().any(|node| node.has_tag_name("body"))
    {
        return None;
    }
    drop(doc);
    drop(checked);
    Some(repaired)
}

/// Reuse the exact void-tag scan and only return after the existing
/// ampersand repair validates the entire combined result.
pub(crate) fn recover_html_voids_and_ampersands(xml: &str, limit: usize) -> Option<String> {
    let candidate = html_void_candidate(xml, limit, false)?;
    recover_html_bare_ampersands(&candidate, limit)
}

/// Try ASCII-case variants only after the existing recovery paths failed.
pub(crate) fn recover_html_uppercase_voids(xml: &str, limit: usize) -> Option<String> {
    let candidate = html_void_candidate(xml, limit, true)?;
    finish_void_recovery(candidate.clone(), limit)
        .or_else(|| recover_html_bare_ampersands(&candidate, limit))
}

/// Quote only unambiguous ASCII HTML attribute values after earlier recovery
/// paths fail. Full XML and root validation remains mandatory.
pub(crate) fn recover_html_unquoted_attributes(xml: &str, limit: usize) -> Option<String> {
    if xml.len() > limit {
        return None;
    }
    let checked = without_plain_doctype(xml);
    if parse_recovery_xml(&checked).is_ok()
        || checked
            .as_bytes()
            .windows(9)
            .any(|w| w.eq_ignore_ascii_case(b"<!doctype"))
        || checked
            .as_bytes()
            .windows(8)
            .any(|w| w.eq_ignore_ascii_case(b"<!entity"))
    {
        return None;
    }
    let bytes = xml.as_bytes();
    let name_start = |b: u8| b.is_ascii_alphabetic() || matches!(b, b'_' | b':');
    let name_part = |b: u8| name_start(b) || b.is_ascii_digit() || matches!(b, b'-' | b'.');
    let safe_value = |b: u8| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'_' | b'-' | b'.' | b'/' | b'#' | b'?' | b'%' | b'+' | b'@' | b'~'
            )
    };
    let mut insertions = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(offset) = bytes[cursor..].iter().position(|&byte| byte == b'<') else {
            break;
        };
        let start = cursor + offset;
        if bytes[start..].starts_with(b"<!--") {
            cursor = start + 4 + xml[start + 4..].find("-->")? + 3;
            continue;
        }
        if bytes[start..].starts_with(b"<![CDATA[") {
            cursor = start + 9 + xml[start + 9..].find("]]>")? + 3;
            continue;
        }
        if bytes[start..].starts_with(b"<?") {
            cursor = start + 2 + xml[start + 2..].find("?>")? + 2;
            continue;
        }
        if bytes[start..].starts_with(b"<!DOCTYPE html>") {
            cursor = start + "<!DOCTYPE html>".len();
            continue;
        }
        if bytes.get(start + 1) == Some(&b'!') {
            return None;
        }
        let mut end = start + 1;
        let mut quote = None;
        while end < bytes.len() {
            let byte = bytes[end];
            if let Some(delimiter) = quote {
                if byte == delimiter {
                    quote = None;
                }
            } else if byte == b'\'' || byte == b'"' {
                quote = Some(byte);
            } else if byte == b'>' {
                break;
            } else if byte == b'<' {
                return None;
            }
            end += 1;
        }
        if end == bytes.len() || quote.is_some() {
            return None;
        }
        let closing = bytes.get(start + 1) == Some(&b'/');
        let mut pos = start + 1 + usize::from(closing);
        if !bytes.get(pos).is_some_and(|b| name_start(*b)) {
            return None;
        }
        let name_begin = pos;
        while pos < end && name_part(bytes[pos]) {
            pos += 1;
        }
        let name = &xml[name_begin..pos];
        let local = name.rsplit(':').next()?;
        if local.eq_ignore_ascii_case("script") {
            return None;
        }
        if local.eq_ignore_ascii_case("style") && name != "style" {
            return None;
        }
        if closing {
            cursor = end + 1;
            continue;
        }
        while pos < end {
            if bytes[pos] == b'/' && pos + 1 == end {
                break;
            }
            if !bytes[pos].is_ascii_whitespace() {
                return None;
            }
            while pos < end && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos == end || bytes[pos] == b'/' && pos + 1 == end {
                break;
            }
            if !name_start(bytes[pos]) {
                return None;
            }
            pos += 1;
            while pos < end && name_part(bytes[pos]) {
                pos += 1;
            }
            while pos < end && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) != Some(&b'=') {
                return None;
            }
            pos += 1;
            while pos < end && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos == end {
                return None;
            }
            if matches!(bytes[pos], b'\'' | b'"') {
                let delimiter = bytes[pos];
                pos += 1;
                while pos < end && bytes[pos] != delimiter {
                    pos += 1;
                }
                if pos == end {
                    return None;
                }
                pos += 1;
                continue;
            }
            let value_start = pos;
            while pos < end && !bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let value = &bytes[value_start..pos];
            // A trailing slash before > is ambiguous in HTML's unquoted syntax.
            if value.is_empty()
                || !value.iter().copied().all(safe_value)
                || pos == end && value.last() == Some(&b'/')
                || insertions.len() >= 16_384
                || xml.len().saturating_add(insertions.len() + 2) > limit
            {
                return None;
            }
            insertions.push(value_start);
            insertions.push(pos);
        }
        if name == "style" && !xml[start + 1..end].trim_end().ends_with('/') {
            let after = &xml[end + 1..];
            let close = after.find("</style>")?;
            if after[..close].contains('<') {
                return None;
            }
            cursor = end + 1 + close;
        } else {
            cursor = end + 1;
        }
    }
    if insertions.is_empty() {
        return None;
    }
    let mut candidate = String::with_capacity(xml.len() + insertions.len());
    let mut last = 0;
    for at in insertions {
        candidate.push_str(&xml[last..at]);
        candidate.push('"');
        last = at;
    }
    candidate.push_str(&xml[last..]);
    finish_void_recovery(candidate.clone(), limit)
        .or_else(|| recover_html_voids(&candidate, limit))
        .or_else(|| recover_html_bare_ampersands(&candidate, limit))
        .or_else(|| recover_html_voids_and_ampersands(&candidate, limit))
        .or_else(|| recover_html_uppercase_voids(&candidate, limit))
}

fn html_void_candidate(xml: &str, limit: usize, fold_void_case: bool) -> Option<String> {
    if xml.len() > limit {
        return None;
    }
    let masked = without_plain_doctype(xml);
    if parse_recovery_xml(&masked).is_ok() {
        return None;
    }
    // Never expand document-defined entities or interpret external DTDs.
    if masked
        .as_bytes()
        .windows(9)
        .any(|w| w.eq_ignore_ascii_case(b"<!doctype"))
        || masked
            .as_bytes()
            .windows(8)
            .any(|w| w.eq_ignore_ascii_case(b"<!entity"))
    {
        return None;
    }
    let bytes = xml.as_bytes();
    let mut insertions = Vec::new();
    let mut replacements: Vec<(usize, &'static str)> = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(offset) = bytes[cursor..].iter().position(|&byte| byte == b'<') else {
            break;
        };
        let start = cursor + offset;
        if bytes[start..].starts_with(b"<!--") {
            cursor = start + 4 + xml[start + 4..].find("-->")? + 3;
            continue;
        }
        if bytes[start..].starts_with(b"<![CDATA[") {
            cursor = start + 9 + xml[start + 9..].find("]]>")? + 3;
            continue;
        }
        if bytes[start..].starts_with(b"<?") {
            cursor = start + 2 + xml[start + 2..].find("?>")? + 2;
            continue;
        }
        if bytes[start..].starts_with(b"<!DOCTYPE html>") {
            cursor = start + "<!DOCTYPE html>".len();
            continue;
        }
        if bytes.get(start + 1) == Some(&b'!') {
            return None;
        }
        let mut end = start + 1;
        let mut quote = None;
        while end < bytes.len() {
            let byte = bytes[end];
            if let Some(delimiter) = quote {
                if byte == delimiter {
                    quote = None;
                }
            } else if byte == b'\'' || byte == b'"' {
                quote = Some(byte);
            } else if byte == b'>' {
                break;
            } else if byte == b'<' {
                return None;
            }
            end += 1;
        }
        if end == bytes.len() || quote.is_some() {
            return None;
        }
        let token = &xml[start + 1..end];
        let name = token
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()?;
        let local = name.rsplit(':').next()?;
        if local.eq_ignore_ascii_case("script") {
            return None;
        }
        // Only the exact unprefixed form has the bounded style-text scan below.
        if local.eq_ignore_ascii_case("style") && name != "style" {
            return None;
        }
        if name == "style" && !token.trim_end().ends_with('/') {
            let after = &xml[end + 1..];
            let close = after.find("</style>")?;
            if after[..close].contains('<') {
                return None;
            }
            cursor = end + 1 + close;
            continue;
        }
        let void = [
            "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
            "source", "track", "wbr",
        ]
        .into_iter()
        .find(|tag| *tag == name || fold_void_case && tag.eq_ignore_ascii_case(name));
        if let Some(canonical) = void.filter(|_| !token.trim_end().ends_with('/')) {
            if xml.len().saturating_add(insertions.len() + 1) > limit {
                return None;
            }
            if canonical != name {
                if replacements.len() == 8_192 {
                    return None;
                }
                replacements.push((start + 1, canonical));
            }
            insertions.push(end);
        }
        cursor = end + 1;
    }
    if insertions.is_empty() {
        return None;
    }
    let mut repaired = String::with_capacity(xml.len() + insertions.len());
    let mut last = 0;
    let mut replacement_index = 0;
    for at in insertions {
        if let Some(&(start, canonical)) = replacements.get(replacement_index)
            && start < at
        {
            repaired.push_str(&xml[last..start]);
            repaired.push_str(canonical);
            last = start + canonical.len();
            replacement_index += 1;
        }
        repaired.push_str(&xml[last..at]);
        repaired.push('/');
        last = at;
    }
    repaired.push_str(&xml[last..]);
    Some(repaired)
}

/// Recover unambiguous literal ampersands without rewriting valid entities,
/// comments, processing instructions, CDATA or any already-valid XML.
pub(crate) fn recover_html_bare_ampersands(xml: &str, limit: usize) -> Option<String> {
    if xml.len() > limit {
        return None;
    }
    let checked = without_plain_doctype(xml);
    if parse_recovery_xml(&checked).is_ok()
        || checked
            .as_bytes()
            .windows(9)
            .any(|w| w.eq_ignore_ascii_case(b"<!doctype"))
        || checked
            .as_bytes()
            .windows(8)
            .any(|w| w.eq_ignore_ascii_case(b"<!entity"))
    {
        return None;
    }
    let bytes = xml.as_bytes();
    let mut cursor = 0;
    let mut insertions = Vec::new();
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"<!--") {
            cursor += 4 + xml[cursor + 4..].find("-->")? + 3;
        } else if bytes[cursor..].starts_with(b"<![CDATA[") {
            cursor += 9 + xml[cursor + 9..].find("]]>")? + 3;
        } else if bytes[cursor..].starts_with(b"<?") {
            cursor += 2 + xml[cursor + 2..].find("?>")? + 2;
        } else if bytes[cursor] == b'&' {
            if bytes.get(cursor + 1) == Some(&b'#') {
                cursor += 1;
                continue;
            }
            let start = cursor + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric()
                    || matches!(bytes[end], b'_' | b'-' | b'.' | b':'))
            {
                end += 1;
                if end - start > 128 {
                    return None;
                }
            }
            if end > start && bytes.get(end) == Some(&b';') {
                cursor = end + 1;
                continue;
            }
            // HTML permits some named references without a semicolon. Do not
            // guess whether those sequences are intended as text or entities.
            for boundary in start + 1..=end {
                if html5ever::data::NAMED_ENTITIES
                    .get(&xml[start..boundary])
                    .is_some_and(|&(first, _)| first != 0)
                {
                    return None;
                }
            }
            if xml
                .len()
                .saturating_add((insertions.len() + 1).saturating_mul(4))
                > limit
            {
                return None;
            }
            insertions.push(cursor + 1);
            cursor += 1;
        } else {
            cursor += 1;
        }
    }
    if insertions.is_empty() {
        return None;
    }
    let mut repaired = String::with_capacity(xml.len() + insertions.len() * 4);
    let mut last = 0;
    for at in insertions {
        repaired.push_str(&xml[last..at]);
        repaired.push_str("amp;");
        last = at;
    }
    repaired.push_str(&xml[last..]);
    let repaired = recover_html_names(repaired, limit);
    let checked = without_plain_doctype(&repaired);
    let doc = parse_recovery_xml(&checked).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "html"
        || !matches!(
            root.tag_name().namespace(),
            None | Some("http://www.w3.org/1999/xhtml")
        )
        || !root.children().any(|node| node.has_tag_name("body"))
    {
        return None;
    }
    drop(doc);
    drop(checked);
    Some(repaired)
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
