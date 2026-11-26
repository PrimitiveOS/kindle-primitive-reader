//! Fail-closed candidate for a narrow Word 97 binary text subset.
//! Only MS-CFB/MS-DOC structures established by the independent parser and
//! visibility proof are used; this does not invoke or include Antiword.
mod cfb;
mod pieces;
mod visibility;

pub(crate) const MAX_INPUT: usize = cfb::MAX_FILE;
const MAX_XHTML: usize = 512 * 1024;
const MAX_ELEMENTS: usize = 32_000;

fn push(output: &mut String, text: &str) -> Result<(), String> {
    if output
        .len()
        .checked_add(text.len())
        .is_none_or(|n| n > MAX_XHTML)
    {
        return Err("Word text exceeds its converted size limit".into());
    }
    output.push_str(text);
    Ok(())
}

/// Convert a proven default-style main document to safe XHTML. This candidate
/// is locally routed but has not been validated or released to users.
pub(crate) fn convert(data: &[u8]) -> Result<String, String> {
    let compound = cfb::read(data)?;
    let document = pieces::analyze(compound)?;
    visibility::prove(&document)?;
    // The ordinary reflow source collapses CSS whitespace. Refuse input whose
    // significant spacing cannot be represented faithfully by these paragraphs.
    let mut at_line_start = true;
    let mut previous_space = false;
    for decoded in char::decode_utf16(document.units.iter().copied()) {
        let ch = decoded.map_err(|_| "Invalid Word Unicode")?;
        match ch {
            '\t' => return Err("Word tabs require whitespace-preserving layout".into()),
            ' ' => {
                if at_line_start || previous_space {
                    return Err("Word spaces require whitespace-preserving layout".into());
                }
                previous_space = true;
            }
            '\r' | '\u{b}' => {
                if at_line_start || previous_space {
                    return Err("Word line spacing requires whitespace-preserving layout".into());
                }
                at_line_start = true;
                previous_space = false;
            }
            _ => {
                at_line_start = false;
                previous_space = false;
            }
        }
    }
    let mut output =
        String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body>");
    let mut paragraph = false;
    let mut visible = false;
    let mut elements = 0usize;
    for decoded in char::decode_utf16(document.units.iter().copied()) {
        let ch = decoded.map_err(|_| "Invalid Word Unicode")?;
        match ch {
            '\r' => {
                if paragraph {
                    push(&mut output, "</p>")?;
                    paragraph = false;
                } else {
                    elements = elements
                        .checked_add(2)
                        .ok_or("Word element count overflow")?;
                    push(&mut output, "<p><br /></p>")?;
                }
            }
            '\u{b}' => {
                if !paragraph {
                    elements = elements
                        .checked_add(1)
                        .ok_or("Word element count overflow")?;
                    push(&mut output, "<p>")?;
                    paragraph = true;
                }
                elements = elements
                    .checked_add(1)
                    .ok_or("Word element count overflow")?;
                push(&mut output, "<br />")?;
            }
            _ => {
                if !paragraph {
                    elements = elements
                        .checked_add(1)
                        .ok_or("Word element count overflow")?;
                    push(&mut output, "<p>")?;
                    paragraph = true;
                }
                visible |= !ch.is_whitespace();
                match ch {
                    '&' => push(&mut output, "&amp;")?,
                    '<' => push(&mut output, "&lt;")?,
                    '>' => push(&mut output, "&gt;")?,
                    '"' => push(&mut output, "&quot;")?,
                    _ => {
                        let mut buffer = [0; 4];
                        push(&mut output, ch.encode_utf8(&mut buffer))?;
                    }
                }
            }
        }
        if elements > MAX_ELEMENTS {
            return Err("Word document has too many layout elements".into());
        }
    }
    if !visible || paragraph {
        return Err("Word document has no valid main text".into());
    }
    push(&mut output, "</body></html>")?;
    Ok(output)
}
