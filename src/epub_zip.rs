//! A bounded, read-only EPUB ZIP view. Recovery replaces only tail metadata in memory.
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};

use crate::Result;
use crate::epub::{MAX_ARCHIVE, MAX_ENTRIES};
use zip::ZipArchive;

const CENTRAL_LIMIT: usize = 4 * 1024 * 1024;
const FOOTER: usize = 22;
const CENTRAL: usize = 46;
const LOCAL: usize = 30;
const MAX_PADDING: usize = 512;
const ZIP64_LOCATOR: usize = 20;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "The EPUB ZIP directory is invalid or exceeds Reader's limits",
    )
}
fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}

struct Directory {
    entries: usize,
    start: u64,
    size: usize,
    recorded_start: u64,
    padding: usize,
    footer: Vec<u8>,
}

impl Directory {
    fn read(input: &mut (impl Read + Seek), len: u64, epub: bool) -> io::Result<Self> {
        let pad_limit = if epub { MAX_PADDING } else { 0 };
        let first =
            len.saturating_sub((ZIP64_LOCATOR + FOOTER + usize::from(u16::MAX) + pad_limit) as u64);
        input.seek(SeekFrom::Start(first))?;
        let mut tail = vec![0; (len - first) as usize];
        input.read_exact(&mut tail)?;
        // A normal ZIP comment may itself contain a short signature-like string.
        for end in (0..=tail.len().saturating_sub(FOOTER)).rev() {
            let r = &tail[end..];
            if r.len() < FOOTER || !r.starts_with(b"PK\x05\x06") {
                continue;
            }
            let used = FOOTER + usize::from(u16le(r, 20));
            let Some(padding) = r.get(used..) else {
                continue;
            };
            if padding.len() > pad_limit || padding.iter().any(|&b| b != 0) {
                continue;
            }
            let count = usize::from(u16le(r, 10));
            let size = u32le(r, 12) as usize;
            let offset = u64::from(u32le(r, 16));
            let Some(start) = (first + end as u64).checked_sub(size as u64) else {
                continue;
            };
            if u16le(r, 4) != 0
                || u16le(r, 6) != 0
                || usize::from(u16le(r, 8)) != count
                || !(1..=MAX_ENTRIES).contains(&count)
                || !(count * CENTRAL..=CENTRAL_LIMIT).contains(&size)
                || offset == u64::from(u32::MAX)
                || (!epub && offset != start)
                || end
                    .checked_sub(ZIP64_LOCATOR)
                    .is_some_and(|p| tail[p..].starts_with(b"PK\x06\x07"))
            {
                continue;
            }
            input.rewind()?;
            return Ok(Self {
                entries: count,
                start,
                size,
                recorded_start: offset,
                padding: padding.len(),
                footer: r[..used].to_vec(),
            });
        }
        Err(invalid())
    }
}

pub(crate) fn check_zip_directory(input: &mut (impl Read + Seek), len: u64) -> Result<usize> {
    Ok(Directory::read(input, len, false)?.entries)
}

pub(crate) struct Archive {
    pub(crate) zip: ZipArchive<EpubInput>,
    /// Original uncompressed lengths for members whose payload could not be recovered.
    pub(crate) missing: HashMap<Vec<u8>, u32>,
}

pub(crate) fn open(file: File, len: u64) -> Result<Archive> {
    if len > MAX_ARCHIVE {
        return Err(invalid().into());
    }
    let mut input = BufReader::new(file);
    let dir = Directory::read(&mut input, len, true)?;
    let (entries, replacement, missing) = if dir.start != dir.recorded_start {
        let recovered = recovered_directory(&mut input, &dir)?;
        (recovered.kept, Some(recovered.directory), recovered.missing)
    } else {
        (dir.entries, None, HashMap::new())
    };
    let logical_len = replacement
        .as_ref()
        .map_or(len - dir.padding as u64, |v| dir.start + v.len() as u64);
    input.rewind()?;
    let view = EpubInput {
        disk: input,
        local_end: dir.start,
        replacement,
        logical_len,
        position: 0,
    };
    let archive = ZipArchive::new(view)?;
    if archive.len() != entries || archive.central_directory_start() != dir.start {
        return Err(invalid().into());
    }
    Ok(Archive {
        zip: archive,
        missing,
    })
}

