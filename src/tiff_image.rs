//! Decode a bounded subset of the Adobe TIFF 6.0 strip layout.
//! https://pdfa.org/wp-content/uploads/2020/07/TIFF6.pdf (sections 2–4 and 8)

use std::fmt;
use std::ops::Range;

use flate2::{Decompress, FlushDecompress, Status};
use image::GrayImage;

const HEADER_BYTES: usize = 8;
const FIELD_BYTES: usize = 12;
const MAX_IFD_ENTRIES: usize = 128;
const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u32 = 8 * 1024 * 1024;
const MAX_BYTES: usize = crate::comic::MAX_IMAGE_BYTES as usize;

const SHORT: u16 = 3;
const LONG: u16 = 4;
const SHORT_ONLY: &[u16] = &[SHORT];
const LONG_ONLY: &[u16] = &[LONG];
const SHORT_OR_LONG: &[u16] = &[SHORT, LONG];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TiffError {
    Header,
    Directory,
    Strips,
    Limit,
    Unsupported,
}

impl fmt::Display for TiffError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Header => "This TIFF has an invalid header",
            Self::Directory => "This TIFF has an invalid image directory",
            Self::Strips => "This TIFF has invalid or overlapping image strips",
            Self::Limit => "This TIFF exceeds Reader's image limits",
            Self::Unsupported => "Reader supports single-page grayscale, RGB and indexed-color TIFF strips, with orientations 1–8",
        })
    }
}

impl std::error::Error for TiffError {}

