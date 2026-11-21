# Finding text from a reading position

In a reflowable book, open **Find in document**, enter a phrase and tap **Find next** or **Find previous**. The first search includes the visible page or spread. After a match opens, **Previous** and **Next** move from that occurrence, including other occurrences on the same page. A search stops at the start or end of the book. Return to the previous location to resume the original passage. **Find all** on the keyboard retains the existing list of up to 100 results in document order.

| Search controls | Matched passage |
|---|---|
| ![The search field, match case and ignore accents options, and directional controls](screenshots/nearby-find.png) | ![The matched text highlighted on the original reading page](screenshots/nearby-match.png) |

KOReader v2026.07.1's search input offers All, Backward and Forward. Its rolling-document panel also exposes a whole-book start/end jump and checks an optional regular expression before searching. Reader now offers directional literal search, case sensitivity and optional accent folding. Whole-book start/end shortcuts, regular expressions and KOReader's page-level grouping remain outside this implementation. [KOReader search input](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L249-L333) · [Directional origin and continuation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L648-L679)

A single worker reads chapters in the requested direction and stops at the nearest match. It loads one spine item at a time and releases Kindle storage before parsing, matching or waiting for input. Replacing or cancelling a request discards its result. Case folding, optional removal of accents and invisible soft hyphens map back to the source's UTF-8 boundaries. Tests cover overlapping strings, multi-character Unicode folds, a decomposed accent and a soft hyphen inside a selected word.

## Original EPUB comparison

The Mac comparison used a position midway through the occurrences in each chapter. The verifier used the active layout engine to find literal source offsets independently, then compared the preceding and following hits with both anchors, the text selected by those anchors and the quoted word in each result. The first 100 whole-book results omitted the chosen position in all seven checks. [Source and geometry verifier](../examples/verify_nearby_search.rs)

| Anonymous original | Target spine item | Target-chapter matches | Last spine item in the 100-result list |
|---|---:|---:|---:|
| 035 | 12 | 320 | 5 |
| 042 | 14 | 491 | 7 |
| 046 | 16 | 867 | 7 |
| 052 | 10 | 590 | 6 |
| 033, English query | 8 | 39 | 8 |
| 033, Vietnamese query | 8 | 382 | 5 |
| 061 | 28 | 112 | 8 |

All chapter and page indices in this document are one-based. The screenshots use an original project fixture. [Harbour log](../tests/fixtures/nearby-search.xhtml)

## Oasis verification

The Oasis 3 ran three native automated tests on authored documents. The tests checked Unicode offsets, chapter boundaries, cancellation, the Find controls, stepping in both directions and returning to the original reading position. Two additional offscreen comparisons used the original files from Documents. The English case in original 035 had 320 occurrences in chapter 12. The Vietnamese case in original 033 had 382 in chapter 8. For each book, both directions returned the adjacent occurrence with the expected start and end offsets and the complete selected word. The existing 100-result lists ended at chapter 5 in both cases.

The verification used isolated storage. SHA-256 checks confirmed both books, the installed Reader binary, the library, the reading statistics and the Browser session were preserved. The Documents inventory matched before and after, and the temporary device files were removed. The normal foreground was available throughout.

The Mac simulator also opened the existing PDF search fixture at 300% zoom. Four words across two lines stayed highlighted in both paged and continuous views, and Back restored the source page. The wrapped-passage capture matched the earlier version byte for byte. A two-result PDF displayed the shared Previous control beside zoom and Exit. [Wrapped passage](screenshots/pdf-wrapped-search.png) · [Two-result search](screenshots/pdf-search-target.png)

## Typographic punctuation in ordinary search

A straight keyboard apostrophe finds the published curly apostrophe. A typed hyphen also finds a typographic dash or minus sign. One space can match the author's nonbreaking, thin or repeated spaces. These equivalences apply to **Find all**, **Find previous** and **Find next** with either case setting. The search result and selected passage keep the original spelling and exact UTF-8 positions. **Regular expression** retains its ECMAScript character semantics. PDF search and saved-quote matching retain their provider-specific behavior.

KOReader's default rolling search sets the space, apostrophe and hyphen-folding flags. Its punctuation table identifies seven apostrophe forms and the typographic spaces. For dashes it uses the Unicode Dash Punctuation category and the minus sign. [Default reflow search flags](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L32-L50) · [CREngine character equivalences](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L12782-L12920)

On the Mac, 16 comparisons across nine original EPUBs included five apostrophe passages, four dash passages and seven passages with nonbreaking spaces. With exact keyboard punctuation the target was missing in all 16 controls. Ordinary search found the original source range in the results list and from both directions in every case. In one book the list contained both the normally spaced spelling and the publisher-spaced spelling. The verifier selected each reference range in the layout and compared its UTF-8 text with the source. An authored UI test entered a straight apostrophe and normal space, moved forward and backward across two differently spaced passages and compared both endpoints with the all-results list. [Original-book verifier](../examples/verify_typographic_search.rs)
