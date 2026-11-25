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
