# Go to page

Open **Go to page** from the reading Navigation bar or its shortcut in Skim. Use **Return to location** to return after a successful jump.

| Input | Reflowable EPUB | Fixed-page PDF |
| --- | --- | --- |
| `25` | Document screen page 25 in the current supported layout | Physical page 25 |
| `+3` or `-3` | Move three screen pages from the current page | Move three physical pages, following the current reading flow |
| `3:2` | Chapter 3, screen page 2 of that chapter | Use a physical number or printed label |
| `50%` | The existing whole-book progress position | The existing physical-page percentage |
| `label 12` | A matching printed or stable page label | A unique printed label. Non-numeric labels such as `iv` also work without `label`. |
| `[4]` or `[2]1` | Use `chapter:page` in EPUBs with hidden flows | Page 4 of the main flow or page 2 of hidden flow 1 when custom hidden flows are enabled |

Relative jumps stop at the first or last page. Enter a number within the displayed range for an absolute jump. The EPUB footer keeps its chapter and page format. The editor shows the whole-document range when it has a valid count.

## Exact EPUB indexing

Reader uses the active font stack, dimensions, styles, hyphenation and viewport to calculate the screen-page index. A signed jump within an already laid-out regular chapter can resolve immediately. If the whole-document count is needed, submitting the request starts the existing bounded background worker when the book is eligible. The editor stays responsive and keeps the current reading location while it counts. The worker reproduces the active chapter and checks that the book and fonts still match before publishing the index. Reader validates the destination chapter against the same count before displaying it. The source scan happens away from the interface and short storage leases cover the file reads.

Editing the input, leaving the form, sleeping, changing the layout or returning from USB discards the pending jump. Unavailable counts leave the editor open with alternatives. Whole-document ordinals are currently available for regular, single-page, horizontal EPUB layout. Scroll, spreads, vertical text, special package layouts, hidden flows and books beyond the worker limits can still use `chapter:page`, percentage or labels where supported. Ordinary reading retains its existing pagination policy until a whole-document jump or the optional header requests a count. [Cache and worker](../src/session/screen_pages.rs) · [Background verification](../src/session/screen_pages/background.rs)

When a signed jump crosses a spine boundary and the full page index is not
available, an eligible verified packaged EPUB can instead count just the
intervening chapters on a cancellable worker. The candidate accepts at most
64 crossed chapters and a 100,000-page offset; it stops at the book's ends.
It requires unique linear spine paths, a regular single-page layout, exact
source/font and quote-prefix verification, and excludes raw or in-page notes.
The editor and reading location stay in place until the prepared destination
and original view are rechecked; a successful jump adds one return-history
entry. Unsupported layouts retain the earlier counting or unavailable path.
One chapter's shaping is not interruptible, and this implementation has not
yet had behavioral, simulator or Kindle verification.

If an indexed destination still needs quote-depth preparation, Reader verifies
its source and opens the closest available page with an approximate-page
notice. It preserves the current passage when exact preparation completes;
because it does not re-resolve the requested document ordinal, a final notice
still marks the opened page as approximate until the next navigation. If
preparation is unavailable, the notice says the page may differ. Relative
jumps within the current chapter remain available while its layout is
provisional. Whole-document numbers remain unavailable until that layout is
exact. [Quote preparation limits](quote-marks.md)

[Approximate page notice](screenshots/quote-repair-completed-narrow.png) ·
[Unavailable preparation notice](screenshots/quote-repair-unavailable-narrow.png)

## Stock comparison

Pinned KOReader dispatches plain numbers to absolute page jumps, signed values to relative jumps, and percentage through its own button. The rolling engine adds a signed amount to the current rendered page. The paging engine walks the next or previous pages and stops at the boundary. KOReader's published-page mode sends the entered text to its page-label lookup. Its `[x]y` form selects an ordinal in flow `y`, with flow 0 as the main flow. Reader preserves its existing `label ` prefix and direct PDF label lookup, and uses its original percentage formulas. KOReader floors fixed percentages against physical page count. Reader rounds a fractional distance between the first and last physical page. [Pinned ReaderGoto](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readergoto.lua#L139) · [Fixed paging](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L1166) · [Rolling pagination](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L749)

The stock harness executes those handlers with eight authored pages, two hidden fragments and a published-label map. The Mac app tests use the production editor and keyboard, a multi-section EPUB, and a PDF with the same hidden-flow layout. They verify the exact destination, return history, range rejection, cancellation, and fallback to chapter addressing in scroll mode. [Stock fixture](../scripts/koreader-go-to-page-check.lua) · [Reader tests](../src/app/go_to_page/tests.rs)

[EPUB counting in the editor](screenshots/go-to-page-index.png) · [PDF hidden-flow instructions in dark mode](screenshots/go-to-page-pdf-flows-dark.png)