pub(crate) struct EpubInput {
    disk: BufReader<File>,
    local_end: u64,
    replacement: Option<Vec<u8>>,
    logical_len: u64,
    position: u64,
}

impl EpubInput {
    /// Library identities always hash the original on-disk central directory and footer.
    pub(crate) fn original_directory(mut self) -> io::Result<(BufReader<File>, u64)> {
        self.disk.seek(SeekFrom::Start(self.local_end))?;
        Ok((self.disk, self.local_end))
    }
}

impl Read for EpubInput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self
            .logical_len
            .saturating_sub(self.position)
            .min(buf.len() as u64) as usize;
        if available == 0 {
            return Ok(0);
        }
        let buf = &mut buf[..available];
        let count = match &self.replacement {
            Some(tail) if self.position >= self.local_end => {
                let offset = (self.position - self.local_end) as usize;
                buf.copy_from_slice(&tail[offset..offset + available]);
                available
            }
            Some(_) => {
                let length = available.min((self.local_end - self.position) as usize);
                self.disk.read(&mut buf[..length])?
            }
            None => self.disk.read(buf)?,
        };
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for EpubInput {
    fn seek(&mut self, seek: SeekFrom) -> io::Result<u64> {
        let to = match seek {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(n) => self.position.checked_add_signed(n).ok_or_else(invalid)?,
            SeekFrom::End(n) => self.logical_len.checked_add_signed(n).ok_or_else(invalid)?,
        };
        if self.replacement.is_none() || to < self.local_end {
            self.disk.seek(SeekFrom::Start(to))?;
        }
        self.position = to;
        Ok(to)
    }
}

struct CentralMember {
    record: std::ops::Range<usize>,
    flags: u16,
    method: u16,
    crc: u32,
    packed: u32,
    size: u32,
}

struct LocalMember {
    index: usize,
    start: u64,
    end: u64,
}

struct Members<'a> {
    list: Vec<CentralMember>,
    names: HashMap<&'a [u8], usize>,
    longest_name: usize,
}

impl<'a> Members<'a> {
    fn parse(bytes: &'a [u8], count: usize) -> io::Result<Self> {
        let mut list = Vec::with_capacity(count);
        let mut names = HashMap::with_capacity(count);
        let mut pos = 0;
        for index in 0..count {
            let c = bytes.get(pos..pos + CENTRAL).ok_or_else(invalid)?;
            if !c.starts_with(b"PK\x01\x02") || u16le(c, 34) != 0 {
                return Err(invalid());
            }
            let name_end = pos + CENTRAL + usize::from(u16le(c, 28));
            let end = name_end + usize::from(u16le(c, 30)) + usize::from(u16le(c, 32));
            let name = bytes.get(pos + CENTRAL..name_end).ok_or_else(invalid)?;
            let member = CentralMember {
                record: pos..end,
                flags: u16le(c, 8),
                method: u16le(c, 10),
                crc: u32le(c, 16),
                packed: u32le(c, 20),
                size: u32le(c, 24),
            };
            if name.is_empty()
                || end > bytes.len()
                || u32le(c, 42) == u32::MAX
                || member.flags & 1 != 0
                || !matches!(member.method, 0 | 8)
                || member.packed == u32::MAX
                || member.size == u32::MAX
                || names.insert(name, index).is_some()
            {
                return Err(invalid());
            }
            list.push(member);
            pos = end;
        }
        if pos != bytes.len() {
            return Err(invalid());
        }
        let longest_name = names
            .keys()
            .map(|name| name.len())
            .max()
            .ok_or_else(invalid)?;
        Ok(Self {
            list,
            names,
            longest_name,
        })
    }