/// Decode one top-left-origin page. Samples use 0 for black and 255 for white.
pub(crate) fn decode(bytes: &[u8]) -> Result<GrayImage, TiffError> {
    let image = Directory::open(bytes)?;
    let width = image.required(256, SHORT_OR_LONG)?;
    let height = image.required(257, SHORT_OR_LONG)?;
    let pixel_count = width.checked_mul(height).ok_or(TiffError::Limit)?;
    if width.min(height) == 0 {
        return Err(TiffError::Directory);
    }
    if width.max(height) > MAX_SIDE || pixel_count > MAX_PIXELS {
        return Err(TiffError::Limit);
    }

    let photometric = image.required(262, SHORT_ONLY)?;
    let samples = image.scalar(277, SHORT_ONLY, 1)?;
    if !matches!(
        (photometric, samples),
        (0 | 1 | 3, 1) | (0 | 1, 2) | (2, 3 | 4)
    ) {
        return Err(TiffError::Unsupported);
    }
    // Alpha-bearing grayscale and RGB must explicitly describe the extra
    // channel. Bound decoded strips and output together to the existing cap.
    let alpha = if samples == 2 || samples == 4 {
        let kind = image.scalar(338, SHORT_ONLY, 0)?;
        if !matches!(kind, 1 | 2) {
            return Err(TiffError::Unsupported);
        }
        // Associated alpha with WhiteIsZero needs separate validation of the
        // premultiplied sample convention; do not guess at that case.
        if photometric == 0 && kind != 2 {
            return Err(TiffError::Unsupported);
        }
        if u64::from(pixel_count) * (u64::from(samples) + 1) > 32 * 1024 * 1024 {
            return Err(TiffError::Limit);
        }
        Some(kind)
    } else {
        if image.numbers(338, SHORT_ONLY, 1)?.is_some() {
            return Err(TiffError::Unsupported);
        }
        None
    };
    let bits_values = image.numbers(258, SHORT_ONLY, samples as usize)?;
    let bits = bits_values.as_ref().map_or(1, |values| values.value(0));
    let formats = image.numbers(339, SHORT_ONLY, samples as usize)?;
    let fill_order = image.scalar(266, SHORT_ONLY, 1)?;
    let compression = image.scalar(259, SHORT_ONLY, 1)?;
    let predictor = image.scalar(317, SHORT_ONLY, 1)?;
    let orientation = image.scalar(274, SHORT_ONLY, 1)?;
    if !matches!(bits, 1 | 2 | 4 | 8)
        || (photometric != 3 && !matches!(bits, 1 | 8))
        || (samples >= 2 && bits != 8)
        || bits_values.as_ref().is_some_and(|values| (0..samples as usize).any(|index| values.value(index) != bits))
        || formats.as_ref().is_some_and(|values| (0..samples as usize).any(|index| values.value(index) != 1))
        || !(fill_order == 1 || (bits < 8 && fill_order == 2))
        || !matches!(compression, 1 | 5 | 8 | 32946 | 32773)
        // TIFF reserves reverse fill order for special-purpose bilevel data;
        // the bit ordering of LZW streams is ambiguous in that combination.
        || (fill_order == 2 && matches!(compression, 5 | 8 | 32946))
        || !matches!(orientation, 1..=8)
        || image.scalar(284, SHORT_ONLY, 1)? != 1 // PlanarConfiguration
        || !(predictor == 1 || (predictor == 2 && bits == 8 && matches!(compression, 5 | 8 | 32946)))
        || image.scalar(254, LONG_ONLY, 0)? != 0 // NewSubfileType
        || image.scalar(255, SHORT_ONLY, 1)? != 1
    // SubfileType
    {
        return Err(TiffError::Unsupported);
    }

    // TIFF ColorMap stores all 16-bit red entries, then green, then blue.
    // Validate its complete count before making any palette lookup.
    let palette_size = 1usize << bits;
    let palette = if photometric == 3 {
        Some(
            image
                .numbers(320, SHORT_ONLY, 3 * palette_size)?
                .ok_or(TiffError::Directory)?,
        )
    } else {
        None
    };

    let rows_per_strip = image.scalar(278, SHORT_OR_LONG, u32::MAX)?;
    if rows_per_strip == 0 {
        return Err(TiffError::Strips);
    }
    let strip_count = height.div_ceil(rows_per_strip) as usize;
    let offsets = image
        .numbers(273, SHORT_OR_LONG, strip_count)?
        .ok_or(TiffError::Strips)?;
    let lengths = image
        .numbers(279, SHORT_OR_LONG, strip_count)?
        .ok_or(TiffError::Strips)?;
    let occupied = [
        Some(&image.range),
        offsets.external.as_ref(),
        lengths.external.as_ref(),
        bits_values
            .as_ref()
            .and_then(|values| values.external.as_ref()),
        formats.as_ref().and_then(|values| values.external.as_ref()),
        palette.as_ref().and_then(|values| values.external.as_ref()),
    ];
    let external: Vec<_> = occupied.iter().flatten().copied().collect();
    if external.iter().enumerate().any(|(index, first)| {
        external[index + 1..]
            .iter()
            .any(|second| overlaps(first, second))
    }) {
        return Err(TiffError::Strips);
    }
    // Checked dimensions and the combined sample/output bound keep the
    // expanded strip and grayscale output within 32 MiB, plus the Deflate
    // decoder's one-byte overflow sentinel.
    let row_bits = width
        .checked_mul(bits)
        .and_then(|n| n.checked_mul(samples))
        .ok_or(TiffError::Limit)?;
    let row_bytes = row_bits.div_ceil(8) as usize;
    let mut ranges = Vec::with_capacity(strip_count);
    let mut remaining_rows = height;
    for index in 0..strip_count {
        let start = offsets.value(index) as usize;
        let length = lengths.value(index) as usize;
        let rows = remaining_rows.min(rows_per_strip);
        remaining_rows -= rows;
        let end = start.checked_add(length).ok_or(TiffError::Strips)?;
        let range = start..end;
        if start < HEADER_BYTES
            || end > bytes.len()
            || length == 0
            || (compression == 1 && length != rows as usize * row_bytes)
            || occupied.iter().flatten().any(|used| overlaps(&range, used))
        {
            return Err(TiffError::Strips);
        }
        ranges.push(range);
    }
    let mut by_offset = ranges.clone();
    by_offset.sort_unstable_by_key(|range| range.start);
    if by_offset
        .windows(2)
        .any(|pair| overlaps(&pair[0], &pair[1]))
    {
        return Err(TiffError::Strips);
    }

    let palette_gray = palette.as_ref().map(|values| {
        (0..palette_size)
            .map(|index| {
                let red = values.value(index) >> 8;
                let green = values.value(palette_size + index) >> 8;
                let blue = values.value(2 * palette_size + index) >> 8;
                ((red * 2126 + green * 7152 + blue * 722) / 10000) as u8
            })
            .collect::<Vec<_>>()
    });
    let invert = if photometric == 0 { 255 } else { 0 };
    let mut pixels = Vec::with_capacity(pixel_count as usize);
    remaining_rows = height;
    for range in ranges {
        let strip = &bytes[range];
        let rows = remaining_rows.min(rows_per_strip) as usize;
        remaining_rows -= rows as u32;
        let mut unpacked = match compression {
            32773 => Some(unpack_packbits(strip, row_bytes, rows)?),
            5 => Some(unpack_lzw(strip, row_bytes, rows)?),
            8 | 32946 => Some(unpack_deflate(strip, row_bytes, rows)?),
            _ => None,
        };
        if predictor == 2 {
            let decoded = unpacked.as_mut().ok_or(TiffError::Unsupported)?;
            let channels = samples as usize;
            for row in decoded.chunks_exact_mut(row_bytes) {
                for index in channels..row.len() {
                    row[index] = row[index].wrapping_add(row[index - channels]);
                }
            }
        }
        let strip = unpacked.as_deref().unwrap_or(strip);
        if let Some(palette) = &palette_gray {
            for row in strip.chunks_exact(row_bytes) {
                for x in 0..width as usize {
                    let offset = x * bits as usize;
                    let byte = if fill_order == 1 {
                        row[offset / 8]
                    } else {
                        row[offset / 8].reverse_bits()
                    };
                    let shift = 8 - bits as usize - offset % 8;
                    let index = usize::from((byte >> shift) & ((1u16 << bits) - 1) as u8);
                    pixels.push(palette[index]);
                }
            }
        } else if let Some(alpha_kind) = alpha {
            // Composite associated or unassociated alpha over the same white
            // page used by Reader's other transparent-image paths.
            pixels.extend(strip.chunks_exact(samples as usize).map(|pixel| {
                let gray = if samples == 2 {
                    u32::from(pixel[0] ^ invert)
                } else {
                    (u32::from(pixel[0]) * 2126
                        + u32::from(pixel[1]) * 7152
                        + u32::from(pixel[2]) * 722)
                        / 10000
                };
                let a = u32::from(pixel[samples as usize - 1]);
                if alpha_kind == 1 {
                    (gray + 255 - a).min(255) as u8
                } else {
                    ((gray * a + 255 * (255 - a) + 127) / 255) as u8
                }
            }));
        } else if samples == 3 {
            // Match image's sRGB-to-luma conversion used by other comic pages.
            pixels.extend(strip.chunks_exact(3).map(|rgb| {
                ((u32::from(rgb[0]) * 2126 + u32::from(rgb[1]) * 7152 + u32::from(rgb[2]) * 722)
                    / 10000) as u8
            }));
        } else if bits == 8 {
            pixels.extend(strip.iter().map(|&gray| gray ^ invert));
        } else {
            for row in strip.chunks_exact(row_bytes) {
                for x in 0..width as usize {
                    let bit = if fill_order == 1 { 7 - x % 8 } else { x % 8 };
                    let sample = (row[x / 8] >> bit) & 1;
                    pixels.push((sample * 255) ^ invert);
                }
            }
        }
    }
    if orientation == 1 {
        return GrayImage::from_raw(width, height, pixels).ok_or(TiffError::Strips);
    }
    // TIFF orientation maps stored samples to visual coordinates. A transpose
    // swaps dimensions but never changes the checked pixel count.
    let (out_width, out_height) = if orientation >= 5 {
        (height, width)
    } else {
        (width, height)
    };
    let mut visual = vec![0; pixel_count as usize];
    for y in 0..height {
        for x in 0..width {
            let (target_x, target_y) = match orientation {
                2 => (width - 1 - x, y),
                3 => (width - 1 - x, height - 1 - y),
                4 => (x, height - 1 - y),
                5 => (y, x),
                6 => (height - 1 - y, x),
                7 => (height - 1 - y, width - 1 - x),
                8 => (y, width - 1 - x),
                _ => unreachable!(),
            };
            visual[(target_y * out_width + target_x) as usize] = pixels[(y * width + x) as usize];
        }
    }
    GrayImage::from_raw(out_width, out_height, visual).ok_or(TiffError::Strips)
}

