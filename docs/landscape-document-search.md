# Full-document search in the reading viewport

KOReader’s pinned search interface keeps its keyboard, modifiers and directional controls available together. Its result list retains the first visible match when closed, shows page and contents context on hold, and opens the exact source match on tap. The result options can focus the current page or return to the search origin. Reader keeps these actions and applies the active viewport to each surface. [KOReader search entry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L278-L380) · [Result list, hold and navigation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readersearch.lua#L713-L841)

The landscape Find screen keeps the modifiers and directional controls above the shared keyboard. Paste shares the text field’s row when the clipboard contains text. Compact Results fit the available height and flexible Results measure excerpts at the active width and physical font size. The saved result window uses the first visible result when reopened after a rotation. Paging, the held context, the result options and the in-book search footer use the same geometry for rendering and touch.

## Mac simulation

Run `cargo test --locked --lib document_search_viewport_`. Set `READER_SELECTION_VIEWPORT_CAPTURES` to a directory to render the 1680 × 1264 captures. The tests use negotiated canvases and authored documents through Reader’s normal search and navigation paths.

- Search a long EPUB with the shared keyboard and paste, page through dense results, hold an excerpt for its page and contents context, open its exact source range and return to the search origin. Close the list, rotate, then reopen the page that contains the previous first result. [Find](screenshots/landscape-find-document.png) · [EPUB results](screenshots/landscape-document-matches.png) · [Held result](screenshots/landscape-held-result-context.png)
- Search a PDF harbour register with its native text layer, page through flexible results, hold an excerpt, and open the exact result in the temporary fitted view. Return through the footer and verify the selected result remains on the visible list page. [Flexible PDF results](screenshots/landscape-flexible-pdf-matches.png)

The separate search settings and history screens still use portrait geometry. A turn while the results list is already open requires the viewport transaction to preserve that first-visible result during presentation. Physical device rotation remains a device verification step.

The first visible result stays on screen through a rotation and back, including when the new viewport places it on the first page. An explicit page turn establishes a new result to retain. The runtime stages the new results cache with the resized document, retains the accepted list on abort, and commits the two together after display confirmation.
