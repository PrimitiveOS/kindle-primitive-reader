# Find a matching PDF or DjVu page

Open **Find in book**, enter a word or phrase and tap **Previous** or **Next** to start searching on the visible page. **From start** scans forward from page one up to the page before the current page. **From end** scans backward from the last page down to the page after the current page. These searches stop at the current page. If there is no match on the requested side, Reader keeps the current view.

On a matching page, the four footer arrows run the same searches. From left to right they jump to the first earlier match, find the previous matching page, find the next matching page and jump to the last later match. All matching words on that page are highlighted. Return to the previous location restores the original viewport across multiple searches. **Find all** displays individual occurrences from the beginning of the document. [KOReader search dialog and endpoint actions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L545-L588) · [KOReader fixed-page search intervals](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1613-L1649)

For a single word, the spaces typed at the ends of the query select which part of a word to match. This also applies to **Find all** and searches repeated from history. For example, entering `sand` anywhere matches *quicksand* and *sandbar*. A space before `sand` matches the start of the word. A space after it matches the end. Spaces on both sides match the whole word. Punctuation belongs to the word box, so a whole-word search for `sand` skips *sand,* but still matches *Sand* when **Match case** is off. Each matching word box counts once, including a word with two occurrences. These switches apply to single-token queries. Multiword and multi-character CJK phrases keep their existing phrase search. [KOReader pattern preparation and word matches](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1503-L1565)

With **Match case** off, typing three periods finds a printed ellipsis, and ordinary letters find compatible forms such as fullwidth letters or a ligature. **Match case** on preserves the spelling and punctuation as printed. **Ignore accents** applies separately. The overlay covers the original word box, including the original glyphs. An authored timetable illustrates a search for three periods that found one printed character. [Simulator capture](screenshots/fixed-find-ellipsis.png)

| Entering a search | Last matching page |
|---|---|
| ![PDF Find controls for the beginning, previous, next and end of the search](screenshots/fixed-find-pages.png) | ![The last matching page with its highlighted word and four navigation controls](screenshots/fixed-find-matches.png) |

In the authored coastal guide, the query with a space on both ends selects only the standalone word. The same letters in *sandbar*, *quicksand* and *Sand,* remain visible for comparison. [Simulator screenshot](screenshots/fixed-find-whole-word.png)

KOReader's PDF and DjVu providers call the same fixed-page search implementation. It includes the current page on a new search, skips it when continuing, and returns all matching word rectangles on the first page it finds. Its reading UI remembers the original viewport even when the first match is on that page. [PDF provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L352-L357) · [DjVu provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/djvudocument.lua#L271-L277) · [Search and page bounds](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1593-L1648) · [Highlighting and return](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L418-L431)

Directional search checks one physical page per event-loop turn and retains up to 5,000 occurrences on its matching page. Embedded text and previously recognized text use the same coordinates as normal selection. Regular expressions are available for reflowable books.

**Find all** shows up to 5,000 occurrences in document order. Results appear as pages are scanned. You can browse the partial list, open a match, or tap Back to stop. The four arrows open the first, previous, next and last available result pages. The Kindle page buttons also work. Opening **Matches** again returns to the last viewed result page. Each tap or repaint builds only the eleven visible rows. The search uses the existing document, yields to input after up to four pages or 60 milliseconds of work, and stops when it reaches the cap. A second limit bounds retained snippets and word geometry to 20 MiB plus the result vector’s spare capacity. [KOReader list ceiling](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L59-L72) · [Fixed document occurrence collection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1650-L1692) · [KOReader menu and last visible row](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L746-L786)

An authored 120-page harbour register has 5,280 matching words. The last list page shows occurrences 4,995–5,000 on physical page 114. Selecting the last row highlights its word. **Previous location** returns to the starting viewport. [Last page of search results](screenshots/fixed-find-last-results.png) · [Reader interface regression](../src/app/fixed_results_tests.rs)

## Original PDFs on the Mac

For each original, an independent Poppler extraction located an interior page with a matching word and its preceding and following matching pages. The normal Reader controls first opened that page, then the next matching page, then returned across it to the preceding match. The tests also ran ordinary **Find all** and confirmed its hundredth occurrence came before these pages.

| Original and physical page count | Independent page containing the 100th matching word | Pages opened in Reader | Word boxes highlighted in Reader |
|---|---:|---|---|
| 030, 229 pages | 8 | 115 → 116 → 115 → 114 | 9, 15, 9, 3 |
| 044, 1,041 pages | 9 | 521 → 522 → 521 → 520 | 30, 57, 30, 41 |
| 054, 1,066 pages | 106 | 542 → 543 → 542 → 538 | 1, 2, 1, 2 |
| 056, 41 pages | 11 | 21 → 22 → 21 → 20 | 7, 6, 7, 5 |

