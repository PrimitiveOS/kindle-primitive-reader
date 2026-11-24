# Full-text search result presentation

Open the top **Search** controls and choose **Search settings**.
**Words in context** retains up to five words on each side of a match by default.
Choose from 1 to 50. Reader uses Unicode word boundaries and keeps long snippets
within its existing memory budget. The result row shows the portion that fits
at the selected density. Changing the word count refreshes snippets on the next
search. The source passage and its match bounds stay the same.

**Results per page** defaults to 10 and accepts 2–24. The result list uses the
selected row count for touch, hardware page buttons, and first/last controls.
The chapter or page label sits beside the snippet in dense rows. Holding a row
shows context for that exact result. Tapping the row opens the matching passage.

The values and bounds follow the pinned KOReader settings. This change covers
the fixed rows-per-page option. KOReader’s separate flexible maximum-lines
mode is a separate control. [KOReader settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L108-L213)

The desktop workflow searches an EPUB with hundreds of matches, compares source
anchors at two context settings, reopens the persisted values, and opens the
last row on a 24-result page. Fixed-layout search uses the same settings and
keeps its match rectangles. Kindle verification is pending.

[Default result list](screenshots/search-results-default-density.png) · [24-result list](screenshots/search-results-dense.png)
