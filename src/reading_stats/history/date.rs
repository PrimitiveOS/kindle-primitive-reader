//! Civil calendar arithmetic and local boundaries for reading history.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub(crate) struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}
#[derive(Clone, Copy)]
pub(crate) enum Zone {
    Local,
    #[cfg(test)]
    Fixed(i32),
}

/// Resolve every lookup in one history query from the same time zone.
pub(super) struct Calendar {
    zone: Zone,
    #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
    device: Option<primitive_device::DeviceTimeZone>,
}
impl Date {
    pub(crate) fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        let date = Self { year, month, day };
        ((1900..=9999).contains(&year)
            && (1..=12).contains(&month)
            && day > 0
            && day <= date.month_days())
        .then_some(date)
    }
    pub(crate) fn today() -> Self {
        crate::mark_time::now()
            .and_then(|s| Zone::Local.calendar().date(s))
            .unwrap_or(Self {
                year: 2000,
                month: 1,
                day: 1,
            })
    }
    pub(crate) fn first(self) -> Self {
        Self { day: 1, ..self }
    }
    pub(crate) fn month_days(self) -> u8 {
        match self.month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                28 + u8::from(self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0))
            }
            _ => 31,
        }
    }
    pub(crate) fn add_days(self, days: i32) -> Option<Self> {
        Self::from_ordinal(self.ordinal().checked_add(i64::from(days))?)
    }
    pub(crate) fn add_months(self, months: i32) -> Option<Self> {
        let m = self
            .year
            .checked_mul(12)?
            .checked_add(i32::from(self.month) - 1)?
            .checked_add(months)?;
        Self::new(m.div_euclid(12), m.rem_euclid(12) as u8 + 1, 1)
    }
    pub(crate) fn weekday(self) -> u8 {
        (self.ordinal() + 4).rem_euclid(7) as u8
    }
    pub(super) fn monday_week(self) -> u8 {
        let first = Self {
            year: self.year,
            month: 1,
            day: 1,
        };
        let first_monday = first.ordinal() + i64::from((8 - first.weekday()) % 7);
        let days = self.ordinal() - first_monday;
        if days < 0 { 0 } else { 1 + (days / 7) as u8 }
    }
    pub(crate) fn label(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    pub(crate) fn month_label(self) -> String {
        format!(
            "{} {}",
            [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December"
            ][usize::from(self.month - 1)],
            self.year
        )
    }
    fn ordinal(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
    }
    fn from_ordinal(days: i64) -> Option<Self> {
        let z = days.checked_add(719468)?;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        Self::new((y + i64::from(m <= 2)).try_into().ok()?, m as u8, d as u8)
    }
}
impl Zone {
    pub(super) fn calendar(self) -> Calendar {
        Calendar {
            zone: self,
            #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
            device: matches!(self, Self::Local).then(primitive_device::DeviceTimeZone::load),
        }
    }

    pub(crate) fn clock(self, time: i64) -> Option<(u32, u32, u32)> {
        self.calendar().clock(time)
    }

    pub(crate) fn range(self, date: Date, minutes: u16) -> Option<(i64, i64)> {
        self.calendar().range(date, minutes)
    }
}

impl Calendar {
    pub(super) fn date(&self, time: i64) -> Option<Date> {
        self.parts(time).map(|(d, _, _, _)| d)
    }

    pub(super) fn clock(&self, time: i64) -> Option<(u32, u32, u32)> {
        self.parts(time).map(|(_, h, m, s)| (h, m, s))
    }

    fn parts(&self, time: i64) -> Option<(Date, u32, u32, u32)> {
        #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
        if let Some(device) = &self.device {
            let time = device.local_time_at(time)?;
            return Some((
                Date::new(
                    time.year,
                    time.month.try_into().ok()?,
                    time.day.try_into().ok()?,
                )?,
                time.hour.try_into().ok()?,
                time.minute.try_into().ok()?,
                time.second.try_into().ok()?,
            ));
        }
        match self.zone {
            #[cfg(test)]
            Zone::Fixed(offset) => {
                let shifted = time.checked_add(i64::from(offset))?;
                let day = Date::from_ordinal(shifted.div_euclid(86400))?;
                let secs = shifted.rem_euclid(86400) as u32;
                Some((day, secs / 3600, secs / 60 % 60, secs % 60))
            }
            Zone::Local => {
                let time = crate::mark_time::local_parts(time)?;
                Some((
                    Date::new(
                        time.year,
                        time.month.try_into().ok()?,
                        time.day.try_into().ok()?,
                    )?,
                    time.hour.try_into().ok()?,
                    time.minute.try_into().ok()?,
                    time.second.try_into().ok()?,
                ))
            }
        }
    }
    pub(super) fn range(&self, date: Date, minutes: u16) -> Option<(i64, i64)> {
        let start = self.start(date, minutes)?;
        let end = self.start(date.add_days(1)?, minutes)?;
        // A zone can skip an entire civil date. No visit belongs to an
        // empty or reversed interval, even if it crosses that instant.
        (start < end).then_some((start, end))
    }
    pub(super) fn midnight_boundary(&self, date: Date) -> Option<i64> {
        let start = self.start(date, 0)?;
        if self.date(start)? >= date {
            return Some(start);
        }
        // libc can normalize a skipped midnight back into the previous date.
        // Find the first instant belonging to this civil date or a later one.
        let mut earlier = start;
        let mut later = self.start(date.add_days(1)?, 0)?;
        if earlier >= later || self.date(later)? < date {
            return None;
        }
        while later - earlier > 1 {
            let middle = earlier + (later - earlier) / 2;
            if self.date(middle)? < date {
                earlier = middle;
            } else {
                later = middle;
            }
        }
        Some(later)
    }

    fn start(&self, date: Date, minutes: u16) -> Option<i64> {
        #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
        if let Some(device) = &self.device {
            let civil_second = date
                .ordinal()
                .checked_mul(86_400)?
                .checked_add(i64::from(minutes) * 60)?;
            return device.resolve_local_boundary(civil_second);
        }
        match self.zone {
            #[cfg(test)]
            Zone::Fixed(offset) => date
                .ordinal()
                .checked_mul(86400)?
                .checked_add(i64::from(minutes) * 60)?
                .checked_sub(i64::from(offset)),
            Zone::Local => {
                // Zero initialization is valid for tm. mktime resolves the local DST offset.
                let mut tm: libc::tm = unsafe { std::mem::zeroed() };
                tm.tm_year = date.year - 1900;
                tm.tm_mon = i32::from(date.month) - 1;
                tm.tm_mday = i32::from(date.day);
                tm.tm_hour = i32::from(minutes / 60);
                tm.tm_min = i32::from(minutes % 60);
                tm.tm_isdst = -1;
                let time = unsafe { libc::mktime(&mut tm) };
                (time != -1).then_some(time as i64)
            }
        }
    }
}
