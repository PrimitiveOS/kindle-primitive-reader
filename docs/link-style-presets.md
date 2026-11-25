# Reading link styles

In a reflowable book, open **Settings › Style tweaks › Links**. Choose
black or blue ink, bold or normal weight, italic or upright text, or
underlines on or off. Each pair is independent. Apply a choice to the open
book or save it as a default. A book can override the default and keeps its
reading position when the style changes.
[Reader settings](screenshots/reader-link-styles-controls.png) ·
[Stock definitions](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L650-L703)

The forced ink, weight, and slant apply to links, named anchors, and their
nested text. Always underlined applies only to anchors with a destination.
Never underlined removes the link’s own decoration. A visible underline
from a surrounding paragraph is preserved. The link destination, source
text, and selection coordinates remain usable after styling.

The pinned KOReader replay invokes all eight real callbacks and verifies
that each pair is mutually exclusive. The renderer paints pure black and
blue as grayscale 0 and 63. Reader measures those same values at the glyph
pixels. Both tests check nested text, named anchors, nearby ordinary text,
and the tap target. The Reader workflow also checks per-book and global
persistence. The decoration comparison covers horizontal reading. An
additional Reader test checks link ink in vertical text.
[Stock replay](../scripts/koreader-link-styles-check.lua) ·
[Shared specimen](../tests/fixtures/reading-link-styles.xhtml) ·
[Reader tests](../src/parley_layout/link_styles_tests.rs) ·
[KOReader specimen before styling](screenshots/stock-link-styles-default.png)

| Choice | Reader | KOReader |
| --- | --- | --- |
| Black | [Page](screenshots/reader-link-black.png) | [Page](screenshots/stock-link-styles-a_black.png) |
| Blue | [Page](screenshots/reader-link-blue.png) | [Page](screenshots/stock-link-styles-a_blue.png) |
| Bold | [Page](screenshots/reader-link-bold.png) | [Page](screenshots/stock-link-styles-a_bold.png) |
| Normal weight | [Page](screenshots/reader-link-no-bold.png) | [Page](screenshots/stock-link-styles-a_not_bold.png) |
| Italic | [Page](screenshots/reader-link-italic.png) | [Page](screenshots/stock-link-styles-a_italic.png) |
| Upright | [Page](screenshots/reader-link-no-italic.png) | [Page](screenshots/stock-link-styles-a_not_italic.png) |
| Always underlined | [Page](screenshots/reader-link-underline.png) | [Page](screenshots/stock-link-styles-a_underline.png) |
| Never underlined | [Page](screenshots/reader-link-no-underline.png) | [Page](screenshots/stock-link-styles-a_not_underline.png) |

Run the pinned native script with absolute paths to the XHTML specimen,
an empty cache directory, a screenshot output directory, and Noto Sans
Regular and Bold. For Reader, use
`cargo test --locked --features kindle --lib reading_style_link`.
Set `READER_LINK_STYLE_CAPTURES` for the page captures and
`READER_SELECTION_VIEWPORT_CAPTURES` for the settings screenshot.
