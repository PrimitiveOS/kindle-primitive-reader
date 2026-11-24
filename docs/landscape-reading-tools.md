# Footnote preview and View HTML in landscape

Both reading tools use the accepted Reader viewport. Opening either tool
from a landscape EPUB keeps the selected turn. Closing returns to the
same book position. View HTML also restores the original text selection.

## Pinned KOReader comparison

The pinned KOReader v2026.07.1 runtime ran at 1680 × 1264 and 300 DPI
with the authored Night Landing EPUB. A tap at the rendered [2] link
opened the long footnote over the reading page. The widget reported a
421-pixel content limit, and its framed panel began at y=839. The source
XPointer stayed identical through opening and closing. This matches the
stock lower-third limit and its separate follow action. [Footnote widget](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/footnotewidget.lua#L141) · [Link controller](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L1540)

The stock highlight controller selected `logbook` through native word
geometry. View HTML opened a code viewer and closing it preserved both
the selection and XPointer. Stock also offers linked CSS and several
CREngine debug views. Reader provides its existing bounded publisher
source view for eligible EPUB selections. [Stock View HTML](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/viewhtml.lua#L36)

## Reader behavior

- The footnote preview reflows into a lower-third panel, covers the reading
  footer, and keeps the note's source page and selection through rotation.
  Use Close, a tap outside the panel, or the back page key on the first
  note page to return to the book. Open in book follows the source link.
- View HTML wraps code to the available width and places the pager and
  Back button within the accepted viewport. A source byte position
  keeps the same passage visible after rotation. Page keys and vertical
  swipes navigate the inspector without changing the book page.
- A staged rotation prepares the resized surfaces independently. Aborting
  keeps the live book, popup and source location. Committing adopts the
  candidate after the viewport handshake.

The host tests exercise Reader's `App` stage, draw, abort and commit
hooks with 1680 × 1264 canvases in both directions. They cover a
multi-page footnote, a multi-page markup selection, the exact reading
anchor, landscape navigation and portrait behavior. The captures use
authored fixtures. Physical Kindle refresh and controls require device
validation. [Footnote](screenshots/landscape-footnote.png) ·
[View HTML](screenshots/landscape-view-html.png) ·
[View HTML in dark mode](screenshots/landscape-view-html-dark.png)
