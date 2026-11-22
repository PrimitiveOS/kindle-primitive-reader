# Library highlight exports

Open **Reading data › Export highlights**. Choose History, select individual books, or choose one or more collections. Select highlight styles, then choose KOReader JSON, plain text, Markdown or HTML. The styles remain selected when Reader reopens. The saved file appears under `reader/data/exports/library-highlights` with the chosen extension.

Each book keeps its own title, author and source file. Different editions with the same title remain separate. Exports include highlighted quotations and attached notes. Page bookmarks are omitted. Books with no matching highlights are omitted. A source that has disappeared or changed is skipped and included in the completion count.

For fixed documents, the page reference is the one-based physical source page. For reflowable documents, Reader resolves the saved text anchor with that book's reading preferences and records a chapter-local page label. A reflow label such as “Chapter 3, page 2” identifies the current layout. Exact start and end anchors also appear in the JSON `primitiveos` field. Unknown creation times remain absent. The saved creation time is retained when it exists.

Reader prepares one book or saved mark per event-loop step. Cancel, Back or sleep cancels an unfinished export. Saving uses the app kit's atomic writer. The open book, its reading position, jump history and saved annotations stay available throughout the export. A successful export replaces the earlier file in the selected format.

## KOReader source comparison

The reference is KOReader commit `9192014d8bd82a91dc1012473be0f238dedfdb54`.

| KOReader behavior | Reader behavior |
|---|---|
| Export History or selected previously opened books | History, individual books and collection unions select existing tracked documents. |
| Filter highlight drawers | Choose one or more of Lighten, Underline, Strikethrough and Invert. |
| Export highlighted text, note, page reference, chapter, creation time and drawer | Those fields are retained or resolved from the saved location. Missing dates or unresolved reflow pages are omitted. |
| A single book is a JSON object, several books use a `documents` envelope | Both shapes follow the exporter schema and can be read by Reader's KOReader JSON parser. |
| Human-readable text, Markdown and HTML | The quotation and note remain literal. HTML escapes markup and uses one document containing book sections. |

The stock Kindle exporter also merges Amazon's `My Clippings.txt`. Its color filters and optional book MD5 setting require further implementation in Reader. This increment covers Reader's own saved highlights. [KOReader export actions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/exporter.koplugin/main.lua), [annotation fields](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/exporter.koplugin/clip.lua), [JSON shapes](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/exporter.koplugin/target/json.lua)

## Verification

The functional test uses the normal Reading data controls with two same-title EPUB editions and a PDF. It checks History, collections, selected books, combined style filters, single-book and multi-book JSON, all human-readable formats, sleep cancellation, reading-order sorting, a source replaced during export, and the original reading position and saved state. The JSON is also parsed by the existing KOReader import parser.

The Mac passed 559 library tests, 14 package integration tests and four layout system tests. Thirty opt-in tests were skipped in the ordinary run. A pressed-frame check covers every actionable row in the export screens in both themes and verifies that feedback changes only its row.

An opt-in test reads the authorized original Documents corpus with an isolated Reader state. It selects actual text from two EPUBs and two PDFs and checks the exported quotation, note, date and source anchor or physical page. The originals are read in place. The test records only aggregate counts in its output.

That original-file test passed on the Mac, with all four source SHA-256 values preserved. The installed KOReader JSON exporter also serialized the authored three-book control through its actual Lua module. Its standard single-document and multi-document fields matched Reader's output. The installed module's SHA-256 matched the pinned reference. The comparison used an in-memory writer and created no Kindle files. Native testing of Reader's new export flow is pending the combined Kindle build.

![Library export options](screenshots/library-highlight-export.png)
