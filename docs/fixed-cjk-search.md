# Fixed-page CJK phrase search

Reader can match a multi-character Chinese, Japanese or Korean phrase even
when the PDF, DjVu or OCR provider groups several characters in one word box.
The match keeps the original UTF-8 text range. Highlighting retains the
provider's whole word box, so a grouped box can cover more characters than the
phrase. Multiple matches inside the same grouped box can share that geometry.
A single-character query continues to count each provider word box once.

Existing visual column boundaries remain in effect. KOReader's pinned fixed-page
matcher splits CJK query text before comparing it to its word boxes; the two
providers may expose different box segmentation. This candidate has focused
source tests for grouped and mixed-script text and for column boundaries, but
has not yet been run against KOReader, in the simulator or on Kindle. [Pinned
KOReader matcher](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/koptinterface.lua#L1503-L1565) · [Reader matcher and tests](../src/search/fixed_pattern.rs)
