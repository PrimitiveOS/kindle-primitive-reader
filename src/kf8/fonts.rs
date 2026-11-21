//! Bounded FONT records from an unprotected KF8 container. Keep only the record
//! ranges after opening a book; read and inflate a face when a chapter requests it.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;

use flate2::{Decompress, FlushDecompress, Status};

use super::{Records, base32};
use crate::Result;

const HEADER: usize = 24;
const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
const MAX_FONT_RECORD: usize = MAX_FONT_BYTES + 64 * 1024;
const MAX_INDEXED_FONTS: usize = 128;
const OBFUSCATED_PREFIX: usize = 1040;

pub(super) fn index(
    records: &Records<'_>,
    first_resource: usize,
    first_nontext: usize,
) -> BTreeMap<usize, Range<usize>> {
    let mut fonts = BTreeMap::new();
    if first_resource < first_nontext {
        return fonts;
    }
    for (offset, bounds) in records.offsets.windows(2).skip(first_resource).enumerate() {
        let range = bounds[0]..bounds[1];
        if (HEADER..=MAX_FONT_RECORD).contains(&range.len())
            && records.bytes[range.clone()].starts_with(b"FONT")
        {
            fonts.insert(offset + 1, range);
            if fonts.len() == MAX_INDEXED_FONTS {
                break;
            }
        }
    }
    fonts
}

pub(crate) fn reference(url: &str) -> Option<usize> {
    let rest = url.strip_prefix("kindle:embed:")?;
    let number = match rest.split_once('?') {
        Some((value, query)) if query.starts_with("mime=") => value,
        Some(_) => return None,
        None => rest,
    };
    base32(number, 4).ok().filter(|number| *number > 0)
}

pub(crate) struct FontSource {
    pub file: File,
    pub records: BTreeMap<usize, Range<usize>>,
}

impl FontSource {
    pub(crate) fn read(&mut self, number: usize, limit: usize) -> Result<Vec<u8>> {
        let range = self
            .records
            .get(&number)
            .ok_or("The publication font is missing")?;
        self.file.seek(SeekFrom::Start(range.start as u64))?;
        decode(
            (&mut self.file).take(range.len() as u64),
            range.len(),
            limit,
        )
    }
}

