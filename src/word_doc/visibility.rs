//! Fail-closed visibility proof for an intentionally minimal MS-DOC subset.
//! Only proven default Normal text and explicitly bounded direct emphasis
//! properties are accepted. Complex formatting and inheritance remain unsupported.
use super::pieces::Analyzed;
use std::collections::BTreeSet;
use std::ops::Range;

fn u16le(b: &[u8], at: usize) -> Result<u16, String> {
    let end = at.checked_add(2).ok_or("Word property offset overflow")?;
    Ok(u16::from_le_bytes(
        b.get(at..end)
            .ok_or("Truncated Word properties")?
            .try_into()
            .map_err(|_| "Truncated Word properties")?,
    ))
}
fn u32le(b: &[u8], at: usize) -> Result<u32, String> {
    let end = at.checked_add(4).ok_or("Word property offset overflow")?;
    Ok(u32::from_le_bytes(
        b.get(at..end)
            .ok_or("Truncated Word properties")?
            .try_into()
            .map_err(|_| "Truncated Word properties")?,
    ))
}
fn bytes(b: &[u8], range: Range<usize>) -> Result<&[u8], String> {
    b.get(range)
        .ok_or_else(|| "Word property range is outside its stream".into())
}

/// Prove both Normal and the default character style have no inherited or
/// direct properties. Other styles must not be referenced by the FKP proof.
fn normal_style(sheet: &[u8]) -> Result<(), String> {
    let header = usize::from(u16le(sheet, 0)?);
    if header != 18 || sheet.len() < header + 2 {
        return Err("Unsupported Word stylesheet header".into());
    }
    let count = usize::from(u16le(sheet, 2)?);
    if !(15..=1024).contains(&count)
        || u16le(sheet, 4)? != 10
        || u16le(sheet, 6)? != 1
        || u16le(sheet, 10)? != 15
    {
        return Err("Unsupported Word stylesheet defaults".into());
    }
    let mut at = 2 + header;
    for index in 0..count {
        let len = usize::from(u16le(sheet, at)?);
        at = at.checked_add(2).ok_or("Word stylesheet overflow")?;
        let end = at.checked_add(len).ok_or("Word stylesheet overflow")?;
        let style = bytes(sheet, at..end)?;
        if index == 0 {
            if len < 16
                || u16le(style, 0)? & 0x0fff != 0
                || u16le(style, 2)? != 0xfff1
                || u16le(style, 4)? & 0x000f != 2
                || u16le(style, 6)? as usize != len
                || u16le(style, 8)? != 0
            {
                return Err("Unsupported Word Normal style".into());
            }
            let names = usize::from(u16le(style, 10)?);
            if names == 0 || names > 255 {
                return Err("Invalid Word Normal style name".into());
            }
            let name_end = 12usize
                .checked_add(names.checked_mul(2).ok_or("Word style name overflow")?)
                .ok_or("Word style name overflow")?;
            let raw = bytes(style, 12..name_end)?;
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect();
            if units.contains(&0)
                || String::from_utf16(&units).is_err()
                || u16le(style, name_end)? != 0
            {
                return Err("Invalid Word style name".into());
            }
            let papx_at = name_end + 2;
            if u16le(style, papx_at)? != 2
                || u16le(style, papx_at + 2)? != 0
                || u16le(style, papx_at + 4)? != 0
                || papx_at + 6 != style.len()
            {
                return Err("Word Normal style has unsupported properties".into());
            }
        }
        if index == 10 {
            // The default character style is istd 10 even if no explicit
            // sprmCIstd occurs. Do not assume an empty or inherited style is safe.
            if len < 14
                || u16le(style, 0)? & 0x0fff != 65
                || u16le(style, 2)? != 0xfff2
                || u16le(style, 4)? & 0x000f != 1
                || u16le(style, 6)? as usize != len
                || u16le(style, 8)? != 0
            {
                return Err("Unsupported default Word character style".into());
            }
            let names = usize::from(u16le(style, 10)?);
            if names == 0 || names > 255 {
                return Err("Invalid default Word character style name".into());
            }
            let name_end = 12usize
                .checked_add(names.checked_mul(2).ok_or("Word style name overflow")?)
                .ok_or("Word style name overflow")?;
            let raw = bytes(style, 12..name_end)?;
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect();
            if units.contains(&0)
                || String::from_utf16(&units).is_err()
                || u16le(style, name_end)? != 0
                || u16le(style, name_end + 2)? != 0
                || name_end + 4 != style.len()
            {
                return Err("Word default character style has unsupported properties".into());
            }
        }
        at = end;
    }
    if at != sheet.len() {
        return Err("Unexpected Word stylesheet data".into());
    }
    Ok(())
}

