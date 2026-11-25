# Publisher initials

Reader applies publisher `::first-letter` to the first retained text node in
a horizontal paragraph. It styles the initial together with its qualifying
punctuation and combining marks. A leading line break leaves the paragraph
in its ordinary style. Text inside a `display:none` element is skipped.
An initial inside an inline element inherits that element’s typography, and
the publisher’s first-letter rule can override it.
[Pinned stock range selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L6353-L6475)

**Settings › Style tweaks › Pseudo-elements › Disable first-letter pseudo
elements** restores ordinary typography and placement. It can be saved for
the current book or used as a default. The before/after control can be
enabled independently. A generated prefix remains on screen while the
initial is resolved from the original text. Reading positions, copied
words, searches, and taps use the same original ranges in each state.
[Pinned menu control](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L821-L826)

In Book and Web geometry, an ordinary left-to-right paragraph can wrap
around a left-floated initial. Numeric `initial-letter` and the WebKit alias
size the cap from the selected font’s capital metrics and reserve the
requested line band, including the cap’s own vertical extent. A displaced
initial keeps an original-text box for selection and a page extent for its
paint. The displaced layout needs room for the cap and neighboring text
within the reading area and uses paragraphs with zero first-line indent.
The displaced layout is limited to horizontal text and excludes
right-sided floats, generated text, artwork, ruby, and table cells.
[Stock numeric sizing and alignment](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/renderutil.cpp#L363-L388) ·
[Reader displaced initial](../src/parley_layout/inline_blocks/initial.rs)

The Mac replay uses the actual KOReader callback to compare a quoted and
accented initial, a leading break, skipped hidden text, punctuation in its
own source node, a generated prefix, and both kinds of displaced initial.
It checks fourteen source pointers, copied text, line geometry, and exact
restoration. Reader checks the same source ranges, independent controls,
and original-word selection inside its displaced glyphs.
[Shared chapter](../tests/fixtures/pseudo-first-letter.xhtml) ·
[Stock replay](../scripts/koreader-pseudo-first-letter-check.lua) ·
[Reader tests](../src/parley_layout/pseudo_first_letter_tests.rs)

| Renderer | Publisher styling | First-letter styling disabled |
| --- | --- | --- |
| KOReader | [Screenshot](screenshots/stock-pseudo-first-letter-publisher.png) | [Screenshot](screenshots/stock-pseudo-first-letter-off.png) |
| Reader | [Screenshot](screenshots/reader-pseudo-first-letter-publisher.png) | [Screenshot](screenshots/reader-pseudo-first-letter-off.png) |

Both renderers restore the publisher capture byte for byte when the tweak
is cleared. Paragraph spacing, line breaks, and glyph rasterization follow
each renderer’s layout engine.

Run the stock replay from the pinned runtime with absolute paths to the
chapter, an empty CRE cache, the Noto Sans Regular fixture font, and the
capture folder. Run the Reader checks with
`cargo test --locked --features kindle --lib pseudo_first_letter`.
