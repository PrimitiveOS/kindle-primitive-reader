//! Narrow, fail-closed navigation-bearing PML fallback.
use std::collections::BTreeSet;

use super::{character, style_tag, tag};
use crate::mobi::{MobiError, XHTML_START, cp1252, escape_xml, finish_xhtml};

const MAX_TARGETS: usize = 1_024;
const MAX_NAME: usize = 128;
const MAX_CHAPTER_LABEL: usize = 512;

type Chapter = (String, usize, String);

fn name(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, MobiError> {
    if chars.next() != Some('=') || chars.next() != Some('"') {
        return Err(MobiError::InvalidText);
    }
    let mut result = String::new();
    loop {
        let ch = chars.next().ok_or(MobiError::InvalidText)?;
        if ch == '"' {
            break;
        }
        if result.len() >= MAX_NAME + 1
            || !(ch.is_ascii_alphanumeric() || matches!(ch, '#' | '_' | '-' | '.'))
        {
            return Err(MobiError::InvalidText);
        }
        result.push(ch);
    }
    if result.is_empty() {
        return Err(MobiError::InvalidText);
    }
    Ok(result)
}

fn target(value: &str) -> Result<String, MobiError> {
    if value.is_empty()
        || value.len() > MAX_NAME
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(MobiError::InvalidText);
    }
    Ok(format!("pml-anchor-{value}"))
}

/// Whole-line `\\x...\\x` and `\\Xn...\\Xn` only. Titles with
/// formatting, commands, or embedded line breaks remain unsupported.
fn chapter(line: &str) -> Result<Option<(String, usize, bool)>, MobiError> {
    let line = line.trim();
    let (opening, closing, depth, new_page) = if line.starts_with("\\x") {
        ("\\x", "\\x", 0usize, true)
    } else if line.starts_with("\\X") {
        let level = line
            .as_bytes()
            .get(2)
            .copied()
            .ok_or(MobiError::InvalidText)?;
        if !matches!(level, b'0'..=b'4') {
            return Err(MobiError::InvalidText);
        }
        let token = &line[..3];
        (token, token, (level - b'0') as usize, false)
    } else {
        return Ok(None);
    };
    let title = line
        .strip_prefix(opening)
        .and_then(|s| s.strip_suffix(closing))
        .ok_or(MobiError::InvalidText)?
        .trim();
    if title.is_empty()
        || title.len() > MAX_CHAPTER_LABEL
        || title.contains('\\')
        || title.chars().any(char::is_control)
    {
        return Err(MobiError::InvalidText);
    }
    Ok(Some((title.to_owned(), depth, new_page)))
}

