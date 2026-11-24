# Search result navigation

Open the options icon at the top of the full-text search result list.
**Focus results near current page** highlights the first match visible on the
current reading page in the list. If that page has no match, Reader uses the
nearest earlier hit, or the first later one if there are no earlier matches.
The book stays at the same reading location.

**Return to original page** restores the position where the current query
began, even after opening several results. When you reopen the results, the
list continues on the page you were viewing. Starting another search saves a
new original location. Fixed documents return to the original zoom, rotation, and
pan. Returning from the results leaves the ordinary location history intact.

KOReader’s result list compares each hit’s page to the active reading page.
For a reflowable book, Reader resolves match anchors against the active
chapter’s pages and uses spine order for results in other chapters. For a fixed
document, Reader compares physical page indexes.
[KOReader result options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L790-L869) · [KOReader active page](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L1000-L1002) · [KOReader original position](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L965-L976)

Desktop tests cover EPUB and PDF result focus, sparse matches, a query from
selected PDF text, restoring the original painted viewport and reopening the
document at the restored position. Returning to the original page clears
temporary match emphasis and preserves list pagination.

The reference harness also ran the installed KOReader's dynamic result menu on
the Oasis with authored input. It confirmed that chapter filtering retained
the two matching rows out of four, All results restored all four, page focus
chose the first hit on that page or the previous hit between pages, and the
original-position action returned to the saved page. The test exercises
stock Lua callbacks without writing a book or a KOReader profile.
[Reference harness](../scripts/koreader-search-controls-check.lua)

[EPUB options](screenshots/search-results-options.png) · [Focused result](screenshots/search-results-current.png) · [PDF options in dark mode](screenshots/search-results-options-pdf.png)
