//! Bounded MS-DOC FIB and piece-table analysis. This deliberately does not
//! expose text to the reader until character and paragraph visibility is proven.
use super::cfb::Compound;
use std::ops::Range;

const MAX_TEXT_UNITS: usize = 4 * 1024 * 1024;
const MAX_PIECES: usize = 8192;

fn short(bytes: &[u8], at: usize) -> Result<u16, String> {
    let data = bytes
        .get(at..at.checked_add(2).ok_or("Word offset overflow")?)
        .ok_or("Truncated Word structure")?;
    Ok(u16::from_le_bytes(
        data.try_into().map_err(|_| "Truncated Word structure")?,
    ))
}
fn long(bytes: &[u8], at: usize) -> Result<u32, String> {
    let data = bytes
        .get(at..at.checked_add(4).ok_or("Word offset overflow")?)
        .ok_or("Truncated Word structure")?;
    Ok(u32::from_le_bytes(
        data.try_into().map_err(|_| "Truncated Word structure")?,
    ))
}
fn slice(bytes: &[u8], at: usize, length: usize) -> Result<&[u8], String> {
    bytes
        .get(at..at.checked_add(length).ok_or("Word range overflow")?)
        .ok_or_else(|| "Word range is outside its stream".into())
}

/// MS-DOC FcCompressed: an eight-bit Unicode value with specified exceptions.
/// This is not a locale-selected code page.
fn compressed_unit(byte: u8) -> u16 {
    match byte {
        0x82 => 0x201a,
        0x83 => 0x0192,
        0x84 => 0x201e,
        0x85 => 0x2026,
        0x86 => 0x2020,
        0x87 => 0x2021,
        0x88 => 0x02c6,
        0x89 => 0x2030,
        0x8a => 0x0160,
        0x8b => 0x2039,
        0x8c => 0x0152,
        0x91 => 0x2018,
        0x92 => 0x2019,
        0x93 => 0x201c,
        0x94 => 0x201d,
        0x95 => 0x2022,
        0x96 => 0x2013,
        0x97 => 0x2014,
        0x98 => 0x02dc,
        0x99 => 0x2122,
        0x9a => 0x0161,
        0x9b => 0x203a,
        0x9c => 0x0153,
        0x9f => 0x0178,
        _ => u16::from(byte),
    }
}

#[derive(Debug)]
pub(super) struct Piece {
    /// Character position interval in UTF-16 code units.
    pub(super) positions: Range<usize>,
    /// Byte interval in the original WordDocument stream.
    pub(super) bytes: Range<usize>,
    pub(super) compressed: bool,
}

pub(super) struct Analyzed {
    pub(super) word: Vec<u8>,
    pub(super) fib_end: usize,
    pub(super) table: Vec<u8>,
    pub(super) stylesheet: Range<usize>,
    pub(super) chpx: Range<usize>,
    pub(super) papx: Range<usize>,
    pub(super) pieces: Vec<Piece>,
    /// Unproven text units are kept private to the importer until visibility
    /// and revision/field/auxiliary content have all been established.
    pub(super) units: Vec<u16>,
}

fn pair(table: &[u8], pairs: &[u8], index: usize) -> Result<Range<usize>, String> {
    let start = long(pairs, index.checked_mul(8).ok_or("FIB index overflow")?)? as usize;
    let length = long(pairs, index * 8 + 4)? as usize;
    slice(table, start, length)?;
    Ok(start..start + length)
}

