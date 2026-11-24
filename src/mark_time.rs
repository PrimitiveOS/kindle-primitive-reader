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
    Some(local_parts(seconds)?.date())
}

#[cfg(feature = "kindle")]
pub(crate) fn local_datetime(seconds: i64) -> Option<String> {
    let clock = local_parts(seconds)?;
    Some(format!(
        "{} {:02}:{:02}:{:02}",
        clock.date(),
        clock.hour,
        clock.minute,
        clock.second
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CivilParts {
    pub(crate) year: i32,
    pub(crate) month: i32,
    pub(crate) day: i32,
    pub(crate) hour: i32,
    pub(crate) minute: i32,
    pub(crate) second: i32,
}

impl CivilParts {
    #[cfg(feature = "kindle")]
    fn date(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    fn from_c_time(time: libc::time_t, utc: bool) -> Option<Self> {
        let mut result = std::mem::MaybeUninit::uninit();
        // These functions initialize the caller-owned tm on success.
        let tm = unsafe {
            let written = if utc {
                libc::gmtime_r(&time, result.as_mut_ptr())
            } else {
                libc::localtime_r(&time, result.as_mut_ptr())
            };
            if written.is_null() {
                return None;
            }
            result.assume_init()
        };
        Some(Self {
            year: tm.tm_year.checked_add(1900)?,
            month: tm.tm_mon + 1,
            day: tm.tm_mday,
            hour: tm.tm_hour,
            minute: tm.tm_min,
            second: tm.tm_sec,
        })
    }

    fn to_c_time(self, utc: bool) -> Option<i64> {
        // A zeroed tm is valid, including its platform time-zone pointer.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = self.year.checked_sub(1900)?;
        tm.tm_mon = self.month - 1;
        tm.tm_mday = self.day;
        tm.tm_hour = self.hour;
        tm.tm_min = self.minute;
        tm.tm_sec = self.second;
        tm.tm_isdst = -1;
        // Both functions normalize the caller-owned tm in place.
        let time = unsafe {
            if utc {
                libc::timegm(&mut tm)
            } else {
                libc::mktime(&mut tm)
            }
        };
        // Round-trip to reject invalid dates, skipped local times and overflow
        // without discarding the valid Unix timestamp -1.
        (Self::from_c_time(time, utc)? == self).then_some(i64::from(time))
    }
}

#[cfg(all(
    feature = "kindle",
    target_os = "linux",
    target_arch = "arm",
    target_env = "musl"
))]
pub(crate) fn local_parts(seconds: i64) -> Option<CivilParts> {
    let clock = primitive_device::local_time_at(seconds)?;
    Some(CivilParts {
        year: clock.year,
        month: clock.month,
        day: clock.day,
        hour: clock.hour,
        minute: clock.minute,
        second: clock.second,
    })
}

#[cfg(all(
    feature = "kindle",
    not(all(target_os = "linux", target_arch = "arm", target_env = "musl"))
))]
pub(crate) fn local_parts(seconds: i64) -> Option<CivilParts> {
    CivilParts::from_c_time(libc::time_t::try_from(seconds).ok()?, false)
}

#[cfg(all(
    feature = "kindle",
    target_os = "linux",
    target_arch = "arm",
    target_env = "musl"
))]
fn local_timestamp(clock: CivilParts) -> Option<i64> {
    // Validate the calendar fields first. The resulting timestamp represents
    // the wall time on the UTC calendar, for the Kindle zone to resolve.
    primitive_device::resolve_local_time(clock.to_c_time(true)?)
}

#[cfg(not(all(
    feature = "kindle",
    target_os = "linux",
    target_arch = "arm",
    target_env = "musl"
)))]
fn local_timestamp(clock: CivilParts) -> Option<i64> {
    clock.to_c_time(false)
}

/// KOReader's `datetime` (readerannotation.lua) uses YYYY-MM-DD HH:MM:SS.
/// Accept date-only records and use the device zone when there is no offset.
/// An explicit offset defines an absolute instant. Invalid dates leave the
/// mark undated.
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
    let clock = CivilParts {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    match offset {
        Some(offset) => clock.to_c_time(true)?.checked_sub(i64::from(offset)),
        None => local_timestamp(clock),
    }
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
            b"2023-02-29 11:00:00+07:00",
            b"2024-04-31 12:00:00",
            b"2024-02-29 22:61:00",
            b"2024-02-29T24:00:00Z",
            b"2024-02-29T00:00:00+01:60",
            b"2024-02-29abc",
            b"",
        ] {
            assert_eq!(parse(invalid), None);
        }
    }

    #[cfg(feature = "kindle")]
    #[test]
    fn local_mark_dates_match_timezone_less_sidecar_timestamps() {
        let sidecar = b"2024-02-29 12:34:56";
        let timestamp = parse(sidecar).unwrap();
        assert_eq!(
            super::local_datetime(timestamp).unwrap().as_bytes(),
            sidecar
        );
        assert_eq!(super::local_date(timestamp).unwrap(), "2024-02-29");
    }

    #[cfg(all(
        feature = "kindle",
        target_os = "linux",
        target_arch = "arm",
        target_env = "musl"
    ))]
    #[test]
    fn kindle_mark_times_match_the_system_clock() {
        // Read both formats from the same instant using the Kindle's date binary.
        let output = std::process::Command::new("date")
            .env_remove("TZ")
            .env("LC_ALL", "C")
            .arg("+%s|%Y-%m-%d %H:%M:%S")
            .output()
            .expect("run the Kindle date command");
        assert!(output.status.success());
        let output = std::str::from_utf8(&output.stdout).unwrap();
        let (unix, wall) = output.trim_end().split_once('|').unwrap();
        let unix: i64 = unix.parse().unwrap();
        assert_eq!(super::local_datetime(unix).as_deref(), Some(wall));
        assert_eq!(super::local_date(unix).as_deref(), wall.get(..10));

        let decoded = parse(wall.as_bytes()).expect("import the system's wall time");
        assert_eq!(super::local_datetime(decoded).as_deref(), Some(wall));
        // During a fall-back fold, an offset-free timestamp uses the first occurrence.
        assert!(decoded <= unix);
    }
}