/// TIFF LZW uses MSB-first codes, an initial Clear, and the TIFF early-change
/// 9-to-12-bit transitions. Each strip resets its own fixed-size dictionary.
fn unpack_lzw(input: &[u8], row_bytes: usize, rows: usize) -> Result<Vec<u8>, TiffError> {
    const CLEAR: usize = 256;
    const END: usize = 257;
    const FIRST: usize = 258;
    const TABLE: usize = 4096;

    let expected = row_bytes.checked_mul(rows).ok_or(TiffError::Limit)?;
    let mut output = Vec::with_capacity(expected);
    let mut prefix = [0u16; TABLE];
    let mut suffix = [0u8; TABLE];
    let mut stack = [0u8; TABLE];
    let (mut position, mut bits, mut buffered) = (0usize, 0u32, 0u32);
    let (mut width, mut next, mut previous) = (9u32, FIRST, None::<usize>);
    let mut started = false;
    let mut clears = 0usize;

    loop {
        while buffered < width {
            bits = (bits << 8) | u32::from(*input.get(position).ok_or(TiffError::Strips)?);
            position += 1;
            buffered += 8;
        }
        buffered -= width;
        let code = ((bits >> buffered) & ((1 << width) - 1)) as usize;
        bits &= (1 << buffered) - 1;
        if code == CLEAR {
            clears += 1;
            if clears > 8192 {
                return Err(TiffError::Limit);
            }
            width = 9;
            next = FIRST;
            previous = None;
            started = true;
            continue;
        }
        if !started {
            return Err(TiffError::Strips);
        }
        if code == END {
            return (output.len() == expected)
                .then_some(output)
                .ok_or(TiffError::Strips);
        }

        let special = code == next && next < TABLE && previous.is_some();
        if code >= next && !special {
            return Err(TiffError::Strips);
        }
        let mut cursor = if special { previous.unwrap() } else { code };
        let mut length = 0;
        while cursor >= FIRST {
            if cursor >= next || length >= stack.len() {
                return Err(TiffError::Strips);
            }
            stack[length] = suffix[cursor];
            length += 1;
            let ancestor = prefix[cursor] as usize;
            if ancestor >= cursor {
                return Err(TiffError::Strips);
            }
            cursor = ancestor;
        }
        if cursor >= CLEAR || length >= stack.len() {
            return Err(TiffError::Strips);
        }
        let first = cursor as u8;
        stack[length] = first;
        length += 1;
        let produced = length + usize::from(special);
        if produced > expected.saturating_sub(output.len()) {
            return Err(TiffError::Strips);
        }
        output.extend(stack[..length].iter().rev().copied());
        if special {
            output.push(first);
        }
        if let Some(old) = previous
            && next < TABLE
        {
            prefix[next] = old as u16;
            suffix[next] = first;
            next += 1;
            // TIFF decoders switch after writing entries 510, 1022 and 2046.
            if width < 12 && next == (1usize << width) - 1 {
                width += 1;
            }
        }
        previous = Some(code);
    }
}

/// Inflate one zlib-compressed strip without permitting output past its
/// declared image dimensions. Some encoders pad a strip with zero bytes.
fn unpack_deflate(input: &[u8], row_bytes: usize, rows: usize) -> Result<Vec<u8>, TiffError> {
    let expected = row_bytes.checked_mul(rows).ok_or(TiffError::Limit)?;
    let capacity = expected.checked_add(1).ok_or(TiffError::Limit)?;
    let mut output = vec![0u8; capacity];
    let mut decoder = Decompress::new(true);
    loop {
        let before_in = decoder.total_in() as usize;
        let before_out = decoder.total_out() as usize;
        let status = decoder
            .decompress(
                &input[before_in..],
                &mut output[before_out..],
                FlushDecompress::Finish,
            )
            .map_err(|_| TiffError::Strips)?;
        let consumed = decoder.total_in() as usize;
        let produced = decoder.total_out() as usize;
        if status == Status::StreamEnd {
            if produced != expected || input[consumed..].iter().any(|&byte| byte != 0) {
                return Err(TiffError::Strips);
            }
            output.truncate(expected);
            return Ok(output);
        }
        if produced > expected || (consumed == before_in && produced == before_out) {
            return Err(TiffError::Strips);
        }
    }
}

