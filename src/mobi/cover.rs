//! Read only a bounded cover declared by the primary MOBI EXTH header.
use crate::epub::CoverKind;

const MAX_COVER_BYTES: usize = 4 * 1024 * 1024;
const MAX_EXTH_RECORDS: usize = 512;

/// EXTH 201 is a zero-based offset from the first image/resource record.
/// A damaged or ambiguous optional cover never prevents opening the book.
pub(crate) fn extract(bytes: &[u8]) -> Option<(CoverKind, &[u8])> {
    if bytes.get(60..68)? != b"BOOKMOBI" || bytes.len() > super::MAX_MOBI_BYTES {
        return None;
    }
    let records = super::record_offsets(bytes).ok()?;
    let record = bytes.get(*records.first()?..*records.get(1)?)?;
    if record.get(16..20)? != b"MOBI" {
        return None;
    }
    let header_len = usize::try_from(super::u32_at(record, 20).ok()?).ok()?;
    let end = 16usize.checked_add(header_len)?;
    let header = record.get(..end)?;
    let first = usize::try_from(super::u32_at(header, 108).ok()?).ok()?;
    let text_records = usize::from(super::u16_at(header, 8).ok()?);
    if first <= text_records || first >= records.len().saturating_sub(1) {
        return None;
    }
    let exth = record.get(end..)?;
    if exth.get(..4)? != b"EXTH" {
        return None;
    }
    let length = usize::try_from(super::u32_at(exth, 4).ok()?).ok()?;
    let count = usize::try_from(super::u32_at(exth, 8).ok()?).ok()?;
    if length < 12 || count > MAX_EXTH_RECORDS {
        return None;
    }
    let exth = exth.get(..length)?;
    let mut at = 12usize;
    let mut offset = None;
    let mut boundary = None;
    for _ in 0..count {
        let kind = super::u32_at(exth, at).ok()?;
        let size = usize::try_from(super::u32_at(exth, at.checked_add(4)?).ok()?).ok()?;
        if size < 8 {
            return None;
        }
        let next = at.checked_add(size)?;
        let item = exth.get(at..next)?;
        if matches!(kind, 121 | 201) {
            if item.len() != 12 {
                return None;
            }
            let value = usize::try_from(super::u32_at(item, 8).ok()?).ok()?;
            let slot = if kind == 201 {
                &mut offset
            } else {
                &mut boundary
            };
            if slot.replace(value).is_some() {
                return None;
            }
        }
        at = next;
    }
    let index = first.checked_add(offset?)?;
    if boundary.is_some_and(|limit| limit > first && index >= limit) {
        return None;
    }
    let image = bytes.get(*records.get(index)?..*records.get(index.checked_add(1)?)?)?;
    if image.is_empty() || image.len() > MAX_COVER_BYTES {
        return None;
    }
    let kind = if image.starts_with(b"\xff\xd8\xff") {
        CoverKind::Jpeg
    } else if image.starts_with(b"\x89PNG\r\n\x1a\n") {
        CoverKind::Png
    } else if crate::gif_image::has_signature(image) {
        CoverKind::Gif
    } else {
        return None;
    };
    crate::epub_raster_dimensions(image).ok()?;
    Some((kind, image))
}
