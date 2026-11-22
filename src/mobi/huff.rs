//! Decode Mobipocket's static Huffman codes and shared phrase dictionary.
//! Records are borrowed from the bounded PDB. Each decoded phrase writes directly
//! into the destination text record under the same byte and work budgets.
use super::{MobiError, u16_at, u32_at};

pub(crate) const COMPRESSION: u16 = 17_480;
const MAX_DICTIONARY_RECORDS: usize = 64;
const MAX_PHRASES: usize = 65_536;
const MAX_DEPTH: usize = 32;

type Result<T> = std::result::Result<T, MobiError>;

#[derive(Clone, Copy)]
struct Phrase<'a> {
    bytes: &'a [u8],
    literal: bool,
}

pub(super) struct Dictionary<'a> {
    prefix: [u32; 256],
    bounds: [(u32, u32); 33],
    phrases: Vec<Phrase<'a>>,
}

impl<'a> Dictionary<'a> {
    pub fn read(bytes: &'a [u8], offsets: &[usize], header: &[u8], texts: usize) -> Result<Self> {
        if header.get(16..20) != Some(b"MOBI") || u32_at(header, 20)? < 104 {
            return Err(MobiError::InvalidHuffman);
        }
        let first = u32_at(header, 112)? as usize;
        let count = u32_at(header, 116)? as usize;
        if first <= texts || !(2..=MAX_DICTIONARY_RECORDS + 1).contains(&count) {
            return Err(MobiError::InvalidHuffman);
        }
        let end = first.checked_add(count).ok_or(MobiError::InvalidHuffman)?;
        let group = offsets.get(first..=end).ok_or(MobiError::InvalidHuffman)?;
        let mut records = group.windows(2).map(|pair| &bytes[pair[0]..pair[1]]);
        let huff = records.next().ok_or(MobiError::InvalidHuffman)?;
        if huff.get(..4) != Some(b"HUFF") || u32_at(huff, 4)? != 24 {
            return Err(MobiError::InvalidHuffman);
        }
        let table = |field, length| -> Result<&[u8]> {
            let start = u32_at(huff, field)? as usize;
            if start < 24 {
                return Err(MobiError::InvalidHuffman);
            }
            huff.get(start..start.checked_add(length).ok_or(MobiError::InvalidHuffman)?)
                .ok_or(MobiError::InvalidHuffman)
        };
        let mut prefix = [0; 256];
        for (entry, bytes) in prefix.iter_mut().zip(table(8, 1024)?.as_chunks::<4>().0) {
            *entry = u32::from_be_bytes(*bytes);
            if *entry & 31 == 0 {
                return Err(MobiError::InvalidHuffman);
            }
        }
        let mut bounds = [(0, 0); 33];
        for (entry, pair) in bounds[1..]
            .iter_mut()
            .zip(table(12, 256)?.as_chunks::<8>().0)
        {
            *entry = (u32_at(pair, 0)?, u32_at(pair, 4)?);
        }

        let mut phrases = Vec::new();
        let mut layout = None;
        for cdic in records {
            if cdic.get(..4) != Some(b"CDIC") || u32_at(cdic, 4)? != 16 {
                return Err(MobiError::InvalidHuffman);
            }
            let total = u32_at(cdic, 8)? as usize;
            let bits = u32_at(cdic, 12)?;
            if !(1..=16).contains(&bits)
                || !(1..=MAX_PHRASES).contains(&total)
                || layout.is_some_and(|v| v != (total, bits))
            {
                return Err(MobiError::InvalidHuffman);
            }
            let per_record = 1usize << bits;
            if total.div_ceil(per_record) != count - 1 {
                return Err(MobiError::InvalidHuffman);
            }
            layout = Some((total, bits));
            let entries = per_record.min(total - phrases.len());
            let data = &cdic[16..];
            let offsets = data.get(..entries * 2).ok_or(MobiError::InvalidHuffman)?;
            for word in offsets.as_chunks::<2>().0 {
                let at = u16::from_be_bytes(*word) as usize;
                if at < offsets.len() {
                    return Err(MobiError::InvalidHuffman);
                }
                let field = u16_at(data, at)?;
                let len = usize::from(field & 0x7fff);
                let phrase = data
                    .get(at + 2..at + 2 + len)
                    .ok_or(MobiError::InvalidHuffman)?;
                phrases.push(Phrase {
                    bytes: phrase,
                    literal: field & 0x8000 != 0,
                });
            }
        }
        Ok(Self {
            prefix,
            bounds,
            phrases,
        })
    }

