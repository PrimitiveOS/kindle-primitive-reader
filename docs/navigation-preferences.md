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

Choose **Top**, **Center** or **Bottom**. Center is the default. The top
clears the system swipe edge. The bottom leaves the reading footer and
Exit visible. The current and opening locations have distinct marks on
the track. Page buttons, swipes, **-10**, **+10** and dragging the track
preview a location. **Cancel** or tapping outside restores the opening
position. **Read here** accepts the preview and stores one return location.

The chapter icons follow the selected Contents levels. Hold them to jump
to the first or last page. The saved-mark icons visit bookmarks, highlights
and notes in reading order. Hold a saved-mark arrow to jump to the first or
last mark in its direction. **Save page** toggles a page bookmark. Hold it
to open your saved marks. Tap **Go to** to accept the preview and open
the existing page and percentage editor. Hold **Go to**, or tap **Origin**,
to return within the preview. The editor retains its format-specific
input rules. Stock's whole-document absolute numbering, signed movement
and hidden-flow syntax are tracked separately.
[Stock chapter and mark shortcuts](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/skimtowidget.lua#L148-L283) · [Stock saved-mark order](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerbookmark.lua#L512-L595)

The executable stock comparison exercises the opening-page setting and the
callback after the restored location is ready. It instantiates the pinned
Skim layout for each setting and verifies the full and compact row
arrangements, ten-page callbacks, chapter and mark taps and holds, and both numeric-control gestures. Reader tests select each setting,
exercise the ten-page steps at ordinary and clamped positions, drag its drawn track, return
to the preview origin, cancel and reopen the saved preference. Additional
tests exercise exact EPUB resume and the restored fixed-page location.
[Executable stock comparison](../scripts/koreader-navigation-settings-check.lua)

[Settings](screenshots/navigation-preferences.png) · [Top](screenshots/skim-position-top.png) · [Center](screenshots/skim-position-center.png) · [Bottom](screenshots/skim-position-bottom.png)

The Mac suite exercises chapter destinations, document boundaries, a PDF note and page bookmark, pressed feedback in both
themes, and restoration of the original location. The pressed check verifies
that only the touched button changes.
[Skim at Top](screenshots/skim-shortcuts-top.png) ·
[Skim at Center](screenshots/skim-shortcuts-center.png) ·
[Skim at Bottom](screenshots/skim-shortcuts-bottom.png)
