# Daily reading statistics

Open **Tools › Reading statistics**, **Reading data › Reading statistics**, or tap **Reading time** in a book's information. The clock icon in Tools opens statistics for the current book. Choose **All books** to compare the library, including books whose source file is no longer available.

| View | Controls and information |
| --- | --- |
| Summary | Recorded reading time, distinct reading screens, capped page time, earlier undated screen time, daily history and calendar |
| Daily history | Dates in reverse order, reading time, distinct screens and books; tap a date for its timeline |
| Time range | Compare reading time and distinct screens by week or month, then open a period for its individual books |
| Calendar | A month grid with reading time, book counts and relative duration bars; tap a day, swipe, or use page buttons to change months |
| Day timeline | An hourly histogram and book intervals clipped to the selected day; tap an interval for its book sessions and **Continue reading** |
| Settings | Enable statistics, minimum and maximum visit time, freeze finished books, week start, calendar day origin, histogram and future months |

<img src="screenshots/reading-calendar.png" width="350" alt="Reader's monthly reading calendar">
<img src="screenshots/reading-timeline.png" width="350" alt="Reader's day timeline with hourly reading bars and book sessions">

[Summary](screenshots/reading-statistics-summary.png) · [Daily history](screenshots/reading-daily-history.png) · [Books](screenshots/reading-statistics-books.png) · [Book sessions](screenshots/reading-book-sessions.png) · [Settings](screenshots/reading-statistics-settings.png)

## Time range reports

**Time range** shows the eight stock report choices. **All books** shows
recorded time across the library. **Books by week** and **Books by month**
open a period and its individual books. The last week, last month, and
last year offer daily reports. The last year also offers weekly groups,
and **All stats by month** includes every recorded month. Select a weekly
or monthly total to see its days, then select a day to see the books read.
Back restores the list and page. Continue reading resumes the selected book.
The book filter applies to dated reports. All books uses the entire library.
[Report menu](screenshots/reading-statistics-range-menu.png) · [Last month by day](screenshots/reading-statistics-range-days.png) · [Weekly report](screenshots/reading-statistics-weekly.png) · [Books in a week](screenshots/reading-statistics-period-books.png)

Reports use each visit's local start date and count repeat visits toward
elapsed time. Each screen counts once per book within the displayed group.
Rolling cutoffs follow the stock 7-, 30-, or 365-day calculation and include
the cutoff instant. Weekly groups follow SQLite's calendar year and Monday
week numbering, including Week 00. Drill-down uses the dates included in
the selected report, including a partial first week or a year boundary.
Earlier undated time remains in the summary. [Pinned Time range callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L1303-L1448) · [Pinned SQL grouping and cutoff](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L1793-L1998)

The executable Lua comparison runs the pinned menu callbacks and extracts
its SQLite queries. An authored UTC fixture checks a visit one second before
and exactly at the cutoff, rereads, and the New Year weekly split. Mac tests
exercise nested reports, saved positions, screenshots and the unchanged
history journal. Kindle verification is pending.
[Stock comparison](../scripts/koreader-statistics-time-ranges-check.lua) · [History aggregation tests](../src/reading_stats/history/report.rs) · [Reader interaction tests](../src/app/reading_history/tests.rs)

## Reset statistics

Open **Reading statistics › Statistics settings › Reset statistics**. Reset
its current book, choose another book from the recorded list, or clear books
below 1, 5, 15, 30 or 60 minutes. Bulk reset protects the current book. The
confirmation shows the selection or count. Cancel returns without applying it.

Reader removes recorded visits and the earlier undated snapshot for the
selected books. Book information starts its displayed timer at zero and the
footer learns a fresh reading pace. Source files, bookmarks, notes and resume
positions are preserved. The reset journal tracks a baseline for the earlier
screen-time counter and invalidates previous pace samples. This keeps the
reset durable in a single atomic save while reading data remains compatible
with existing profiles.

The pinned Lua comparison executes the reset menu and callbacks. It verifies
the current-book and per-book actions, strict cutoffs and confirmation. Mac
tests cover all three paths, exact threshold behavior, write failure, reopening,
old pace samples and the unchanged source EPUB. Kindle verification is pending.
[Reset menu](screenshots/reading-statistics-reset-menu.png) · [Confirmation](screenshots/reading-statistics-reset-confirm.png) · [Pinned KOReader callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L2338-L2534) · [Executable comparison](../scripts/koreader-statistics-reset-check.lua)

