# Remaining pages and time in the selected chapter

Under **Navigation → Chapter navigation**, exclude one or more Contents
levels and turn on **Use levels for chapter jumps**. For fixed-page documents
and single-section reflow books, the same selected headings set the boundary
for pages and time remaining in the chapter. Reader uses the next included
heading or the end of the book. Hidden sections count within their own flow.

This applies to the corresponding reading footer items, the custom footer's
`%l` and `%h` fields, and reading-alarm message fields. The saved include-current
option still controls the page count. When the user selects edition-based page
numbers, the displayed page count keeps its edition convention and the time
estimate uses rendered pages. Multi-section reflow books keep their existing
section estimates. The track geometry, Exit and chapter/page caption stay in
place.

The simulator checks selected and excluded duplicate headings, chapter
boundaries, hidden flows, original caption geometry, and the pages and time
shown by the footer and alarm message.

## Chapter title levels

**Chapter titles from ticks only** independently uses the included levels for the
Chapter title footer item, the chapter title shown by the Remaining display,
and the custom footer's `%C` field. Reader chooses the first included heading
on the current page, or the last included heading on an earlier page.
Unresolved headings are skipped. The setting is available for fixed-page,
single-section reflow and alternative Contents books. It starts off and
also applies when Reader records a [new bookmark or highlight](bookmark-browser.md#chapter-titles-on-saved-marks).

[Selected heading in the footer](screenshots/footer-title-levels.png) · [Stock heading order and title preference](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L382-L451)

[Pages remaining](screenshots/chapter-remainder-pages.png) · [Time remaining](screenshots/chapter-remainder-time.png) · [KOReader remaining pages](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L688-L739) · [Footer calculation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L294-L335)
