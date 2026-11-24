# Menu search

Open the upper reading toolbar and choose **Search › Menu search**. Search
for a word or phrase in a control’s visible name. Search ignores case and
matches punctuation literally. Reader remembers the previous query and
lists each result with its menu path.

Tap a result to review its path, then choose **Open**. Hold a result to go
directly to the highlighted control. Opening a result keeps the current
reading position and leaves the control ready for an explicit tap. **Back**
from results returns to the query. **Back** from the query returns to the
same toolbar. [Results](screenshots/menu-search-results.png) · [Path preview](screenshots/menu-search-preview.png) · [Highlighted setting](screenshots/menu-search-target.png)

Reader searches the current provider’s top groups, lower configuration
panels, and supported settings branches. An EPUB and a PDF expose different
controls. Searches include menu titles and settings labels. Document text,
saved marks, free-form profile contents, and specialized editor contents
remain available through their respective screens.

## KOReader comparison

The pinned KOReader **Help › Menu search** searches the live menu tree for
case-insensitive literal substrings, including applicable dynamic descendants.
The stock result opens a breadcrumb confirmation. Its Open button and a
long press each jump to the final control without executing it. The optional
stock **Walk me there** shows an animated sequence. Reader uses one direct
jump and marks the destination in its existing interface.
[Pinned entry callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/common_info_menu_table.lua#L40-L46) · [Pinned search and navigation code](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/touchmenu.lua#L976-L1340)

The native replay exercised the Help callback, dynamic items, literal
punctuation, remembered query, breadcrumb confirmation, and the Open and
hold dispatch paths. The fixture records the destination and checks that
search never invokes the target callback. The Mac Reader tests also open
and mark the real result, exercise EPUB and PDF differences, preserve the
reading location, and verify query restoration after reopening. Kindle
touch and E Ink acceptance remain open.
[Stock result and confirmation](screenshots/stock-menu-search-preview.png)

```sh
cargo test --locked --features kindle --lib menu_search
```

Set `READER_MENU_SEARCH_RESULTS_CAPTURE`, `READER_MENU_SEARCH_PREVIEW_CAPTURE`,
and `READER_MENU_SEARCH_TARGET_CAPTURE` to PNG paths to reproduce the Mac
frames. Run [the stock driver](../scripts/koreader-menu-search-check.lua)
in the pinned KOReader runtime with a scratch profile and output directory
to reproduce the stock frame.
