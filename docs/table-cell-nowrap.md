# Table cell wrapping

Reader recognizes the legacy `nowrap` attribute on data and header cells.
The attribute takes effect whenever it is present, including `nowrap="false"`.
Text inherits the cell’s setting through inline emphasis and other descendants.
Author CSS `white-space: normal` restores ordinary wrapping, and author CSS
`white-space: nowrap` survives **Settings › Style tweaks › Tables › Ignore
tables related HTML presentational hints**.
[Pinned HTML presentational rule](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/cr3gui/data/html5.css#L795-L798)

During table sizing, Reader measures the unbroken width of protected text.
A wide enough cell retains the line. Explicit breaks still start a new line.
If the text cannot fit within the reading column or an explicit Web-mode
table width, Reader prefers available breaks outside the protected text.
It then uses the normal break opportunities within that text and can split
between shaped character clusters to keep a long unbroken word readable.
Search and selection continue to use each word’s source position.
[Pinned width calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L11835-L11841) ·
[Pinned overflow fallback](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L5356-L5367)

The Mac replay runs the actual KOReader menu callback and verifies six
tables in the same chapter before and after filtering the HTML hints.
Reader checks the same line relationships, exact source anchors, word
hits, restoration of the previous geometry, and an oversized single word
in a constrained 210px table.
[Shared chapter](../tests/fixtures/table-nowrap.xhtml) ·
[Stock replay](../scripts/koreader-table-nowrap-check.lua) ·
[Reader checks](../src/parley_layout/table_nowrap_tests.rs)

| Style | Reader on Mac | KOReader on Mac |
| --- | --- | --- |
| Publisher markup | [Page](screenshots/reader-table-nowrap-publisher.png) | [Page](screenshots/stock-table-nowrap-publisher.png) |
| Ignore HTML hints | [Page](screenshots/reader-table-nowrap-without-html-hints.png) | [Page](screenshots/stock-table-nowrap-without-html-hints.png) |

With HTML hints ignored, normal wrapping and hyphenation resume. The Reader
capture hyphenates “lighthouse” across two lines and selection still returns
the complete original word. The KOReader capture moves the whole word to its
next line.

Run the stock replay from the pinned native KOReader runtime. Pass absolute
paths to the shared chapter, an empty CRE cache directory, the Noto Sans
Regular fixture font, and an output directory. Run the Reader checks with
`cargo test --locked --features kindle --lib reading_style_table_nowrap`.
Set `READER_TABLE_NOWRAP_CAPTURES` to save the Reader frames.
