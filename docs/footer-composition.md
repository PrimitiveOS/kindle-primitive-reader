# Chapter progress and footer composition

Open **Reading status bar** and set **Status text** to **Custom items**.
Tap **Current page in chapter** or **Dynamic filler** to select either
item. Use **Status item order** at the end of the settings to place
the filler among the other selected items. Presets retain the selected
items and their order. [Custom-item controls](footer-item-cycle.md) ·
[PDF simulator capture](screenshots/footer-filler-landscape-pdf.png)

## Current page in chapter

The dedicated item displays the current page within the chapter and the
chapter's total, for example **2 ⁄⁄ 4**. It uses resolved Contents
boundaries in fixed and reflowable books, honors the applicable chapter
depth navigation filter, and uses alternative heading spans when they
are resolved. When the EPUB footer uses publisher or generated reference
labels, a resolved span in the current layout counts reference pages.
The measurement shares the displayed page numbering across font and
viewport changes. [KOReader chapter progress](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L2579-L2596) · [KOReader chapter page accounting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L688-L767)

## Dynamic filler

The filler expands to fit the measured space between items on the same
line. It replaces the separator at its position and can also go before
the first or after the last item. The footer uses the available text
region and reserves fixed-page zoom and Exit controls. A visible
alongside track, single-item cycling, and a line too narrow for a gap use
the ordinary composition. [KOReader dynamic filler](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L413-L459) · [KOReader joined items](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L2129-L2166)

The pinned native KOReader replay runs the real chapter progress and
combined footer generators with native font measurement. It verifies
the fractional item, edge and interior filler, and when an alongside
track takes precedence. Mac Reader tests exercise an EPUB with authored
labels, a PDF with a nested outline, a live landscape viewport, grouping
and narrow fallback, cycle, presets, reopening, and the original
chapter-and-page footer frame. [Native stock replay](../scripts/koreader-footer-composition-check.lua) ·
[Reader regression tests](../src/app/reading_footer/composition_tests.rs)

The custom footer has 16 selectable items. The stock light, warmth,
memory and Wi-Fi indicators, plug-in supplied items, and physical E Ink
verification remain open.

To replay the Mac workflows and write the simulator capture, create an
output directory and set it before running the tests.

    READER_SELECTION_VIEWPORT_CAPTURES="$output" \
      cargo test --locked --features kindle --lib footer_composition
