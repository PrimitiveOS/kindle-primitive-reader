//! Legacy MOBI and KF8 share the INDX/TAGX/CNCX label and tree format.
use super::{MobiError, Records, Result, integer, read_index, varint};
use crate::mobi;
const MAX_ENTRIES: usize = 4096;
const MAX_LABEL_BYTES: usize = 2048;
const MAX_DEPTH: usize = 32;

pub(super) fn single(row: &super::Entry<'_>, tag: u8) -> Result<usize> {
    match row.tags.get(&tag).map(Vec::as_slice) {
        Some([value]) => Ok(*value),
        _ => Err(MobiError::InvalidHeader),
    }
}

pub(super) fn read_navigation<T>(
    records: &Records<'_>,
    header: &[u8],
    mut convert: impl FnMut(String, usize, &super::Entry<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    if header.get(16..20) != Some(b"MOBI") || integer(header, 20)? < 232 {
        return Ok(Vec::new());
    }
    let index = integer(header, 244)?;
    if index == 0 || index == u32::MAX as usize {
        return Ok(Vec::new());
    }
    if index <= usize::from(mobi::u16_at(header, 8)?) {
        return Err(MobiError::InvalidHeader);
    }
    let meta = records.at(index)?;
    let segments = integer(meta, 24)?;
    let labels = integer(meta, 52)?;
    if !(1..=64).contains(&labels) || !(1..=super::MAX_INDEX_RECORDS).contains(&segments) {
        return Err(MobiError::InvalidHeader);
    }
    let label_base = index
        .checked_add(segments + 1)
        .ok_or(MobiError::InvalidHeader)?;
    records.at(label_base
        .checked_add(labels - 1)
        .ok_or(MobiError::InvalidHeader)?)?;
    let encoding = integer(meta, 28)?;
    let mut items = Vec::new();
    let mut relationships = Vec::new();
    for row in read_index(&records, index, MAX_ENTRIES)? {
        let pointer = single(&row, 3)?;
        let depth = single(&row, 4)?;
        if depth > MAX_DEPTH {
            return Err(MobiError::InvalidHeader);
        }
        let optional = |tag| row.tags.get(&tag).map(|_| single(&row, tag)).transpose();
        relationships.push((depth, optional(21)?, optional(22)?, optional(23)?));
        let segment = pointer >> 16;
        if segment >= labels {
            return Err(MobiError::InvalidHeader);
        }
        let record = records.at(label_base + segment)?;
        let mut text = record
            .get(pointer & 0xffff..)
            .ok_or(MobiError::InvalidHeader)?;
        let size = varint(&mut text)?;
        if !(1..=MAX_LABEL_BYTES).contains(&size) {
            return Err(MobiError::InvalidHeader);
        }
        let label =
            mobi::navigation_label(text.get(..size).ok_or(MobiError::InvalidHeader)?, encoding)?;
        let label = if label.is_empty() {
            "Untitled section".into()
        } else {
            label
        };
        items.push(convert(label, depth, &row)?);
    }
    publisher_order(items, &relationships)
}

type Relationship = (usize, Option<usize>, Option<usize>, Option<usize>);

pub(super) fn publisher_order<T>(items: Vec<T>, relationships: &[Relationship]) -> Result<Vec<T>> {
    if relationships
        .iter()
        .all(|(_, parent, first, last)| parent.is_none() && first.is_none() && last.is_none())
    {
        let mut previous = None;
        for &(depth, _, _, _) in relationships {
            if previous.map_or(depth != 0, |level| depth > level + 1) {
                return Err(MobiError::InvalidHeader);
            }
            previous = Some(depth);
        }
        return Ok(items);
    }
    let mut children = vec![Vec::new(); items.len()];
    let mut roots = Vec::new();
    for (i, &(depth, parent, first, last)) in relationships.iter().enumerate() {
        if first.is_some() != last.is_some()
            || first
                .zip(last)
                .is_some_and(|(a, b)| a > b || b >= items.len())
        {
            return Err(MobiError::InvalidHeader);
        }
        if let Some(parent) = parent {
            let &(parent_depth, _, Some(start), Some(end)) =
                relationships.get(parent).ok_or(MobiError::InvalidHeader)?
            else {
                return Err(MobiError::InvalidHeader);
            };
            if depth != parent_depth + 1 || !(start..=end).contains(&i) {
                return Err(MobiError::InvalidHeader);
            }
            children[parent].push(i);
        } else if depth == 0 {
            roots.push(i);
        } else {
            return Err(MobiError::InvalidHeader);
        }
    }
    for (&(_, _, first, last), list) in relationships.iter().zip(&children) {
        if first != list.first().copied() || last != list.last().copied() {
            return Err(MobiError::InvalidHeader);
        }
    }
    let mut rows: Vec<_> = items.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(rows.len());
    roots.reverse();
    while let Some(i) = roots.pop() {
        out.push(rows[i].take().ok_or(MobiError::InvalidHeader)?);
        roots.extend(children[i].iter().rev().copied());
    }
    if out.len() != rows.len() {
        return Err(MobiError::InvalidHeader);
    }
    Ok(out)
}
