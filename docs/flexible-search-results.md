# Maximum lines per search result

Open the upper **Search** menu and choose **Search settings → Max lines per
result**. The setting starts disabled. The picker starts at four lines
and accepts 1–10. **Disable** restores the saved fixed **Results per page**
value. The **Words in context** choice still determines the text passed to
the result row.

Flexible rows take the height they need up to the line cap. The displayed
excerpt keeps the matching word in view when earlier context is too long.
Page buttons, swipes, taps, and holds use the same measured rows that the
list draws. New search batches can fill a partially measured last page
without moving existing result targets.

The EPUB test checks settings persistence, paging, and the original source
anchor after opening a result. The PDF test scans an authored document,
checks late delivery of additional results, and verifies the exact page
and highlighted bounds after selecting a measured row.

[EPUB results](screenshots/flexible-epub-search-results.png) · [PDF results in night mode](screenshots/flexible-pdf-search-results.png) · [Pinned KOReader option](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L155-L205) · [Reader workflow tests](../src/app/flexible_results/tests.rs)
