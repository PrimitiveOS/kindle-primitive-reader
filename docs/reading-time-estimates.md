# Time left in a chapter or book

Tap the reading footer to cycle to **Chapter time left** or **Book time left**. To keep a time estimate beside the percentage, open **Reading controls → Reading status bar**, enable **Show items together**, then choose **Chapter** or **Book** under **Time estimate in combined status**. The choice applies to the next book you open. Each book learns its own pace. [Full Kindle-size screenshot from the simulator](screenshots/reading-time-left.png)

Reader starts with one minute per page. It replaces that starting value with the average time spent on distinct pages of the current book. A visit under five seconds counts as a skim. Rereading adds to the previous time for that page up to two minutes total. A two-page spread divides the active time by the number of displayed pages. Menus and sleep pause the clock. For example, 20 seconds on one page and two visits of 40 and 10 seconds on another give a pace of 35 seconds per page.

The footer multiplies the pace by the pages still to read after the displayed page or spread. In a PDF or other fixed-page book, the count runs to the next chapter destination or the end of the document. Reflowable books use the actual pages in the current spine item. A book-wide estimate for later items uses their uncompressed source sizes and the current item's laid-out page density. **About** marks that estimate. Illustrations, markup density and changes in type size can affect it. A final page shows **0 min**.

## KOReader comparison

KOReader v2026.07.1 computes the two footer items from the remaining book or chapter pages and its statistics plugin's seconds per page. The plugin starts a new book at half of the maximum page time. It records eligible page visits, excludes suspension time and queries the sum of capped durations per distinct page. Reader follows these defaults. KOReader can paginate a whole EPUB before reading. PrimitiveOS Reader lays out one spine item at a time, so the table distinguishes measured chapter pages from weighted whole-book estimates. [KOReader footer](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L322-L334) · [Chapter boundaries](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L714-L738) · [Default pace and SQL](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L26-L50) · [Initial pace](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L174-L184) · [Pausing and resuming](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L2694-L2722) · [Conversion to duration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/statistics.koplugin/main.lua#L3025-L3030)

Reader loads calibration for the open book only. It stores the accumulated seconds for up to 16,384 qualifying source-position keys per title. A reflow page uses the chapter and the source position of its first text or illustration. Each fixed page has its own key. An atomic file in `data/reading-pace/` persists the samples across exits and USB sessions. The existing `reading-stats.json` continues to hold total active reading seconds, including short visits. Existing totals do not identify time on individual pages, so calibration begins with new visits.

## Verification

Four originals from the Oasis library were replayed on the Mac with private, isolated state. The same clock trace spent 72 seconds reading, 1,000 seconds in the tools and 1,000 seconds asleep. Each test reopened the pace file, preserved the seeded 3,600-second reading history, and reported 3,672 active seconds. The two measured pages contained 70 seconds combined. The original files retained their SHA-256 digests. The pinned upstream SQLite query independently yielded two distinct pages and 35 seconds per page.

| Original | Reader position after replay | Chapter pages left | Book pages left | Chapter time | Book time |
| --- | --- | ---: | ---: | ---: | ---: |
| 030, PDF | Physical page 117 | 112 | 112 | 1 hr 5 min | 1 hr 5 min |
| 035, EPUB | Spine item 12, layout page 7 | 12 | About 319 | 7 min | About 3 hr 6 min |
| 042, EPUB | Spine item 15, layout page 5 | 28 | About 493 | 16 min | About 4 hr 47 min |
| 054, PDF | Physical page 544 | 18 | 522 | 10 min | 5 hr 4 min |

Apple PDFKit independently counted all 229 and 1,066 physical pages in the two PDFs and followed their publisher outlines. Both chapter boundaries and both remaining book counts agreed with Reader. The Mac suite passed 441 library tests, 14 EPUB-package tests and four layout integration tests. Five opt-in library scenarios require separate fixtures.

On the Oasis 3, the installed KOReader found a unique line of text from Reader’s displayed page in EPUB 035. CREngine placed it on page 99 of 466 and counted 14 chapter pages and 367 book pages left. At the shared 35-second pace, these produce 8 min and 3 hr 34 min. Reader showed 7 min and about 3 hr 6 min at the same passage. Substituting Reader’s remaining-page counts into KOReader’s own time function reproduced both Reader durations exactly. This comparison measures the effect of pagination and book-size estimation on the displayed times.

On the same Oasis, Reader and the installed KOReader matched both physical-page and chapter counts in PDFs 030 and 054. The three native replays reproduced the Mac values and preserved all three original digests, the Documents inventory and existing Reader and browser state. The three authored tests passed. The full native replay process peaked at 37,372 KiB for EPUB 035, 20,952 KiB for PDF 030 and 36,576 KiB for PDF 054. The private test profiles were removed after verification. These runs used background CPU priority while other verification was active.

To replay the authored book and generate the screenshot, run the library test in the normal Reader checkout.

```sh
READER_ETA_CHAPTER_SCREEN="$PWD/reading-time-left.png" \
  cargo test --locked --lib replays_reading_pauses_rereads_and_reopens_the_measured_footer
```

The same replay accepts a local file through `READER_ETA_DOCUMENT`, with one-based `READER_ETA_CHAPTER` and `READER_ETA_PAGE`. Reading-time unit tests also cover the 120-second cap, spreads, revisits, atomic write retry and recovery of existing data. The authored UI test checks both footer modes and zero remaining on the final page. The Kindle stock comparator reads the installed statistics functions, uses an in-memory database and reports anonymous numeric results. [Reader replay](../src/app/reading_footer/pace_replay.rs) · [KOReader comparison script](../scripts/koreader-reading-pace-check.lua)