pub(super) fn analyze(mut compound: Compound) -> Result<Analyzed, String> {
    let word = compound
        .streams
        .remove("WordDocument")
        .ok_or("WordDocument stream is missing")?;
    if word.len() < 32 || short(&word, 0)? != 0xa5ec || short(&word, 2)? != 0x00c1 {
        return Err("Unsupported Word binary version".into());
    }
    let flags = short(&word, 10)?;
    // Templates, glossary documents, pictures and encrypted text need separate
    // handling. No content is exposed by this parser on these paths.
    if flags & (0x0001 | 0x0002 | 0x0008 | 0x0100) != 0 || short(&word, 8)? != 0 {
        return Err("Unsupported or protected Word document".into());
    }
    let table_name = if flags & 0x0200 != 0 {
        "1Table"
    } else {
        "0Table"
    };
    let table = compound
        .streams
        .remove(table_name)
        .ok_or("The selected Word table stream is missing")?;
    let csw = short(&word, 32)? as usize;
    if csw != 14 {
        return Err("Unsupported Word FIB layout".into());
    }
    let lw_count_at = 34usize.checked_add(csw * 2).ok_or("FIB size overflow")?;
    let cslw = short(&word, lw_count_at)? as usize;
    if cslw != 22 {
        return Err("Unsupported Word FIB long fields".into());
    }
    let lw_at = lw_count_at + 2;
    let lw = slice(&word, lw_at, cslw * 4)?;
    let cb_mac = long(lw, 0)? as usize;
    let count = long(lw, 12)? as usize;
    if count == 0
        || count > MAX_TEXT_UNITS
        || cb_mac > word.len()
        || [4, 5, 7, 8, 9, 10]
            .into_iter()
            .any(|i| long(lw, i * 4).ok() != Some(0))
    {
        return Err("Unsupported Word text or auxiliary content".into());
    }
    let pairs_count_at = lw_at + cslw * 4;
    let pair_count = short(&word, pairs_count_at)? as usize;
    if !(34..=183).contains(&pair_count) {
        return Err("Unsupported Word FIB table".into());
    }
    let pairs_at = pairs_count_at + 2;
    let pair_len = pair_count.checked_mul(8).ok_or("FIB size overflow")?;
    let pairs = slice(&word, pairs_at, pair_len)?;
    let extension_at = pairs_at + pair_len;
    // Only the original Word 97 FIB with no version extension is in scope.
    // Later Word 2000–2003 extensions need their own full bounds/proof.
    if short(&word, extension_at)? != 0 {
        return Err("Unsupported extended Word FIB version".into());
    }
    let fib_end = extension_at + 2;
    let stylesheet = pair(&table, pairs, 1)?;
    let chpx = pair(&table, pairs, 12)?;
    let papx = pair(&table, pairs, 13)?;
    if stylesheet.is_empty()
        || chpx.is_empty()
        || papx.is_empty()
        || (16..=20).any(|i| long(pairs, i * 8 + 4).ok() != Some(0))
    {
        return Err("Unsupported Word property or field tables".into());
    }
    let clx = pair(&table, pairs, 33)?;
    let mut tables = [stylesheet.clone(), chpx.clone(), papx.clone(), clx.clone()];
    tables.sort_by_key(|range| range.start);
    if tables
        .windows(2)
        .any(|ranges| ranges[0].end > ranges[1].start)
    {
        return Err("Word structural tables overlap".into());
    }
    let clx = &table[clx];
    if clx.first() != Some(&2) || clx.len() < 5 {
        return Err("Unsupported Word complex properties".into());
    }
    let plc_len = long(clx, 1)? as usize;
    if plc_len.checked_add(5) != Some(clx.len()) || plc_len < 16 || (plc_len - 4) % 12 != 0 {
        return Err("Invalid Word piece table".into());
    }
    let number = (plc_len - 4) / 12;
    if number == 0 || number > MAX_PIECES {
        return Err("Word piece count exceeds its limit".into());
    }
    let plc = &clx[5..];
    let pcd_at = (number + 1)
        .checked_mul(4)
        .ok_or("Word piece count overflow")?;
    if long(plc, 0)? != 0 || long(plc, number * 4)? as usize != count {
        return Err("Invalid Word text range".into());
    }
    let mut pieces = Vec::with_capacity(number);
    let mut units = Vec::with_capacity(count);
    for i in 0..number {
        let begin = long(plc, i * 4)? as usize;
        let end = long(plc, (i + 1) * 4)? as usize;
        if begin >= end || end > count {
            return Err("Invalid Word character positions".into());
        }
        let pcd = pcd_at + i * 8;
        let piece_flags = short(plc, pcd)?;
        if piece_flags & 4 != 0 || short(plc, pcd + 6)? != 0 {
            return Err("Unsupported Word piece properties".into());
        }
        let fc = long(plc, pcd + 2)?;
        if fc & 0x8000_0000 != 0 {
            return Err("Invalid Word piece offset".into());
        }
        let compressed = fc & 0x4000_0000 != 0;
        let stored = (fc & 0x3fff_ffff) as usize;
        if stored % 2 != 0 {
            return Err("Invalid Word piece alignment".into());
        }
        let start = if compressed { stored / 2 } else { stored };
        let bytes_len = (end - begin)
            .checked_mul(if compressed { 1 } else { 2 })
            .ok_or("Word piece size overflow")?;
        let finish = start
            .checked_add(bytes_len)
            .ok_or("Word piece size overflow")?;
        if start < fib_end || finish > cb_mac {
            return Err("Word piece lies outside text storage".into());
        }
        let payload = slice(&word, start, bytes_len)?;
        let first_unit = units.len();
        if compressed {
            units.extend(payload.iter().copied().map(compressed_unit));
        } else {
            units.extend(
                payload
                    .chunks_exact(2)
                    .map(|v| u16::from_le_bytes([v[0], v[1]])),
            );
        }
        if piece_flags & 1 != 0 && units[first_unit..].contains(&13) {
            return Err("Word piece contradicts its paragraph flag".into());
        }
        pieces.push(Piece {
            positions: begin..end,
            bytes: start..finish,
            compressed,
        });
    }
    let mut physical: Vec<_> = pieces.iter().map(|piece| piece.bytes.clone()).collect();
    physical.sort_by_key(|range| range.start);
    if physical
        .windows(2)
        .any(|window| window[0].end > window[1].start)
        || units.len() != count
        || units.last() != Some(&13)
    {
        return Err("Overlapping or incomplete Word text pieces".into());
    }
    for ch in char::decode_utf16(units.iter().copied()) {
        let ch = ch.map_err(|_| "Invalid Word Unicode")?;
        if (ch.is_control() && !matches!(ch, '\t' | '\r' | '\u{b}'))
            || matches!(ch, '\u{fffe}' | '\u{ffff}')
        {
            return Err("Unsupported Word field or control character".into());
        }
    }
    Ok(Analyzed {
        word,
        fib_end,
        table,
        stylesheet,
        chpx,
        papx,
        pieces,
        units,
    })
}
