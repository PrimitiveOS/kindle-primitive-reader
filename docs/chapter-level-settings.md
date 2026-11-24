# Contents levels and chapter markers

While reading, open **Navigation → Chapter navigation**. Each depth in the
active Contents shows how many headings it contains. All levels start on.
Switch off a level to leave it out of progress markers where page positions
are available. Turn on **Use levels for chapter jumps** if Previous and Next
chapter should use the same selection. The choices belong to the current book
and persist when you reopen it. Contents itself still shows all headings.

Reader selects levels before combining headings at the same page. Turning off
a parent level does not remove a child at another selected level. A custom
outline has one level and uses the same book preferences.

Fixed-page documents and single-section reflow books use the filtered page
markers in the reading footer and Skim. For a multi-section reflow book,
filtering applies to the existing reading-header chapter markers in the current
section. The whole-book footer continues using source section boundaries so
opening a settings panel never needs to lay out the rest of the book. Chapter
title and time-to-end rules have their own stock settings.

The desktop workflow checks normal and filtered chapter jumps, reopening the
book, backup round trips, custom outlines, and same-page headings. The footer
regression compares the original and filtered frame and verifies that the
caption, Exit, and track geometry remain identical while selected ticks change.

[Chapter settings](screenshots/chapter-levels.png) · [Selected PDF markers](screenshots/progress-markers-filtered.png) · [Selected markers in Skim](screenshots/progress-markers-skim.png)

[KOReader depth and jump rules](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L548-L619) · [Stock settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L1257-L1325) · [Reader jump tests](../src/app/chapter_navigation/tests.rs) · [Reader marker tests](../src/app/progress_markers/tests.rs)
