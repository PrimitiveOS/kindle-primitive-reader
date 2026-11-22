//! Civil calendar arithmetic; local boundaries use mktime so DST days may be 23/25 hours.
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
            .and_then(|s| Zone::Local.date(s))
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
    pub(crate) fn date(self, time: i64) -> Option<Date> {
        self.parts(time).map(|(d, _, _, _)| d)
    }
    pub(crate) fn clock(self, time: i64) -> Option<(u32, u32, u32)> {
        self.parts(time).map(|(_, h, m, s)| (h, m, s))
    }
    fn parts(self, time: i64) -> Option<(Date, u32, u32, u32)> {
        match self {
            #[cfg(test)]
            Self::Fixed(offset) => {
                let shifted = time.checked_add(i64::from(offset))?;
                let day = Date::from_ordinal(shifted.div_euclid(86400))?;
                let secs = shifted.rem_euclid(86400) as u32;
                Some((day, secs / 3600, secs / 60 % 60, secs % 60))
            }
            Self::Local => {
                let time: libc::time_t = time.try_into().ok()?;
                let mut tm = std::mem::MaybeUninit::uninit();
                // localtime_r initializes this caller-owned tm, including its platform fields.
                let tm = unsafe {
                    if libc::localtime_r(&time, tm.as_mut_ptr()).is_null() {
                        return None;
                    }
                    tm.assume_init()
                };
                Some((
                    Date::new(tm.tm_year + 1900, (tm.tm_mon + 1) as u8, tm.tm_mday as u8)?,
                    tm.tm_hour as u32,
                    tm.tm_min as u32,
                    tm.tm_sec as u32,
                ))
            }
        }
    }
    pub(crate) fn range(self, date: Date, minutes: u16) -> Option<(i64, i64)> {
        Some((
            self.start(date, minutes)?,
            self.start(date.add_days(1)?, minutes)?,
        ))
    }
    fn start(self, date: Date, minutes: u16) -> Option<i64> {
        match self {
            #[cfg(test)]
            Self::Fixed(offset) => date
                .ordinal()
                .checked_mul(86400)?
                .checked_add(i64::from(minutes) * 60)?
                .checked_sub(i64::from(offset)),
            Self::Local => {
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
