# Reading style presets

Open **Appearance › Style tweaks** to apply a change to the open book or
set a default for other books. A book can override the default, and turning
off style tweaks preserves the saved choices.

**Tailor widths and text-indent for CJK** centers a whole-cell line width
and rounds the first-line indent to the same cell size. The cell size
tracks **CJK character width**. A 557px text area at 40px type uses 520px
at 100% and 550px at 125%. A 1.2em indent becomes 80px or 100px,
respectively. Reader preserves the source text and saved position as it
wraps. Float contours, inline illustrations, and columns narrower than
one cell retain their ordinary layout. Vertical text uses its own
layout path.
[Reader setting](screenshots/reader-style-grid-controls.png) ·
[Pinned style definition](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L527-L533) ·
[Native width calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8911-L8925)

**No indentation on following paragraphs** applies only to a paragraph
immediately following another paragraph in the same container. The first
paragraph and a paragraph after a heading keep their existing indentation.
[Pinned indentation controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L537-L565)

**Pure black and white** clears publisher background fills and uses black
for the supported borders, including fitted text boxes and table cells.
Reader's regular text ink, inline images, and annotations keep their
own paint behavior.
[Pinned black-and-white style](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L836-L847)

## Simulator comparison

The pinned KOReader replay uses the real style callbacks and CRE renderer.
It measures the cell width, starting positions, source location, and line
endpoints at 100% and 125%. The color specimen checks the border and
background pixels before and after the preset. Reader tests use the same
specimens and exercise the saved menu choice, the reading anchor,
selection, and search. The Reader color test uses its Web EPUB render
mode so the specimen's fitted border is painted. Each engine retains its
own pagination and glyph rasterization.
[Stock replay](../scripts/koreader-reading-style-grid-check.lua) ·
[Reader layout tests](../src/parley_layout/style_presets_tests.rs)

| Mode | Reader | KOReader |
| --- | --- | --- |
| CJK grid at 100% | [Page](screenshots/reader-style-grid-100.png) | [Page](screenshots/stock-reading-cjk-grid-100-on.png) |
| CJK grid at 125% | [Page](screenshots/reader-style-grid-125.png) | [Page](screenshots/stock-reading-cjk-grid-125-on.png) |
| Publisher fills and borders | [Page](screenshots/reader-style-monochrome-off.png) | [Page](screenshots/stock-reading-monochrome-off.png) |
| Pure black and white | [Page](screenshots/reader-style-monochrome-on.png) | [Page](screenshots/stock-reading-monochrome-on.png) |
