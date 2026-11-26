# Chapter navigation

Tap near the top of the reading page, open **Navigation**, and choose
**Previous chapter** or **Next chapter**. Reader moves to the nearest
chapter start on that side, using the same active outline as Contents. A jump
adds one step to return history and preserves the original reading location.
A command at the end of the available outline leaves the position and history
intact.

For facing pages, Previous searches from the first page and Next searches from
the last page. Reader resolves a reflow target to its exact fragment or saved
anchor, skipping unavailable targets and duplicate destinations on the same
visible page. The command respects the current hidden flow. Continuous reading
can land on a short final page and continue to the next section. In a
fixed-page document, returning after the jump restores the original zoom and
pan.

For an eligible verified packaged EPUB, a destination in another spine
chapter is resolved by a cancellable worker. It considers up to 2,048 outline
targets in at most 64 candidate chapters, nearest first; the current reading
frame and location are kept until the winning prepared chapter and source,
font, settings and outline snapshots are rechecked. An unreadable or unverifiable
chapter stops the search rather than selecting a potentially later heading;
unresolved heading fragments retain the existing skip behavior.
Same-chapter and fixed-page jumps retain their existing path. Folder EPUBs,
raw whitespace, in-page notes, hidden/nonlinear flows and Skim retain their
existing foreground path; other over-limit or ambiguous cases report that
navigation is unavailable. Layout of one chapter cannot be interrupted during
an individual shaping operation. This candidate has not yet been behaviorally
verified, and device latency and memory remain open.

[Navigation grid on the simulator](screenshots/chapter-navigation.png)

The simulator runs authored EPUB and PDF workflows. They cover out-of-order
headings, missing fragments, facing pages, custom outlines, hidden flows,
continuous scrolling, fixed-page zoom and pan, and the return step. Chapter
depth rules for markers, titles, and chapter commands remain independent
parity work.

The Mac suite passed 1,005 Reader library tests, one Kindle binary test,
14 EPUB package tests and four layout checks. The ARMHF suite passed 1,009
Reader library tests, 14 EPUB package tests and four layout checks natively
on the Oasis 3. Both skipped 62 optional Reader tests. The chapter navigation,
24-row settings, 24-row stable-label and EPUB page-label frames matched
between Mac and Oasis. The PDF Contents frame differed by one pixel.
These captures compare software rendering. Foreground input and E Ink
refresh remain separate checks.

[Reader chapter workflow](../src/app/chapter_navigation/tests.rs) · [KOReader chapter starts](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L597-L646) · [Reflow jump](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L704-L761) · [Fixed-page jump](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L1147-L1209)
