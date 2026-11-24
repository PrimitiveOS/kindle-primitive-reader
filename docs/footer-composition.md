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

The custom footer has 18 selectable items, including [brightness and warmth](#inactive-items-and-clock).
Memory and Wi-Fi indicators, plug-in supplied items, and physical E Ink
verification remain open.

## Inactive items and clock

**Reading status bar → Hide inactive items** applies when custom items
share the footer. It hides brightness and warmth when the front light is
off, the bookmark count when the book has no annotations, and the page-turn
inversion item while the turn direction is normal. Active items reappear
with their separators. Cycling shows each item's value or **Off**.
[KOReader inactive-item setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L1529-L1547)

**Brightness level** shows the Kindle light level. **Warmth level** shows
a percentage on devices with the warmth control. Both use the same readings
as Reader Quick Settings. **12-hour clock** changes the clock in the
optional EPUB header and the Current time footer item. The device's date,
time zone, and clock are configured in Launcher. Custom-text date and time
tokens retain their documented formats.
[KOReader front light and warmth generators](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L145-L178) ·
[KOReader clock format](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/datetime.lua#L239-L263)

The Mac replay compares active, inactive and cycling readings, saved settings,
legacy order migration, and a pixel-identical return to the default chapter
footer. The native KOReader replay uses the pinned stock generators with
simulated hardware values and deterministic civil time. Physical light,
warmth, and E Ink updates remain for the Kindle run.
[Stock replay](../scripts/koreader-footer-inactive-items-check.lua) ·
[Inactive items shown](screenshots/status-inactive-shown.png) ·
[Inactive items hidden](screenshots/status-inactive-hidden.png) ·
[Active items with icons](screenshots/status-active-icons.png)

To replay the Mac workflows and write the simulator capture, create an
output directory and set it before running the tests.

    READER_SELECTION_VIEWPORT_CAPTURES="$output" \
      cargo test --locked --features kindle --lib app::reading_footer::composition_tests
