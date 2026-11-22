# Horizontal padding in published books

Reader now applies the left and right padding that EPUB publishers use to set off quotations, notes, list-like paragraphs and table cells. Every line wraps to the available width. A first-line indent can still move one line independently. The user’s page margins continue to set the outside boundary.

| Ordinary paragraph boxes | The same text with publisher padding |
| --- | --- |
| ![Quote and nested paragraph at the ordinary left edge](screenshots/publisher-box-no-padding.png) | ![Quote inset on both sides and nested note further inset](screenshots/publisher-box-padding.png) |

Both images show the same authored document at 740 × 1240 pixels with 30 px type. The quotation gains 52 px on either side. A 36 px inset on its second paragraph brings that paragraph’s total displacement to 88 px. The paragraph after the quotation returns to the page edge. [Screenshot fixture](../tests/fixtures/publisher-box-padding.xhtml) · [Source-position and hit-testing checks](../src/parley_layout/publisher_padding_tests.rs)

## Publisher rules

The horizontal renderer supports physical left and right padding, the one- to four-value CSS shorthand, percentages of the containing box, and font-relative or absolute lengths. Nested boxes contribute their own padding once. An explicit zero clears the current box’s padding. A negative first-line indent can use the leading padding, including the right edge of a right-to-left paragraph. A 70 px left inset paired with a −70 px first-line indent keeps the first line flush and moves following lines 70 px. Oversize padding leaves room for text within the reading column.

KOReader v2026.07.1 uses CREngine’s shorthand expansion, content-box measurement and negative-indent correction. This comparison covers horizontal reflow. [Shorthand expansion](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L4292-L4326) · [Table-cell content boxes](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L1652-L1685) · [Negative indent against leading padding](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8854-L8900)

## Original publications

The Mac comparison held fonts, page size, source markup and remaining publisher styles constant. It removed only horizontal padding for the control. Across 20 sections from eight original EPUBs, Reader changed the geometry of 609 paragraphs. Of these, 50 changed line count. Selections still resolved the same original text in both layouts. Sampled pages rendered and returned to their saved source positions. The authored regression also checks search matches and word taps.

| Anonymous EPUB | Chapters checked | Paragraphs with changed geometry | Paragraphs with changed line count |
| --- | ---: | ---: | ---: |
| 033 | 2 | 198 | 19 |
| 036 | 3 | 31 | 8 |
| 042 | 2 | 0 | 0 |
| 047 | 2 | 0 | 0 |
| 058 | 2 | 25 | 0 |
| 061 | 2 | 52 | 6 |
| 062 | 3 | 16 | 1 |
| 069 | 4 | 287 | 16 |

Original 047 contains 811 matching paragraphs in the sampled chapters. The publisher offsets their leading padding with an equal negative first-line indent. Their text geometry remained identical in both modes. Original 036 also uses unequal lengths. Its displaced paragraphs began 59 to 84 px farther inward when the complete rule was applied. The chapter and byte positions in both comparisons refer to the same original publication.

## Reference render modes

KOReader’s reading interface selects **Book** by default. It prevents a negative first line from painting outside its paragraph box. Its **Web** mode allows that overflow and is also the default when starting a standalone CREngine document view. The comparator explicitly selects the reading UI’s Book flags before opening each original. [KOReader render-mode setting](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/ui/data/creoptions.lua#L317-L330) · [Flags used by the reading interface](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readertypeset.lua#L378-L438)

Original 036 provided a precise check. The publisher specifies a 2.35em leading inset and a −0.65em first-line indent. Installed CREngine resolved one em to 35 or 42 pixels at the sampled passages. Its Book-mode first lines moved 59 or 70 px when the padding was enabled. In Web mode they moved 82 or 98 px. Reader now follows CREngine’s 1/256-unit CSS lengths and integer font sizing at block edges. The authored regression reproduces the two Book-mode positions and the following lines’ 82 and 98 px insets. [CSS number parsing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L817-L829) · [Conversion to layout pixels](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L2582-L2633) · [Device comparator](../scripts/koreader-box-padding.lua)

On an Oasis 3, the final ARM build repeated five original sections from 033 and 036 and six authored checks. All 229 affected source selections and the geometry summaries agreed with the Mac. The original documents, user reading records, Browser session and installed Reader retained their checksums.

Installed KOReader in Book mode located 43 source passages in four original EPUBs. Reader matched the measured displacement at 38. Reapplying the zero-padding control in each original restored the sampled positions and the page count.

| Original and source layout | Passages with identical horizontal displacement | Observed displacement |
| --- | ---: | --- |
| 033, ordinary and nested blocks | 10 of 10 | 0, 2 or 80 px in both engines |
| 036, negative first-line indent paired with padding | 15 of 15 | 59 or 70 px in both engines |
| 061, full-width table | 5 of 5 | 10 px in both engines |
| 061, 95%-wide table with 0.5em padding | 0 of 5 | Reader measured 4 or 16 px. KOReader measured 6, 9 or 10 px. |
| 069, nested lists | 8 of 8 | 63 or 189 px in both engines |

All five differences came from the second and third columns of the same three-column table. The original specifies both 10 px cell padding and a padded outer table with a width of 95%. Reader places the cells using its assigned column fractions. CREngine also sizes the table and columns from their contents. These measurements leave a 2–7 px difference in those columns. The full-width table in another chapter had zero outer padding. [Reader cell positioning](../src/table_layout.rs) · [CREngine column sizing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L1064-L1341)
