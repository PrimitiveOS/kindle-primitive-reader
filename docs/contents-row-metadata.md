# Contents page labels and chapter lengths

Contents rows show publisher page labels or fixed document page numbers at the
right edge. In reflow documents without an enabled stable page map, Reader uses
`chapter:page` so the label follows the same notation as **Go to**. A destination
inside an unopened reflow section needs its layout before Reader can give it a
screen page. A publisher label with the exact destination can still appear.

**Navigation → Contents settings** offers **Show chapter length** (off by
default) and **Dot leaders** (on by default). Lengths use the complete outline,
including folded sections. Each section ends at the next entry of equal or
shallower depth, or at the end of the book. Screen page lengths appear only
when Reader already knows the layout needed to calculate them. Opening
Contents never paginates the rest of the book. Stable page maps calculate
lengths from their reference labels, using source anchors for mapped endpoints.
Reader leaves unavailable labels and lengths blank.

KOReader appends the length beside the title, aligns the page label to the right,
and only draws leaders if at least three dots fit. Its reflow lengths can use
the full rendered document, including the rendered screen page at the top of a
reference range. Reader keeps its chapter layout and uses the source page
reference where available. [KOReader chapter length rules](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L317-L377) · [KOReader row construction](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L809-L878)

The Reader workflow tests open a PDF with nested outline entries and Roman and
decimal labels and an EPUB with publisher labels and unopened targets. They
check the displayed values, saved settings and unchanged jump destinations.

[PDF Contents](screenshots/contents-numbered-pdf.png) · [EPUB Contents](screenshots/contents-numbered-epub.png) · [Dark mode](screenshots/contents-numbered-dark.png)
