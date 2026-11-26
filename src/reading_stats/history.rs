//! Dated, filtered page visits. Earlier aggregate screen time stays explicitly undated.
pub(crate) mod date;
pub(crate) mod report;
pub(crate) mod reset;
pub(crate) mod sqlite_import;
use date::{Date, Zone};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_VISITS: usize = 100_000;
const MAX_BOOKS: usize = 10_000;
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const CHECKPOINT: Duration = Duration::from_secs(60);
pub(crate) const MIN_DURATION_RANGE: std::ops::RangeInclusive<u32> = 0..=120;
pub(crate) const MAX_DURATION_RANGE: std::ops::RangeInclusive<u32> = 10..=7200;
const MAX_VISIT_SECONDS: u32 = *MAX_DURATION_RANGE.end();

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct Options {
    pub enabled: bool,
    pub min_seconds: u32,
    pub max_seconds: u32,
    pub freeze_finished: bool,
    pub week_start: u8,
    pub day_start: u16,
    /// Older journals always applied a saved timeline shift to the month view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_calendar: Option<bool>,
    pub calendar_book_spans: u8,
    pub histogram: bool,
    pub future_months: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            min_seconds: 5,
            max_seconds: 120,
            freeze_finished: false,
            week_start: 1,
            day_start: 0,
            shift_calendar: None,
            calendar_book_spans: 3,
            histogram: true,
            future_months: false,
        }
    }
}
impl Options {
    pub(crate) fn duration_limits_valid(self) -> bool {
        let stock_limits = MIN_DURATION_RANGE.contains(&self.min_seconds)
            && MAX_DURATION_RANGE.contains(&self.max_seconds);
        let legacy_limits = self.min_seconds > 0
            && self.min_seconds <= self.max_seconds
            && self.max_seconds <= 3600;
        stock_limits || legacy_limits
    }

    fn valid(self) -> bool {
        self.duration_limits_valid()
            && self.week_start <= 6
            && self.day_start < 1440
            && (1..=5).contains(&self.calendar_book_spans)
    }

    pub(crate) fn calendar_uses_shift(self) -> bool {
        self.shift_calendar.unwrap_or(self.day_start > 0)
    }

    fn calendar_start(self) -> u16 {
        if self.calendar_uses_shift() {
            self.day_start
        } else {
            0
        }
    }
}

