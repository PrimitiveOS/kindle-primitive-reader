# Search result navigation

Open the options icon at the top of the full-text search result list.
Choose **Results in current chapter** to see only the section at your current
reading page. Matches that arrive later join the same section. Use **All results**
to return to the full list. Both fixed and flexible rows use the filtered order
for paging and hit selection. Previous, Next, and the fixed-page preview's
first and last shortcuts follow that order as well.

The section comes from the book's active Contents. Fixed-page outlines use the
first heading on the current page or the last heading on an earlier page.
Reflowable documents use source anchors for each match, including sections
whose boundaries cross a spine item. Repeated titles at different locations
stay distinct. Reader resolves unopened boundary chapters from the source
without repaginating the book. A document without an outline shows the full
result list. Starting a new query or opening a book clears the chapter filter.

[Chapter results options](screenshots/search-results-chapter-options.png) · [Flexible EPUB results](screenshots/search-results-chapter-epub.png) · [Fixed PDF results in dark mode](screenshots/search-results-chapter-pdf.png)

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
temporary match emphasis and preserves list pagination. Additional EPUB and
PDF tests apply the chapter filter before the scan completes and verify that
later matches use the same section. They exercise fixed and flexible rows,
long-press previews, the fitted PDF preview and return to the original view.

[Focused result](screenshots/search-results-current.png)
