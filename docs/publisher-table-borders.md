# Borders on EPUB tables

Reader draws borders around cells when the publication supplies them. A collapsed grid draws each internal line once. In a separate grid, the two cells can draw different borders and the publisher can leave space between them. Transparent borders reserve their width. Hidden borders occupy zero pixels.

| Collapsed grid | Separate cells | Transparent lines |
| --- | --- | --- |
| ![A harbor survey with one shared line between adjacent cells](screenshots/table-border-collapsed.png) | ![A harbor survey with individually outlined, spaced cells](screenshots/table-border-separate.png) | ![The same positions and header shading with the lines made transparent](screenshots/table-border-transparent.png) |

The authored table uses a 4 px dark gray outer line and 2 px cell lines. The separate example adds 8 px horizontal and 6 px vertical gaps. In the transparent comparison every tested word retains its location, wrapping and tap target. [Screenshot fixture](../tests/fixtures/harbor-survey-table.xhtml) · [Rendering and selection checks](../src/parley_layout/table_border_tests.rs)

## Reading behavior

- Stylesheets and inline styles can independently set each edge’s width, pattern and color. Reader handles solid, double, dashed and dotted lines. It converts named colors, hexadecimal values and RGB colors to grayscale, including alpha blending.
- At a shared boundary, the cell on the right supplies the left stroke and the cell below supplies the top stroke. Outer edges may use a wider stroke from their row, row group or table. This follows the stock renderer’s ownership rule. Adjacent cells do not both paint the same collapsed edge. [CREngine border ownership](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L826-L940)
- Merged cells keep their edge across the rows or columns they occupy. A rule continues across visible portions of a multi-page table. Source ranges, search and selection still use the original publication text. [Reader stroke placement](../src/table_borders.rs)
- Hidden and zero-width strokes take no space. A transparent color keeps the width and skips the paint. [CREngine measurement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L4873-L4926) · [CREngine transparent strokes](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L9290-L9323)

Reader caps stroke width and table spacing at 64 px. Tables that do not fit the horizontal layout continue using the narrow-screen text view. CSS relief patterns such as groove and ridge, full CSS text color inheritance and entirely empty source rows remain outside this implementation.

## Original EPUB comparison

On an original three-column table with 1 px solid rules, the installed KOReader and Reader located the same five passages before and after changing the rules. Transparent borders preserved all five positions in both engines. Hidden borders moved the start of every selection left exactly 1 px. Both engines restored the original positions and grayscale frames after resetting the styles. No publication text or screenshots from this book are included here.

| Geometry at the five passages | Reader on the Mac | Installed KOReader on Oasis 3 |
| --- | --- | --- |
| Published collapsed borders | 451, 836, 451, 836, 451 px | 451, 831, 450, 831, 450 px |
| Same rules made transparent | 451, 836, 451, 836, 451 px | 451, 831, 450, 831, 450 px |
| Hidden borders | 450, 835, 450, 835, 450 px | 450, 830, 449, 830, 449 px |
| Separate borders, zero spacing | 457, 829, 457, 829, 457 px | 457, 824, 456, 824, 456 px |

To measure actual paint, the comparison captured the pages containing the five passages and changed only the border color to transparent. Reader changed 14,052 pixels in collapsed mode and 27,572 in separate mode. KOReader changed 15,236 and 30,016 pixels respectively. Both produced eight continuous horizontal runs of at least 64 px with collapsed edges and sixteen with separate edges. Both drew a longest horizontal rule of 1,156 px. Font shaping, row height and page break positions affect vertical measurements and total ink. [KOReader in-memory comparison](../scripts/koreader-table-borders.lua) · [Earlier width measurements](table-columns.md)
