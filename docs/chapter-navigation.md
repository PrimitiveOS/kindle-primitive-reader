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

[Navigation grid on the simulator](screenshots/chapter-navigation.png)

The simulator runs authored EPUB and PDF workflows. They cover out-of-order
headings, missing fragments, facing pages, custom outlines, hidden flows,
continuous scrolling, fixed-page zoom and pan, and the return step. Chapter
depth rules for markers, titles, and chapter commands remain independent
parity work.

[Reader chapter workflow](../src/app/chapter_navigation/tests.rs) · [KOReader chapter starts](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L597-L646) · [Reflow jump](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerrolling.lua#L704-L761) · [Fixed-page jump](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L1147-L1209)