#[derive(Default)]
pub(crate) struct CalendarDay {
    pub(crate) seconds: u64,
    /// Most reading time first, with a stable book-ID tiebreaker.
    pub(crate) books: Vec<u64>,
    pub(crate) hours: [u32; 24],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Visit {
    pub book: u64,
    pub page: u64,
    pub start: Option<i64>,
    pub seconds: u32,
    /// Optional spine index recorded for newer reflow visits; old journals
    /// and fixed-document visits remain unassigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflow_chapter: Option<ReflowChapter>,
    // Checkpoints can append to the same visit without making a second reread.
    id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct ReflowChapter {
    pub publication: u64,
    pub chapter: u32,
}

impl Visit {
    fn valid(&self) -> bool {
        (1..=MAX_VISIT_SECONDS).contains(&self.seconds) && self.start.is_none_or(valid_wall)
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct Data {
    version: u8,
    #[serde(default)]
    options: Options,
    prior: BTreeMap<u64, u64>,
    titles: BTreeMap<u64, String>,
    visits: Vec<Visit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    resets: BTreeMap<u64, reset::Baseline>,
}

pub(crate) fn backup_value(bytes: &[u8]) -> io::Result<serde_json::Value> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    let data: Data = serde_json::from_slice(bytes)?;
    if data.version != 1
        || !data.options.valid()
        || data.prior.len() > MAX_BOOKS
        || data.titles.len() > MAX_BOOKS
        || data.resets.len() > MAX_BOOKS
        || data.visits.len() > MAX_VISITS
        || data.titles.values().any(|s| s.len() > 1024)
        || data.visits.iter().any(|visit| !visit.valid())
    {
        return Err(invalid());
    }
    serde_json::to_value(data).map_err(io::Error::other)
}
#[derive(Clone)]
struct Part {
    start: Option<i64>,
    seconds: u32,
}
#[derive(Clone)]
struct Active {
    book: u64,
    page: u64,
    reflow_chapter: Option<ReflowChapter>,
    id: u64,
    observed: Instant,
    last_wall: Option<i64>,
    cursor: Option<i64>,
    running: bool,
    elapsed: Duration,
    credited: u32,
    pending: Vec<Part>,
    next_checkpoint: Instant,
}
impl Active {
    fn observe(&mut self, now: Instant, wall: Option<i64>, options: Options) {
        if !self.running {
            return;
        }
        let elapsed = now.saturating_duration_since(self.observed);
        let known = now >= self.observed
            && self.last_wall.zip(wall).is_some_and(|(a, b)| {
                b.checked_sub(a).is_some_and(|difference| {
                    difference >= 0 && difference.abs_diff(elapsed.as_secs() as i64) <= 2
                })
            });
        self.elapsed = self.elapsed.saturating_add(elapsed);
        let accounted = self.credited + self.pending.iter().map(|p| p.seconds).sum::<u32>();
        let total = self.elapsed.as_secs().min(u64::from(options.max_seconds)) as u32;
        if total > accounted {
            let seconds = total - accounted;
            let start = if known {
                self.cursor.or(self.last_wall)
            } else {
                None
            };
            if let Some(last) = self.pending.last_mut()
                && last.start.map(|s| s + i64::from(last.seconds)) == start
            {
                last.seconds += seconds;
            } else {
                self.pending.push(Part { start, seconds });
            }
            self.cursor = if known {
                start.and_then(|s| s.checked_add(i64::from(seconds)))
            } else {
                wall
            };
        } else if !known {
            self.cursor = None;
        }
        self.observed = now.max(self.observed);
        self.last_wall = wall;
    }
}

pub(crate) struct History {
    path: PathBuf,
    data: Data,
    saved_digest: Option<ring::digest::Digest>,
    saved_max_visit_id: u64,
    active: Option<Active>,
    next_id: u64,
    dirty: bool,
    read_only: bool,
    session_since: Option<i64>,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub seconds: u64,
    pub screens: usize,
    pub capped_seconds: u64,
    pub books: usize,
}
impl History {
    pub(crate) fn load(path: PathBuf, prior: BTreeMap<u64, u64>) -> Self {
        let loaded = primitive_app::with_storage(|| {
            let bytes = read_journal(&path)?;
            let data: Data = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if data.version != 1
                || !data.options.valid()
                || data.prior.len() > MAX_BOOKS
                || data.titles.len() > MAX_BOOKS
                || data.resets.len() > MAX_BOOKS
                || data.visits.len() > MAX_VISITS
                || data.titles.values().any(|s| s.len() > 1024)
                || data.visits.iter().any(|visit| !visit.valid())
            {
                return Err(invalid());
            }
            Ok((data, ring::digest::digest(&ring::digest::SHA256, &bytes)))
        });
        let missing = loaded
            .as_ref()
            .is_err_and(|e| e.kind() == io::ErrorKind::NotFound);
        let read_only = loaded.is_err() && !missing;
        let (data, saved_digest) = match loaded {
            Ok((data, digest)) => (data, Some(digest)),
            Err(_) => (
                Data {
                    version: 1,
                    prior,
                    options: Options::default(),
                    ..Default::default()
                },
                None,
            ),
        };
        let next_id = data
            .visits
            .iter()
            .map(|v| v.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let saved_max_visit_id = data.visits.iter().map(|visit| visit.id).max().unwrap_or(0);
        Self {
            path,
            data,
            saved_digest,
            saved_max_visit_id,
            active: None,
            next_id,
            dirty: missing,
            read_only,
            session_since: crate::mark_time::now(),
        }
    }
    pub(crate) fn options(&self) -> Options {
        self.data.options
    }
    /// Use the same dated-visit rule as the current-session statistics report.
    pub(crate) fn visit_in_current_session(&self, visit: &Visit, now: i64) -> bool {
        visit.start.is_some_and(|start| {
            start <= now && self.session_since.is_some_and(|since| start >= since)
        })
    }
    pub(crate) fn begin_session(&mut self, wall: Option<i64>) {
        self.session_since = wall.filter(|&time| valid_wall(time));
    }
    pub(crate) fn configure(
        &mut self,
        options: Options,
        now: Instant,
        wall: Option<i64>,
    ) -> io::Result<()> {
        if !options.valid() {
            return Err(invalid());
        }
        self.set_page_at(None, now, wall);
        self.active = None;
        if self.read_only {
            return Err(invalid());
        }
        let previous = std::mem::replace(&mut self.data.options, options);
        self.dirty = true;
        if let Err(error) = self.save(now, wall, true) {
            self.data.options = previous;
            return Err(error);
        }
        if !previous.enabled && options.enabled {
            self.begin_session(wall);
        }
        Ok(())
    }
    pub(crate) fn remember_title(&mut self, book: u64, title: &str) {
        if !self.read_only
            && title.len() <= 1024
            && (self.data.titles.len() < MAX_BOOKS || self.data.titles.contains_key(&book))
            && self.data.titles.get(&book).is_none_or(|old| old != title)
        {
            self.data.titles.insert(book, title.to_owned());
            self.dirty = true;
        }
    }
    pub(crate) fn title(&self, book: u64) -> Option<&str> {
        self.data.titles.get(&book).map(String::as_str)
    }
    pub(crate) fn prior(&self, book: Option<u64>) -> u64 {
        self.data
            .prior
            .iter()
            .filter(|(id, _)| book.is_none_or(|b| b == **id))
            .fold(0u64, |a, (_, b)| a.saturating_add(*b))
    }
    pub(crate) fn warning(&self) -> Option<&'static str> {
        if self.read_only {
            Some("Stored history is unrecognized and has been preserved.")
        } else if self.data.visits.len() >= MAX_VISITS || self.next_id == u64::MAX {
            Some("Reading history is full. Existing records have been preserved.")
        } else {
            None
        }
    }
    pub(crate) fn set_page(&mut self, page: Option<(u64, u64)>, now: Instant) {
        self.set_page_at(page, now, crate::mark_time::now());
    }
    pub(crate) fn set_page_at(
        &mut self,
        page: Option<(u64, u64)>,
        now: Instant,
        wall: Option<i64>,
    ) {
        self.set_page_at_with_chapter(page, None, now, wall);
    }

    /// Attribute newly observed reflow visits without changing their page key
    /// or the interpretation of existing unassigned journal entries.
    pub(crate) fn set_page_with_chapter(
        &mut self,
        page: Option<(u64, u64)>,
        chapter: Option<ReflowChapter>,
        now: Instant,
    ) {
        self.set_page_at_with_chapter(page, chapter, now, crate::mark_time::now());
    }

    fn set_page_at_with_chapter(
        &mut self,
        page: Option<(u64, u64)>,
        chapter: Option<ReflowChapter>,
        now: Instant,
        wall: Option<i64>,
    ) {
        let wall = wall.filter(|&w| valid_wall(w));
        if let Some(active) = &mut self.active
            && page == Some((active.book, active.page))
            && chapter == active.reflow_chapter
            && !active.running
        {
            active.running = true;
            active.observed = now;
            active.last_wall = wall;
            active.cursor = wall;
            active.next_checkpoint = now + CHECKPOINT;
            return;
        }
        self.accrue(now, wall);
        if page.is_none() {
            if let Some(active) = &mut self.active {
                active.running = false;
            }
            return;
        }
        if self
            .active
            .as_ref()
            .map(|a| (a.book, a.page, a.reflow_chapter))
            == page.map(|(book, page)| (book, page, chapter))
        {
            return;
        }
        self.active = None;
        if self.data.options.enabled
            && self.warning().is_none()
            && let Some((book, page)) = page
        {
            self.active = Some(Active {
                book,
                page,
                reflow_chapter: chapter,
                id: self.next_id,
                observed: now,
                last_wall: wall,
                cursor: wall,
                running: true,
                elapsed: Duration::ZERO,
                credited: 0,
                pending: Vec::new(),
                next_checkpoint: now + CHECKPOINT,
            });
            self.next_id = self.next_id.saturating_add(1);
        }
    }
    pub(crate) fn finish(&mut self, now: Instant) {
        self.set_page(None, now);
        self.active = None;
    }
    pub(crate) fn discard_active(&mut self) {
        self.active = None;
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.active
            .as_ref()
            .filter(|a| a.running)
            .map(|a| a.next_checkpoint)
    }
    fn accrue(&mut self, now: Instant, wall: Option<i64>) {
        let Some(active) = &mut self.active else {
            return;
        };
        active.observe(now, wall.filter(|&w| valid_wall(w)), self.data.options);
        let seconds = active.elapsed.as_secs();
        // Stock counts a visit above the maximum at the cap, even when Min exceeds Max.
        if seconds < u64::from(self.data.options.min_seconds)
            && seconds <= u64::from(self.data.options.max_seconds)
        {
            return;
        }
        let mut consumed = 0;
        for part in &active.pending {
            if let Some(last) = self.data.visits.last_mut()
                && last.id == active.id
                && last.start.map(|s| s + i64::from(last.seconds)) == part.start
            {
                last.seconds += part.seconds;
            } else if self.data.visits.len() < MAX_VISITS {
                self.data.visits.push(Visit {
                    book: active.book,
                    page: active.page,
                    start: part.start,
                    seconds: part.seconds,
                    reflow_chapter: active.reflow_chapter,
                    id: active.id,
                });
            } else {
                break;
            }
            active.credited += part.seconds;
            consumed += 1;
            self.dirty = true;
        }
        active.pending.drain(..consumed);
    }
    pub(crate) fn save(
        &mut self,
        now: Instant,
        wall: Option<i64>,
        force: bool,
    ) -> io::Result<bool> {
        if self.read_only {
            return Ok(false);
        }
        if !force && self.deadline().is_none_or(|deadline| now < deadline) {
            return Ok(false);
        }
        // Roll back the projected active segment on I/O failure. A USB return
        // can then discard it without ever charging the disconnected interval.
        let before = self.data.visits.clone();
        let active = self.active.clone();
        let dirty = self.dirty;
        self.accrue(now, wall);
        let result = (|| {
            if !self.dirty {
                return Ok(false);
            }
            self.persist()?;
            Ok(true)
        })();
        if result.is_ok() {
            self.dirty = false;
        } else {
            self.data.visits = before;
            self.active = active;
            self.dirty = dirty;
        }
        if let Some(active) = &mut self.active {
            active.next_checkpoint = now + CHECKPOINT;
        }
        result
    }
    fn persist(&mut self) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(&self.data).map_err(io::Error::other)?;
        let mut reduced = None;
        if bytes.len() as u64 > MAX_BYTES {
            // Never remove committed timing or attribution. If newly added
            // optional chapter metadata alone exceeds the journal budget,
            // retain the visits and persist those new records unassigned.
            let mut candidate = self.data.clone();
            let mut omitted = 0usize;
            for visit in &mut candidate.visits {
                if visit.id > self.saved_max_visit_id && visit.reflow_chapter.take().is_some() {
                    omitted += 1;
                }
            }
            if omitted == 0 {
                return Err(invalid());
            }
            bytes = serde_json::to_vec(&candidate).map_err(io::Error::other)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(invalid());
            }
            reduced = Some((candidate, omitted));
        }
        primitive_app::with_storage(|| {
            let lock = journal_lock(&self.path)?;
            let on_disk = match read_journal(&self.path) {
                Ok(current) => Some(ring::digest::digest(&ring::digest::SHA256, &current)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if on_disk.as_ref().map(AsRef::<[u8]>::as_ref)
                != self.saved_digest.as_ref().map(AsRef::<[u8]>::as_ref)
            {
                return Err(io::Error::other(
                    "Reading statistics changed on disk. Reopen Reader before saving.",
                ));
            }
            let result = write_journal(&self.path, &bytes);
            drop(lock);
            result
        })?;
        if let Some((candidate, omitted)) = reduced {
            self.data = candidate;
            eprintln!(
                "reader statistics_chapter_attribution_omitted={omitted} reason=journal_limit"
            );
        }
        self.saved_digest = Some(ring::digest::digest(&ring::digest::SHA256, &bytes));
        self.saved_max_visit_id = self
            .data
            .visits
            .iter()
            .map(|visit| visit.id)
            .max()
            .unwrap_or(0);
        Ok(())
    }
    pub(crate) fn visits(&self) -> &[Visit] {
        &self.data.visits
    }

    /// Sum only explicitly attributed visits for this edition. Older or
    /// imported page keys cannot be reversed to recover a spine chapter.
    pub(crate) fn reflow_chapter_totals(
        &self,
        book: u64,
        publication: u64,
        chapter_count: usize,
        now: Option<i64>,
    ) -> (HashMap<usize, (u64, bool)>, bool) {
        let mut totals = HashMap::new();
        let mut incomplete = false;
        if !self.data.options.enabled {
            return (totals, false);
        }
        for visit in self.data.visits.iter().filter(|visit| visit.book == book) {
            let Some(chapter) = visit
                .reflow_chapter
                .filter(|location| location.publication == publication)
                .map(|location| location.chapter as usize)
            else {
                incomplete = true;
                continue;
            };
            if chapter >= chapter_count {
                incomplete = true;
                continue;
            }
            let entry = totals.entry(chapter).or_insert((0u64, false));
            entry.0 = entry.0.saturating_add(u64::from(visit.seconds));
            entry.1 |= now.is_some_and(|wall| self.visit_in_current_session(visit, wall));
        }
        (totals, incomplete)
    }
    pub(crate) fn books(&self) -> BTreeSet<u64> {
        self.data
            .visits
            .iter()
            .map(|v| v.book)
            .chain(self.data.prior.keys().copied())
            .collect()
    }
    pub(crate) fn summary(&self, book: Option<u64>, range: Option<(i64, i64)>) -> Summary {
        let mut pages = BTreeMap::<(u64, u64), u64>::new();
        let mut seconds = 0u64;
        let mut books = BTreeSet::new();
        for visit in &self.data.visits {
            if book.is_some_and(|b| b != visit.book)
                || range.is_some_and(|(a, b)| visit.start.is_none_or(|s| s < a || s >= b))
            {
                continue;
            }
            seconds = seconds.saturating_add(u64::from(visit.seconds));
            books.insert(visit.book);
            *pages.entry((visit.book, visit.page)).or_default() += u64::from(visit.seconds);
        }
        Summary {
            seconds,
            screens: pages.len(),
            books: books.len(),
            capped_seconds: pages
                .values()
                .map(|&s| s.min(u64::from(self.data.options.max_seconds)))
                .sum(),
        }
    }
    pub(crate) fn days(&self, book: Option<u64>, zone: Zone) -> BTreeMap<Date, Summary> {
        let calendar = zone.calendar();
        let mut grouped = BTreeMap::<Date, (u64, BTreeMap<(u64, u64), u64>, BTreeSet<u64>)>::new();
        for visit in &self.data.visits {
            if book.is_some_and(|b| b != visit.book) {
                continue;
            }
            let Some(date) = visit.start.and_then(|start| calendar.date(start)) else {
                continue;
            };
            let (seconds, pages, books) = grouped.entry(date).or_default();
            *seconds += u64::from(visit.seconds);
            *pages.entry((visit.book, visit.page)).or_default() += u64::from(visit.seconds);
            books.insert(visit.book);
        }
        grouped
            .into_iter()
            .map(|(date, (seconds, pages, books))| {
                (
                    date,
                    Summary {
                        seconds,
                        screens: pages.len(),
                        books: books.len(),
                        capped_seconds: pages
                            .values()
                            .map(|&s| s.min(u64::from(self.data.options.max_seconds)))
                            .sum(),
                    },
                )
            })
            .collect()
    }

    pub(crate) fn calendar_month(
        &self,
        month: Date,
        book: Option<u64>,
        zone: Zone,
    ) -> BTreeMap<Date, CalendarDay> {
        let calendar = zone.calendar();
        let origin = self.data.options.calendar_start();
        let ranges: Vec<_> = (1..=month.month_days())
            .filter_map(|day| {
                let date = Date { day, ..month };
                let (start, end) = calendar.range(date, origin)?;
                Some((date, start, end))
            })
            .collect();
        let mut days = BTreeMap::<Date, (BTreeMap<u64, u64>, [u32; 24])>::new();
        for visit in &self.data.visits {
            if book.is_some_and(|b| b != visit.book) {
                continue;
            }
            let Some(start) = visit.start else {
                continue;
            };
            let end = start.saturating_add(i64::from(visit.seconds));
            let first = ranges.partition_point(|&(_, _, until)| until <= start);
            for &(date, from, until) in ranges[first..]
                .iter()
                .take_while(|&&(_, from, _)| from < end)
            {
                let lower = start.max(from);
                let upper = end.min(until);
                if lower < upper {
                    let (books, hours) = days.entry(date).or_default();
                    *books.entry(visit.book).or_default() += (upper - lower) as u64;
                    if self.data.options.histogram {
                        add_hourly_time(&calendar, hours, origin, lower, upper);
                    }
                }
            }
        }
        days.into_iter()
            .map(|(date, (books, hours))| {
                let mut books: Vec<_> = books.into_iter().collect();
                books.sort_unstable_by_key(|&(id, seconds)| (std::cmp::Reverse(seconds), id));
                let day = CalendarDay {
                    seconds: books.iter().map(|&(_, seconds)| seconds).sum(),
                    books: books.into_iter().map(|(id, _)| id).collect(),
                    hours,
                };
                (date, day)
            })
            .collect()
    }

    pub(crate) fn first_calendar_day(&self, book: Option<u64>, zone: Zone) -> Option<Date> {
        let start = self
            .data
            .visits
            .iter()
            .filter(|visit| book.is_none_or(|id| visit.book == id))
            .filter_map(|visit| visit.start)
            .min()?;
        zone.date_with_origin(start, self.data.options.calendar_start())
    }

    /// Calendar/timeline windows clip a visit across midnight; daily totals above
    /// intentionally use its start date like KOReader's SQL history query.
    pub(crate) fn timeline(&self, date: Date, book: Option<u64>, zone: Zone) -> Vec<&Visit> {
        let Some((start, end)) = zone.range(date, self.data.options.day_start) else {
            return Vec::new();
        };
        self.visits_in_window(book, start, end).collect()
    }

    fn visits_in_window(
        &self,
        book: Option<u64>,
        start: i64,
        end: i64,
    ) -> impl Iterator<Item = &Visit> {
        self.data.visits.iter().filter(move |v| {
            book.is_none_or(|b| b == v.book)
                && v.start
                    .is_some_and(|s| s < end && s.saturating_add(i64::from(v.seconds)) > start)
        })
    }

    pub(crate) fn day_seconds(&self, date: Date, book: Option<u64>, zone: Zone) -> u64 {
        let Some((start, end)) = zone.range(date, self.data.options.day_start) else {
            return 0;
        };
        self.visits_in_window(book, start, end)
            .map(|v| {
                let s = v.start.unwrap();
                (s.saturating_add(i64::from(v.seconds)).min(end) - s.max(start)).max(0) as u64
            })
            .sum()
    }
    /// Hourly bins are relative to the timeline origin, e.g. 04:20–05:20 is bin zero.
    pub(crate) fn hourly(&self, date: Date, book: Option<u64>, zone: Zone) -> [u32; 24] {
        let mut bins = [0u32; 24];
        let calendar = zone.calendar();
        let Some((start, end)) = calendar.range(date, self.data.options.day_start) else {
            return bins;
        };
        for v in self.visits_in_window(book, start, end) {
            let lower = v.start.unwrap().max(start);
            let upper = (v.start.unwrap() + i64::from(v.seconds)).min(end);
            add_hourly_time(
                &calendar,
                &mut bins,
                self.data.options.day_start,
                lower,
                upper,
            );
        }
        bins
    }
}

fn read_journal(path: &Path) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    Ok(bytes)
}

fn journal_lock(path: &Path) -> io::Result<fs::File> {
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.with_extension("lock"))?;
    let info = lock.metadata()?;
    if !info.is_file() || info.nlink() != 1 {
        return Err(invalid());
    }
    lock.lock()?;
    Ok(lock)
}

fn write_journal(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use ring::rand::{SecureRandom, SystemRandom};

    let random = SystemRandom::new();
    for _ in 0..4 {
        let mut nonce = [0; 16];
        random
            .fill(&mut nonce)
            .map_err(|_| io::Error::other("Could not reserve a journal file"))?;
        let temp = path.with_file_name(format!(
            ".reading-history-{:032x}.tmp",
            u128::from_ne_bytes(nonce)
        ));
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let node = file.metadata()?;
        let owns_path = || {
            fs::symlink_metadata(&temp).is_ok_and(|info| {
                info.is_file() && info.dev() == node.dev() && info.ino() == node.ino()
            })
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            if !owns_path() {
                return Err(io::Error::other("The temporary journal changed"));
            }
            fs::rename(&temp, path)
        })();
        if result.is_err() && owns_path() {
            let _ = fs::remove_file(&temp);
        }
        return result;
    }
    Err(io::Error::other("Could not reserve a unique journal file"))
}
fn add_hourly_time(
    calendar: &date::Calendar,
    bins: &mut [u32; 24],
    origin: u16,
    mut at: i64,
    end: i64,
) {
    while at < end {
        let Some((h, m, s)) = calendar.clock(at) else {
            break;
        };
        let clock = i64::from(h * 3600 + m * 60 + s);
        let shifted = (clock - i64::from(origin) * 60).rem_euclid(86400);
        let bin = (shifted / 3600) as usize;
        // Stop at civil hour changes as well as the shifted hour boundary.
        let step = (3600 - shifted % 3600)
            .min(3600 - clock % 3600)
            .min(end - at);
        bins[bin] = bins[bin].saturating_add(step as u32);
        at += step;
    }
}
fn valid_wall(time: i64) -> bool {
    (946_684_800..=253_402_214_400).contains(&time)
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Reading history exceeds the supported format or limits",
    )
}
#[cfg(test)]
mod tests;
