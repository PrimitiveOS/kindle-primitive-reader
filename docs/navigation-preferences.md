# Navigation preferences

Open **Settings › Navigation settings** from the reading controls. The
settings apply to EPUB, PDF and other documents opened in Reader.

## Opening location

Turn on **Add opening page to location history** to record the first position
where each book opens. For a previously read file, this is the restored
position. The record uses the same source anchor or fixed-page viewport as
the rest of Location history. It becomes available after opening a book,
stays within that reading session, and clears when Reader returns to the
library or opens another file. Existing profiles keep this option off.
[Pinned KOReader option and ready callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L111-L118)

## Skim position

Choose **Top**, **Center** or **Bottom**. Center is the default and shows
the full preview panel. Top and Bottom use a compact panel. The top stays
clear of the system swipe edge, and the bottom leaves the reading footer
and Exit visible. Page buttons, the **-10** and **+10** shortcuts, and
dragging the progress track preview a location. **Origin** returns within
the preview, **Cancel**
restores the starting position, and **Read here** accepts the preview and
records a single return location. The remaining stock shortcuts for
chapter and bookmark browsing, numeric jump, and held first or last still
need comparison and implementation in Skim.
[Pinned KOReader placement](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/skimtowidget.lua#L335-L373)

The executable stock comparison exercises the opening-page setting and the
callback after the restored location is ready. It instantiates the pinned
Skim layout for each setting and verifies the full and compact row
arrangements and both ten-page callbacks. Reader tests select each setting,
exercise the ten-page steps at ordinary and clamped positions, drag its drawn track, return
to the preview origin, cancel and reopen the saved preference. Additional
tests exercise exact EPUB resume and the restored fixed-page location.
[Executable stock comparison](../scripts/koreader-navigation-settings-check.lua)

[Settings](screenshots/navigation-preferences.png) · [Top](screenshots/skim-position-top.png) · [Center](screenshots/skim-position-center.png) · [Bottom](screenshots/skim-position-bottom.png)
