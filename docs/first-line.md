# Publisher first lines

Reader applies `::first-line` typography to the first rendered line of a
horizontal text paragraph. Font family, size, weight, slant, line height,
and text decoration can change where the line wraps. The selected endpoint
is a boundary in the original line-breaking text. Reader lays out the
rest of the paragraph with its ordinary typography and keeps the same
search, selection, bookmark and resume positions.

**Settings › Style tweaks › Pseudo-elements › Disable first-line pseudo
elements** switches off the publisher's first-line style for the book or as
a default. The controls for generated text and first-letter styling can be
enabled independently. [Pinned KOReader menu control](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L828-L832)
[Reader controls](screenshots/reader-pseudo-element-controls.png) ·
[Book CSS limits](style-tweaks.md)

Explicit typography on the original first letter or an inline child uses
the first line's inherited styles as its base. An absolute size stays
absolute, while a percentage on a child uses the first line's size. Supported
generated inline text uses first-line typography on that line and keeps its
ordinary generated style on subsequent lines. It remains outside the
original text and saved positions.
[Pinned node cloning](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L6566-L6596) ·
[Pinned continuation logic](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L5584-L5644)

## Mac replay

At 510 px, the larger first line wraps before **Harbor**. Disabling the
first-line style puts Harbor on the first line and keeps the explicit quoted
initial. In the same viewport, weight alone moves **Fluxgate** to the second
line at the same font size. At 650 px, the pinned right-to-left sample fits
five Arabic words on the styled first line and seven words when disabled.
A hard break ends the first line, and a leading hard break leaves later
text at its ordinary size. The pinned callback kept 29 original source
ranges unchanged. Two fresh profiles reproduced all ten captures byte for
byte, and clearing the tweak restored the original pixels.
[Shared chapter](../tests/fixtures/pseudo-first-line.xhtml) ·
[Pinned callback replay](../scripts/koreader-pseudo-first-line-check.lua)

| Renderer and viewport | Publisher styling | First-line styling disabled |
| --- | --- | --- |
| KOReader, 510 px | [Screenshot](screenshots/stock-pseudo-first-line-510-publisher.png) | [Screenshot](screenshots/stock-pseudo-first-line-510-line-off.png) |
| KOReader, 650 px | [Screenshot](screenshots/stock-pseudo-first-line-650-publisher.png) | [Screenshot](screenshots/stock-pseudo-first-line-650-line-off.png) |
| Reader, 510 px | [Screenshot](screenshots/reader-pseudo-first-line-510-publisher.png) | [Screenshot](screenshots/reader-pseudo-first-line-510-line-off.png) |
| Reader, 650 px | [Screenshot](screenshots/reader-pseudo-first-line-650-publisher.png) | [Screenshot](screenshots/reader-pseudo-first-line-650-line-off.png) |

Reader reproduces the narrow English and bold-only wrap changes. Its
650 px Arabic sample fits six styled words and all ten words after clearing
first-line styling. Both renderers resume with ordinary styling at their own
logical wrap point. Reader kept all 29 original anchors, verified taps in
both directions, and retained an authored discretionary hyphen across the
change of style. The ten Reader captures reproduced byte for byte on a
second run.

Run Reader's focused checks with
`cargo test --locked --features kindle --lib pseudo_first_line`.
Set `READER_PSEUDO_FIRST_LINE_CAPTURES` to an existing folder to save the
simulator captures. [Reader range and rendering tests](../src/parley_layout/pseudo_first_line_tests.rs)

The first-line layout covers ordinary text and supported generated inline
content. Layouts with ruby, replaced inline boxes, table cells, displaced
initials, vertical text, or an exceptionally large paragraph retain ordinary
layout. If reshaping changes a text atom at the required first-line break,
Reader retains the whole paragraph with its ordinary line breaks.
