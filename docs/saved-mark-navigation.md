# Navigate saved marks

The reading Navigation menu offers Previous, Next, First and Last saved mark when the current book has saved marks. Previous and Next choose the nearest strictly earlier or later rendered page. First chooses the earliest marked page strictly before the current page; Last chooses the latest marked page strictly after it. These actions do not cycle within the current page or wrap at either end. Candidates include page bookmarks, reflow highlights and fixed-page highlights. Their original entries and text are never changed. The cancellable source-verified navigator determines page order and skips targets whose source anchors cannot be resolved. Fixed-page marks use the existing page jump. On a successful move, the existing location history supports returning to the previous position.

Latest saved mark opens the bookmark or applicable highlight with the newest recorded creation timestamp, including a mark on the current page. The menu, optional swipe and assigned gesture share the cancellable source-verified navigator and preserve the ordinary jump/history behavior. A known timestamp sorts after a missing one. Equal timestamps, including the case where all are missing, choose the last item in the existing iteration order: bookmarks first, followed by reflow highlights for a reflow book or fixed-page highlights for a fixed document. This is an ordering fallback, not proof of actual creation chronology. If that exact latest target cannot be verified, Reader reports the problem rather than jumping to a different mark; the current view and saved marks are retained. Source, folder, snapshot and memory restrictions described below also apply. This background handoff has not yet been behaviorally tested.

These are explicit navigation-menu actions; they do not alter physical page buttons or claim KOReader's persistent bookmark-browsing mode. Legacy page-only bookmarks use the existing saved-position restoration. The implementation has not yet been tested in simulation or on a Kindle, and matching on changed editions, hidden or stale markers, large chapter layouts and continuous-scroll behavior remains to be validated.

Selecting an interior HTML chapter of an extracted EPUB can require a bounded
background snapshot when the book has raw-position reading data. The chapter
request remains attached to that open: Reader verifies the edition identity,
chapter path and spine index against the immutable snapshot, seeks in the
candidate before replacing the current view, and retains the book's previous
position in Back history. Cancellation leaves the current view and saved data
untouched. A changed, ambiguous or unverifiable edition fails closed. Archive
restoration and the normal non-snapshot opening path retain their existing
behavior. This deferred chapter navigation has not yet been validated in
simulation or on Kindle.

When Bookmark Browser is opened from a document, Sources also offers **Current book**. It narrows the existing cross-book browser to that document, where its type, highlight-style and highlight-color filters remain available. This selection is transient: it does not replace a saved default source or reset the browser's existing filter preferences. Only marks already present in the Reader library are included; this does not add filtering to the separate per-book saved-marks screen. This workflow still requires simulation and device validation.

Bookmark Browser's Filters screen offers **Show current page in results** when opened from a document. It focuses the result-list page containing the nearest visible mark on or after the current page, falling back to the nearest earlier page. Ties use the first row in the existing display order. The command honors the source, search and filters, preserves date or reverse ordering, and does not move the reading position. Fixed-layout books search their matching result rows throughout the book. Eligible reflow books search matching chapters in a cancellable worker, including bounded cross-chapter quote verification; unverifiable books and oversized result snapshots retain the current-chapter fallback. If no eligible mark matches, Reader reports that instead of guessing. See [Bookmark Browser focus](bookmark-browser.md) for source requirements and limits. The new behavior has not yet been tested in simulation or on a Kindle.

The adjacent **Latest saved mark** shortcut opens details for the most recently
created matching mark in the open book. It searches only the existing filtered
rows and does not change filters, sorting, the mark or the reading position.
Back returns to the results page containing that mark. Known creation times
sort after missing times; equal times choose the later entry in Reader's stable
bookmark, then applicable highlight, and per-kind index order, matching its
existing latest-mark selector. KOReader's [Latest bookmark list command](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L1024-L1040)
also opens mark details but is disabled under some filters; Reader retains the
existing filtered-results scope. Simulation and Kindle validation remain pending.

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

Bookmark-only navigation collects and rechecks the complete bookmark snapshots
in slices of at most 128 entries. A cancellable background worker groups the
eligible entries by chapter, opens each needed chapter at most once, and keeps
only the nearest verified target. A progress popup offers Cancel; no reading
position or history changes until the complete snapshot and source/layout
stamps have been rechecked. Sleep, leaving the reading surface, book changes,
and storage return cancel the search. A retry reports if a canceled worker is
still stopping, so it cannot start overlapping layout workers. The temporary 512-bookmark and
eight-chapter limits no longer apply. To bound memory, snapshot data and the
worker's sort indices have a 4 MiB estimated budget; searches exceeding it fail
without moving. Regular reflow books without actual matching file source stamps
fail closed. Eligible extracted EPUB folders use a shared immutable snapshot and
full folder proof: at most 512 entries and 16 MiB of member data, no symlinks
or hidden members, and a legacy identity matching the open book. The original
visible anchor must resolve against that snapshot, and font stamps must match
the original foreground layout. Candidate layouts and flow maps come from the
snapshot; a final background proof and font recheck precedes installing the
staged Reading. Folder raw anchors and unsupported renderers fail closed;
hidden/nonlinear flow preparation is limited to 32 chapters. Capturing a full
strong proof at original book open, which could safely permit hidden members,
remains open. Fixed documents are checked for file changes during
the search but the current provider does not establish a file stamp from the
time the book was opened. File-stamp race behavior, the progress presentation,
and Kindle performance remain unvalidated.

KOReader's [current-document mark list](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L675-L705) and [highlight filters](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L960-L980) motivated the Current book source.

The shortcut follows the intent of KOReader's [Current page bookmark-list command](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L1008-L1022); its filtered fallback, verification limits and separate cross-book browser are Reader-specific behavior.


Navigation's **Previous**, **Next**, **First** and **Last saved mark** use the cancellable, source-verified background navigator for page bookmarks and the open provider's highlights. Candidates with missing or stale start anchors are skipped; fixed highlights must have valid page geometry. The worker preserves the original position until final snapshot and source checks, then uses the normal jump history. It snapshots marks within the existing 4 MiB budget and rechecks their complete saved values before committing. Skim retains its existing preview path. These changes are implementation-only and require simulator and Kindle validation.


The adjacent and outermost mark navigator orders candidate chapters toward the requested result and checks every candidate in the first chapter with a valid target. Remaining candidate chapters are skipped once the winning chapter is fully checked; stale anchors do not stop the search. Fixed-page entries still share one page-ordered candidate set. Snapshot, source and cancellation checks remain unchanged. This performance change is unverified in the simulator and on Kindle.


For eligible archive-backed EPUBs, the saved-mark worker moves its already-shaped winning Parley chapter to the reader after the source, settings and saved-mark snapshots are rechecked. It transfers the existing layout rather than cloning a second one. If the worker marks generated quotes as provisional, the renderer is not Parley, or hidden/nonlinear flows require other chapters, Reader retains the existing foreground restoration. Folder EPUBs and fixed documents keep their separate verified paths. This optimization requires simulator, memory and Kindle validation.
