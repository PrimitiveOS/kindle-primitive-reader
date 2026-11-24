# Preview fixed-layout search results

Tap a result in **Search results** to see the whole matched page, with that
result's exact text highlighted. The arrows step to nearby results. Hold an
arrow to reach the first or last retained result. Tap **Results** to open
the list at the highlighted result. Tap **Close search** or the page to
continue reading in the previous zoom, pan, rotation, and scroll mode.

Reader keeps the pre-preview viewport in saved reading state while the
preview is open. After closing, reading continues on the reached page in the
restored mode. Location history keeps the starting location for the preview
session. The directional-search fit setting governs its own workflow.

The simulator exercises repeated matches on the same PDF page, stepping
between pages, returning to results and reading, dismissing the preview,
saving while it is open, and restoring a rotated and panned view.

[Simulator capture](screenshots/fixed-search-result-preview.png) · [KOReader result preview and view restoration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L873-L982)