fn decode(mut input: impl Read, length: usize, limit: usize) -> Result<Vec<u8>> {
    if !(HEADER..=MAX_FONT_RECORD).contains(&length) {
        return Err("Invalid publication font length".into());
    }
    let mut header = [0; HEADER];
    input.read_exact(&mut header)?;
    let number = |at| u32::from_be_bytes(header[at..at + 4].try_into().unwrap()) as usize;
    let size = number(4);
    let flags = number(8);
    let start = number(12);
    if &header[..4] != b"FONT"
        || size < 12
        || size > limit.min(MAX_FONT_BYTES)
        || flags & !3 != 0
        || !(HEADER..=length).contains(&start)
    {
        return Err("Unsupported publication font header".into());
    }
    let mut key = Vec::new();
    if flags & 2 != 0 {
        let key_len = number(16);
        let key_start = number(20);
        if key_len == 0
            || key_len > 1040
            || key_start < HEADER
            || key_start.checked_add(key_len).is_none_or(|end| end > start)
        {
            return Err("Invalid publication font key".into());
        }
        std::io::copy(
            &mut input.by_ref().take((key_start - HEADER) as u64),
            &mut std::io::sink(),
        )?;
        key.resize(key_len, 0);
        input.read_exact(&mut key)?;
        std::io::copy(
            &mut input.by_ref().take((start - key_start - key_len) as u64),
            &mut std::io::sink(),
        )?;
    } else {
        std::io::copy(
            &mut input.by_ref().take((start - HEADER) as u64),
            &mut std::io::sink(),
        )?;
    }
    let mut data = vec![0; size + 1];
    let mut written = 0usize;
    let mut payload_offset = 0usize;
    let mut decoder = (flags & 1 != 0).then(|| Decompress::new(true));
    let mut finished = false;
    let mut padding = 0usize;
    let mut chunk = [0; 16 * 1024];
    while payload_offset < length - start {
        let n = (length - start - payload_offset).min(chunk.len());
        input.read_exact(&mut chunk[..n])?;
        // Palm record alignment bytes are outside the font payload and its optional masking.
        let zero_tail = chunk[..n]
            .iter()
            .rev()
            .take(3)
            .take_while(|&&v| v == 0)
            .count();
        if !key.is_empty() && payload_offset < OBFUSCATED_PREFIX {
            for (i, byte) in chunk[..n.min(OBFUSCATED_PREFIX - payload_offset)]
                .iter_mut()
                .enumerate()
            {
                *byte ^= key[(payload_offset + i) % key.len()];
            }
        }
        payload_offset += n;
        let mut consumed = 0;
        if !finished {
            if let Some(decoder) = &mut decoder {
                let before = decoder.total_in();
                let status =
                    decoder.decompress(&chunk[..n], &mut data[written..], FlushDecompress::None)?;
                consumed = (decoder.total_in() - before) as usize;
                written = decoder.total_out() as usize;
                finished = status == Status::StreamEnd;
                if written > size || (!finished && consumed != n) {
                    return Err("Invalid publication font stream".into());
                }
            } else {
                consumed = n.min(size - written);
                data[written..written + consumed].copy_from_slice(&chunk[..consumed]);
                written += consumed;
                finished = written == size;
            }
        }
        padding += n - consumed;
        if padding > 3 || n - consumed > zero_tail {
            return Err("Invalid publication font padding".into());
        }
    }
    if !finished
        || written != size
        || !(data.starts_with(b"\0\x01\0\0") || data.starts_with(b"OTTO"))
    {
        return Err("Invalid publication font data".into());
    }
    data.truncate(written);
    Ok(data)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    pub(crate) fn record(data: &[u8], compressed: bool, masked: bool) -> Vec<u8> {
        let mut payload = if compressed {
            let mut stream =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            stream.write_all(data).unwrap();
            stream.finish().unwrap()
        } else {
            data.to_vec()
        };
        let key = if masked {
            b"publication fixture".as_slice()
        } else {
            &[]
        };
        if !key.is_empty() {
            for (n, b) in payload.iter_mut().take(1040).enumerate() {
                *b ^= key[n % key.len()];
            }
        }
        let mut out = b"FONT".to_vec();
        for value in [
            data.len(),
            usize::from(compressed) + 2 * usize::from(masked),
            HEADER + key.len(),
            key.len(),
            HEADER,
        ] {
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        out.extend_from_slice(key);
        out.extend_from_slice(&payload);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    #[test]
    fn kf8_font_streams_stored_compressed_and_prefix_masked_faces() {
        let face = std::fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
        for compressed in [false, true] {
            for masked in [false, true] {
                let bytes = record(&face, compressed, masked);
                assert_eq!(
                    decode(bytes.as_slice(), bytes.len(), face.len()).unwrap(),
                    face
                );
                assert!(decode(bytes.as_slice(), bytes.len(), face.len() - 1).is_err());
            }
        }
        for bad in [
            "kindle:embed:0000",
            "kindle:embed:A00Z",
            "https://host/face.ttf",
            "kindle:embed:0001?full=",
        ] {
            assert!(reference(bad).is_none());
        }
        assert_eq!(
            reference("kindle:embed:0010?mime=application/x-font-ttf"),
            Some(32)
        );
    }

    #[test]
    fn kf8_font_checks_source_bounds_padding_and_complete_zlib_trailer() {
        let face = std::fs::read(crate::test_fixture("fonts/Silkscreen-Regular.ttf")).unwrap();
        let good = record(&face, true, false);
        for (at, v) in [
            (0, 0),
            (4, u32::MAX),
            (4, 16),
            (8, 4),
            (12, 22),
            (12, u32::MAX),
            (8, 3),
        ] {
            let mut bad = good.clone();
            bad[at..at + 4].copy_from_slice(&v.to_be_bytes());
            assert!(
                decode(bad.as_slice(), bad.len(), MAX_FONT_BYTES).is_err(),
                "header {at}"
            );
        }
        let short = &good[..good.len() - 8];
        assert!(decode(short, short.len(), MAX_FONT_BYTES).is_err());
        let mut trailing = good.clone();
        trailing.push(0xff);
        assert!(decode(trailing.as_slice(), trailing.len(), MAX_FONT_BYTES).is_err());
        let mut short_key = record(&face, false, true);
        short_key[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(short_key.as_slice(), short_key.len(), MAX_FONT_BYTES).is_err());
        let mut unsupported = face.clone();
        unsupported[..4].copy_from_slice(b"HTML");
        let data = record(&unsupported, false, false);
        assert!(decode(data.as_slice(), data.len(), MAX_FONT_BYTES).is_err());
    }
}
