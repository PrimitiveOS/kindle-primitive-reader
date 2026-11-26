//! Fail-closed visibility proof for an intentionally minimal MS-DOC subset.
//! Only default Normal text with no direct character/paragraph properties is
//! accepted. Complex formatting and inheritance must be handled separately.
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

/// Validate FKP coverage and prove there are no direct modifications. PAPX
/// allows only istd=0 with an empty property list; CHPX only empty runs.
fn property_map(
    doc: &Analyzed,
    range: Range<usize>,
    paragraph: bool,
) -> Result<BTreeSet<usize>, String> {
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
        for j in 0..runs {
            let offset = usize::from(page[base + j * width]) * 2;
            if !paragraph {
                if offset != 0 {
                    return Err("Word has unsupported direct character properties".into());
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
    Ok(pages)
}

pub(super) fn prove(doc: &Analyzed) -> Result<(), String> {
    normal_style(bytes(&doc.table, doc.stylesheet.clone())?)?;
    let chpx = property_map(doc, doc.chpx.clone(), false)?;
    let papx = property_map(doc, doc.papx.clone(), true)?;
    if !chpx.is_disjoint(&papx) {
        return Err("Word character and paragraph property pages overlap".into());
    }
    Ok(())
}