/// TIFF PackBits encodes each row independently. Bound output to exactly the
/// expected strip dimensions and reject packets that cross a row boundary.
fn unpack_packbits(input: &[u8], row_bytes: usize, rows: usize) -> Result<Vec<u8>, TiffError> {
    let expected = row_bytes.checked_mul(rows).ok_or(TiffError::Limit)?;
    let mut output = Vec::with_capacity(expected);
    let mut offset = 0usize;
    for _ in 0..rows {
        let row_end = output.len() + row_bytes;
        while output.len() < row_end {
            let control = *input.get(offset).ok_or(TiffError::Strips)?;
            offset += 1;
            match control {
                0..=127 => {
                    let count = usize::from(control) + 1;
                    let end = offset.checked_add(count).ok_or(TiffError::Strips)?;
                    if count > row_end - output.len() {
                        return Err(TiffError::Strips);
                    }
                    output.extend_from_slice(input.get(offset..end).ok_or(TiffError::Strips)?);
                    offset = end;
                }
                128 => {} // PackBits no-op.
                129..=255 => {
                    let count = 257 - usize::from(control);
                    let byte = *input.get(offset).ok_or(TiffError::Strips)?;
                    offset += 1;
                    if count > row_end - output.len() {
                        return Err(TiffError::Strips);
                    }
                    output.resize(output.len() + count, byte);
                }
            }
        }
    }
    if input[offset..].iter().any(|&byte| byte != 128) {
        return Err(TiffError::Strips);
    }
    Ok(output)
}

#[derive(Clone, Copy)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn short(self, bytes: [u8; 2]) -> u16 {
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }

    fn long(self, bytes: [u8; 4]) -> u32 {
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }
}

struct Directory<'a> {
    bytes: &'a [u8],
    range: Range<usize>,
    entries: &'a [[u8; FIELD_BYTES]],
    order: ByteOrder,
}

impl<'a> Directory<'a> {
    fn open(bytes: &'a [u8]) -> Result<Self, TiffError> {
        if bytes.len() > MAX_BYTES {
            return Err(TiffError::Limit);
        }
        let order = match bytes.get(..4) {
            Some(b"II*\0") => ByteOrder::Little,
            Some(b"MM\0*") => ByteOrder::Big,
            Some(b"II+\0" | b"MM\0+") => return Err(TiffError::Unsupported),
            _ => return Err(TiffError::Header),
        };
        let start = order.long(at(bytes, 4).ok_or(TiffError::Header)?) as usize;
        if start < HEADER_BYTES || !start.is_multiple_of(2) {
            return Err(TiffError::Directory);
        }
        let count = order.short(at(bytes, start).ok_or(TiffError::Directory)?) as usize;
        if count == 0 {
            return Err(TiffError::Directory);
        }
        if count > MAX_IFD_ENTRIES {
            return Err(TiffError::Limit);
        }
        let entries_end = start
            .checked_add(2 + count * FIELD_BYTES)
            .ok_or(TiffError::Directory)?;
        let next = order.long(at(bytes, entries_end).ok_or(TiffError::Directory)?);
        if next != 0 {
            return Err(TiffError::Unsupported);
        }
        let entries = bytes[start + 2..entries_end].as_chunks::<FIELD_BYTES>().0;
        let mut previous = 0;
        for tag in entries
            .iter()
            .map(|entry| order.short([entry[0], entry[1]]))
        {
            if tag <= previous {
                return Err(TiffError::Directory);
            }
            if matches!(tag, 322..=325 | 330) {
                return Err(TiffError::Unsupported); // Tiles or SubIFDs.
            }
            previous = tag;
        }
        Ok(Self {
            bytes,
            range: start..entries_end + 4,
            entries,
            order,
        })
    }

    fn required(&self, tag: u16, types: &[u16]) -> Result<u32, TiffError> {
        self.numbers(tag, types, 1)?
            .map(|values| values.value(0))
            .ok_or(TiffError::Directory)
    }

    fn scalar(&self, tag: u16, types: &[u16], default: u32) -> Result<u32, TiffError> {
        Ok(self
            .numbers(tag, types, 1)?
            .map_or(default, |values| values.value(0)))
    }

    fn numbers(
        &self,
        tag: u16,
        types: &[u16],
        count: usize,
    ) -> Result<Option<Numbers<'a>>, TiffError> {
        let entry = self
            .entries
            .binary_search_by_key(&tag, |entry| self.order.short([entry[0], entry[1]]));
        let Ok(index) = entry else { return Ok(None) };
        let entry = &self.entries[index];
        let value_type = self.order.short([entry[2], entry[3]]);
        let data_count = self.order.long([entry[4], entry[5], entry[6], entry[7]]);
        if !types.contains(&value_type) || data_count as usize != count {
            return Err(TiffError::Directory);
        }
        let size = if value_type == SHORT { 2 } else { 4 };
        let length = count * size;
        let (values, external) = if length <= 4 {
            (&entry[8..8 + length], None)
        } else {
            let start = self.order.long([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let end = start.checked_add(length).ok_or(TiffError::Directory)?;
            let range = start..end;
            if start < HEADER_BYTES || !start.is_multiple_of(2) || overlaps(&range, &self.range) {
                return Err(TiffError::Directory);
            }
            (
                self.bytes.get(range.clone()).ok_or(TiffError::Directory)?,
                Some(range),
            )
        };
        Ok(Some(Numbers {
            values,
            size,
            order: self.order,
            external,
        }))
    }
}

struct Numbers<'a> {
    values: &'a [u8],
    size: usize,
    order: ByteOrder,
    external: Option<Range<usize>>,
}

impl Numbers<'_> {
    fn value(&self, index: usize) -> u32 {
        let p = index * self.size;
        let bytes = self.values;
        if self.size == 2 {
            u32::from(self.order.short([bytes[p], bytes[p + 1]]))
        } else {
            self.order
                .long([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]])
        }
    }
}