const MAX_STYLE_RUNS: usize = 32_768;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ScriptPosition {
    #[default]
    Normal,
    Superscript,
    Subscript,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Emphasis {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) position: ScriptPosition,
}

#[derive(Clone, Debug)]
pub(super) struct StyleSpan {
    pub(super) positions: Range<usize>,
    pub(super) style: Emphasis,
}

#[derive(Clone)]
struct PhysicalRun {
    bytes: Range<usize>,
    style: Emphasis,
}

fn character_properties(group: &[u8]) -> Result<Emphasis, String> {
    if group.len() % 3 != 0 {
        return Err("Malformed Word character properties".into());
    }
    let mut style = Emphasis::default();
    let mut seen_bold = false;
    let mut seen_italic = false;
    let mut seen_underline = false;
    let mut seen_strike = false;
    let mut seen_position = false;
    for property in group.chunks_exact(3) {
        let code = u16::from_le_bytes([property[0], property[1]]);
        match code {
            0x0835 if !seen_bold => {
                style.bold = toggle(property[2])?;
                seen_bold = true;
            }
            0x0836 if !seen_italic => {
                style.italic = toggle(property[2])?;
                seen_italic = true;
            }
            0x0837 if !seen_strike => {
                style.strike = toggle(property[2])?;
                seen_strike = true;
            }
            // Kul is an enumeration, not a toggle operand. Preserve only
            // no underline and a normal single underline; all other styles
            // require rendering behavior that this adapter cannot prove.
            0x2a3e if !seen_underline => {
                style.underline = match property[2] {
                    0x00 => false,
                    0x01 => true,
                    _ => return Err("Unsupported Word underline style".into()),
                };
                seen_underline = true;
            }
            // Iss is an enumeration; it is not a boolean or toggle operand.
            0x2a48 if !seen_position => {
                style.position = match property[2] {
                    0 => ScriptPosition::Normal,
                    1 => ScriptPosition::Superscript,
                    2 => ScriptPosition::Subscript,
                    _ => return Err("Unsupported Word script position".into()),
                };
                seen_position = true;
            }
            _ => return Err("Unsupported or repeated Word character property".into()),
        }
    }
    Ok(style)
}

fn toggle(operand: u8) -> Result<bool, String> {
    // Both proven base styles have false bold/italic/strike. 0x80 inherits
    // that value; 0x81 applies its opposite.
    match operand {
        0x00 | 0x80 => Ok(false),
        0x01 | 0x81 => Ok(true),
        _ => Err("Unsupported Word toggle operand".into()),
    }
}

