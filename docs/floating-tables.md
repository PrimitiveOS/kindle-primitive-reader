# Floating tables

In Book and Web modes, supported horizontal tables can float against the
physical left or right edge. Surrounding text wraps beside the table and
recovers the full line width below it. The caption, cells, borders and fills
move together. A table can use its intrinsic width or the publisher's width.
[Native float sizing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7727-L7874) · [Default table box sizing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/html5.css#L628-L630)

The legacy HTML `align="left"` and `align="right"` hints take effect unless
author CSS overrides them. For example, `float: none` keeps the table in line.
**Ignore HTML table presentation** removes those hints while preserving
author CSS. The float side stays physical under RTL, and the cells keep their
RTL column order. Flat and Legacy modes keep the table in the normal reading
flow. [Legacy HTML rules](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/html5.css#L718-L767) · [Native float placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6777-L6830)

A floated table and adjacent paragraphs honor physical clearance. A table
with `clear: left` can move below an earlier left float while text still
wraps beside the first table. A full-width floated table leaves no space
for text beside it. Search, selection, links and saved positions continue
to use the document source.

## Mac simulator checks

The pinned KOReader replay uses the real render-mode and style-tweak
callbacks. It compares the left float, intrinsic right float, CSS precedence
with RTL, `float: none`, two opposing tables with clearance, full-width
float and a centered table. For the left float with a bottom caption, Book
and Web place the neighboring word at x=240 while the table fills the first
200 pixels after the 40-pixel page margin. The right-aligned RTL table
keeps the same frame when the HTML hints are disabled. [EPUB fixture](../tests/fixtures/floating-tables.epub) · [Fixture source](../scripts/generate-floating-tables-fixture.py) · [Native replay](../scripts/koreader-floating-tables-check.lua) · [Reader tests](../src/float_layout/tables/tests.rs)

| Case | Reader | KOReader |
| --- | --- | --- |
| HTML left alignment and bottom caption | [Page](screenshots/reader-floating-table-left.png) | [Page](screenshots/stock-floating-table-left.png) |
| The same table with HTML hints ignored | [Page](screenshots/reader-floating-table-left-without-hints.png) | [Page](screenshots/stock-floating-table-left-without-hints.png) |
| CSS right alignment on an RTL table | [Page](screenshots/reader-floating-table-rtl.png) | [Page](screenshots/stock-floating-table-rtl.png) |

Set `READER_FLOAT_TABLE_CAPTURES` and run the `floating_tables` Rust test
filter to regenerate the Reader screenshots. The native replay takes the
EPUB, a cache directory, Noto Sans Regular and a capture directory.

The float path covers standalone native horizontal grids in the page-wide
flow. Nested grids, repaired CSS tables, vertical text, constrained parent
boxes and tables that exceed a page use ordinary flow.
