//! Strict, bounded MS-CFB reader for the Word binary-document candidate.
//! This module contains no Word parser and is not wired to the picker.
use std::collections::{BTreeMap, BTreeSet};

const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;
const FAT: u32 = 0xffff_fffd;
const DIFAT: u32 = 0xffff_fffc;
const NO_STREAM: u32 = 0xffff_ffff;
pub(super) const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_DIRECTORY: usize = 4096;
const MAX_FAT_SECTORS: usize = 1024;
const MAX_STREAM_TOTAL: usize = 24 * 1024 * 1024;

pub(super) struct Compound {
    pub(super) streams: BTreeMap<String, Vec<u8>>,
}

fn u16le(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("Truncated compound file")?
            .try_into()
            .map_err(|_| "Truncated compound file")?,
    ))
}
fn u32le(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("Truncated compound file")?
            .try_into()
            .map_err(|_| "Truncated compound file")?,
    ))
}
fn u64le(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .ok_or("Truncated compound file")?
            .try_into()
            .map_err(|_| "Truncated compound file")?,
    ))
}
fn sector(data: &[u8], size: usize, id: u32) -> Result<&[u8], String> {
    let at = (id as usize)
        .checked_add(1)
        .and_then(|n| n.checked_mul(size))
        .ok_or("Compound sector overflow")?;
    data.get(at..at.checked_add(size).ok_or("Compound sector overflow")?)
        .ok_or_else(|| "Compound sector outside file".into())
}

fn claim(owners: &mut [bool], id: u32) -> Result<usize, String> {
    let index = usize::try_from(id).map_err(|_| "Invalid compound sector")?;
    let used = owners
        .get_mut(index)
        .ok_or("Compound sector outside file")?;
    if *used {
        return Err("Compound sector is repeated or overlaps another chain".into());
    }
    *used = true;
    Ok(index)
}

fn chain(
    first: u32,
    entries: &[u32],
    owners: &mut [bool],
    max: usize,
    exact: Option<usize>,
) -> Result<Vec<u32>, String> {
    let mut result = Vec::new();
    let mut id = first;
    while id != END {
        if result.len() >= max {
            return Err("Compound chain exceeds its limit".into());
        }
        let index = claim(owners, id)?;
        result.push(id);
        id = *entries
            .get(index)
            .ok_or("Compound FAT entry is unavailable")?;
    }
    if exact.is_some_and(|count| count != result.len()) {
        return Err("Compound chain length does not match its declared size".into());
    }
    Ok(result)
}

fn collect(data: &[u8], size: usize, ids: &[u32], length: usize) -> Result<Vec<u8>, String> {
    if length
        > ids
            .len()
            .checked_mul(size)
            .ok_or("Compound stream overflow")?
    {
        return Err("Compound stream is truncated".into());
    }
    let mut result = Vec::with_capacity(length);
    for &id in ids {
        let part = sector(data, size, id)?;
        let take = length.saturating_sub(result.len()).min(size);
        result.extend_from_slice(&part[..take]);
    }
    Ok(result)
}

#[derive(Clone)]
struct Directory {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    first: u32,
    size: usize,
}