    pub fn expand(
        &self,
        packed: &[u8],
        limit: usize,
        out: &mut Vec<u8>,
        book_work: &mut usize,
    ) -> Result<()> {
        let end = out
            .len()
            .checked_add(limit)
            .ok_or(MobiError::TextTooLarge)?;
        // A 4 KiB text record ordinarily emits far fewer than 4,096 symbols.
        let record_budget = (limit * 16 + 4096).min(*book_work);
        let mut work = record_budget;
        let result = self.expand_into(packed, end, out, &mut Vec::new(), &mut work);
        *book_work -= record_budget - work;
        result
    }
    fn expand_into(
        &self,
        packed: &[u8],
        end: usize,
        out: &mut Vec<u8>,
        stack: &mut Vec<usize>,
        work: &mut usize,
    ) -> Result<()> {
        let bits = packed.len().checked_mul(8).ok_or(MobiError::TextTooLarge)?;
        let mut pos = 0;
        while pos < bits {
            *work = work.checked_sub(1).ok_or(MobiError::InvalidHuffman)?;
            // A short final code is padded for the lookup only. Its actual length
            // determines whether another complete symbol exists in this record.
            let tail = &packed[pos / 8..];
            let n = tail.len().min(5);
            let mut window = [0; 8];
            window[..n].copy_from_slice(&tail[..n]);
            let top = ((u64::from_be_bytes(window) << (pos % 8)) >> 32) as u32;
            let entry = self.prefix[(top >> 24) as usize];
            let mut width = (entry & 31) as usize;
            let maximum = if entry & 0x80 != 0 {
                entry >> 8
            } else {
                while (top >> (32 - width)) < self.bounds[width].0 {
                    width += 1;
                    if width > 32 {
                        return Err(MobiError::InvalidHuffman);
                    }
                }
                self.bounds[width].1
            };
            if width > bits - pos {
                break;
            }
            pos += width;
            let symbol = maximum
                .checked_sub(top >> (32 - width))
                .ok_or(MobiError::InvalidHuffman)? as usize;
            let phrase = self.phrases.get(symbol).ok_or(MobiError::InvalidHuffman)?;
            if phrase.literal {
                if phrase.bytes.len() > end - out.len() {
                    return Err(MobiError::TextTooLarge);
                }
                out.extend_from_slice(phrase.bytes);
            } else {
                if stack.len() >= MAX_DEPTH || stack.contains(&symbol) {
                    return Err(MobiError::InvalidHuffman);
                }
                stack.push(symbol);
                self.expand_into(phrase.bytes, end, out, stack, work)?;
                stack.pop();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], at: usize, value: u32) {
        buf[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn tables(nested: &[u8], literal: bool, short_lookup: bool) -> (Vec<u8>, Vec<usize>, Vec<u8>) {
        let mut huff = vec![0; 24 + 1024 + 256];
        huff[..4].copy_from_slice(b"HUFF");
        put(&mut huff, 4, 24);
        put(&mut huff, 8, 24);
        put(&mut huff, 12, 1048);
        for i in 0..256 {
            put(&mut huff, 24 + 4 * i, if short_lookup { 0x382 } else { 1 });
        }
        put(&mut huff, 1048, 2);
        put(&mut huff, 1048 + 12, 3);
        let mut cdic = vec![0; 24];
        cdic[..4].copy_from_slice(b"CDIC");
        put(&mut cdic, 4, 16);
        put(&mut cdic, 8, 4);
        put(&mut cdic, 12, 2);
        for (i, (text, is_literal)) in [
            (&b""[..], true),
            (&b"!"[..], true),
            (nested, literal),
            (&b"A"[..], true),
        ]
        .into_iter()
        .enumerate()
        {
            let at = (cdic.len() - 16) as u16;
            cdic[16 + 2 * i..18 + 2 * i].copy_from_slice(&at.to_be_bytes());
            cdic.extend_from_slice(
                &((text.len() as u16) | if is_literal { 0x8000 } else { 0 }).to_be_bytes(),
            );
            cdic.extend_from_slice(text);
        }
        let mut header = vec![0; 120];
        header[16..20].copy_from_slice(b"MOBI");
        put(&mut header, 20, 104);
        put(&mut header, 112, 2);
        put(&mut header, 116, 2);
        let ends = vec![0, 0, 0, huff.len(), huff.len() + cdic.len()];
        huff.extend(cdic);
        (huff, ends, header)
    }
    #[test]
    #[ignore = "supply an unencrypted MOBI or KF8 and independent libmobi raw text"]
    fn local_huff_record_stream_matches_independent_decoder_byte_for_byte() {
        use std::{env, fs};
        let bytes = fs::read(env::var_os("READER_TEST_MOBI").unwrap()).unwrap();
        let raw = fs::read(env::var_os("READER_TEST_HUFF_RAW").unwrap()).unwrap();
        let record = env::var("READER_TEST_HUFF_EDITION")
            .ok()
            .map_or(0, |n| n.parse::<usize>().unwrap());
        let offsets = super::super::record_offsets(&bytes).unwrap();
        let records = &offsets[record..];
        let header = &bytes[records[0]..records[1]];
        assert_eq!(u16_at(header, 0), Ok(COMPRESSION));
        assert_eq!(u16_at(header, 12), Ok(0));
        let decoded = super::super::decode_records(
            &bytes,
            records,
            header,
            u16_at(header, 242).unwrap_or(0),
            u32_at(header, 4).unwrap() as usize,
        )
        .unwrap();
        assert!(
            decoded == raw,
            "uncompressed source differs from independent extraction"
        );
        eprintln!(
            "verified {} uncompressed bytes from {} text records",
            decoded.len(),
            u16_at(header, 8).unwrap()
        );
    }

    #[test]
    fn huff_short_and_long_lookups_expand_nested_phrases_within_record_limits() {
        for direct in [true, false] {
            let (bytes, records, header) = tables(&[0x22], false, direct);
            let dict = Dictionary::read(&bytes, &records, &header, 1).unwrap();
            let mut text = b"prefix ".to_vec();
            dict.expand(&[0x13], 6, &mut text, &mut 4096).unwrap();
            assert_eq!(&text, b"prefix AA!A!A");
            let mut budget = 1;
            assert_eq!(
                dict.expand(&[0x13], 4096, &mut Vec::new(), &mut budget),
                Err(MobiError::InvalidHuffman)
            );
            assert_eq!(budget, 0);
            assert_eq!(
                dict.expand(&[0x13], 5, &mut Vec::new(), &mut 4096),
                Err(MobiError::TextTooLarge)
            );
        }
    }
    #[test]
    fn huff_refuses_invalid_indices_cycles_or_truncated_dictionary_records() {
        let (mut bytes, records, header) = tables(&[0x55], false, true);
        let dict = Dictionary::read(&bytes, &records, &header, 1).unwrap();
        assert_eq!(
            dict.expand(&[0x55], 4096, &mut Vec::new(), &mut 4096),
            Err(MobiError::InvalidHuffman)
        );
        bytes[records[3] + 16..records[3] + 18].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(Dictionary::read(&bytes, &records, &header, 1).is_err());
        assert!(Dictionary::read(&bytes, &records[..3], &header, 1).is_err());
    }
}
