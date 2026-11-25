# Links

Open **Settings → Links** while reading a book. **Tap to follow links**
starts on. Turning it off lets page-turn taps and other reading gestures
take over. **Ignore external links on tap** starts off and leaves local
document links available. Both choices apply to EPUB and PDF.

In reflowable documents, **Allow larger tap area around links** starts off.
When enabled, a tap just outside a visible link can follow its text or image
region. Reader looks for the closest eligible region within 57 logical
pixels on the 300 DPI Oasis 3 canvas. This is the result of KOReader's
actual DPI scaler. A direct link takes priority when web links are allowed.
With **Ignore external links on tap** on, only internal links are eligible.
If enlargement is off too, the touch must fall on an internal link region.
Fixed-page documents use their native annotation bounds.
[Pinned tap policy and radius](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L706-L745) ·
[Stock geometry replay](../scripts/koreader-larger-link-tap-check.lua) ·
[Reader screenshot](screenshots/larger-link-tap-web-address.png)

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

## Swipe to the latest saved bookmark

Enable **Settings › Links › Swipe to jump to latest bookmark**. Swipe left
on a reading page to open the most recently dated bookmark, highlight or
note in that book. EPUB uses the saved passage anchor. Fixed pages open the
saved source page. An eligible link takes precedence when **Swipe to follow
nearest link** is enabled. **Swipe to go back** returns to the passage you
left. The new option starts off.

The pinned stock callbacks confirm the order of the link and mark actions,
the newest annotation selection and the EPUB and PDF destination types.
Reader's interaction checks cover the two document paths, touch lock, saved
preferences and return history. [Settings capture](screenshots/latest-bookmark-swipe-settings.png) · [Pinned swipe dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L1040-L1073) · [Pinned annotation selection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L618-L629) · [Executable comparison](../scripts/koreader-latest-bookmark-swipe-check.lua)
