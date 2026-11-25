# Positioning table captions

Reader places captions above or below their native horizontal tables. A
caption displayed above the grid can appear after the rows in the book's
source. Search, selection, links and saved positions still use that source
order.

**Ignore HTML table presentation** removes the legacy `align="bottom"`
hint from a caption. A publisher's CSS `caption-side: bottom` remains
effective, including when the caption inherits it from the table. The
default is `top`. CSS `initial` restores it, and `inherit` and `unset`
use the parent's value. [Pinned CSS values](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L3148-L3155) · [Pinned HTML caption hint](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/html5.css#L764-L767)

A caption uses the table width inside its outer borders. The table padding
separates it from the cells. For collapsed borders, the grid carries the
collapsed strokes and the caption retains its own box. [Pinned caption geometry](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L1488-L1527) · [Bottom placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2114-L2123)

## Mac comparison

The pinned KOReader replay calls the live HTML table presentation control,
checks the four caption positions, fills and borders, and restores the
original frame. Reader tests also verify the linked caption's target, source
ranges and return positions after pagination. In Book mode the adjacent tables
remain separated in both renderers. Each engine uses its own font metrics and
page layout. [XHTML specimen](../tests/fixtures/table-captions.xhtml) · [EPUB specimen](../tests/fixtures/table-captions.epub) · [Native replay](../scripts/koreader-table-captions-check.lua) · [Reader tests](../src/parley_layout/table_caption_tests.rs)

| Mode | Reader | KOReader |
| --- | --- | --- |
| Publisher styles and HTML hints | [Page](screenshots/reader-table-captions-publisher.png) | [Page](screenshots/stock-table-captions-publisher.png) |
| HTML hints ignored | [Page](screenshots/reader-table-captions-without-html-hints.png) | [Page](screenshots/stock-table-captions-without-html-hints.png) |

Set `READER_TABLE_CAPTION_CAPTURES` and run the `table_caption` Rust test
filter to reproduce the Reader captures. The native replay takes the EPUB
specimen, a cache directory, Noto Sans Regular and a capture directory.

Nested and repaired CSS grids, vertical writing, and chapters using float
geometry keep their existing source layout. A joined whole note takes
precedence only when its block range intersects a captioned table.
