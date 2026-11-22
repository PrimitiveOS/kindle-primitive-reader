# Lines at the top and bottom of a page

Publishers can specify how many lines of the same paragraph should stay at either side of a page break. With `orphans: 2`, Reader moves a lone opening line to the next page. With `widows: 2`, it keeps the last two lines together. Reader applies the rules to horizontal reflow when publisher styling is on. Opening a book with the rules disabled keeps the original text and source positions.

These are consecutive pages of an authored test book. The second paragraph begins “The first ripple reaches the marker.” Reclaiming its first line for the next page leaves the preceding paragraph intact.

| Paragraph line setting | Two consecutive pages |
| --- | --- |
| One line allowed on either side | ![Last line of the left page starts the second paragraph](screenshots/paragraph-ends-before.png) |
| Two lines kept together | ![The second paragraph starts at the top of the right page](screenshots/paragraph-ends-after.png) |

## Implementation and reference

KOReader v2026.07.1 pins CREngine `b32a88ff`. Its CSS parser accepts integer counts from 1 through 9. Both values inherit and default to 1. The renderer marks the boundaries inside the opening and closing groups of lines as breaks to avoid. Reader applies the same limits to formatted paragraphs and keeps the surrounding chapter’s normal page fitting. If the requested group cannot fit on an empty page, the text can still span pages. Source byte offsets remain available for searches, selections and bookmarks. [CREngine CSS values](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L3053-L3067) · [Default line counts](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L5158-L5164) · [Protected line boundaries](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L9085-L9124)

## Checks on original publications

The Mac comparison used identical shaped text, fonts, margins and other publisher styles. It changed only the protected line counts to one as its control. Every rejoined pair remained addressable through its original source anchors. The selected pages rendered with visible ink.

| Original EPUB | Sections checked | Styled paragraphs with multiple lines | Protected breaks under one-line control | Protected breaks with publisher rules |
| --- | ---: | ---: | ---: | ---: |
| 052 | 10 | 570 | 37 | 0 |
| 061 | 7 | 139 | 9 | 0 |
| 068 | 2 | 5 | 0 | 0 |
| 036, explicit one-line setting | 2 | 0 requiring protection | 0 | 0 |

Across the four originals, the one-line control split 23 pairs inside an opening group and 24 inside a closing group. One split counted in both groups. Enabling the publisher rules rejoined all 46 distinct pairs. The 46 affected pages rendered and mapped back to their original text anchors.

Original 068 used both one-line and two-line declarations. Its sampled layout already satisfied them. Unit tests cover both paragraph ends, invalid values, cascade priority, inheritance, explicit resets, and a lines-per-page limit too small to satisfy the publisher. [Authored fixture](../tests/fixtures/paragraph-ends.xhtml) · [Layout tests](../src/publisher_breaks/line_ends_tests.rs) · [Installed KOReader comparison](../scripts/koreader-paragraph-ends.lua)

On an Oasis 3, a separately built ARM reader repeated four sections from originals 052 and 061. The native control split 16 protected line pairs. Reader rejoined all 16, matching the Mac section counts, source positions and rendered-page checks. Three authored native tests also passed.

Installed KOReader compared the same original books with publisher styles and with an explicit one-line override. The comparator applied the reading UI’s default Book-mode flags (0x03375131) before opening either book. Original 052 grew from 454 to 457 pages with publisher settings, changing 436 page starts. Original 061 grew from 457 to 463 pages with publisher settings, changing 429 page starts. Restoring the override recovered every original KOReader page start in both books. CREngine’s standalone Web mode previously changed 434 and 426 starts respectively. [KOReader reading modes](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readertypeset.lua#L378-L415) Seven passage pairs that Reader had rejoined could be located uniquely in KOReader. KOReader already kept all seven together under both settings. The two engines can wrap the same text at different positions. The original files, existing user reading records, installed Reader and Browser session retained their checksums.
