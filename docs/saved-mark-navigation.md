# Navigate saved marks

The reading Navigation menu offers Previous, Next, First and Last saved mark when the current book has saved marks. Previous and Next choose the nearest strictly earlier or later rendered page. First chooses the earliest marked page strictly before the current page; Last chooses the latest marked page strictly after it. These actions do not cycle within the current page or wrap at either end. Candidates include page bookmarks, reflow highlights and fixed-page highlights. Their original entries and text are never changed. The existing Skim nearest-mark resolver determines page order. For a reflow mark with a source anchor, Reader requires the target chapter to resolve that anchor before reopening the saved position; an unresolvable nearest target produces an error instead of silently jumping to a fallback. Fixed-page marks use the existing page jump. On a successful move, the existing location history supports returning to the previous position.

Latest saved mark opens the bookmark or applicable highlight with the newest recorded creation timestamp, using the same selector as Reader's optional latest-mark gesture and existing jump/history behavior. A known timestamp sorts after a missing one. Equal timestamps, including the case where all are missing, choose the last item in the existing iteration order: bookmarks first, followed by reflow highlights for a reflow book or fixed-page highlights for a fixed document. This is an ordering fallback, not proof of actual creation chronology.

These are explicit navigation-menu actions; they do not alter physical page buttons or claim KOReader's persistent bookmark-browsing mode. Legacy page-only bookmarks use the existing saved-position restoration. The implementation has not yet been tested in simulation or on a Kindle, and matching on changed editions, hidden or stale markers, large chapter layouts and continuous-scroll behavior remains to be validated.

When Bookmark Browser is opened from a document, Sources also offers **Current book**. It narrows the existing cross-book browser to that document, where its type, highlight-style and highlight-color filters remain available. This selection is transient: it does not replace a saved default source or reset the browser's existing filter preferences. Only marks already present in the Reader library are included; this does not add filtering to the separate per-book saved-marks screen. This workflow still requires simulation and device validation.

Bookmark Browser's Filters screen offers **Show current page in results** when opened from a document. It focuses the result-list page containing the nearest visible mark on or after the current page, falling back to the nearest earlier page. Ties use the first row in the existing display order. The command honors the source, search and filters, preserves date or reverse ordering, and does not move the reading position. Fixed-layout books search their matching result rows throughout the book. Reflow books search only the current chapter using its existing layout; source anchors that cannot be resolved are skipped, while legacy page-only bookmarks must identify that chapter and a valid page. Searching adjacent chapters remains open. If no eligible mark matches, Reader reports that instead of guessing. The new behavior has not yet been tested in simulation or on a Kindle.

Reference: pinned KOReader [`ReaderBookmark` previous/next/first/last bookmarked-page helpers](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L512-L547) select annotations on strictly earlier or later pages, including highlights unless the page-bookmark-only option is set. KOReader's [`getLatestBookmark`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L619-L628) chooses by recorded annotation time. Reader independently reuses its existing Skim ordering and latest-mark selector; it does not copy upstream source.

When the open book has page bookmarks, the Navigation grid also offers
**Previous page bookmark** and **Next page bookmark**. They use the same
strict page ordering and history behavior, but skip highlights and their notes.
The existing saved-mark actions and Skim continue to include both bookmarks and
highlights. The bookmark-only actions verify reflow anchors against the current
source before ranking, skip unresolvable entries, and check anchorless legacy
pages are in range; no saved mark is changed. KOReader's previous/next helpers expose the analogous
`page_bookmark_only` parameter; Reader exposes it as separate actions rather
than claiming that KOReader has the same menu. These actions require simulator
and Kindle validation.

To bound foreground work, bookmark-only navigation checks at most 512 saved
bookmarks and loads at most eight distinct noncurrent chapter layouts per
attempt. If either limit would be exceeded, it reports that the bookmark list
should be used instead; it never jumps using an incomplete set. Layouts are
reused within the attempt. These limits are a temporary Reader constraint,
not KOReader parity. A follow-up should move source verification into a
bounded cancellable background search so books beyond these limits can still
use bookmark-only navigation without blocking the reading interface.

KOReader's [current-document mark list](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L675-L705) and [highlight filters](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L960-L980) motivated the Current book source.

The shortcut follows the intent of KOReader's [Current page bookmark-list command](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L1008-L1022); its filtered fallback, reflow chapter limit and separate cross-book browser are Reader-specific behavior.