fn at<const N: usize>(bytes: &[u8], start: usize) -> Option<[u8; N]> {
    bytes.get(start..)?.first_chunk().copied()
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    struct Geometry {
        width: u32,
        height: u32,
        rows_per_strip: u32,
        bits: u32,
        photometric: u32,
        fill_order: u32,
    }

    const GRAY: Geometry = Geometry {
        width: 3,
        height: 5,
        rows_per_strip: 2,
        bits: 8,
        photometric: 1,
        fill_order: 1,
    };

    const STRIPS: [&[u8]; 3] = [&[1, 2, 3, 4, 5, 6], &[7, 8, 9, 10, 11, 12], &[13, 14, 15]];

    fn fixture(order: ByteOrder, geometry: Geometry, strips: &[&[u8]]) -> Vec<u8> {
        let samples = if geometry.photometric == 2 { 3 } else { 1 };
        let mut fields = vec![
            (256u16, SHORT, vec![geometry.width]),
            (257, LONG, vec![geometry.height]),
            (258, SHORT, vec![geometry.bits; samples]),
            (259, SHORT, vec![1]),
            (262, SHORT, vec![geometry.photometric]),
            (266, SHORT, vec![geometry.fill_order]),
            (273, LONG, vec![0; strips.len()]),
            (274, SHORT, vec![1]),
            (277, SHORT, vec![samples as u32]),
            (278, LONG, vec![geometry.rows_per_strip]),
            (
                279,
                SHORT,
                strips.iter().map(|strip| strip.len() as u32).collect(),
            ),
            (284, SHORT, vec![1]),
            (339, SHORT, vec![1; samples]),
        ];
        if geometry.photometric == 3 {
            let colors = 1usize << geometry.bits;
            let palette = (0..3)
                .flat_map(|channel| {
                    (0..colors).map(move |index| {
                        let rgb = match index % 4 {
                            0 => [0, 0, 0],
                            1 => [65535, 0, 0],
                            2 => [0, 65535, 0],
                            _ => [65535, 65535, 65535],
                        };
                        rgb[channel]
                    })
                })
                .collect();
            fields.insert(12, (320, SHORT, palette));
        }
        let mut bytes = vec![0; HEADER_BYTES + 2 + fields.len() * FIELD_BYTES + 4];
        bytes[..2].copy_from_slice(match order {
            ByteOrder::Little => b"II",
            ByteOrder::Big => b"MM",
        });
        put(&mut bytes, 2, SHORT, 42, order);
        put(&mut bytes, 4, LONG, HEADER_BYTES as u32, order);
        put(&mut bytes, HEADER_BYTES, SHORT, fields.len() as u32, order);
        for (index, strip) in strips.iter().enumerate().rev() {
            fields[6].2[index] = bytes.len() as u32;
            bytes.extend_from_slice(strip);
        }
        for (index, (tag, kind, values)) in fields.iter().enumerate() {
            let field = HEADER_BYTES + 2 + index * FIELD_BYTES;
            put(&mut bytes, field, SHORT, u32::from(*tag), order);
            put(&mut bytes, field + 2, SHORT, u32::from(*kind), order);
            put(&mut bytes, field + 4, LONG, values.len() as u32, order);
            let size = if *kind == SHORT { 2 } else { 4 };
            let length = size * values.len();
            let start = if length <= 4 {
                field + 8
            } else {
                let offset = bytes.len().next_multiple_of(2);
                bytes.resize(offset + length, 0);
                put(&mut bytes, field + 8, LONG, offset as u32, order);
                offset
            };
            for (index, &value) in values.iter().enumerate() {
                put(&mut bytes, start + size * index, *kind, value, order);
            }
        }
        bytes
    }

    fn put(bytes: &mut [u8], offset: usize, kind: u16, value: u32, order: ByteOrder) {
        if kind == SHORT {
            let value = value as u16;
            let data = match order {
                ByteOrder::Little => value.to_le_bytes(),
                ByteOrder::Big => value.to_be_bytes(),
            };
            bytes[offset..offset + 2].copy_from_slice(&data);
        } else {
            let data = match order {
                ByteOrder::Little => value.to_le_bytes(),
                ByteOrder::Big => value.to_be_bytes(),
            };
            bytes[offset..offset + 4].copy_from_slice(&data);
        }
    }

    fn field(bytes: &[u8], tag: u16, order: ByteOrder) -> usize {
        let index = Directory::open(bytes)
            .unwrap()
            .entries
            .iter()
            .position(|entry| order.short([entry[0], entry[1]]) == tag)
            .unwrap();
        HEADER_BYTES + 2 + index * FIELD_BYTES
    }

    fn lzw_short(codes: &[u16]) -> Vec<u8> {
        let mut output = Vec::new();
        let (mut buffer, mut bits) = (0u32, 0u32);
        for &code in codes {
            buffer = (buffer << 9) | u32::from(code);
            bits += 9;
            while bits >= 8 {
                bits -= 8;
                output.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
        if bits > 0 {
            output.push((buffer << (8 - bits)) as u8);
        }
        output
    }

    #[test]
    fn tiff_lzw_decodes_each_strip_and_special_dictionary_entries() {
        let encode = |raw: &[u8]| {
            let mut codes = vec![256];
            codes.extend(raw.iter().map(|&sample| u16::from(sample)));
            codes.push(257);
            lzw_short(&codes)
        };
        let first = encode(STRIPS[0]);
        let second = encode(STRIPS[1]);
        let third = encode(STRIPS[2]);
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut image = fixture(order, GRAY, &[&first, &second, &third]);
            let compression = field(&image, 259, order);
            put(&mut image, compression + 8, SHORT, 5, order);
            assert_eq!(
                decode(&image).unwrap().into_raw(),
                (1..=15).collect::<Vec<_>>()
            );
        }
        assert_eq!(
            unpack_lzw(&lzw_short(&[256, 65, 258, 257]), 3, 1).unwrap(),
            b"AAA"
        );
        assert!(unpack_lzw(&lzw_short(&[65, 257]), 1, 1).is_err());
        assert!(unpack_lzw(&lzw_short(&[256, 65, 258, 257]), 2, 1).is_err());
        assert!(unpack_lzw(&lzw_short(&[256, 65]), 1, 1).is_err());
    }

    #[test]
    fn tiff_deflate_reconstructs_each_strip_with_horizontal_predictor() {
        let encode = |raw: &[u8]| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(raw).unwrap();
            encoder.finish().unwrap()
        };
        let first = encode(&[1, 1, 1, 4, 1, 1]);
        let second = encode(&[7, 1, 1, 10, 1, 1]);
        let third = encode(&[13, 1, 1]);
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for compression in [8, 32946] {
                let mut image = fixture(order, GRAY, &[&first, &second, &third]);
                let tag = field(&image, 259, order);
                put(&mut image, tag + 8, SHORT, compression, order);
                // Replace the optional SampleFormat tag with Predictor.
                let tag = field(&image, 339, order);
                put(&mut image, tag, SHORT, 317, order);
                put(&mut image, tag + 8, SHORT, 2, order);
                assert_eq!(
                    decode(&image).unwrap().into_raw(),
                    (1..=15).collect::<Vec<_>>()
                );
            }
        }
        assert!(unpack_deflate(&first[..first.len() - 1], 3, 2).is_err());
        assert!(unpack_deflate(&first, 2, 2).is_err());
    }

    #[test]
    fn palette_indices_follow_bit_depth_fill_order_and_orientation() {
        let cases: [(u32, &[u8], &[u8]); 4] = [
            (1, &[0b0101_0000], &[0, 54, 0, 54]),
            (2, &[0b0001_1011], &[0, 54, 182, 255]),
            (4, &[0x01, 0x23], &[0, 54, 182, 255]),
            (8, &[0, 1, 2, 3], &[0, 54, 182, 255]),
        ];
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for (bits, packed, expected) in cases {
                for fill_order in if bits < 8 { &[1, 2][..] } else { &[1][..] } {
                    let data = packed
                        .iter()
                        .map(|&byte| {
                            if *fill_order == 2 {
                                byte.reverse_bits()
                            } else {
                                byte
                            }
                        })
                        .collect::<Vec<_>>();
                    let geometry = Geometry {
                        width: 4,
                        height: 1,
                        rows_per_strip: 1,
                        bits,
                        photometric: 3,
                        fill_order: *fill_order,
                    };
                    let mut bytes = fixture(order, geometry, &[&data]);
                    assert_eq!(decode(&bytes).unwrap().into_raw(), expected);
                    let tag = field(&bytes, 274, order);
                    put(&mut bytes, tag + 8, SHORT, 2, order);
                    assert_eq!(
                        decode(&bytes).unwrap().into_raw(),
                        expected.iter().rev().copied().collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    #[test]
    fn palette_compressed_strips_and_predictor_use_indices() {
        let raw = [0, 1, 2, 3];
        let predicted = [0, 1, 1, 1];
        let mut codes = vec![256];
        codes.extend(predicted.iter().copied().map(u16::from));
        codes.push(257);
        let lzw = lzw_short(&codes);
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&predicted).unwrap();
        let zlib = encoder.finish().unwrap();
        let packbits = [3, 0, 1, 2, 3];
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for (compression, data, predictor) in [
                (1, raw.as_slice(), 1),
                (32773, packbits.as_slice(), 1),
                (5, lzw.as_slice(), 2),
                (8, zlib.as_slice(), 2),
                (32946, zlib.as_slice(), 2),
            ] {
                let geometry = Geometry {
                    width: 4,
                    height: 1,
                    rows_per_strip: 1,
                    bits: 8,
                    photometric: 3,
                    fill_order: 1,
                };
                let mut bytes = fixture(order, geometry, &[data]);
                let tag = field(&bytes, 259, order);
                put(&mut bytes, tag + 8, SHORT, compression, order);
                if predictor == 2 {
                    // Replace optional PlanarConfiguration, preserving tag order.
                    let tag = field(&bytes, 284, order);
                    put(&mut bytes, tag, SHORT, 317, order);
                    put(&mut bytes, tag + 8, SHORT, predictor, order);
                }
                assert_eq!(decode(&bytes).unwrap().into_raw(), [0, 54, 182, 255]);
            }
        }
    }

    #[test]
    fn palette_rejects_missing_wrong_sized_or_overlapping_color_maps() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let geometry = Geometry {
                width: 96,
                height: 1,
                rows_per_strip: 1,
                bits: 2,
                photometric: 3,
                fill_order: 1,
            };
            let bytes = fixture(order, geometry, &[&[0; 24]]);
            let map = field(&bytes, 320, order);
            let mut short = bytes.clone();
            put(&mut short, map + 4, LONG, 4, order);
            assert_eq!(decode(&short).unwrap_err(), TiffError::Directory);
            let mut missing = bytes.clone();
            put(&mut missing, map, SHORT, 319, order);
            assert_eq!(decode(&missing).unwrap_err(), TiffError::Directory);
            let mut alias = bytes.clone();
            let strip = order.long(at(&bytes, field(&bytes, 273, order) + 8).unwrap());
            put(&mut alias, map + 8, LONG, strip, order);
            assert_eq!(decode(&alias).unwrap_err(), TiffError::Strips);
        }
    }

    #[test]
    fn rgb_strips_match_existing_grayscale_and_predict_per_channel() {
        let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let predicted = [255, 0, 0, 1, 255, 0, 0, 0, 255, 255, 255, 0];
        let lzw = |input: &[u8]| {
            let mut codes = vec![256];
            codes.extend(input.iter().copied().map(u16::from));
            codes.push(257);
            lzw_short(&codes)
        };
        let deflate = |input: &[u8]| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(input).unwrap();
            encoder.finish().unwrap()
        };
        let packed = [5, 255, 0, 0, 0, 255, 0, 5, 0, 0, 255, 255, 255, 255];
        let lzw_data = lzw(&predicted);
        let zlib_data = deflate(&predicted);
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for (compression, data, predictor) in [
                (1, pixels.as_slice(), 1),
                (32773, packed.as_slice(), 1),
                (5, lzw_data.as_slice(), 2),
                (8, zlib_data.as_slice(), 2),
                (32946, zlib_data.as_slice(), 2),
            ] {
                let geometry = Geometry {
                    width: 2,
                    height: 2,
                    rows_per_strip: 2,
                    bits: 8,
                    photometric: 2,
                    fill_order: 1,
                };
                let mut bytes = fixture(order, geometry, &[data]);
                let tag = field(&bytes, 259, order);
                put(&mut bytes, tag + 8, SHORT, compression, order);
                if predictor == 2 {
                    // Replace the optional SampleFormat entry with Predictor.
                    let tag = field(&bytes, 339, order);
                    put(&mut bytes, tag, SHORT, 317, order);
                    put(&mut bytes, tag + 4, LONG, 1, order);
                    put(&mut bytes, tag + 8, SHORT, predictor, order);
                }
                let image = decode(&bytes).unwrap();
                assert_eq!(image.into_raw(), [54, 182, 18, 255]);
                let tag = field(&bytes, 274, order);
                put(&mut bytes, tag + 8, SHORT, 6, order);
                assert_eq!(decode(&bytes).unwrap().into_raw(), [18, 54, 255, 182]);
            }
        }
    }

    #[test]
    fn rgb_rejects_mixed_bit_depth_and_metadata_overlapping_strip() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let geometry = Geometry {
                width: 2,
                height: 2,
                rows_per_strip: 2,
                bits: 8,
                photometric: 2,
                fill_order: 1,
            };
            let bytes = fixture(order, geometry, &[&[0; 12]]);
            let bits = field(&bytes, 258, order);
            let array = order.long(at(&bytes, bits + 8).unwrap()) as usize;
            let mut mixed = bytes.clone();
            put(&mut mixed, array + 4, SHORT, 16, order);
            assert_eq!(decode(&mixed).unwrap_err(), TiffError::Unsupported);
            let mut aliased = bytes.clone();
            let strip = order.long(at(&bytes, field(&bytes, 273, order) + 8).unwrap());
            put(&mut aliased, bits + 8, LONG, strip, order);
            // All three values still look valid, but may not alias image data.
            for offset in [0, 2, 4] {
                put(&mut aliased, strip as usize + offset, SHORT, 8, order);
            }
            assert_eq!(decode(&aliased).unwrap_err(), TiffError::Strips);
        }
    }

    #[test]
    fn decodes_strips_in_logical_order_in_both_byte_orders_and_photometries() {
        for (order, photometric, invert) in [(ByteOrder::Little, 1, 0), (ByteOrder::Big, 0, 255)] {
            let bytes = fixture(
                order,
                Geometry {
                    photometric,
                    ..GRAY
                },
                &STRIPS,
            );
            let image = decode(&bytes).unwrap();
            assert_eq!(image.dimensions(), (3, 5));
            assert_eq!(
                image.into_raw(),
                (1..=15).map(|sample| sample ^ invert).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bilevel_rows_keep_their_padding_and_follow_the_declared_bit_order() {
        let order = ByteOrder::Big;
        let packed: [u8; 6] = [0x81, 0x7f, 0, 0x80, 0xf0, 0x3f];
        for fill in [1, 2] {
            let stored = packed.map(|value| {
                if fill == 1 {
                    value
                } else {
                    value.reverse_bits()
                }
            });
            let bytes = fixture(
                order,
                Geometry {
                    width: 10,
                    height: 3,
                    rows_per_strip: 2,
                    bits: 1,
                    photometric: 0,
                    fill_order: fill,
                },
                &[&stored[..4], &stored[4..]],
            );
            let pixels = decode(&bytes).unwrap().into_raw();
            let expected = b"100000010100000000101111000000"
                .iter()
                .map(|&sample| if sample == b'0' { 255 } else { 0 })
                .collect::<Vec<_>>();
            assert_eq!(pixels, expected);
        }
    }

    #[test]
    fn tiff_orientations_transform_non_square_pages_in_both_byte_orders() {
        let expected: [&[u8]; 8] = [
            &[1, 2, 3, 4, 5, 6],
            &[3, 2, 1, 6, 5, 4],
            &[6, 5, 4, 3, 2, 1],
            &[4, 5, 6, 1, 2, 3],
            &[1, 4, 2, 5, 3, 6],
            &[4, 1, 5, 2, 6, 3],
            &[6, 3, 5, 2, 4, 1],
            &[3, 6, 2, 5, 1, 4],
        ];
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for (index, &pixels) in expected.iter().enumerate() {
                let mut bytes = fixture(
                    order,
                    Geometry {
                        width: 3,
                        height: 2,
                        rows_per_strip: 2,
                        bits: 8,
                        photometric: 1,
                        fill_order: 1,
                    },
                    &[&[1, 2, 3, 4, 5, 6]],
                );
                let tag = field(&bytes, 274, order);
                put(&mut bytes, tag + 8, SHORT, index as u32 + 1, order);
                let image = decode(&bytes).unwrap();
                assert_eq!(image.dimensions(), if index < 4 { (3, 2) } else { (2, 3) });
                assert_eq!(image.into_raw(), pixels, "orientation {}", index + 1);
            }
        }
    }

    #[test]
    fn tiff_orientation_applies_after_packbits_and_bilevel_expansion() {
        let order = ByteOrder::Little;
        // Two rows of three one-bit pixels: 100 / 011.
        let packed = [0, 0b1000_0000, 0, 0b0110_0000];
        let mut bytes = fixture(
            order,
            Geometry {
                width: 3,
                height: 2,
                rows_per_strip: 2,
                bits: 1,
                photometric: 1,
                fill_order: 1,
            },
            &[&packed],
        );
        let compression = field(&bytes, 259, order);
        put(&mut bytes, compression + 8, SHORT, 32773, order);
        let orientation = field(&bytes, 274, order);
        put(&mut bytes, orientation + 8, SHORT, 6, order);
        let image = decode(&bytes).unwrap();
        assert_eq!(image.dimensions(), (2, 3));
        assert_eq!(image.into_raw(), [0, 255, 255, 0, 255, 0]);
    }

    #[test]
    fn rejects_short_overlapping_out_of_file_and_metadata_aliasing_strips() {
        let order = ByteOrder::Little;
        let bytes = fixture(order, GRAY, &STRIPS);
        let offset_field = field(&bytes, 273, order);
        let offsets = order.long(at(&bytes, offset_field + 8).unwrap()) as usize;
        let second = order.long(at(&bytes, offsets + 4).unwrap());
        for bad in [HEADER_BYTES as u32, second, offsets as u32, u32::MAX - 1] {
            let mut malformed = bytes.clone();
            put(&mut malformed, offsets, LONG, bad, order);
            assert_eq!(decode(&malformed).unwrap_err(), TiffError::Strips);
        }
        let length_field = field(&bytes, 279, order);
        let mut overlapping_arrays = bytes.clone();
        put(
            &mut overlapping_arrays,
            length_field + 8,
            LONG,
            offsets as u32,
            order,
        );
        assert_eq!(decode(&overlapping_arrays).unwrap_err(), TiffError::Strips);
        let short = fixture(order, GRAY, &[STRIPS[0], &STRIPS[1][1..], STRIPS[2]]);
        assert_eq!(decode(&short).unwrap_err(), TiffError::Strips);
        assert_eq!(
            decode(&bytes[..bytes.len() - 1]).unwrap_err(),
            TiffError::Directory
        );
    }

    #[test]
    fn rejects_unsupported_compression_color_orientation_and_additional_directories() {
        let order = ByteOrder::Little;
        let bytes = fixture(order, GRAY, &STRIPS);
        for (tag, value) in [
            (258, 4),
            (259, 6),
            (262, 2),
            (266, 2),
            (274, 0),
            (274, 9),
            (277, 3),
            (284, 2),
            (339, 2),
        ] {
            let mut unsupported = bytes.clone();
            put(
                &mut unsupported,
                field(&bytes, tag, order) + 8,
                SHORT,
                value,
                order,
            );
            assert_eq!(decode(&unsupported).unwrap_err(), TiffError::Unsupported);
        }
        let sample_format = field(&bytes, 339, order);
        let mut subifd = bytes.clone();
        put(&mut subifd, sample_format, SHORT, 330, order);
        assert_eq!(decode(&subifd).unwrap_err(), TiffError::Unsupported);
        let mut tiled = bytes.clone();
        put(&mut tiled, sample_format, SHORT, 324, order);
        assert_eq!(decode(&tiled).unwrap_err(), TiffError::Unsupported);
        let next_ifd = Directory::open(&bytes).unwrap().range.end - 4;
        let mut multipage = bytes.clone();
        put(&mut multipage, next_ifd, LONG, 8, order);
        assert_eq!(decode(&multipage).unwrap_err(), TiffError::Unsupported);
        assert_eq!(decode(b"II+\0").unwrap_err(), TiffError::Unsupported);
    }

    #[test]
    fn rejects_malformed_image_fields_and_dimensions_before_allocation() {
        let order = ByteOrder::Little;
        let bytes = fixture(order, GRAY, &STRIPS);
        let mut oversized = bytes.clone();
        put(
            &mut oversized,
            field(&bytes, 257, order) + 8,
            LONG,
            u32::MAX,
            order,
        );
        assert_eq!(decode(&oversized).unwrap_err(), TiffError::Limit);
        let mut duplicated = bytes.clone();
        put(
            &mut duplicated,
            field(&bytes, 257, order),
            SHORT,
            256,
            order,
        );
        assert_eq!(decode(&duplicated).unwrap_err(), TiffError::Directory);
        let mut unaligned = bytes.clone();
        put(&mut unaligned, 4, LONG, 9, order);
        assert_eq!(decode(&unaligned).unwrap_err(), TiffError::Directory);
        let mut missing = bytes.clone();
        put(&mut missing, field(&bytes, 262, order), SHORT, 261, order);
        assert_eq!(decode(&missing).unwrap_err(), TiffError::Directory);
    }

    #[test]
    fn opens_a_standalone_tiff_through_reader_image_page() {
        struct FixtureFile(PathBuf);
        impl Drop for FixtureFile {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let path =
            std::env::temp_dir().join(format!("reader-gray-scan-{}.TIFF", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let fixture_file = FixtureFile(path);
        file.write_all(&fixture(ByteOrder::Big, GRAY, &STRIPS))
            .unwrap();
        drop(file);

        let page = crate::comic::ImagePage::open(&fixture_file.0).unwrap();
        assert_eq!(page.page_size(), (3.0, 5.0));
        assert_eq!(page.fit(3, 5, 0).unwrap(), (1..=15).collect::<Vec<_>>());
    }
}
