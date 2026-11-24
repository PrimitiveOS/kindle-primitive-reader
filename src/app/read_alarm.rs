//! The reading alarm uses the monotonic UI clock, just like KOReader's read timer.
//! Wall time is consulted only when the user chooses a particular local clock time.
use super::*;
use primitive_ui::editor::Multiline;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_INTERVAL: u16 = 17 * 60 + 59;
const OPTIONS_LIMIT: u64 = 8192;
const MESSAGE_LIMIT: usize = 400;

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Expiry {
    #[default]
    Message,
    Notification,
    Silent,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Options {
    minutes: u16,
    repeat: bool,
    header: bool,
    footer: bool,
    expiry: Expiry,
    message: String,
}

pub(super) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
    let options: Options = serde_json::from_slice(bytes)?;
    if options.minutes > MAX_INTERVAL || options.message.chars().count() > MESSAGE_LIMIT {
        return Err(io::Error::other("The read timer backup is invalid"));
    }
    serde_json::to_value(options).map_err(io::Error::other)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Interval,
    Alarm,
}

#[derive(Clone, Copy)]
struct Running {
    due: Instant,
    repeat_interval: Option<Duration>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Settings,
    Set(Kind, u16),
    EditMessage,
}

struct Alert {
    text: String,
    expand: bool,
    repeat_interval: Option<Duration>,
    hide: Option<Instant>,
}

pub(super) struct ReadAlarm {
    file: PathBuf,
    options: Options,
    running: Option<Running>,
    panel: Option<Panel>,
    editor: Multiline,
    alert: Option<Alert>,
    refresh: Option<Instant>,
}

impl ReadAlarm {
    pub(super) fn open(file: PathBuf) -> Self {
        let mut options: Options = with_storage(|| {
            let mut bytes = Vec::new();
            fs::File::open(&file)?
                .take(OPTIONS_LIMIT + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > OPTIONS_LIMIT {
                return Err(io::Error::other("Read timer settings are too large"));
            }
            serde_json::from_slice(&bytes).map_err(io::Error::other)
        })
        .unwrap_or_default();
        if options.minutes > MAX_INTERVAL {
            options.minutes = 0;
        }
        options.message = options.message.chars().take(MESSAGE_LIMIT).collect();
        Self {
            file,
            options,
            running: None,
            panel: None,
            editor: Multiline::new("", MESSAGE_LIMIT),
            alert: None,
            refresh: None,
        }
    }

    fn save(&self, next: Options) -> io::Result<Options> {
        primitive_app::save(
            &self.file,
            &serde_json::to_vec(&next).map_err(io::Error::other)?,
        )?;
        Ok(next)
    }

    pub(super) fn stop(&mut self) -> Change {
        let was_running = self.running.take().is_some();
        self.refresh = None;
        if was_running {
            Change::Partial
        } else {
            Change::None
        }
    }

    fn remaining(&self, now: Instant) -> Option<Duration> {
        Some(self.running?.due.saturating_duration_since(now))
    }

    pub(super) fn caption(&self, now: Instant) -> Option<String> {
        let remaining = self.remaining(now)?;
        let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        let rounded = seconds.div_ceil(60);
        Some(format!("{:02}:{:02}", rounded / 60, rounded % 60))
    }

    fn start(&mut self, now: Instant, span: Duration, repeat_interval: Option<Duration>) {
        self.running = now.checked_add(span).map(|due| Running {
            due,
            repeat_interval,
        });
        self.alert = None;
        self.update_refresh(now);
    }

    fn update_refresh(&mut self, now: Instant) {
        self.refresh = self.remaining(now).and_then(|time| {
            if time.is_zero()
                || !(self.options.header
                    || self.options.footer
                    || self.panel == Some(Panel::Settings))
            {
                return None;
            }
            let mut tail = Duration::new(time.as_secs() % 60, time.subsec_nanos());
            if tail.is_zero() {
                tail = Duration::from_secs(60);
            }
            now.checked_add(tail)
        });
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.running
            .map(|r| r.due)
            .into_iter()
            .chain(self.refresh)
            .chain(self.alert.as_ref().and_then(|a| a.hide))
            .min()
    }

    pub(super) fn tick(&mut self, now: Instant, resumed: bool) -> Change {
        if let Some(running) = self.running.filter(|r| r.due <= now) {
            self.running = None;
            self.refresh = None;
            let automatic = self.options.repeat && running.repeat_interval.is_some();
            // The stock resume handler unschedules the expired callback after delivering it.
            if automatic
                && !resumed
                && let Some(span) = running.repeat_interval
            {
                self.start(now, span, Some(span));
            }
            if self.options.expiry != Expiry::Silent {
                self.alert = Some(Alert {
                    expand: true,
                    text: if self.options.message.is_empty() {
                        "Time is up".to_owned()
                    } else {
                        self.options.message.clone()
                    },
                    repeat_interval: if automatic || self.options.expiry == Expiry::Notification {
                        None
                    } else {
                        running.repeat_interval
                    },
                    hide: (self.options.expiry == Expiry::Notification)
                        .then(|| now + Duration::from_secs(5)),
                });
            }
            return Change::Partial;
        }
        if self
            .alert
            .as_ref()
            .and_then(|a| a.hide)
            .is_some_and(|until| until <= now)
        {
            self.alert = None;
            return Change::Partial;
        }
        if self.refresh.is_some_and(|next| next <= now) {
            self.update_refresh(now);
            return Change::Partial;
        }
        Change::None
    }

    pub(super) fn footer(&self) -> Option<String> {
        self.options
            .footer
            .then(|| self.caption(Instant::now()))
            .flatten()
    }
}

fn unix_seconds(now: SystemTime) -> Option<i64> {
    i64::try_from(now.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()
}

#[allow(clippy::useless_conversion)] // time_t is narrower on some supported targets.
fn libc_local_time(seconds: i64) -> Option<(libc::time_t, libc::tm)> {
    let seconds = libc::time_t::try_from(seconds).ok()?;
    let mut civil = std::mem::MaybeUninit::uninit();
    // localtime_r initializes this caller-owned buffer on success.
    unsafe {
        if libc::localtime_r(&seconds, civil.as_mut_ptr()).is_null() {
            return None;
        }
        Some((seconds, civil.assume_init()))
    }
}

fn local_clock(now: SystemTime) -> Option<primitive_device::LocalTime> {
    let seconds = unix_seconds(now)?;
    #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
    {
        primitive_device::local_time_at(seconds)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "arm", target_env = "musl")))]
    {
        let (_, civil) = libc_local_time(seconds)?;
        Some(primitive_device::LocalTime {
            year: civil.tm_year.checked_add(1900)?,
            month: civil.tm_mon + 1,
            day: civil.tm_mday,
            hour: civil.tm_hour,
            minute: civil.tm_min,
            second: civil.tm_sec,
            weekday: civil.tm_wday,
            utc_offset: None,
        })
    }
}