All four returned to the original reading position. A separate authored PDF test restored a zoomed and panned source point to within 0.01 PDF points. The five-page authored coastal guide in the screenshots contains repeated terms, an intervening page without a match and two matching words on the displayed page. A rotated DjVu with compressed embedded text verified that an accent-insensitive search selects its original word rectangle. [Functional tests and authored guide](../src/app/nearby_search.rs) · [Installed KOReader comparison](../scripts/koreader-fixed-direction-check.lua)

## Oasis 3 comparison

The installed KOReader fixed-page matcher and the native Reader agreed on all sixteen page destinations and all sixteen word-box counts in the table. The four Reader runs also opened their final passage with the expected marks and restored the original viewport through the normal history control. The installed KOReader comparison used its own unwrapped-page coordinate conversion, page-direction logic and MuPDF text extraction. Four additional native tests used the authored PDF and DjVu fixtures.

| Original | Complete native UI test | Peak resident memory |
|---|---:|---:|
| 030 | 2.09 s | 21,192 KiB |
| 044 | 43.10 s | 186,612 KiB |
| 054 | 4.29 s | 36,868 KiB |
| 056 | 2.72 s | 19,736 KiB |

The elapsed times include opening the book, rendering the target pages, checking the matching regions and returning to the starting position. The low-priority tests ran while another library audit was active. SHA-256 checks before and after each case confirmed the original PDF, Documents inventory, installed Reader, library, reading statistics and Browser session were preserved. All temporary device files were removed.

### Single-word boundaries on original pages

An independent Poppler extraction, the Mac Reader, the ARM Reader on the Oasis 3 and the installed KOReader KOpt search all agreed on the eight counts below. The check compared **Find all** rectangles with **Next** overlays on the same page, restored the starting viewport and replayed the original query from a fresh Reader session. When a constrained query had no match on the current page, cancelling the forward search preserved the viewport.

| Physical page | Anywhere | Space before | Space after | Spaces on both sides |
|---|---:|---:|---:|---:|
| 521 of 1,041 | 30 | 28 | 27 | 27 |
| 542 of 1,066 | 1 | 1 | 0 | 0 |

The Oasis check used the two original PDFs in Documents and an isolated Reader profile. The authored tests, both original checksum checks, retrieval of the KOpt counts and the Reader navigation checks completed in 216 seconds. Both source files retained their original checksums. The test removed its temporary state after collecting these results.

### First and last matches on original pages

A full independent Poppler extraction identified the first and last physical pages containing the query. Reader started inside each original PDF, used **From end**, tapped the first-match arrow, returned with the last-match arrow, then restored the original viewport. A repeated request for an earlier first match or a later last match preserved the displayed page and reported the exhausted direction.

| Physical pages | Starting page | First matching page | Last matching page | Pages containing a match |
|---:|---:|---:|---:|---:|
| 229 | 115 | 2 | 229 | 225 |
| 1,041 | 521 | 1 | 1,041 | 1,025 |
| 1,066 | 542 | 3 | 1,066 | 673 |
| 41 | 21 | 1 | 41 | 41 |

The installed KOReader KOpt search, the ARM Reader on Oasis 3, the Mac Reader and Poppler agreed on all sixteen visited page numbers and twelve counts of highlighted word boxes. Both readers found no match beyond either end of each four-step route. The six authored controls and the four original comparisons ran in 110 seconds on the Kindle. Reader restored each original viewport and the four source files retained their checksums.

### Unicode search on original pages

