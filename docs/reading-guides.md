# Reading guides

Open **Appearance › Reading guides** while reading. Turn the two guide
lines on or off and set their thickness, distance from the edges, intensity
and inward movement interval. Use an interval of zero to keep the guides
still. The About screen explains what the lines do. Existing profiles start
with guides off.

The default uses two-pixel lines, a margin of 10% and a shade of 179 from
the 8-bit grayscale range. The guides start 5% below the top of the screen
and cover 90% of its height. They respect the Reader header, bars and
footer. The right line includes the left line's width, matching the stock
horizontal group. On page updates, the guides move inward by three
percentage points once the configured interval has been reached. Further
movement stops when the margin reaches or crosses 37% of the screen width.
[Pinned KOReader overlay and settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/perceptionexpander.koplugin/main.lua#L36-L217) · [Pinned group geometry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/horizontalgroup.lua#L13-L61) · [Pinned grayscale conversion](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/blitbuffer.lua#L2497-L2504)

Reader validates thickness from 1 to 20 pixels, margins from 1.0% to 45.0%,
intensity from 1 to 10 and intervals up to 9,999 pages. Preferences are
saved for the Reader profile. The current movement counter and additional
margin stay in memory during the session. Scroll and scan reflow can show
the guide lines. Committed page turns from the regular paged document view
advance them. Landscape and physical E Ink refresh still require Kindle
acceptance.

The stock comparison executes the pinned controls and real widget paint
calls against a recording framebuffer. It verifies the default and custom
line rectangles, interval, cap, Apply, Cancel, enable, About and settings
flush. Reader tests the native geometry, fixed and reflowed documents, saved
settings, footer clearance, exact book position and unchanged source bytes.
[Executable comparison](../scripts/koreader-reading-guides-check.lua) · [Settings](screenshots/reading-guides-settings.png) · [EPUB](screenshots/reading-guides-page.png) · [PDF text reflow](screenshots/reading-guides-pdf-reflow.png)
