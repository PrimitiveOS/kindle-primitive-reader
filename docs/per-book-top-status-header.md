# Top status header per book

Swipe up from the bottom while reading a reflowable book, then open **Screen**.
Under **Top status header**, tap **On** or **Off**. The change applies to the
open book and the Screen panel stays open. A book keeps its choice when you
reopen it. In continuous scrolling, the row shows that the header appears in
page-turn mode. Returning to page turns restores the saved choice.

**Default for new books** sets the initial choice for books you open later.
Holding **On** or **Off** in the book row opens the same confirmation as tapping
those options in the default row. The default starts Off in new profiles.
Books already read on earlier Reader versions inherit the old global setting.
Typography profiles and status bar presets keep both the per-book choice and
the new-book default. The existing status bar settings still control what the
header shows across books.

## KOReader comparison

The pinned CRE `status_line` option is in the lower Settings panel. It starts
Off, stores activation in the document settings, and has a hold-to-confirm
default. Its top-menu Alt status bar section manages the shared presentation
and automatic refresh. Reader uses the same workflow in its existing **Screen**
section and also shows the default setting there so it can be reached by tap.
The control is available for reflowable books, including those opened without
an EPUB container, when Reader can display a paged layout. Fixed-format views
omit this control.
[Stock lower-panel definition](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L666-L677) · [Per-document persistence](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerconfig.lua#L183-L195) · [Hold-to-default confirmation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/configdialog.lua#L1392-L1425) · [Shared presentation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readercoptlistener.lua#L365-L397)

## Desktop verification

The interaction test opens Screen with a bottom-edge swipe, turns the header on,
and checks that it keeps the existing reading location and every pixel of the
reading footer, including Exit. The header also leaves Exit visible when the
footer is collapsed over the page. The test holds a
choice, cancels the prompt by tapping outside, then sets the default through
the visible controls. It reopens two books with different saved choices,
opens a third book with the new default, and checks that PDF offers no toggle.
Separate tests check migration from the previous global setting, header
reflow, and restoration when changing from continuous scrolling to page turns.
Oasis E Ink and hardware input verification are pending.

[Screen controls](screenshots/top-status-header-book-config.png) · [Default confirmation](screenshots/top-status-header-default-confirmation.png) · [Reading page and footer](screenshots/top-status-header-book-reading.png)
