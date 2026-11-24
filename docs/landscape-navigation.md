# Navigation at the reading viewport size

Reader uses the open document’s viewport for Go to page, Contents, and the in-book bookmarks and highlights lists. The right edge anchors Back and Next. The shorter landscape height determines how many rows fit above the footer. Contents search and bookmark search use the same active viewport for keyboard drawing and key hit testing. Flexible bookmark rows measure their text at the active width before paginating.

The same destination resolution and return history apply in either orientation. On the 1264 × 1680 portrait viewport, the shared Back and pager positions are identical. The host tests also compare the Contents and Go to page frames before and after a portrait–landscape–portrait round trip.

## Mac simulation

The fixture covers fourteen source-anchored EPUB passages and a fourteen-page PDF with a text layer, an embedded outline, and saved page bookmarks. Its 1680 × 1264 frames exercise the controls beyond the portrait right edge and the keyboard above the portrait footer. The tests cover the EPUB Contents search and return location, PDF page and outline navigation, dense and flexible bookmark rows, bookmark search, and physical page-button input.

Run `cargo test --locked --lib navigation_viewport` on the host. Set the corresponding variables to PNG paths to render the three captures.

- `READER_NAV_LANDSCAPE_CONTENTS` captures the EPUB outline and its pager. [Contents screenshot](screenshots/landscape-contents.png)
- `READER_NAV_LANDSCAPE_GOTO` captures the PDF destination editor and its keyboard. [Go to page screenshot](screenshots/landscape-go-to-page.png)
- `READER_NAV_LANDSCAPE_MARKS_DARK` captures flexible PDF bookmarks in dark mode. [Bookmarks screenshot](screenshots/landscape-saved-marks-dark.png)

The simulation prepares the document’s landscape viewport directly. The saved-marks measurement and rendered canvas share the active viewport dimensions and the native short-edge scale. The transition repaginates the list around the first saved item on screen. Contents custom-outline and settings screens use the same viewport. [Book Map and Page Overview](reading-orientation-contract.md#book-map-and-page-overview) · [Full-document search](landscape-document-search.md) · [Custom Contents](landscape-custom-contents.md) Physical display rotation and touch transformation are pending Kindle verification.

## Documents and document search

KOReader rebuilds its File Manager at the new screen size, preserving the folder and focused file, and updates touch zones for the new dimensions. A locked turn survives a trip to the File Manager. [Rotation policy](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L60-L66) · [Folder and focus capture](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L751-L773) · [Touch dimensions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L95-L100)

Reader’s Documents view keeps the current folder, filter, and visible item when Keep orientation carries a landscape turn out of an open book or across a restart. The landscape card fits seven full rows and the portrait card fits ten. Previous, Next, Exit and the Find keyboard use the live canvas. The eleven Mac Library tests include a staged abort and retry, folder and file rows, page-button and far-right touch navigation, lookup and clearing the filter. [Documents screenshot](screenshots/landscape-documents.png) · [Find screenshot](screenshots/landscape-document-find.png)


## Cross-book Bookmark Browser

The cross-book browser uses the viewport for search, filter cards, result rows, note previews, and descriptions. Rotating the result list keeps its first visible item in view through both turns. Changing the page establishes a new visible item to retain. A multi-page note or description retains its byte offset through rewrapping, and a rejected frame retains the previously measured pages. The simulator browsed an authored EPUB and PDF, searched by note text with the landscape keyboard, and changed orientation while a long note was open. [Saved note screenshot](screenshots/landscape-bookmark-detail.png)

## Page links, location history, and printed pages

Links on this page, Location history, and the stable page number list use the reading viewport for the rows, Back button, and pager. The printed-page list keeps its first visible label in view through a rotation and back, while the current printed-page marker updates from the newly laid-out book. Location history reuses the original history index. A link list keeps the previous first destination if it is still on the page after reflow. If that link has moved off the page, the list stays within the available links. The address and QR panels use the same viewport and stay inside the display.

Run `cargo test --locked --lib reading_lists_viewport` to verify these paths with an authored EPUB page list, EPUB reading locations, and an authored PDF with link annotations. The tests cover a rejected viewport, wide-row input, the printed-page first and last shortcuts, exact reading destinations, and the PDF link’s offline copy and QR actions. Set `READER_SELECTION_VIEWPORT_CAPTURES` to save the 1680 × 1264 simulation. [Printed pages screenshot](screenshots/landscape-publisher-pages.png)
