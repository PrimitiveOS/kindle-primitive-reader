# Link taps

Open **Settings → Link taps** while reading a book. **Tap to follow links**
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

[Simulator capture](screenshots/link-tap-settings.png) · [KOReader tap settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L515-L603) · [KOReader tap dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L703-L745)
