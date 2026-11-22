//! Creation times on saved marks. KOReader writes its sidecar dates in device-local time.
#[cfg(feature = "kindle")]
pub(crate) fn now() -> Option<i64> {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs()
        .try_into()
        .ok()
}

#[cfg(feature = "kindle")]
pub(crate) fn local_date(seconds: i64) -> Option<String> {
    let seconds = libc::time_t::try_from(seconds).ok()?;
    let mut result = std::mem::MaybeUninit::uninit();
    // localtime_r writes into this caller-owned buffer and reports out-of-range dates.
    let result = unsafe {
        if libc::localtime_r(&seconds, result.as_mut_ptr()).is_null() {
            return None;
        }
        result.assume_init()
    };
    Some(format!(
        "{:04}-{:02}-{:02}",
        result.tm_year + 1900,
        result.tm_mon + 1,
        result.tm_mday
    ))
}

/// KOReader's `datetime` (readerannotation.lua) uses YYYY-MM-DD HH:MM:SS.
/// Its date parser supports date-only records. ISO strings with an explicit
/// offset are absolute. Missing or malformed dates leave the mark undated.
pub(crate) fn parse(value: &[u8]) -> Option<i64> {
    fn digits(bytes: &[u8]) -> Option<i32> {
        bytes.iter().try_fold(0, |value, &b| {
            b.is_ascii_digit().then(|| value * 10 + i32::from(b - b'0'))
        })
    }
    if value.len() < 10 || value[4] != b'-' || value[7] != b'-' {
        return None;
    }
    let (year, month, day) = (
        digits(&value[..4])?,
        digits(&value[5..7])?,
        digits(&value[8..10])?,
    );
    let (hour, minute, second, suffix) = match value.get(10..) {
        Some([]) => (0, 0, 0, &[][..]),
        Some([b' ' | b'T', h1, h2, b':', m1, m2, b':', s1, s2, suffix @ ..]) => (
            digits(&[*h1, *h2])?,
            digits(&[*m1, *m2])?,
            digits(&[*s1, *s2])?,
            suffix,
        ),
        _ => return None,
    };
    let offset = match suffix {
        [] => None,
        b"Z" => Some(0),
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hours = digits(&[*h1, *h2])?;
            let minutes = digits(&[*m1, *m2])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            Some((hours * 3600 + minutes * 60) * if *sign == b'-' { -1 } else { 1 })
        }
        _ => return None,
    };
    // All fields of tm, including the platform's time-zone pointer, may be zero-initialized.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let expected = (year - 1900, month - 1, day, hour, minute, second);
    (
        tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec,
    ) = expected;
    tm.tm_isdst = -1;
    // Both conversions accept a caller-owned tm and normalize it in place.
    let seconds = unsafe {
        if offset.is_some() {
            libc::timegm(&mut tm)
        } else {
            libc::mktime(&mut tm)
        }
    };
    // Check the actual instant. This rejects out-of-range time_t values, invalid
    // dates and skipped wall-clock times without discarding epoch second -1.
    let mut check = std::mem::MaybeUninit::uninit();
    let check = unsafe {
        let ptr = if offset.is_some() {
            libc::gmtime_r(&seconds, check.as_mut_ptr())
        } else {
            libc::localtime_r(&seconds, check.as_mut_ptr())
        };
        if ptr.is_null() {
            return None;
        }
        check.assume_init()
    };
    let actual = (
        check.tm_year,
        check.tm_mon,
        check.tm_mday,
        check.tm_hour,
        check.tm_min,
        check.tm_sec,
    );
    (actual == expected).then(|| i64::from(seconds) - i64::from(offset.unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn distinguishes_local_creation_time_from_explicit_zones_and_invalid_dates() {
        assert_eq!(parse(b"1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse(b"1969-12-31T23:59:59Z"), Some(-1));
        assert_eq!(
            parse(b"2024-02-29T11:01:02+07:00"),
            parse(b"2024-02-29T04:01:02Z")
        );
        assert_eq!(
            parse(b"2024-02-29T01:01:02-03:00"),
            parse(b"2024-02-29T04:01:02Z")
        );
        assert!(parse(b"2024-02-29").is_some());
        assert_eq!(parse(b"2024-02-29"), parse(b"2024-02-29 00:00:00"));
        for invalid in [
            b"2023-02-29".as_slice(),
            b"2024-02-29 22:61:00",
            b"2024-02-29T00:00:00+01:60",
            b"2024-02-29abc",
            b"",
        ] {
            assert_eq!(parse(invalid), None);
        }
    }
}
