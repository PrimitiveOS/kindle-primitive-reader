# Crop while scrolling a PDF

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

From **Scan → Crop**, adjust the page bounds and tap **Apply** to use
the selected rectangle. Reader checks each page for its own rectangle,
the last one set on the same odd or even page group, then the last one
set on a preceding page. When there is no saved rectangle, Reader
estimates the used area from the page raster. **Auto crop** measures each
full page independently and uses the saved rectangle when the detected
area is too small. **Reset** clears the active crop and keeps saved
rectangles available in the editor. Both modes work in continuous
width-fit reading. Other layouts keep their existing crop controls.

The stock probe loads an authored PDF using the pinned MuPDF and KOPT
runtime. It supplies a top offset and invokes KOReader's scroll-state
and coordinate-transform routines in both directions. With normal
display orientation and fixed-page wrapping and straightening disabled,
the manual seeds yield page scales of 2.5796 and 2.8089. Auto yields
3.1241 and 2.6311. Semi-auto yields 5.1632 and 3.8683. The stock
transforms round-trip the sampled source points within half a PDF point.
Reader matches the native bounds on the authored test pages within
0.002 PDF point. The fallback without a saved rectangle estimates
visible ink. KOReader measures drawn object extents, which can differ on
light or sparse pages. [Stock probe](../scripts/verify-stock-continuous-crop.lua) · [Scroll states](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L675-L754) · [Source transforms](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L409-L442)

Run the probe with the authored four-page PDF and display settings
from the crop comparison. [Fixture and setup](semi-auto-pdf-crop.md)

Reader's integrated test shows two differently cropped pages at once.
It checks exact Cancel restoration, Apply and reopen, a retained note
and link on the second page, a clipped link, return navigation, and
the width-fit scale through rotation and zoom. [Simulator capture](screenshots/continuous-semi-auto-crop.png)

The manual and automatic workflow checks both switching directions,
an independently scaled page seam, uncropped link recovery in Auto,
reopen, original note and link coordinates, and retained manual
settings through rotation and zoom. [Manual crop capture](screenshots/continuous-manual-pdf-crop.png)


## Detected crop margins

With Semi-auto crop applied, or Auto crop in continuous width-fit, open
the bottom **Scan** controls and choose a margin from 0.05, 0.10, 0.25,
0.40, 0.55, 0.70, 0.85 and 1.00 inches. **Custom** accepts 0.00 through
1.50 in 0.01-inch increments. Reader saves the value for the current PDF.
The default 0.10-inch margin reserves 30 pixels across both sides on the
300-dpi Oasis display, when both sides have room for it.

Auto and Semi-auto include that margin in the detected source rectangle
before the page fits to the screen. Manual crop uses the exact saved box;
no-crop mode uses the full page. KOReader's pinned implementation and the
stock comparison harness leave those two bounds unchanged when this value
changes. In continuous reading, every eligible strip gets its own effective
margin and width-fit scale. Changing the value anchors the current source
location as far as the new crop allows. Hit testing and saved annotations use
the original source coordinates.

The pinned executable checks both authored pages at each preset,
both endpoints and a repeat of the default. KOReader leaves manual
and full-page fit unchanged. The authored Auto and Semi-auto samples
agree with Reader within 0.002 PDF point. Auto stops at the last native
sample on the right and bottom. Oversized Semi-auto boxes fit the full
page. When a valid Semi-auto fit still extends beyond one edge, Reader
clips the box to the physical page. KOReader can include the outside
area in that fit. [Stock comparison](../scripts/verify-stock-continuous-crop.lua) ·
[Scan margin controls](screenshots/detected-crop-margin.png) ·
[Split-page capture](screenshots/detected-crop-margin-strips.png)

## Landscape crop preview

Reader opens the crop preview in the accepted reading viewport. The page
and its handles use the PDF source transform. The toolbar and its touch
targets follow the visible bottom edge. Cancel restores the original view
and source position. Apply stores the modified native crop seed and keeps
the PDF’s content rotation. A live draft keeps its existing screen size
until you apply or cancel it. KOReader also uses the active screen’s
dimensions for the crop widget and restores the original view dimensions
when the widget closes. [Stock crop geometry and exit](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercropping.lua#L55-L143)
