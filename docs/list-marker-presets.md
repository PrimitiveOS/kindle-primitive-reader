# List item styles

Open **Settings › Style tweaks › List items** while reading a reflowable
book. **Force bullets with unordered lists** uses a filled disc in place of
publisher rings, squares or hidden markers. **Force decimal numbers with
ordered lists** changes the number format while preserving the list’s start,
individual item values, and nested sequence. Each choice has its own book
override and all-books default. [Pinned KOReader presets](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L796-L810) · [Reader controls](screenshots/reader-list-item-controls.png)

Both presets apply to list items directly inside the corresponding list type.
An ordered list nested inside an unordered list keeps its numbering unless
the decimal preset is enabled. Marker shapes and numbers stay outside the
original text reported by search or clipboard selection.

Publisher CSS can select `list-style-position: inside` or use the equivalent
`list-style` shorthand. The current implementation candidate gives the marker
an inline layout box on the first line while keeping it out of the book's
canonical text, so selection and search continue to address the original
passage. The default remains the outside position. KOReader v2026.07.1 on a
Kindle Oasis 3 (`scripts/koreader-list-marker-inside-check.lua`) and Reader agree on the
order that matters: with the inside marker, wrapped lines start left of the first line's
marker; with the outside marker, they start right of it. KOReader's numbers were a
marker ink starting at x=63 and wrapped lines at x=59 (inside), and wrapped lines at
x=99 (outside), at its own 617 px geometry. The offsets differ slightly from Reader's
380 px geometry, and a screenshot comparison has not been made. The pinned CREngine interface
exposes the built-in list marker types and inside/outside position; custom
`@counter-style` rules and image markers are outside that reference scope.
[Pinned CREngine list styles](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/include/cssdef.h#L1576-L1615)

## Mac comparison

The shared chapter covers ring, square and hidden unordered markers, a
nested Roman list starting at seven, and an ordered list with an explicit
item value of 13. The pinned replay calls both real KOReader menu actions
independently and together, checks the marker rasters and original source
pointers, and reproduces the publisher frame after both actions are cleared.
Reader additionally checks the original hyperlink, saved reading position,
all-books default, per-book override and reopening.
[Chapter](../tests/fixtures/list-marker-presets.xhtml) · [Native replay](../scripts/koreader-list-marker-presets-check.lua) · [Reader layout checks](../src/parley_layout/style_presets_tests.rs)

The forced frame in each renderer shows 7, 8 and 13 for the three ordered
items. List indentation and line spacing still differ between the renderers.

| State | Reader | KOReader |
| --- | --- | --- |
| Publisher markers | [Page](screenshots/reader-list-marker-publisher.png) | [Page](screenshots/stock-list-marker-publisher.png) |
| Both overrides | [Page](screenshots/reader-list-marker-forced.png) | [Page](screenshots/stock-list-marker-forced.png) |

Run the native replay with the chapter, an empty CRE cache directory, the
Noto Sans Regular fixture font, and a capture directory. In Reader, run
`cargo test --locked --features kindle --lib list_marker`. Set
`READER_STYLE_GRID_CAPTURES` for the pages or
`READER_SELECTION_VIEWPORT_CAPTURES` for the controls.