    fn local(
        &self,
        input: &mut (impl Read + Seek),
        start: u64,
        directory: u64,
    ) -> io::Result<Option<LocalMember>> {
        if start + LOCAL as u64 > directory {
            return Ok(None);
        }
        input.seek(SeekFrom::Start(start))?;
        let mut header = [0; LOCAL];
        input.read_exact(&mut header)?;
        if !header.starts_with(b"PK\x03\x04") {
            return Ok(None);
        }
        let name_len = usize::from(u16le(&header, 26));
        let data_start = start + LOCAL as u64 + name_len as u64 + u64::from(u16le(&header, 28));
        if name_len == 0 || name_len > self.longest_name || data_start > directory {
            return Ok(None);
        }
        let mut name = vec![0; name_len];
        input.read_exact(&mut name)?;
        let Some(&index) = self.names.get(name.as_slice()) else {
            return Ok(None);
        };
        let c = &self.list[index];
        if c.flags != u16le(&header, 6) || c.method != u16le(&header, 8) {
            return Ok(None);
        }
        let fields = [c.crc, c.packed, c.size];
        let descriptor = c.flags & 8 != 0;
        for (i, expected) in fields.iter().enumerate() {
            let value = u32le(&header, 14 + i * 4);
            if value != *expected && !(descriptor && value == 0) {
                return Ok(None);
            }
        }
        let mut end = data_start + u64::from(c.packed);
        if end > directory {
            return Ok(None);
        }
        if descriptor {
            if end + 12 > directory {
                return Ok(None);
            }
            input.seek(SeekFrom::Start(end))?;
            let mut desc = [0; 12];
            input.read_exact(&mut desc)?;
            if desc.starts_with(b"PK\x07\x08") {
                if end + 16 > directory {
                    return Ok(None);
                }
                desc.copy_within(4.., 0);
                input.read_exact(&mut desc[8..])?;
                end += 4;
            }
            if fields
                .iter()
                .enumerate()
                .any(|(i, v)| u32le(&desc, i * 4) != *v)
            {
                return Ok(None);
            }
            end += 12;
        }
        Ok(Some(LocalMember { index, start, end }))
    }

    /// The next compressed stream may have lost bytes. Resume at a complete central/local
    /// header pair. Candidate scanning is bounded even if the byte pattern repeats in data.
    fn next_local(
        &self,
        input: &mut (impl Read + Seek),
        mut start: u64,
        directory: u64,
        budget: &mut usize,
    ) -> io::Result<Option<LocalMember>> {
        let mut block = [0u8; 16 * 1024];
        while start + LOCAL as u64 <= directory {
            let length = (directory - start).min(block.len() as u64) as usize;
            input.seek(SeekFrom::Start(start))?;
            input.read_exact(&mut block[..length])?;
            for (i, window) in block[..length].windows(4).enumerate() {
                if window != b"PK\x03\x04" {
                    continue;
                }
                *budget = budget.checked_sub(1).ok_or_else(invalid)?;
                if let Some(found) = self.local(input, start + i as u64, directory)? {
                    return Ok(Some(found));
                }
            }
            start += (length - 3) as u64;
        }
        Ok(None)
    }
}

struct Recovered {
    kept: usize,
    directory: Vec<u8>,
    missing: HashMap<Vec<u8>, u32>,
}

