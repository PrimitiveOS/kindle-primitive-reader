# Importing PDF annotation text

Reader's PDF annotation import recognizes `/Squiggly` as an underline, as
KOReader does. For a supported foreign markup annotation without a stored
Reader quotation, the import preview attempts to recover text from the PDF's
word geometry. A word is included when at least half its box overlaps an
annotation rectangle. Consecutive selected words retain their intervening
whitespace; gaps containing unselected text become a space. The saved note,
color, bounds and existing marks keep their prior behavior.

The text recovery uses at most 8,192 word boxes per page and 32,768 geometry
comparisons per annotation, caches only the last page, and extracts at most 16
pages per import preview. Truncated, unavailable,
oversized or unmatched text falls back to the page-based highlight label. This
is a geometry approximation; selected words and spacing can differ from a
reader with glyph-precise text geometry. Import remains an explicit preview
and confirmation, and does not write to the source PDF. Export writes a
separate copy as before.

KOReader maps PDF squiggly annotations to its underline drawer and obtains
text between the annotation's first and last page positions. Reader selects
overlapping extracted word boxes, so the implementations need comparison on
real documents. [KOReader annotation mapping](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhighlight.lua#L2631-L2637) · [KOReader import](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerhighlight.lua#L2660-L2725)

This change has not yet been built, tested or compared with KOReader. Rotated
and cropped pages, overlapping marks, scan-only pages, punctuation, and source
PDF preservation remain verification requirements.
