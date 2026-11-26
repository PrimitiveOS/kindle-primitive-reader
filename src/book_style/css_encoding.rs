//! Conservative compatibility decoding for previously unreadable CSS resources.
use std::borrow::Cow;

/// Keep all previously accepted UTF-8 untouched. Non-UTF-8 input and output
/// must both fit the caller's fallback budget; unknown or conflicting labels
/// and malformed encoded text are rejected.
pub(super) fn decode(bytes: &[u8], limit: usize) -> Option<Cow<'_, str>> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(Cow::Borrowed(text));
    }
    if bytes.len() > limit
        || bytes.starts_with(&[0xff, 0xfe, 0, 0])
        || bytes.starts_with(&[0, 0, 0xfe, 0xff])
    {
        return None;
    }
    let (encoding, payload, bom) = if bytes.starts_with(&[0xff, 0xfe]) {
        (encoding_rs::UTF_16LE, &bytes[2..], true)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (encoding_rs::UTF_16BE, &bytes[2..], true)
    } else {
        let label = charset(bytes)?;
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes())?;
        let allowed = [
            encoding_rs::GBK,
            encoding_rs::GB18030,
            encoding_rs::BIG5,
            encoding_rs::SHIFT_JIS,
            encoding_rs::EUC_JP,
            encoding_rs::EUC_KR,
            encoding_rs::WINDOWS_1250,
            encoding_rs::WINDOWS_1251,
            encoding_rs::WINDOWS_1252,
            encoding_rs::WINDOWS_1253,
            encoding_rs::WINDOWS_1254,
            encoding_rs::WINDOWS_1255,
            encoding_rs::WINDOWS_1256,
            encoding_rs::WINDOWS_1257,
            encoding_rs::WINDOWS_1258,
            encoding_rs::WINDOWS_874,
            encoding_rs::KOI8_R,
            encoding_rs::KOI8_U,
            encoding_rs::IBM866,
            encoding_rs::MACINTOSH,
            encoding_rs::X_MAC_CYRILLIC,
        ];
        if !allowed.contains(&encoding) {
            return None;
        }
        (encoding, bytes, false)
    };
    let text = encoding.decode_without_bom_handling_and_without_replacement(payload)?;
    if text.len() > limit || text.contains('\0') {
        return None;
    }
    if bom && text.starts_with("@charset") {
        let label = charset(text.as_bytes())?;
        let declared = encoding_rs::Encoding::for_label(label.as_bytes())?;
        if !label.eq_ignore_ascii_case("utf-16") && declared != encoding {
            return None;
        }
    }
    Some(text)
}

/// CSS encoding declarations are accepted only in the exact leading syntax.
fn charset(bytes: &[u8]) -> Option<&str> {
    let rest = bytes.strip_prefix(b"@charset \"")?;
    let end = rest.iter().position(|&byte| byte == b'"')?;
    if end == 0
        || end > 40
        || rest.get(end + 1) != Some(&b';')
        || !rest[..end]
            .iter()
            .copied()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return None;
    }
    std::str::from_utf8(&rest[..end]).ok()
}
