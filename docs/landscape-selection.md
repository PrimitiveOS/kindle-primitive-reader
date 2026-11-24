# Selection and lookup in the reading viewport

EPUB and PDF selection actions use the visible text bounds, reading height, and Done button of the active viewport. Saved highlight and note panels use the same bounds for drawing and touch. PDF note indicators are tappable where they are painted, including at the far right of the landscape page. EPUB note indicators share their margin calculation between painting and touch. The note editor and highlight-range footer use the active keyboard and bottom controls.

Dictionary and online lookup panels retain a fixed physical reading width, so rotating the viewport preserves the wrapped text and lookup position. The landscape height controls the definition area and keeps the online result pager on screen. Dictionary editors and online language editors use the shared keyboard at the active size. Results, image viewing, and text actions use the same translated bounds for drawing and touch.

## Mac simulation

Run `cargo test --locked --lib selection_viewport_`. Set `READER_SELECTION_VIEWPORT_CAPTURES` to a directory to save the 1680 × 1264 light and dark frames. The tests drive Reader’s viewport staging and commit callbacks with the same native panel scale used by the app runtime.

- Hold a real word in an authored EPUB, open an installed fixture dictionary, edit the query, then hold Done to return to the same selection. The portrait frame before and after the round trip is identical. [Selection actions](screenshots/landscape-selection-actions.png) · [Dictionary](screenshots/landscape-dictionary-dark.png)
- Hold a word in a PDF text layer, save a note using the landscape keyboard, tap the far-right note marker, and open and cancel the note editor. Closing the note restores the reading location. [Saved PDF note](screenshots/landscape-saved-note-dark.png)
- Translate an EPUB selection, change the target language with the landscape keyboard, then page through an authored provider response and return to the selection. The simulation runs offline. [Translation result](screenshots/landscape-translation-result.png)
- Open an illustrated Wikipedia fixture, tap the painted image to show its caption, hold it to open the gallery, and page through the gallery before returning to the book.

Physical device rotation needs hardware verification. Related surfaces with portrait geometry still include footnote previews, View HTML, the hyphenation editor, full-document search and its matches, and the separate dictionary and online lookup settings and history pages.
