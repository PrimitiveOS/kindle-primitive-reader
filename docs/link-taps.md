# Links

Open **Settings → Links** while reading a book. **Tap to follow links**
starts on. Turning it off lets page-turn taps and other reading gestures
take over. **Ignore external links on tap** starts off and leaves local
document links available. Both choices apply to EPUB and PDF.

In reflowable documents, **Allow larger tap area around links** starts off.
When enabled, a tap just outside a local link can follow its text or image
region. A direct link takes priority. Reader compares distance to visible
regions using the 54-pixel Oasis 3 reference radius. The enlarged target does
not apply to fixed-page documents.

To access a link explicitly, open **Navigation → Links on this page** or
**Pages → Links**. The list includes visible local destinations and web
addresses. EPUB web addresses support HTTP and HTTPS. Reader opens the link
address panel with Copy and QR controls, keeping the reading location in
place. The link tap settings apply to ordinary taps on the reading page.

**Swipe to go back** starts off. Turn it on to swipe right to your previous
location. Reader restores the saved EPUB passage or PDF viewport. After
the final return, the next right swipe displays “Location history is
empty.” Further swipes use the normal page turn or pan. On a newly opened
book with empty history, swipes use the normal reading action immediately.
Assigned edge gestures, selection and open panels keep their existing
controls. Page buttons still turn pages.

**Swipe to follow nearest link** starts off. When enabled, swipe left
to follow the eligible link closest to where your finger starts.
Reflowable books use the visible text and linked illustrations. PDFs
use visible annotations on the page under the starting touch, including
when zoomed. A page with no eligible link keeps its normal swipe behavior.

**Ignore external links on swipe** starts on. Turn it off to include web
addresses. Reader opens its address panel so you can inspect the link,
copy it or view a QR code. Tap controls and swipe controls are independent.

[Swipe-back simulator capture](screenshots/link-swipe-back.png) · [Nearest-link simulator capture](screenshots/link-swipe-follow.png) · [KOReader link settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L515-L603) · [KOReader swipe dispatch and distance](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L1040-L1272)
