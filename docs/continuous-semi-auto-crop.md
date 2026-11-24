# Semi-auto crop while scrolling a PDF

In **View → Layout**, select continuous reading. From **Scan → Semi-auto
crop**, adjust the full-page preview of the current reading page. Reader
uses its top source page as the reading page while scrolling. **Cancel**
restores the previous scroll frame. **Apply** returns to width-fit
continuous reading, anchors the saved source position where the new crop
allows it, and saves the page and parity seed.

Each visible page uses its own crop for width-fit scaling, screen
placement, PDF links, highlights and text hit testing. The space
between pages keeps its configured height. Reopening restores the
saved top source page and its scroll offset.

The stock probe loads an authored PDF using the pinned MuPDF and KOPT
runtime. It supplies a top offset and invokes KOReader's scroll-state
and coordinate-transform routines. At a split viewport, the manual
seeds yield page scales of 2.5796 and 2.8089. Semi-auto seeds on those
pages yield scales of 5.1632 and 3.8683. The stock transforms round-trip
the sampled source points within half a PDF point. [Stock probe](../scripts/verify-stock-continuous-crop.lua) · [Scroll states](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L675-L754) · [Source transforms](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L409-L442)

Run the probe with the authored four-page PDF and display settings
from the crop comparison. [Fixture and setup](semi-auto-pdf-crop.md)

Reader's integrated test shows two differently cropped pages at once.
It checks exact Cancel restoration, Apply and reopen, a retained note
and link on the second page, a clipped link, return navigation, and
the width-fit scale through rotation and zoom. [Simulator capture](screenshots/continuous-semi-auto-crop.png)
