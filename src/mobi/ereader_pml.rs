//! Bounded first subset of Palm Markup Language for eReader PDB text.
mod navigation;
use super::{MobiError, XHTML_START, cp1252, escape_xml, finish_xhtml};

const MAX_MARKUP: usize = 32_000;

fn tag(output: &mut String, markup: &str, count: &mut usize) -> Result<(), MobiError> {
    *count = count.checked_add(1).ok_or(MobiError::TextTooLarge)?;
    if *count > MAX_MARKUP {
        return Err(MobiError::TextTooLarge);
    }
    output.push_str(markup);
    if output.len() > super::MAX_XHTML_BYTES {
        return Err(MobiError::TextTooLarge);
    }
    Ok(())
}

fn style_tag(style: char, open: bool) -> &'static str {
    match (style, open) {
        ('b' | 'B', true) => "<strong>",
        ('b' | 'B', false) => "</strong>",
        ('i', true) => "<em>",
        ('i', false) => "</em>",
        ('u', true) => "<u>",
        ('u', false) => "</u>",
        ('o', true) => "<s>",
        ('o', false) => "</s>",
        _ => unreachable!(),
    }
}

fn character(output: &mut String, ch: char, visible: &mut bool) -> Result<(), MobiError> {
    if ch.is_control() || matches!(ch, '\u{fffe}' | '\u{ffff}') {
        return Err(MobiError::InvalidText);
    }
    *visible |= !ch.is_whitespace();
    let mut encoded = [0; 4];
    escape_xml(output, ch.encode_utf8(&mut encoded))
}

/// Unsupported or ambiguous PML is rejected, never silently omitted.
pub(super) fn xhtml(text: &str) -> Result<String, MobiError> {
    let normalized = text.replace("\r\n", "\n");
    let mut output = String::from(XHTML_START);
    let mut styles = Vec::new();
    let mut paragraph = false;
    let mut visible = false;
    let mut markup = 0usize;
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
        if paragraph {
            tag(&mut output, "<br />", &mut markup)?;
        } else {
            tag(&mut output, "<p>", &mut markup)?;
            for &style in &styles {
                tag(&mut output, style_tag(style, true), &mut markup)?;
            }
            paragraph = true;
        }
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                if ch == '\t' {
                    escape_xml(&mut output, "\t")?;
                } else {
                    character(&mut output, ch, &mut visible)?;
                }
                continue;
            }
            match chars.next().ok_or(MobiError::InvalidText)? {
                '\\' => character(&mut output, '\\', &mut visible)?,
                '-' => character(&mut output, '\u{ad}', &mut visible)?,
                'a' => {
                    let mut value = 0u32;
                    for _ in 0..3 {
                        let digit = chars
                            .next()
                            .and_then(|c| c.to_digit(10))
                            .ok_or(MobiError::InvalidText)?;
                        value = value * 10 + digit;
                    }
                    if !(1..=255).contains(&value) {
                        return Err(MobiError::InvalidText);
                    }
                    character(&mut output, cp1252(value as u8), &mut visible)?;
                }
                'U' => {
                    let mut value = 0u32;
                    for _ in 0..4 {
                        let digit = chars
                            .next()
                            .and_then(|c| c.to_digit(16))
                            .ok_or(MobiError::InvalidText)?;
                        value = value * 16 + digit;
                    }
                    let ch = char::from_u32(value).ok_or(MobiError::InvalidText)?;
                    character(&mut output, ch, &mut visible)?;
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
                _ => return Err(MobiError::InvalidText),
            }
        }
    }
    if !styles.is_empty() || !visible {
        return Err(MobiError::InvalidText);
    }
    if paragraph {
        tag(&mut output, "</p>", &mut markup)?;
    }
    finish_xhtml(output)
}

/// Used only after the original PML subset rejects a document.
pub(super) fn with_navigation(
    text: &str,
) -> Result<(String, Vec<(String, usize, String)>), MobiError> {
    navigation::parse(text, None)
}

/// Used only for previously rejected eReader files with validated image records.
pub(super) fn with_images(
    text: &str,
    images: &std::collections::BTreeMap<String, String>,
) -> Result<(String, Vec<(String, usize, String)>), MobiError> {
    navigation::parse(text, Some(images))
}
