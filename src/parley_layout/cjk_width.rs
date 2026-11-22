//! Horizontal glyph advances used by KOReader's CJK width control.

/// CREngine's lStr_isCJK with its default inclusion of punctuation and fullwidth ASCII.
pub(super) fn eligible(ch: char) -> bool {
    matches!(ch as u32, 0x2e80..=0x9fff | 0xac00..=0xd7ff | 0xf900..=0xfaff |
        0xfe30..=0xfe4f | 0xff00..=0xffef | 0x1f200..=0x1f2ff | 0x20000..=0x3134f)
}

pub(super) fn policy(
    text: &str,
    percent: u8,
) -> Option<impl Fn(&str, f32, f32) -> u16 + Send + Sync + 'static> {
    (percent > 100 && text.chars().any(eligible))
        .then(|| move |source: &str, advance, em| extra(source, advance, em, percent))
}

pub(super) fn extra(source: &str, advance: f32, em: f32, percent: u8) -> u16 {
    let Some(first) = source.chars().next() else {
        return 0;
    };
    let wide_quote = matches!(first, '‘' | '’' | '“' | '”') && advance >= 0.8 * em;
    if !eligible(first) && !wide_quote {
        return 0;
    }
    // CREngine measures in integer pixels, then rounds the increase down.
    (advance.round() * f32::from(percent - 100) / 100.0).floor() as u16
}
