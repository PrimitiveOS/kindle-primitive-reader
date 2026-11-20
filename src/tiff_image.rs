//! Decode a bounded subset of the Adobe TIFF 6.0 strip layout.
//! https://pdfa.org/wp-content/uploads/2020/07/TIFF6.pdf (sections 2–4 and 8)

use std::fmt;
use std::ops::Range;

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
            Self::Unsupported => "Reader supports single-page, uncompressed grayscale TIFF stored in strips with top-left orientation (1-bit or 8-bit)",
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

    let bits = image.scalar(258, SHORT_ONLY, 1)?;
    let photometric = image.required(262, SHORT_ONLY)?;
    let fill_order = image.scalar(266, SHORT_ONLY, 1)?;
    if !matches!(bits, 1 | 8)
        || photometric > 1
        || !(fill_order == 1 || (bits == 1 && fill_order == 2))
        || image.scalar(259, SHORT_ONLY, 1)? != 1 // Compression
        || image.scalar(274, SHORT_ONLY, 1)? != 1 // Orientation
        || image.scalar(277, SHORT_ONLY, 1)? != 1 // SamplesPerPixel
        || image.scalar(284, SHORT_ONLY, 1)? != 1 // PlanarConfiguration
        || image.scalar(317, SHORT_ONLY, 1)? != 1 // Predictor
        || image.scalar(339, SHORT_ONLY, 1)? != 1 // SampleFormat
        || image.scalar(254, LONG_ONLY, 0)? != 0 // NewSubfileType
        || image.scalar(255, SHORT_ONLY, 1)? != 1
    // SubfileType
    {
        return Err(TiffError::Unsupported);
    }

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
    if let (Some(a), Some(b)) = (&offsets.external, &lengths.external)
        && overlaps(a, b)
    {
        return Err(TiffError::Strips);
    }
    let row_bytes = (width * bits).div_ceil(8) as usize;
    let occupied = [
        Some(&image.range),
        offsets.external.as_ref(),
        lengths.external.as_ref(),
    ];
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
            || length != rows as usize * row_bytes
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

    let invert = if photometric == 0 { 255 } else { 0 };
    let mut pixels = Vec::with_capacity(pixel_count as usize);
    for range in ranges {
        let strip = &bytes[range];
        if bits == 8 {
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
    GrayImage::from_raw(width, height, pixels).ok_or(TiffError::Strips)
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
            if matches!(tag, 322..=325 | 330 | 338) {
                return Err(TiffError::Unsupported); // Tiles, SubIFDs, or extra samples.
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
        let mut fields = [
            (256u16, SHORT, vec![geometry.width]),
            (257, LONG, vec![geometry.height]),
            (258, SHORT, vec![geometry.bits]),
            (259, SHORT, vec![1]),
            (262, SHORT, vec![geometry.photometric]),
            (266, SHORT, vec![geometry.fill_order]),
            (273, LONG, vec![0; strips.len()]),
            (274, SHORT, vec![1]),
            (277, SHORT, vec![1]),
            (278, LONG, vec![geometry.rows_per_strip]),
            (
                279,
                SHORT,
                strips.iter().map(|strip| strip.len() as u32).collect(),
            ),
            (284, SHORT, vec![1]),
            (339, SHORT, vec![1]),
        ];
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
            (259, 32773),
            (262, 2),
            (266, 2),
            (274, 6),
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