KOReader passes each fixed-page word and each case-insensitive query token through utf8proc's NFKC case fold. Reader applies that comparison to PDF, DjVu and recognized word boxes. Phrases can span neighboring words on the same reading column. The matches retain the source text's original byte ranges and page geometry. Reader's normalization passed all 1,112,064 scalar values in Unicode 18's independent mapping table. The case-sensitive comparison preserves the original characters. [KOReader word matching](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1503-L1565) · [KOReader's utf8proc wrapper](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/utf8proc.lua#L20-L38) · [Unicode normalization mappings](https://www.unicode.org/Public/18.0.0/ucd/DerivedNormalizationProps.txt)

Mac Reader and an independent extraction using KOReader’s word boundaries agreed on 36 combinations of query and case mode across three original PDFs. For example, three periods found one word containing an ellipsis on one page and three words on another. With **Match case** enabled, three periods found zero words on both pages. The printed ellipsis itself found all four. Leading and trailing ordinary spaces apply their usual word-edge rules. Tabs delimit query tokens without setting those edge rules. A nonbreaking space attached to a query remains part of its token when it is folded.

KOReader’s MuPDF provider closes the current word box just after a general punctuation character such as an ellipsis. On an original page containing an ellipsis immediately followed by a closing quotation mark, the two characters have adjacent source byte ranges and separate selection rectangles. Reader preserves that boundary without inserting a space into the original text. A space after a three-period query therefore finds all three ellipsis-ending words on that page. A whitespace-only extraction groups the closing quotation mark with the preceding word and would count two. [KOReader’s PDF word-box builder](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/mupdf.lua#L537-L568)

The final Oasis 3 check compared the installed KOReader KOpt results with Reader on three original PDFs. All 36 query and case combinations agreed, with 31 matching word-box instances. Source byte ranges and normalized rectangle coordinates were identical on the Mac and Oasis. KOReader and Reader estimate text height differently. Every pair of corresponding rectangles overlapped at least 98.2% of the smaller rectangle, and the lowest intersection-over-union was 0.776. The 104-second native check preserved the three source checksums and removed its temporary state. Six further Mac runs on four originals verified the earlier word-boundary and first/last-page navigation results with the final word-box extractor.

### Full fixed-document results on the Oasis 3

The Mac and the native Reader each opened four original PDFs from isolated profiles, searched from the normal Find form, and navigated using the first, last and physical page buttons. Tapping the final row highlighted its original word rectangle. Reopening Matches retained the result page. Returning to the previous location restored the original viewport in all four books. The installed KOReader's KOpt engine returned the same four total counts. The table measures Reader on the Oasis with the ordinary search scheduler and low process priority. First result measures the first refresh with a selectable row. Last-page input includes building the destination rows. Peak memory covers the whole test, including opening and rendering the book. [Native comparison harness](../scripts/koreader-fixed-direction-check.lua) · [Reader UI test](../src/app/fixed_results_tests.rs)

| PDF pages | Results | Pages scanned | First result | Scan complete | Last-page input | Peak memory |
|---:|---:|---:|---:|---:|---:|---:|
| 41 | 360 | 41 | 254 ms | 6.76 s | 0.68 ms | 22,448 KiB |
| 229 | 4,131 | 229 | 102 ms | 14.16 s | 0.84 ms | 25,520 KiB |
| 1,041 | 5,000 | 161 | 107 ms | 23.07 s | 1.44 ms | 154,188 KiB |
| 1,066 | 1,150 | 1,066 | 642 ms | 319.22 s | 248.26 ms | 64,648 KiB |

The four Kindle lists retained 46,664 to 548,032 bytes of snippets, word bounds and row headers. The largest input was 96,319,018 bytes. KOReader's full reference run took 44.27 seconds on the 1,066-page input. Reader's longer scan on this book remains a performance gap. The complete comparison took 571 seconds. SHA-256 checks confirmed all four original files were unchanged and the temporary Kindle profiles were removed.

For three originals, every occurrence appeared on the same physical page in both readers. The fourth contains a decorated initial on page 9. Its first letter is about 2.7 times the height of the two adjacent letters. Reader and Poppler's raw text recognize the resulting word. KOReader reports its parts as separate boxes. This gives Reader 58 matches on that page and KOReader 57. KOReader also returns five fully off-page matches on page 14 and four on page 16. Reader restricts selectable words to the visible crop. A one-to-one comparison matched all 103 on-page word boxes on page 14 at a minimum 85% overlap of the smaller box. Both lists reach 5,000 on page 161. Comparing only drawable stock matches leaves the one decorated-initial difference and its effect on the final retained row. [Page text and cropped word geometry](../src/fixed_text.rs)

### Fixed-page text extraction costs

Reader reuses parsed PDF resources while it extracts consecutive pages from one open document. The cache is rebuilt after 16 reads, a jump beyond the adjacent page, an extraction error, or a truncated text layer. Reopening starts with an empty cache. Cached resources hold in-memory document data and have no persistent file. [Hayro’s document-scoped interpreter cache](../vendor/hayro-interpret/src/context.rs) · [Reader’s cache lifetime and tests](../src/pdf_text_session.rs)

The Mac comparison used the same executable with caching enabled and disabled for each original PDF. All four runs reproduced every result’s original page, total count, retained search data size, and returning viewport. On the 1,066-page original, the full scan fell from 54.2 to 21.0 seconds. The first usable result arrived in 93 ms without the cache and 119 ms with the cache in those runs. The full scans in the three other originals also completed sooner. On a 64-page interior sample, caches shared across 1, 4, 16 and 64 pages produced identical decoded text, word boxes and line boxes.

The Oasis ran the same paired comparison against all four original PDFs. Each pair used one native binary and the same result screen and scheduler. All per-page result counts, allocations and previous-location checks matched. The elapsed times include yielding to the input loop. Other on-device checks ran during the comparison, so these measurements also reflect the device workload.

| Physical pages | Matches | Fresh resources on each page | Reused resources |
| ---: | ---: | ---: | ---: |
| 41 | 360 | 4.49 s | 1.90 s |
| 229 | 4,131 | 27.26 s | 18.62 s |
| 1,041 | 5,000 | 55.49 s | 16.49 s |
| 1,066 | 1,150 | 150.02 s | 36.98 s |

The four original source files, Reader library and Browser session retained their checksums. The temporary verification directories were removed.
