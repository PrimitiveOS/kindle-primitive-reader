//! Bounded WOFF 1 to sfnt reconstruction for embedded publication fonts.
//!
//! WOFF 2 uses different transforms and is intentionally not handled here.
use flate2::{Decompress, FlushDecompress, Status};
use std::ops::Range;

use crate::Result;

const HEADER: usize = 44;
const ENTRY: usize = 20;
// The sfnt searchRange and rangeShift fields are u16 quantities.
const MAX_TABLES: usize = 4095;

#[derive(Debug)]
struct Table {
    tag: [u8; 4],
    source: Range<usize>,
    original: usize,
    checksum: u32,
}

fn number(data: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_be_bytes(data.get(offset..offset + 4)?.try_into().ok()?) as usize)
}

fn padded(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3)
}

fn range(data: &[u8], offset: usize, length: usize, floor: usize) -> Option<Range<usize>> {
    let end = offset.checked_add(length)?;
    (offset >= floor && offset % 4 == 0 && end <= data.len()).then_some(offset..end)
}

/// Reconstruct an sfnt in memory with at most `limit` output bytes.
/// The caller also bounds the compressed publication resource before calling.
pub(crate) fn decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let invalid = || "Invalid or unsupported WOFF font";
    if data.len() < HEADER || &data[..4] != b"wOFF" || number(data, 8) != Some(data.len()) {
        return Err(invalid().into());
    }
    let flavor = &data[4..8];
    if flavor != b"\0\x01\0\0" && flavor != b"OTTO" {
        return Err(invalid().into());
    }
    let count = u16::from_be_bytes(data[12..14].try_into()?) as usize;
    if count == 0 || count > MAX_TABLES || data[14..16] != [0, 0] {
        return Err(invalid().into());
    }
    let directory_end = HEADER
        .checked_add(ENTRY.checked_mul(count).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    if directory_end > data.len() {
        return Err(invalid().into());
    }
    let declared = number(data, 16).ok_or_else(invalid)?;
    let sfnt_header = 12 + count * 16;
    if declared < sfnt_header || declared > limit {
        return Err(invalid().into());
    }

    let mut occupied = Vec::with_capacity(count);
    let mut metadata = None;
    let mut private = None;
    // Metadata and private data are never decompressed by Reader, but must not
    // alias font tables or escape the publication resource.
    for (offset_at, size_at, original_at) in [(24, 28, Some(32)), (36, 40, None)] {
        let offset = number(data, offset_at).ok_or_else(invalid)?;
        let size = number(data, size_at).ok_or_else(invalid)?;
        let original = match original_at {
            Some(at) => Some(number(data, at).ok_or_else(invalid)?),
            None => None,
        };
        if offset == 0 {
            if size != 0 || original.is_some_and(|value| value != 0) {
                return Err(invalid().into());
            }
        } else {
            // The metadata payload is informational. Its declared expanded
            // length may be invalid and must not prevent font loading.
            if size == 0 {
                return Err(invalid().into());
            }
            let block = range(data, offset, size, directory_end).ok_or_else(invalid)?;
            if original_at.is_some() {
                metadata = Some(block);
            } else {
                private = Some(block);
            }
        }
    }

    let mut tables = Vec::with_capacity(count);
    let mut expected = sfnt_header;
    let mut previous = None;
    for index in 0..count {
        let at = HEADER + index * ENTRY;
        let tag: [u8; 4] = data[at..at + 4].try_into()?;
        if previous.is_some_and(|value| value >= tag) {
            return Err(invalid().into());
        }
        previous = Some(tag);
        let offset = number(data, at + 4).ok_or_else(invalid)?;
        let compressed = number(data, at + 8).ok_or_else(invalid)?;
        let original = number(data, at + 12).ok_or_else(invalid)?;
        let checksum = number(data, at + 16).ok_or_else(invalid)? as u32;
        if compressed > original || (original > 0 && compressed == 0) {
            return Err(invalid().into());
        }
        let source = range(data, offset, compressed, directory_end).ok_or_else(invalid)?;
        if !source.is_empty() {
            occupied.push(source.clone());
        }
        expected = expected
            .checked_add(padded(original).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        if expected > declared {
            return Err(invalid().into());
        }
        tables.push(Table {
            tag,
            source,
            original,
            checksum,
        });
    }
    if expected != declared {
        return Err(invalid().into());
    }
    occupied.sort_unstable_by_key(|item| item.start);
    // WOFF has no unreferenced data. Tables are contiguous after the
    // directory, with only the zero padding needed for four-byte alignment.
    let mut end = directory_end;
    for table in &occupied {
        if table.start != end {
            return Err(invalid().into());
        }
        end = padded(table.end).ok_or_else(invalid)?;
        if data
            .get(table.end..end)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        {
            return Err(invalid().into());
        }
    }
    if let Some(block) = &metadata {
        if block.start != end {
            return Err(invalid().into());
        }
        end = block.end;
    }
    if let Some(block) = &private {
        let aligned = padded(end).ok_or_else(invalid)?;
        if block.start != aligned
            || data
                .get(end..aligned)
                .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
            || block.end != data.len()
        {
            return Err(invalid().into());
        }
    } else if metadata.is_some() {
        // Metadata at EOF need not be padded, but aligned zero padding is
        // harmless. Metadata content itself must not affect font loading.
        let aligned = padded(end).ok_or_else(invalid)?;
        if data.len() != end && (data.len() != aligned || data[end..].iter().any(|byte| *byte != 0))
        {
            return Err(invalid().into());
        }
    } else if end != data.len() {
        return Err(invalid().into());
    }

    let mut result = vec![0; declared];
    result[..4].copy_from_slice(flavor);
    result[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    let power = 1usize << (usize::BITS - 1 - count.leading_zeros());
    result[6..8].copy_from_slice(&((power * 16) as u16).to_be_bytes());
    result[8..10].copy_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    result[10..12].copy_from_slice(&((count * 16 - power * 16) as u16).to_be_bytes());
    let mut destination = sfnt_header;
    let mut checksum_adjustment = None;
    for (index, table) in tables.iter().enumerate() {
        let at = 12 + index * 16;
        result[at..at + 4].copy_from_slice(&table.tag);
        result[at + 4..at + 8].copy_from_slice(&table.checksum.to_be_bytes());
        result[at + 8..at + 12].copy_from_slice(&(destination as u32).to_be_bytes());
        result[at + 12..at + 16].copy_from_slice(&(table.original as u32).to_be_bytes());
        if &table.tag == b"head" {
            if table.original < 12 {
                return Err(invalid().into());
            }
            checksum_adjustment = Some(destination + 8);
        }
        let encoded = &data[table.source.clone()];
        if encoded.len() == table.original {
            result[destination..destination + table.original].copy_from_slice(encoded);
        } else {
            // One extra output byte detects a stream that expands beyond its claim.
            let mut decoded = vec![0; table.original + 1];
            let mut stream = Decompress::new(true);
            loop {
                let before_in = stream.total_in() as usize;
                let before_out = stream.total_out() as usize;
                let status = stream.decompress(
                    &encoded[before_in..],
                    &mut decoded[before_out..],
                    FlushDecompress::Finish,
                )?;
                if status == Status::StreamEnd {
                    if stream.total_in() as usize != encoded.len()
                        || stream.total_out() as usize != table.original
                    {
                        return Err(invalid().into());
                    }
                    break;
                }
                if (stream.total_in() as usize, stream.total_out() as usize)
                    == (before_in, before_out)
                    || stream.total_out() as usize > table.original
                {
                    return Err(invalid().into());
                }
            }
            result[destination..destination + table.original]
                .copy_from_slice(&decoded[..table.original]);
        }
        destination += padded(table.original).ok_or_else(invalid)?;
    }
    // The reconstructed tables may occupy different offsets than the original
    // sfnt. Recompute the whole-font checkSumAdjustment for that directory.
    let adjustment = checksum_adjustment.ok_or_else(invalid)?;
    result[adjustment..adjustment + 4].fill(0);
    let sum = result.chunks_exact(4).fold(0u32, |sum, chunk| {
        sum.wrapping_add(u32::from_be_bytes(chunk.try_into().unwrap()))
    });
    result[adjustment..adjustment + 4]
        .copy_from_slice(&0xB1B0_AFBAu32.wrapping_sub(sum).to_be_bytes());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn specimen() -> Vec<u8> {
        let file = std::fs::File::open(crate::test_fixture("publisher-woff-woff.epub")).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut font = Vec::new();
        archive
            .by_name("EPUB/fonts/Silkscreen-Regular.woff")
            .unwrap()
            .read_to_end(&mut font)
            .unwrap();
        font
    }

    fn u32_at(data: &[u8], at: usize) -> usize {
        number(data, at).unwrap()
    }

    fn put32(data: &mut [u8], at: usize, value: usize) {
        data[at..at + 4].copy_from_slice(&(value as u32).to_be_bytes());
    }

    fn sfnt_tables(data: &[u8]) -> Vec<([u8; 4], u32, Vec<u8>)> {
        let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
        (0..count)
            .map(|index| {
                let at = 12 + 16 * index;
                let tag = data[at..at + 4].try_into().unwrap();
                let checksum = u32::from_be_bytes(data[at + 4..at + 8].try_into().unwrap());
                let offset = u32_at(data, at + 8);
                let length = u32_at(data, at + 12);
                (tag, checksum, data[offset..offset + length].to_vec())
            })
            .collect()
    }

    #[test]
    fn woff_reconstructs_every_licensed_font_table_under_the_output_limit() {
        let original = std::fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
        let packed = specimen();
        let declared = u32_at(&packed, 16);
        let unpacked = decode(&packed, declared).unwrap();
        assert_eq!(unpacked.len(), declared);
        let mut expected = sfnt_tables(&original);
        let mut actual = sfnt_tables(&unpacked);
        for (tag, _, data) in expected.iter_mut().chain(actual.iter_mut()) {
            if tag == b"head" {
                data[8..12].fill(0);
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(
            unpacked.chunks_exact(4).fold(0u32, |sum, chunk| {
                sum.wrapping_add(u32::from_be_bytes(chunk.try_into().unwrap()))
            }),
            0xB1B0_AFBA
        );
        assert!(decode(&packed, declared - 1).is_err());
    }

    #[test]
    fn woff_rejects_bad_headers_directories_overlaps_and_expansion_claims() {
        let packed = specimen();
        let limit = u32_at(&packed, 16);
        let mut bad = packed.clone();
        bad[3] = b'2';
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        bad[14] = 1;
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        put32(&mut bad, 16, limit + 4);
        assert!(decode(&bad, limit + 4).is_err());
        bad = packed.clone();
        bad[64..68].copy_from_slice(&packed[44..48]);
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        put32(&mut bad, 68, u32_at(&packed, 48));
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        put32(&mut bad, 56, u32::MAX as usize);
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        put32(&mut bad, 24, u32_at(&packed, 48));
        put32(&mut bad, 28, 4);
        put32(&mut bad, 32, 4);
        assert!(decode(&bad, limit).is_err());
        bad = packed.clone();
        bad.pop();
        let new_len = bad.len();
        put32(&mut bad, 8, new_len);
        assert!(decode(&bad, limit).is_err());
    }

    #[test]
    fn woff_rejects_corrupt_zlib_and_streams_with_trailing_bytes() {
        let packed = specimen();
        let limit = u32_at(&packed, 16);
        let compressed = u32_at(&packed, 52);
        let original = u32_at(&packed, 56);
        let source = u32_at(&packed, 48);
        assert!(compressed < original && compressed % 4 != 0);
        let mut bad = packed.clone();
        bad[source + compressed - 1] ^= 1;
        assert!(decode(&bad, limit).is_err());
        bad = packed;
        put32(&mut bad, 52, compressed + 1);
        assert!(decode(&bad, limit).is_err());
    }

    #[test]
    fn woff_rejects_gaps_extraneous_bytes_and_nonzero_padding() {
        let packed = specimen();
        let limit = u32_at(&packed, 16);
        let mut trailing = packed.clone();
        trailing.push(0x42);
        let trailing_len = trailing.len();
        put32(&mut trailing, 8, trailing_len);
        assert!(decode(&trailing, limit).is_err());

        // A gap before all of the font tables remains nonoverlapping and
        // in bounds, but is not permitted in a WOFF container.
        let count = u16::from_be_bytes(packed[12..14].try_into().unwrap()) as usize;
        let directory_end = HEADER + ENTRY * count;
        let mut gap = packed.clone();
        gap.splice(directory_end..directory_end, [0u8; 4]);
        for index in 0..count {
            let offset_at = HEADER + index * ENTRY + 4;
            put32(&mut gap, offset_at, u32_at(&packed, offset_at) + 4);
        }
        let gap_len = gap.len();
        put32(&mut gap, 8, gap_len);
        assert!(decode(&gap, limit).is_err());

        let mut nonzero = packed.clone();
        let entry = (0..count)
            .map(|index| HEADER + index * ENTRY)
            .find(|&at| u32_at(&packed, at + 8) % 4 != 0)
            .unwrap();
        let padding = u32_at(&packed, entry + 4) + u32_at(&packed, entry + 8);
        assert_eq!(packed[padding], 0);
        nonzero[padding] = 1;
        assert!(decode(&nonzero, limit).is_err());
    }

    #[test]
    fn woff_validates_optional_block_order_and_alignment() {
        let packed = specimen();
        let limit = u32_at(&packed, 16);
        let expected = decode(&packed, limit).unwrap();
        let mut missing = packed.clone();
        put32(&mut missing, 32, 1);
        assert!(decode(&missing, limit).is_err());

        // Invalid metadata is informational and must not affect loading.
        let mut metadata = packed.clone();
        let meta_at = metadata.len();
        metadata.extend_from_slice(b"abc");
        put32(&mut metadata, 24, meta_at);
        put32(&mut metadata, 28, 3);
        put32(&mut metadata, 32, 12);
        let metadata_len = metadata.len();
        put32(&mut metadata, 8, metadata_len);
        assert_eq!(decode(&metadata, limit).unwrap(), expected);
        let mut invalid_metadata = metadata.clone();
        put32(&mut invalid_metadata, 32, 0);
        assert_eq!(decode(&invalid_metadata, limit).unwrap(), expected);
        put32(&mut invalid_metadata, 32, u32::MAX as usize);
        assert_eq!(decode(&invalid_metadata, limit).unwrap(), expected);
        let mut metadata_padded = metadata.clone();
        metadata_padded.push(0);
        let metadata_padded_len = metadata_padded.len();
        put32(&mut metadata_padded, 8, metadata_padded_len);
        assert_eq!(decode(&metadata_padded, limit).unwrap(), expected);
        *metadata_padded.last_mut().unwrap() = 1;
        assert!(decode(&metadata_padded, limit).is_err());

        let mut private_only = packed.clone();
        let private_at = private_only.len();
        private_only.extend_from_slice(b"xy");
        put32(&mut private_only, 36, private_at);
        put32(&mut private_only, 40, 2);
        let private_only_len = private_only.len();
        put32(&mut private_only, 8, private_only_len);
        assert_eq!(decode(&private_only, limit).unwrap(), expected);

        // A private block starts after the metadata's alignment padding and
        // ends exactly at EOF, regardless of its arbitrary payload bytes.
        let mut both = metadata.clone();
        both.push(0);
        let private_at = both.len();
        both.extend_from_slice(b"xy");
        put32(&mut both, 36, private_at);
        put32(&mut both, 40, 2);
        let both_len = both.len();
        put32(&mut both, 8, both_len);
        assert_eq!(decode(&both, limit).unwrap(), expected);
        both[private_at - 1] = 1;
        assert!(decode(&both, limit).is_err());

        let mut reversed = packed;
        let private_at = reversed.len();
        reversed.extend_from_slice(b"priv");
        let meta_at = reversed.len();
        reversed.extend_from_slice(b"abc");
        put32(&mut reversed, 24, meta_at);
        put32(&mut reversed, 28, 3);
        put32(&mut reversed, 32, 12);
        put32(&mut reversed, 36, private_at);
        put32(&mut reversed, 40, 4);
        let reversed_len = reversed.len();
        put32(&mut reversed, 8, reversed_len);
        assert!(decode(&reversed, limit).is_err());
    }
}
