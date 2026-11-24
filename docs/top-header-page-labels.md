# Top status header page labels

The top status header uses the EPUB’s own page labels when **Printed pages in
footer** is enabled. Selecting **Character-count pages** switches the header to
the completed character-count map. The current label follows the last boundary
visible on the page, including in a two-page view, or the preceding boundary
when there is none in view. Before the first boundary, the first label appears.

**Header current page** and **Header total pages** can be displayed separately.
With both enabled, a publisher listing can read `iii / A-19`. `A-19` is the last
label supplied by that edition. It can differ from the number of labels and
from the number of screen pages. A finished character-count map supplies its
own first-to-last numbers independently of screen layout.

Reader retains the original chapter-local screen numbers if the map is still
being built, the relevant publisher option is off, or an open-chapter boundary
is unresolved or out of order. Reader checks the source list’s spine order and
resolves boundaries in the current layout. The last published label comes from
the EPUB metadata. Displaying the header opens no additional chapters and does
no whole-book layout.

KOReader’s alternate header uses the current and last stable labels whenever
its page-map option is active. With that option off, CRE supplies its exact
whole-document screen page count. Reader's reflow renderer uses the current
chapter’s exact screen page count in that case. [KOReader alternate header](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L90-L156) · [Current and last page-map labels](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpagemap.lua#L330-L357)

Desktop workflows cover Roman and alphanumeric labels, the independent fields,
malformed boundaries, a pending and completed character map, and reopening.
Switching to publisher labels leaves the reading location and every pixel below
the top header intact, including the progress footer and Exit. Kindle
verification remains pending.

[Publisher labels in the header](screenshots/top-status-header-publisher-labels.png) · [Character-count labels in the header](screenshots/top-status-header-character-labels.png)