/// Validate FKP coverage. PAPX allows only istd=0 with no direct properties;
/// CHPX allows only explicitly decoded emphasis and script position.
fn property_map(
    doc: &Analyzed,
    range: Range<usize>,
    paragraph: bool,
) -> Result<(BTreeSet<usize>, Vec<PhysicalRun>), String> {
    let plc = bytes(&doc.table, range)?;
    if plc.len() < 12 || (plc.len() - 4) % 8 != 0 {
        return Err("Invalid Word property index".into());
    }
    let count = (plc.len() - 4) / 8;
    if count == 0 || count > 4096 {
        return Err("Word property index exceeds its limit".into());
    }
    let pn_base = (count + 1) * 4;
    let mut boundaries = Vec::with_capacity(count + 1);
    for i in 0..=count {
        boundaries.push(u32le(plc, i * 4)? as usize);
    }
    if boundaries.windows(2).any(|p| p[0] >= p[1]) {
        return Err("Unsorted Word property boundaries".into());
    }
    for piece in &doc.pieces {
        if piece.bytes.start < boundaries[0] || piece.bytes.end > boundaries[count] {
            return Err("Word property coverage is incomplete".into());
        }
    }
    let mut pages = BTreeSet::new();
    let mut styled = Vec::new();
    for i in 0..count {
        let pn = u32le(plc, pn_base + i * 4)?;
        if pn >> 22 != 0 {
            return Err("Unsupported Word property page number".into());
        }
        let start = (pn as usize)
            .checked_mul(512)
            .ok_or("Word property page overflow")?;
        let end = start
            .checked_add(512)
            .ok_or("Word property page overflow")?;
        if start < doc.fib_end
            || !pages.insert(start)
            || doc
                .pieces
                .iter()
                .any(|piece| piece.bytes.start < end && start < piece.bytes.end)
        {
            return Err("Word property pages overlap text or each other".into());
        }
        let page = bytes(&doc.word, start..end)?;
        let runs = usize::from(page[511]);
        let width = if paragraph { 13 } else { 1 };
        let records = (runs + 1)
            .checked_mul(4)
            .and_then(|v| v.checked_add(runs * width))
            .ok_or("Word property page overflow")?;
        if runs == 0 || records > 511 {
            return Err("Invalid Word property page".into());
        }
        let mut offsets = Vec::with_capacity(runs + 1);
        for j in 0..=runs {
            offsets.push(u32le(page, j * 4)? as usize);
        }
        if offsets.windows(2).any(|p| p[0] >= p[1])
            || offsets[0] > boundaries[i]
            || offsets[runs] < boundaries[i + 1]
        {
            return Err("Word property page coverage is invalid".into());
        }
        let base = (runs + 1) * 4;
        let mut groups: Vec<Range<usize>> = Vec::new();
        for j in 0..runs {
            let offset = usize::from(page[base + j * width]) * 2;
            if !paragraph {
                let style = if offset == 0 {
                    Emphasis::default()
                } else {
                    if offset < records || offset >= 511 {
                        return Err("Word character property offset is invalid".into());
                    }
                    let end = offset
                        .checked_add(1 + usize::from(page[offset]))
                        .filter(|&end| end <= 511)
                        .ok_or("Word character property is truncated")?;
                    let group = offset..end;
                    if groups
                        .iter()
                        .any(|old| old != &group && old.start < group.end && group.start < old.end)
                    {
                        return Err("Word character property groups overlap".into());
                    }
                    groups.push(group.clone());
                    character_properties(&page[group.start + 1..group.end])?
                };
                if style != Emphasis::default() {
                    let start = offsets[j].max(boundaries[i]);
                    let end = offsets[j + 1].min(boundaries[i + 1]);
                    if start < end {
                        if styled.len() >= MAX_STYLE_RUNS {
                            return Err("Word has too many styled runs".into());
                        }
                        styled.push(PhysicalRun {
                            bytes: start..end,
                            style,
                        });
                    }
                }
            } else {
                if offset < records
                    || offset.checked_add(4).is_none_or(|end| end > 511)
                    || page[offset] != 0
                    || page[offset + 1] != 1
                    || u16le(page, offset + 2)? != 0
                {
                    return Err("Word has unsupported direct paragraph properties".into());
                }
            }
        }
    }
    Ok((pages, styled))
}

pub(super) fn prove(doc: &Analyzed) -> Result<Vec<StyleSpan>, String> {
    normal_style(bytes(&doc.table, doc.stylesheet.clone())?)?;
    let (chpx, runs) = property_map(doc, doc.chpx.clone(), false)?;
    let (papx, _) = property_map(doc, doc.papx.clone(), true)?;
    if !chpx.is_disjoint(&papx) {
        return Err("Word character and paragraph property pages overlap".into());
    }
    let mut spans = Vec::new();
    for piece in &doc.pieces {
        let width = if piece.compressed { 1 } else { 2 };
        let first = runs.partition_point(|run| run.bytes.end <= piece.bytes.start);
        for run in runs[first..]
            .iter()
            .take_while(|run| run.bytes.start < piece.bytes.end)
        {
            let start = run.bytes.start.max(piece.bytes.start) - piece.bytes.start;
            let end = run.bytes.end.min(piece.bytes.end) - piece.bytes.start;
            if start % width != 0 || end % width != 0 {
                return Err("Word style splits a text code unit".into());
            }
            let start = piece.positions.start + start / width;
            let end = piece.positions.start + end / width;
            // A formatting edge cannot split a UTF-16 surrogate pair.
            for boundary in [start, end] {
                if boundary > 0
                    && boundary < doc.units.len()
                    && (0xd800..=0xdbff).contains(&doc.units[boundary - 1])
                    && (0xdc00..=0xdfff).contains(&doc.units[boundary])
                {
                    return Err("Word style splits a Unicode character".into());
                }
            }
            if start < end {
                if spans.len() >= MAX_STYLE_RUNS {
                    return Err("Word has too many styled spans".into());
                }
                spans.push(StyleSpan {
                    positions: start..end,
                    style: run.style,
                });
            }
        }
    }
    spans.sort_unstable_by_key(|span| span.positions.start);
    if spans
        .windows(2)
        .any(|items| items[0].positions.end > items[1].positions.start)
    {
        return Err("Word styled spans overlap".into());
    }
    Ok(spans)
}
