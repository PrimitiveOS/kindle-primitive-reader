# Fit the page during fixed-layout search

While reading a searchable fixed document, open **Search → Search settings**
and turn on **Fit fixed pages during search**. It starts off. When enabled,
**Find next** or **Find previous** temporarily shows the full page and marks
the matches. The current rotation stays in use.

**Close search** restores the zoom, pan and scroll mode. If you reached
another page, it stays open in the restored view. **Return to location**
recovers the original page and viewport. Reader keeps the pre-search view
in saved reading state while the preview is open. Going back, leaving the
preview, or reaching the search boundary also restores the previous mode.

The setting applies to directional search. The separate list of all search
results keeps its existing navigation. Workflow tests exercise rotated,
zoomed and continuous PDFs, sample the actual painted matches, and check
cancellation, saved position and return history.

The installed KOReader's LuaJIT ran its actual settings and directional search
callbacks on the Oasis with in-memory views and settings. The comparison
confirmed default-off fit, persistent fit across stepping, and fit restoration
on dismissal and exhaustion. It also checked selection Find dispatch and
the stock context, fixed-row and flexible-row ranges. This reference harness
uses authored inputs and exercises callbacks without taking over the display.
[Stock callback check](../scripts/koreader-search-controls-check.lua)

[Upright PDF](screenshots/fixed-search-fit.png) · [Rotated PDF](screenshots/fixed-search-fit-rotated.png) · [KOReader setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L215-L222) · [Fit and restore](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L304-L334)