### Reset one book for a day, week or month

Open the list of books for the date or period, then hold a book row. The
confirmation shows the selected book, dates and number of recorded visits.
Reset removes those visits and refreshes the report. Other books, visits
outside the displayed dates, undated screen time and the reading pace are
preserved. Cancel leaves the report intact.

The pinned comparison exercises Cancel and Reset, checks the selected book
and interval in the SQLite statement and verifies the refresh callback. Reader
uses an exclusive upper date boundary to preserve visits at the next midnight.
[Simulator](screenshots/reading-statistics-period-reset.png) · [Pinned KOReader period reset](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L2222-L2265)

## Counting time

New history defaults to the stock five-second minimum and 120-second maximum per visit. Quick skims do not count. Re-reading keeps a new visit. Reading time includes those rereads; capped page time sums at most the configured maximum for each distinct reading screen. These are separate values, as in KOReader's uncapped and capped book queries. Finished books continue recording by default, with an option to freeze them. [Stock defaults and total queries](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L29-L90) · [Page visit accounting](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L2560-L2652)

Daily history groups visits by their recorded start date. The calendar and histogram instead clip each recorded interval at the selected day's boundaries. Thus a visit across midnight can contribute time to two calendar days while its start-date total remains on the first day. The calendar can start its day at midnight, 04:00 or 06:00. The date list retains midnight grouping. A day has the duration given by the device's local calendar, including daylight-saving transitions. [Stock daily totals](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L1453-L1479) · [Stock timeline clipping and gap merging](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L2862-L2934)

Disabling statistics or freezing a finished book pauses all three time trackers, including the existing screen-time and pace counters. Sleep, USB storage and non-reading screens pause recording. Short intervals on the same page can combine across sleep toward the minimum, without counting the pause or resetting the visit's maximum. Returning to Library finishes that visit. Timeline rows merge adjacent intervals for the same book when their gap is no more than the larger of 30 seconds and the minimum visit time. Gaps contribute no reading time to totals or the histogram.

Elapsed time comes from the monotonic clock. UTC timestamps provide dates. If the observed wall clock changes independently, the ambiguous interval stays explicitly undated; later reliable intervals can receive dates again. Dates before 2000 are also treated as uncertain device time. Changing the device time zone regroups known instants in its current local zone. [Device time zone and day boundaries](device-local-time.md) Whole-second clock sampling allows two seconds of observation tolerance.

## Existing data and storage

The existing `reading-stats.json` screen-time counter is retained. On the first use of the separate `reading-history.json` journal, its saved totals become **Earlier screen time · undated**. Reopening Reader preserves that snapshot even as the older counter continues updating while statistics are enabled. No dates are invented for those aggregates, and they are never added to recorded history totals. **Uncertain clock · undated** belongs to the new filtered history and is already included in recorded reading time.

The new journal has its own versioned format and atomic save. It checkpoints active reading every minute and at save transitions. Failed writes retain completed pending records for retry and leave the existing file intact. USB return discards an uncommitted active interval. Unrecognized, corrupt or oversized history is preserved with a warning. The journal is bounded to 100,000 interval records, 10,000 book labels and 16 MiB. It reports a full history rather than silently deleting earlier records.

## Evidence and remaining comparisons

Deterministic tests cover min/max boundaries, rereads, capped and uncapped totals, migration and reopening, midnight, shifted days, UTC offsets, leap years, year changes, fractional notifications, clock corrections, sleep, USB and atomic write failures. Separate process tests exercise 23-hour and 25-hour local days. Desktop controls check calendar navigation, book filters, settings and black/white held feedback. Real cached EPUB and PDF workflows open the source documents, browse statistics, save and verify unchanged reading positions and source bytes. Native acceptance remains pending the combined Kindle build.

Reflow uses source-position reading-screen identities, so page numbers
and totals need separate layout-change comparisons with KOReader's
rescaling view. Reader's pace estimator has its own sample policy.
Minute-level day-origin editing, configurable book-span labels and local
statistics database import remain open. Reader records the actual pre- and
post-pause portions of each visit.
[Stock layout rescaling](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L439-L441) · [Stock pause and resume timestamps](https://github.com/koreader/koreader/blob/v2026.07.1/plugins/statistics.koplugin/main.lua#L2694-L2732)
