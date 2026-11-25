# Reading table styles

Open **Settings › Style tweaks › Tables** in a reflowable book. Each style
can apply to the open book or become an all-books default. Table text,
links, and saved locations continue to use the original source.
[Reader controls](screenshots/reader-table-style-controls.png) ·
[Pinned KOReader presets](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L597-L648)

| Control | Behavior and comparison |
| --- | --- |
| Ignore HTML table presentation | Suppresses supported legacy HTML width, alignment, background, padding, spacing, and border hints. CSS declarations and row and column spans still apply. [Presentation comparison](#html-presentation) |
| Full-width tables | Uses the available column width. |
| Ignore publisher table and cell widths | Uses intrinsic sizing for the table and cells while keeping explicit column track sizes. |
| Center small tables | Centers the measured table when there is free horizontal space. The three width choices work together. [Width evidence](table-columns.md) |
| Alternate background color of table rows | Uses #EEE and #CCC in row order and keeps the spacing between separate cells clear. Each row group starts its own sequence. [Paint comparison](publisher-table-borders.md) |
| Show borders on all tables | Adds 1px collapsed borders, including on tables with one column. Publisher border choices retain their normal cascade priority. [Border comparison](publisher-table-borders.md) |
| Force table borders (legacy) | Retains the original Reader preset for saved profiles that explicitly override table and cell borders. |

The stock cell vertical-alignment switch still requires support for shifting
cell content inside a measured row. Caption placement, legacy floating
table alignment, no-wrap hints, and some table background and border
details also remain open. Baseline table font sizes and cell padding
differ in the two engines, so a cell's text can wrap at a different point.

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

The remaining stock preset gives `td` cells baseline alignment and keeps
the alignment declared on `th` cells. In the pinned Book-mode replay, a
middle-aligned and a bottom-aligned cell moved onto the first baseline.
An explicitly bottom-aligned header kept its position. A row mixing 12px
and 32px text grew by 22px, shifting the following prose by the same amount.
The link and source positions survived the change.
[Stock preset](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L628-L632) ·
[Row sizing and placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L1853-L1934)

Reader currently lays out cell content as it encounters each row. Matching
the baseline behavior requires measuring cell content and span heights,
then placing text and illustrations relative to the completed row grid.
Borders and background fills use the completed row rectangles. The next
comparison covers mixed font sizes, cells spanning multiple rows, link
targets, and the position of the following text.
[Current row flow](../src/table_layout.rs)
