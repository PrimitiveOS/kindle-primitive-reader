# Reading table styles

Open **Settings › Style tweaks › Tables** in a reflowable book. Each style
can apply to the open book or become an all-books default. Table text,
links, and saved locations continue to use the original source.
[Reader controls](screenshots/reader-table-style-controls.png) ·
[Pinned KOReader presets](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L597-L648)

| Control | Behavior and comparison |
| --- | --- |
| Ignore HTML table presentation | Suppresses supported legacy HTML width, alignment, background, padding, spacing, border, and `nowrap` hints. CSS declarations and row and column spans still apply. [Presentation comparison](#html-presentation) · [Cell wrapping](table-cell-nowrap.md) |
| Full-width tables | Uses the available column width. |
| Ignore publisher table and cell widths | Uses intrinsic sizing for the table and cells while keeping explicit column track sizes. |
| Center small tables | Centers the measured table when there is free horizontal space. The three width choices work together. [Width evidence](table-columns.md) |
| Ignore publisher vertical alignment in tables | Aligns TD content to its row's first-line baseline. TH uses the publisher's alignment. [Vertical alignment comparison](#cell-vertical-alignment) |
| Alternate background color of table rows | Uses #EEE and #CCC in row order and keeps the spacing between separate cells clear. Each row group starts its own sequence. [Paint comparison](publisher-table-borders.md) |
| Show borders on all tables | Adds 1px collapsed borders, including on tables with one column. Publisher border choices retain their normal cascade priority. [Border comparison](publisher-table-borders.md) |
| Force table borders (legacy) | Retains the original Reader preset for saved profiles that explicitly override table and cell borders. |

Native horizontal grids honor top and bottom caption placement. Legacy
floating table alignment and some table background and border details remain
open. Differences in the layout engines can still move a line break within a
cell. [Caption comparison](table-caption-placement.md)

## HTML presentation

The native replay invokes the real **Ignore tables related HTML presentational
hints** callback. It compares an HTML-styled table with its CSS-styled control,
measuring row backgrounds and the position of ordinary and linked cells.
The same source positions, merged cell text, and link destination survive
the switch.
[Native replay](../scripts/koreader-table-presentation-check.lua) ·
[Authored table](../tests/fixtures/table-presentational-hints.xhtml)

Reader keeps the HTML attributes separate from publisher styles during CSS
resolution. The tests check which widths, tracks, alignment, spacing,
padding and borders are removed, then verify the rendered link and its
source range. The visible menu test saves an all-books default and reopens
the same passage.
[Cascade tests](../src/book_style/table_hints.rs) ·
[Reader layout test](../src/parley_layout/table_presentation_tests.rs)

Book and Web render modes apply the Traditional EPUB table defaults when
that base stylesheet is selected. They use 80% text size, keep that size in
nested tables, add 3 px cell padding and 1 px border spacing, and give table
headers and captions their default backgrounds. Rows and row groups use
middle alignment by default. Publisher CSS can override these values.
[KOReader EPUB defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/cr3gui/data/epub.css#L84-L111) ·
[KOReader HTML row alignment](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/cr3gui/data/html5.css#L638-L654)

| Layout | Reader | KOReader |
| --- | --- | --- |
| With HTML presentation | [Page](screenshots/reader-table-presentation-attributes.png) | [Page](screenshots/stock-table-presentation-attributes.png) |
| With the switch enabled | [Page](screenshots/reader-table-presentation-stylesheet.png) | [Page](screenshots/stock-table-presentation-stylesheet.png) |

Run the native replay from the pinned runtime with absolute paths to the
XHTML fixture, an empty cache, Noto Sans Regular, and an output folder. Run
the Reader tests with
`cargo test --locked --features kindle --lib table_presentational`.
Use `READER_STYLE_GRID_CAPTURES` for the page images and
`READER_SELECTION_VIEWPORT_CAPTURES` for the menu image.

## Cell vertical alignment

Reader positions a native horizontal table after measuring its cell
contents. Top alignment uses the cell's leading edge. Middle and bottom
use the available height across the row span. Baseline aligns the first
shaped text lines. A smaller-font cell can move down and increase the row
height. Pictures, Ruby annotations, list markers, links, and selection
follow the content. Borders and row fills use the final tracks. The
ordinary layout handles nested grids, chapters that use
floating layout, and spanning cells whose baseline would overflow.
[Row sizing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L1853-L1934) ·
[Content placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2145-L2204)

The pinned replay enables the real TD-only preset. On the authored
specimen, middle and bottom TD text moves from y=59 and y=81 to y=37.
The bottom-aligned TH stays at y=81. A 12 px cell moves down 22 px to
share a 32 px cell's baseline. The row and following text gain the same
22 px. The short rowspan moves from the bottom of its final row to the
first row's baseline. The tall exact-fit rowspan stays at its start.
The source anchors and links survive the toggle, and turning it off
restores the initial frame.
[Stock preset](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L628-L632)

| Layout | Reader | KOReader |
| --- | --- | --- |
| Publisher alignment | [Page](screenshots/reader-table-vertical-publisher.png) | [Page](screenshots/stock-table-vertical-publisher.png) |
| TD baseline override | [Page](screenshots/reader-table-vertical-baseline.png) | [Page](screenshots/stock-table-vertical-baseline.png) |

The four-table specimen uses Noto Sans at 20 px, a 760 × 930 viewport,
and 32 px page margins. Pass the specimen, cache folder, font path, and
output folder to the native replay. [Specimen](../tests/fixtures/table-vertical-alignment.xhtml) ·
[Native replay](../scripts/koreader-table-vertical-alignment-check.lua)

Run the Mac checks with
`cargo test --locked --features kindle --lib table_vertical` and set
`READER_STYLE_GRID_CAPTURES` to save the Reader frames.
