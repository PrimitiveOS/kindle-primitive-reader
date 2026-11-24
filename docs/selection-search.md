# Find selected text in the book

Choose **Find** in the text selection popup. By default, Reader searches
forward starting on the current page. To browse every match together, open
the upper **Search** menu, then **Search settings**, and enable **Show all
results on text selection**. Both paths use a case-insensitive literal query.
Search book from a dictionary lookup still opens the results list.

Opening the list and going Back preserves the reading location. It also keeps
the width, pan, rotation and saved view of a zoomed fixed-page book. The EPUB
and PDF workflow tests select text through the reading screen and check
both search modes, cancellation, and persisted settings.

[Selection search results](screenshots/search-from-selection-results.png) · [KOReader selection dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L240-L266)
