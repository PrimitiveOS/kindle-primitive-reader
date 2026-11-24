# Selection and lookup in the reading viewport

EPUB and PDF selection actions use the visible text bounds, reading height, and Done button of the active viewport. Saved highlight and note panels use the same bounds for drawing and touch. PDF note indicators are tappable where they are painted, including at the far right of the landscape page. EPUB note indicators share their margin calculation between painting and touch. The note editor and highlight-range footer use the active keyboard and bottom controls.

Dictionary and online lookup panels retain a fixed physical reading width, so rotating the viewport preserves the wrapped text and lookup position. The landscape height controls the definition area and keeps the online result pager on screen. Dictionary editors and online language editors use the shared keyboard at the active size. Results, image viewing, and text actions use the same translated bounds for drawing and touch.

## Mac simulation

Run `cargo test --locked --lib selection_viewport_`. Set `READER_SELECTION_VIEWPORT_CAPTURES` to a directory to save the 1680 × 1264 light and dark frames. The tests drive Reader’s viewport staging and commit callbacks with the same native panel scale used by the app runtime.

- Hold a real word in an authored EPUB, open an installed fixture dictionary, edit the query, then hold Done to return to the same selection. The portrait frame before and after the round trip is identical. [Selection actions](screenshots/landscape-selection-actions.png) · [Dictionary](screenshots/landscape-dictionary-dark.png)
- Hold a word in a PDF text layer, save a note using the landscape keyboard, tap the far-right note marker, and open and cancel the note editor. Closing the note restores the reading location. [Saved PDF note](screenshots/landscape-saved-note-dark.png)
- Translate an EPUB selection, change the target language with the landscape keyboard, then page through an authored provider response and return to the selection. The simulation runs offline. [Translation result](screenshots/landscape-translation-result.png)
- Open an illustrated Wikipedia fixture, tap the painted image to show its caption, hold it to open the gallery, and page through the gallery before returning to the book.

## Lookup settings and history

Dictionary sources, per-book preferences, dictionary history, online history,
and Wikipedia and translation settings use the active viewport. Landscape
shows seven full rows at the same font and row height as portrait. Closing
a lookup opened from history returns to the list window that contained it,
including when the popup was open during a rotation. Clear online history
uses a centered confirmation with matching touch targets.

Pinned KOReader opens dictionary and Wikipedia history in a KeyValuePage
sized to the current screen. Tapping a row opens the saved lookup over that
list. The preferred dictionary dialog saves the change and refreshes its
rows. Reader keeps those in-book routes and its own history and dictionary
preferences. [Dictionary history](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L261-L311) · [Wikipedia history](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerwikipedia.lua#L82-L122) · [List dimensions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/keyvaluepage.lua#L303-L319) · [Book dictionary preference](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L733-L758)

Run `cargo test --locked --lib lookup_settings_viewport` to check a local
StarDict fixture and an authored EPUB with offline lookup history. The tests
exercise full rows, page buttons, far-right touch, viewport rollback, saved
preferences, history return, and clear confirmation in both orientations.
[Online lookup history](screenshots/landscape-lookup-history.png)

Physical device rotation needs Kindle verification.


### Oasis reading-frame check

The tested ARM Reader `f3215a3` opened an authored EPUB in Landscape Left.
A timed touch contact selected **lantern** and opened the local dictionary
above the word. A one-entry StarDict fixture installed in the scratch
profile returned its authored definition. The Highlight toggle was
reversed before Done. Done returned to the same selection and its
action grid, including Dictionary and Translate. Cancel then restored
the original page-2 framebuffer byte for byte.

This check used the Oasis touch evdev node. It establishes the live
selection and popup behavior for this EPUB, while the mechanical gesture,
cross-page PDF selection and direct Kindle KOReader comparison remain open.

[Dictionary definition on Oasis](screenshots/oasis-landscape-dictionary.png) ·
[Selection actions on Oasis](screenshots/oasis-landscape-selection-actions.png)