fn directory(data: &[u8], version: u16) -> Result<Vec<Directory>, String> {
    if data.is_empty() || data.len() % 128 != 0 || data.len() / 128 > MAX_DIRECTORY {
        return Err("Compound directory exceeds its limit".into());
    }
    let mut entries = Vec::with_capacity(data.len() / 128);
    for item in data.chunks_exact(128) {
        let kind = item[66];
        if !matches!(kind, 0 | 1 | 2 | 5) {
            return Err("Unsupported compound directory entry".into());
        }
        let name = if kind == 0 {
            String::new()
        } else {
            let length = usize::from(u16le(item, 64)?);
            if !(2..=64).contains(&length) || length % 2 != 0 || item[length - 2..length] != [0, 0]
            {
                return Err("Invalid compound entry name".into());
            }
            let units: Vec<u16> = item[..length - 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let value = String::from_utf16(&units).map_err(|_| "Invalid compound entry name")?;
            if value.contains('\0') {
                return Err("Invalid compound entry name".into());
            }
            value
        };
        let size64 = u64le(item, 120)?;
        if version == 3 && size64 >> 32 != 0 {
            return Err("Invalid version-3 compound stream size".into());
        }
        let size = usize::try_from(size64).map_err(|_| "Compound stream is too large")?;
        if size > MAX_FILE {
            return Err("Compound stream is too large".into());
        }
        entries.push(Directory {
            name,
            kind,
            left: u32le(item, 68)?,
            right: u32le(item, 72)?,
            child: u32le(item, 76)?,
            first: u32le(item, 116)?,
            size,
        });
    }
    Ok(entries)
}

/// Return only a tightly constrained root stream set. Nested storages,
/// orphan entries and unsupported root streams are rejected.
pub(super) fn read(data: &[u8]) -> Result<Compound, String> {
    if data.len() < 512
        || data.len() > MAX_FILE
        || data.get(..8) != Some(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1")
    {
        return Err("Not a bounded compound document".into());
    }
    if data[8..24].iter().any(|&b| b != 0)
        || u16le(data, 28)? != 0xfffe
        || u16le(data, 32)? != 6
        || data[34..40].iter().any(|&b| b != 0)
    {
        return Err("Unsupported compound header".into());
    }
    let version = u16le(data, 26)?;
    let sector_size = match (version, u16le(data, 30)?) {
        (3, 9) => 512usize,
        (4, 12) => 4096usize,
        _ => return Err("Unsupported compound sector size".into()),
    };
    if data.len() < sector_size || data.len() % sector_size != 0 || u32le(data, 56)? != 4096 {
        return Err("Invalid compound file alignment".into());
    }
    let sector_count = data.len() / sector_size - 1;
    let directory_sectors =
        usize::try_from(u32le(data, 40)?).map_err(|_| "Invalid compound directory size")?;
    if version == 3 && directory_sectors != 0 {
        return Err("Invalid version-3 directory size".into());
    }
    let fat_count = usize::try_from(u32le(data, 44)?).map_err(|_| "Invalid FAT size")?;
    let mini_fat_count = usize::try_from(u32le(data, 64)?).map_err(|_| "Invalid mini FAT size")?;
    let difat_count = usize::try_from(u32le(data, 72)?).map_err(|_| "Invalid DIFAT size")?;
    if fat_count == 0
        || fat_count > MAX_FAT_SECTORS
        || fat_count > sector_count
        || mini_fat_count > sector_count
        || difat_count > sector_count
    {
        return Err("Compound allocation tables exceed limits".into());
    }
    let mut owners = vec![false; sector_count];
    let mut fat_ids = Vec::with_capacity(fat_count);
    let mut difat_ids = Vec::with_capacity(difat_count);
    let mut saw_free = false;
    let mut take_fat = |id: u32| -> Result<(), String> {
        if id == FREE {
            saw_free = true;
            return Ok(());
        }
        if saw_free || fat_ids.len() >= fat_count {
            return Err("Invalid compound DIFAT entries".into());
        }
        fat_ids.push(id);
        Ok(())
    };
    for at in (76..512).step_by(4) {
        take_fat(u32le(data, at)?)?;
    }
    let mut next_difat = u32le(data, 68)?;
    for _ in 0..difat_count {
        let id = next_difat;
        claim(&mut owners, id)?;
        difat_ids.push(id);
        let chunk = sector(data, sector_size, id)?;
        for at in (0..sector_size - 4).step_by(4) {
            take_fat(u32le(chunk, at)?)?;
        }
        next_difat = u32le(chunk, sector_size - 4)?;
    }
    if next_difat != END || fat_ids.len() != fat_count {
        return Err("Invalid compound DIFAT chain".into());
    }
    let mut fat = Vec::with_capacity(fat_count * (sector_size / 4));
    for &id in &fat_ids {
        claim(&mut owners, id)?;
        let chunk = sector(data, sector_size, id)?;
        for at in (0..sector_size).step_by(4) {
            fat.push(u32le(chunk, at)?);
        }
    }
    if fat.len() < sector_count
        || fat_ids.iter().any(|&id| fat.get(id as usize) != Some(&FAT))
        || difat_ids
            .iter()
            .any(|&id| fat.get(id as usize) != Some(&DIFAT))
    {
        return Err("Invalid compound FAT markers".into());
    }
    let dir_ids = chain(
        u32le(data, 48)?,
        &fat,
        &mut owners,
        MAX_DIRECTORY / (sector_size / 128),
        if version == 4 {
            Some(directory_sectors)
        } else {
            None
        },
    )?;
    let dir_bytes = collect(data, sector_size, &dir_ids, dir_ids.len() * sector_size)?;
    let entries = directory(&dir_bytes, version)?;
    let root = entries.first().ok_or("Compound root is missing")?;
    if root.kind != 5
        || root.name != "Root Entry"
        || root.left != NO_STREAM
        || root.right != NO_STREAM
    {
        return Err("Invalid compound root".into());
    }
    let mut visited = BTreeSet::new();
    let mut pending = vec![root.child];
    let mut names = BTreeSet::new();
    let mut selected = Vec::new();
    while let Some(id) = pending.pop() {
        if id == NO_STREAM {
            continue;
        }
        let index = usize::try_from(id).map_err(|_| "Invalid compound directory index")?;
        if index == 0 || !visited.insert(index) {
            return Err("Repeated compound directory entry".into());
        }
        let entry = entries
            .get(index)
            .ok_or("Compound directory index outside range")?;
        if entry.kind != 2 || entry.child != NO_STREAM {
            return Err("Nested or non-stream compound entries are unsupported".into());
        }
        if !matches!(
            entry.name.as_str(),
            "WordDocument"
                | "0Table"
                | "1Table"
                | "\u{5}SummaryInformation"
                | "\u{5}DocumentSummaryInformation"
        ) || !names.insert(entry.name.to_ascii_lowercase())
        {
            return Err("Unsupported or duplicate compound stream".into());
        }
        pending.push(entry.left);
        pending.push(entry.right);
        selected.push(index);
    }
    if entries
        .iter()
        .enumerate()
        .skip(1)
        .any(|(i, e)| e.kind != 0 && !visited.contains(&i))
    {
        return Err("Orphan compound directory entry".into());
    }
    let mini_ids = chain(
        u32le(data, 60)?,
        &fat,
        &mut owners,
        sector_count,
        Some(mini_fat_count),
    )?;
    let mini_raw = collect(data, sector_size, &mini_ids, mini_ids.len() * sector_size)?;
    let mut mini_fat = Vec::with_capacity(mini_raw.len() / 4);
    for at in (0..mini_raw.len()).step_by(4) {
        mini_fat.push(u32le(&mini_raw, at)?);
    }
    let root_count = root.size.div_ceil(sector_size);
    let root_ids = chain(
        root.first,
        &fat,
        &mut owners,
        sector_count,
        Some(root_count),
    )?;
    let mini_stream = collect(data, sector_size, &root_ids, root.size)?;
    let mut mini_owners = vec![false; mini_stream.len().div_ceil(64)];
    let mut streams = BTreeMap::new();
    let mut total = 0usize;
    for index in selected {
        let entry = &entries[index];
        total = total
            .checked_add(entry.size)
            .ok_or("Compound stream size overflow")?;
        if total > MAX_STREAM_TOTAL {
            return Err("Compound streams exceed their memory budget".into());
        }
        let content = if entry.size < 4096 {
            let count = entry.size.div_ceil(64);
            let mini_limit = mini_owners.len();
            let ids = chain(
                entry.first,
                &mini_fat,
                &mut mini_owners,
                mini_limit,
                Some(count),
            )?;
            let mut bytes = Vec::with_capacity(entry.size);
            for id in ids {
                let at = (id as usize)
                    .checked_mul(64)
                    .ok_or("Mini stream offset overflow")?;
                let len = entry.size.saturating_sub(bytes.len()).min(64);
                bytes.extend_from_slice(
                    mini_stream
                        .get(at..at + len)
                        .ok_or("Mini stream is truncated")?,
                );
            }
            bytes
        } else {
            let count = entry.size.div_ceil(sector_size);
            let ids = chain(entry.first, &fat, &mut owners, sector_count, Some(count))?;
            collect(data, sector_size, &ids, entry.size)?
        };
        streams.insert(entry.name.clone(), content);
    }
    if !streams.contains_key("WordDocument") {
        return Err("WordDocument stream is missing".into());
    }
    Ok(Compound { streams })
}