fn recovered_directory(input: &mut (impl Read + Seek), dir: &Directory) -> io::Result<Recovered> {
    input.seek(SeekFrom::Start(dir.start))?;
    let mut bytes = vec![0; dir.size];
    input.read_exact(&mut bytes)?;
    let members = Members::parse(&bytes, dir.entries)?;
    let mut at = members.local(input, 0, dir.start)?.ok_or_else(invalid)?;
    let mut offsets = vec![None; dir.entries];
    let mut budget = MAX_ENTRIES * 8;
    loop {
        let next = if at.end == dir.start {
            None
        } else {
            members.local(input, at.end, dir.start)?
        };
        if at.end == dir.start || next.is_some() {
            if offsets[at.index].replace(at.start as u32).is_some() {
                return Err(invalid());
            }
            let Some(next) = next else { break };
            at = next;
        } else {
            // The declared payload does not end at another validated boundary. Exclude this
            // member. Consumers see a missing resource instead of reading another member's bytes.
            let Some(next) = members.next_local(input, at.start + 4, dir.start, &mut budget)?
            else {
                break;
            };
            at = next;
        }
    }
    let missing = members
        .names
        .iter()
        .filter_map(|(&name, &index)| {
            offsets[index]
                .is_none()
                .then(|| (name.to_vec(), members.list[index].size))
        })
        .collect();
    let mut corrected = Vec::with_capacity(dir.size + dir.footer.len());
    let mut count = 0;
    for (member, offset) in members.list.iter().zip(offsets) {
        if let Some(offset) = offset {
            let pos = corrected.len();
            corrected.extend_from_slice(&bytes[member.record.clone()]);
            corrected[pos + 42..pos + 46].copy_from_slice(&offset.to_le_bytes());
            count += 1;
        }
    }
    if count == 0 {
        return Err(invalid());
    }
    let size = corrected.len() as u32;
    let mut footer = dir.footer.clone();
    footer[8..10].copy_from_slice(&(count as u16).to_le_bytes());
    footer[10..12].copy_from_slice(&(count as u16).to_le_bytes());
    footer[12..16].copy_from_slice(&size.to_le_bytes());
    footer[16..20].copy_from_slice(&(dir.start as u32).to_le_bytes());
    corrected.extend_from_slice(&footer);
    Ok(Recovered {
        kept: count,
        directory: corrected,
        missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn one_member() -> Vec<u8> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("chapter", SimpleFileOptions::default())
            .unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn foreign_archives_still_require_a_footer_at_eof_and_exact_central_offsets() {
        let mut bytes = one_member();
        let footer = bytes.len() - FOOTER;
        bytes.push(0);
        let count = bytes.len() as u64;
        assert!(check_zip_directory(&mut Cursor::new(&bytes), count).is_err());
        assert_eq!(
            Directory::read(&mut Cursor::new(&bytes), count, true)
                .unwrap()
                .padding,
            1
        );

        bytes.pop();
        let recorded = u32le(&bytes, footer + 16);
        bytes[footer + 16..footer + 20].copy_from_slice(&(recorded + 4).to_le_bytes());
        let count = bytes.len() as u64;
        assert!(check_zip_directory(&mut Cursor::new(&bytes), count).is_err());
        let dir = Directory::read(&mut Cursor::new(&bytes), count, true).unwrap();
        assert_eq!(dir.start, u64::from(recorded));
        assert_eq!(dir.recorded_start, u64::from(recorded) + 4);

        bytes[footer + 16..footer + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Directory::read(&mut Cursor::new(&bytes), count, true).is_err());
    }

    #[test]
    fn streaming_members_require_the_exact_crc_and_lengths_in_their_data_descriptor() {
        for signature in [false, true] {
            let mut bytes = one_member();
            let original =
                Directory::read(&mut Cursor::new(&bytes), bytes.len() as u64, true).unwrap();
            let central = original.start as usize;
            let flags = u16le(&bytes, central + 8) | 8;
            bytes[central + 8..central + 10].copy_from_slice(&flags.to_le_bytes());
            bytes[6..8].copy_from_slice(&flags.to_le_bytes());
            let mut descriptor = Vec::new();
            if signature {
                descriptor.extend_from_slice(b"PK\x07\x08");
            }
            descriptor.extend_from_slice(&bytes[central + 16..central + 28]);
            bytes[14..26].fill(0);
            bytes.splice(central..central, descriptor.clone());
            let dir = Directory::read(&mut Cursor::new(&bytes), bytes.len() as u64, true).unwrap();
            assert_ne!(dir.start, dir.recorded_start);
            assert_eq!(
                recovered_directory(&mut Cursor::new(&bytes), &dir)
                    .unwrap()
                    .kept,
                1
            );

            bytes[central + descriptor.len() - 1] ^= 1;
            assert!(recovered_directory(&mut Cursor::new(&bytes), &dir).is_err());
        }
    }

    #[test]
    fn repair_only_trusts_unique_unencrypted_supported_central_entries() {
        let bytes = one_member();
        let dir = Directory::read(&mut Cursor::new(&bytes), bytes.len() as u64, true).unwrap();
        let record = &bytes[dir.start as usize..dir.start as usize + dir.size];
        assert!(Members::parse(record, 1).is_ok());
        assert!(Members::parse(&[record, record].concat(), 2).is_err());
        for (offset, value) in [(8, 1u16), (10, 93), (34, 1)] {
            let mut altered = record.to_vec();
            altered[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            assert!(Members::parse(&altered, 1).is_err());
        }
        for offset in [20, 24, 42] {
            let mut altered = record.to_vec();
            altered[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(Members::parse(&altered, 1).is_err());
        }
    }
}
