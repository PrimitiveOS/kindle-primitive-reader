# Custom Contents in the reading viewport

KOReader saves custom Contents per book, uses a source pointer for reflowed text and a page number for fixed documents, and retains the custom list when switching back to the publisher’s outline. Its title editor keeps the keyboard available for creation and revision. Reader keeps the same source-based editing model and uses the active viewport for its chapter lists, editor, and removal confirmation. [KOReader storage and outline selection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhandmade.lua#L17-L105) · [KOReader chapter editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhandmade.lua#L401-L545)

In landscape, chapter and bookmark rows use the full available width, and the shorter viewport determines the page capacity. The chapter editor shares the active keyboard, and both the removal confirmation and the Contents density picker use the same bounds for drawing and touch. Chapter navigation settings and the generated outline use the same viewport-aware list controls.

## Mac simulation

Run `cargo test --locked --lib custom_contents_viewport`. Set `READER_SELECTION_VIEWPORT_CAPTURES` to a directory to render the 1680 × 1264 frames. The tests use negotiated Reader viewports and authored documents.

- Import the last of fourteen EPUB bookmarks, keep the title draft through a portrait round trip, save from the landscape keyboard, and open its exact source position. The same fixture verifies a pixel-identical portrait list, chapter-level controls, and the Contents density picker. [Bookmark picker](screenshots/landscape-custom-chapter-bookmarks.png) · [Title editor](screenshots/landscape-custom-chapter-title.png) · [Density picker](screenshots/landscape-contents-density.png)
- Create a PDF chapter from a bookmarked page. Cancel the removal by button and by tapping outside, then confirm it. The reading location, page bookmark, and source PDF survive a reopen. [Chapter removal](screenshots/landscape-custom-chapter-remove-dark.png)
- Build an EPUB outline from authored headings without a publisher outline. Use the landscape pager and open a generated entry beyond the first page, then return to the original reading location. [Generated outline](screenshots/landscape-generated-contents.png)

Physical Kindle rotation still needs device verification.