pub(super) fn parse(text: &str) -> Result<(String, Vec<Chapter>), MobiError> {
    let normalized = text.replace("\r\n", "\n");
    let mut output = String::from(XHTML_START);
    let mut styles = Vec::new();
    let mut anchors = BTreeSet::new();
    let mut references = BTreeSet::new();
    let mut chapters = Vec::new();
    let mut paragraph = false;
    let mut pending_page = false;
    let mut visible = false;
    let mut markup = 0usize;
    let mut saw_navigation = false;
    for line in normalized.split(['\n', '\r']) {
        if line.trim().is_empty() {
            if paragraph {
                for &style in styles.iter().rev() {
                    tag(&mut output, style_tag(style, false), &mut markup)?;
                }
                tag(&mut output, "</p>", &mut markup)?;
                paragraph = false;
            }
            continue;
        }
        if line.trim() == "\\p" {
            if !styles.is_empty() || pending_page {
                return Err(MobiError::InvalidText);
            }
            if paragraph {
                tag(&mut output, "</p>", &mut markup)?;
                paragraph = false;
            }
            pending_page = true;
            saw_navigation = true;
            continue;
        }
        if let Some((label, depth, new_page)) = chapter(line)? {
            if !styles.is_empty() || chapters.len() >= MAX_TARGETS {
                return Err(MobiError::InvalidText);
            }
            if paragraph {
                tag(&mut output, "</p>", &mut markup)?;
                paragraph = false;
            }
            let id = format!("pml-chapter-{}", chapters.len() + 1);
            let level = depth + 1;
            let page = if new_page || pending_page {
                "always"
            } else {
                "auto"
            };
            tag(
                &mut output,
                &format!("<h{level} id=\"{id}\" style=\"page-break-before: {page}\">"),
                &mut markup,
            )?;
            escape_xml(&mut output, &label)?;
            tag(&mut output, &format!("</h{level}>"), &mut markup)?;
            chapters.push((label, depth, id));
            pending_page = false;
            visible = true;
            saw_navigation = true;
            continue;
        }
        let page_applied = pending_page;
        if paragraph {
            tag(&mut output, "<br />", &mut markup)?;
        } else {
            tag(
                &mut output,
                if pending_page {
                    "<p style=\"page-break-before: always\">"
                } else {
                    "<p>"
                },
                &mut markup,
            )?;
            pending_page = false;
            for &style in &styles {
                tag(&mut output, style_tag(style, true), &mut markup)?;
            }
            paragraph = true;
        }
        let mut active_link = false;
        let mut line_visible = false;
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                if ch == '\t' {
                    escape_xml(&mut output, "\t")?;
                } else {
                    character(&mut output, ch, &mut line_visible)?;
                }
                continue;
            }
            match chars.next().ok_or(MobiError::InvalidText)? {
                '\\' => character(&mut output, '\\', &mut line_visible)?,
                '-' => character(&mut output, '\u{ad}', &mut line_visible)?,
                'a' => {
                    let mut value = 0u32;
                    for _ in 0..3 {
                        value = value * 10
                            + chars
                                .next()
                                .and_then(|c| c.to_digit(10))
                                .ok_or(MobiError::InvalidText)?;
                    }
                    if !(1..=255).contains(&value) {
                        return Err(MobiError::InvalidText);
                    }
                    character(&mut output, cp1252(value as u8), &mut line_visible)?;
                }
                'U' => {
                    let mut value = 0u32;
                    for _ in 0..4 {
                        value = value * 16
                            + chars
                                .next()
                                .and_then(|c| c.to_digit(16))
                                .ok_or(MobiError::InvalidText)?;
                    }
                    character(
                        &mut output,
                        char::from_u32(value).ok_or(MobiError::InvalidText)?,
                        &mut line_visible,
                    )?;
                }
                style @ ('b' | 'B' | 'i' | 'u' | 'o') => {
                    if styles.last() == Some(&style) {
                        styles.pop();
                        tag(&mut output, style_tag(style, false), &mut markup)?;
                    } else if styles.contains(&style) {
                        return Err(MobiError::InvalidText);
                    } else {
                        styles.push(style);
                        tag(&mut output, style_tag(style, true), &mut markup)?;
                    }
                }
                'Q' => {
                    if active_link || !styles.is_empty() || anchors.len() >= MAX_TARGETS {
                        return Err(MobiError::InvalidText);
                    }
                    let id = target(&name(&mut chars)?)?;
                    if !anchors.insert(id.clone()) {
                        return Err(MobiError::InvalidText);
                    }
                    tag(&mut output, &format!("<a id=\"{id}\"></a>"), &mut markup)?;
                    saw_navigation = true;
                }
                'q' => {
                    if !styles.is_empty() {
                        return Err(MobiError::InvalidText);
                    }
                    if active_link {
                        tag(&mut output, "</a>", &mut markup)?;
                        active_link = false;
                    } else {
                        let value = name(&mut chars)?;
                        let id = target(value.strip_prefix('#').ok_or(MobiError::InvalidText)?)?;
                        if references.len() >= MAX_TARGETS && !references.contains(&id) {
                            return Err(MobiError::InvalidText);
                        }
                        references.insert(id.clone());
                        tag(&mut output, &format!("<a href=\"#{id}\">"), &mut markup)?;
                        active_link = true;
                    }
                    saw_navigation = true;
                }
                _ => return Err(MobiError::InvalidText),
            }
        }
        if active_link || (page_applied && !line_visible) {
            return Err(MobiError::InvalidText);
        }
        visible |= line_visible;
    }
    if pending_page
        || !styles.is_empty()
        || !visible
        || !saw_navigation
        || !references.is_subset(&anchors)
    {
        return Err(MobiError::InvalidText);
    }
    if paragraph {
        tag(&mut output, "</p>", &mut markup)?;
    }
    Ok((finish_xhtml(output)?, chapters))
}
