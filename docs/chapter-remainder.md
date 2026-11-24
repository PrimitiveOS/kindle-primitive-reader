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

[Pages remaining](screenshots/chapter-remainder-pages.png) · [Time remaining](screenshots/chapter-remainder-time.png) · [KOReader remaining pages](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L688-L739) · [Footer calculation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L294-L335)