struct AlarmOccurrence {
    delay: Duration,
    hour: i32,
    minute: i32,
}

impl AlarmOccurrence {
    fn confirmation(&self) -> String {
        let seconds = self.delay.as_secs();
        let duration = match (seconds / 60, seconds % 60) {
            (0, _) => format!("{seconds} sec"),
            (_, 0) => super::reading_time(seconds),
            (_, rest) => format!("{} {rest} sec", super::reading_time(seconds)),
        };
        format!(
            "Alarm set for {:02}:{:02}.\n{duration} from now.",
            self.hour, self.minute
        )
    }
}

fn clock_occurrence(now: SystemTime, minutes: u16) -> Option<AlarmOccurrence> {
    if minutes >= 24 * 60 {
        return None;
    }
    let seconds = unix_seconds(now)?;
    #[cfg(all(target_os = "linux", target_arch = "arm", target_env = "musl"))]
    {
        let zone = primitive_device::DeviceTimeZone::load();
        if let Some(offset) = zone.local_time_at(seconds)?.utc_offset {
            return clock_occurrence_in_zone(seconds, offset, minutes, |civil, after| {
                let target = zone.resolve_local_time_after(civil, after)?;
                let local = zone.local_time_at(target)?;
                Some((target, local.hour, local.minute))
            });
        }
    }
    libc_clock_occurrence(seconds, minutes)
}

#[cfg(any(
    test,
    all(target_os = "linux", target_arch = "arm", target_env = "musl")
))]
fn clock_occurrence_in_zone(
    seconds: i64,
    offset: i32,
    minutes: u16,
    resolve_after: impl Fn(i64, i64) -> Option<(i64, i32, i32)>,
) -> Option<AlarmOccurrence> {
    const DAY: i64 = 24 * 60 * 60;
    let local_day = seconds.checked_add(i64::from(offset))?.div_euclid(DAY);
    let today = local_day
        .checked_mul(DAY)?
        .checked_add(i64::from(minutes) * 60)?;
    let (target, hour, minute) = resolve_after(today, seconds)
        .or_else(|| resolve_after(today.checked_add(DAY)?, seconds))?;
    Some(AlarmOccurrence {
        delay: positive_delay(target.checked_sub(seconds)?)?,
        hour,
        minute,
    })
}

fn libc_clock_occurrence(seconds: i64, minutes: u16) -> Option<AlarmOccurrence> {
    let (seconds, mut civil) = libc_local_time(seconds)?;
    civil.tm_hour = i32::from(minutes / 60);
    civil.tm_min = i32::from(minutes % 60);
    civil.tm_sec = 0;
    civil.tm_isdst = -1;
    let mut actual = civil;
    // mktime normalizes the civil date in the host's local time zone.
    let mut target = unsafe { libc::mktime(&mut actual) };
    if target <= seconds {
        actual = civil;
        actual.tm_mday += 1;
        target = unsafe { libc::mktime(&mut actual) };
    }
    Some(AlarmOccurrence {
        delay: positive_delay(target.checked_sub(seconds)?)?,
        hour: actual.tm_hour,
        minute: actual.tm_min,
    })
}

fn positive_delay(seconds: impl TryInto<u64>) -> Option<Duration> {
    seconds
        .try_into()
        .ok()
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
}

mod message;
mod popup;
#[cfg(test)]
mod tests;
