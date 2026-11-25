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

With stable labels disabled or unavailable, Reader shows whole-book screen
numbers once it has measured the supported layout. While that measurement is
pending, or for a layout without a whole-book count, it uses the current
section. Reader resolves publisher boundaries in source order in the current
layout and reads the last published label from EPUB metadata. Painting the
header uses available measurements without opening other sections.

In a section with hidden flows, the regular page count uses `x // y` and a
supplemental flow uses `[x / y]flow`. A supplemental progress percentage appears
in brackets. Publisher and character-count labels take precedence for the
page fields. The Rust renderer counts the active section’s flows. KOReader’s
CRE tracks whole-document flows. Facing-page and vertical layouts can still
use a local screen count in Reader.
[KOReader alternate header](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L90-L156)
· [Current and last page-map labels](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpagemap.lua#L330-L357)

Desktop workflows cover Roman and alphanumeric labels, the independent fields,
malformed boundaries, a pending and completed character map, and reopening.
Switching to publisher labels leaves the reading location and every pixel below
the top header intact, including the progress footer and Exit. Kindle
verification remains pending.

[Publisher labels in the header](screenshots/top-status-header-publisher-labels.png) · [Character-count labels in the header](screenshots/top-status-header-character-labels.png)
