# Generated reading content

Reader applies publisher `::before` and `::after` to horizontal text in
existing reading paragraphs and their inline elements. The stylesheet can use
quoted text, CSS escapes, `attr(...)`, quote tokens, and the historical
single-colon aliases. [Generated quotation marks](quote-marks.md)
Generated runs inherit the host’s typography and can set font family, size,
weight, slant, line height, decoration, and visibility. `content: none` or
`display: none` suppresses that run. Open **Settings › Style tweaks ›
Pseudo-elements › Disable before/after pseudo elements** to suppress both
sides. The choice can apply to the open book or become its default.
[Pinned KOReader setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L810-L820)

The **Disable first-letter pseudo elements** and **Disable first-line pseudo
elements** options in the same menu each have their own default and book
choice. A generated prefix can remain visible with either typography tweak
disabled. [Publisher initials](first-letter.md) · [Publisher first lines](first-line.md)

Book-specific CSS can also set inline generated text and typography, or hide
an individual pseudo-element. The editor rejects declarations that the
generated layout does not support. [Book CSS limits](style-tweaks.md) ·
[Reader controls](screenshots/reader-pseudo-element-controls.png)

Generated text wraps in the reading column. Search, copying, link targets and
saved positions use the original source bytes. The source projection tracks
the original text around the generated glyphs, including paragraphs where a
publisher hides an inline element. A line carrying only generated content
paints normally and offers no selectable text or caret.
[Layout projection](../src/parley_layout/inline_blocks/generated.rs)

Reader supports inline generated text in ordinary horizontal rendering and
flattened in-page notes. Supported flattened notes can share a line with images
or ruby annotation. Ordinary mixed-media paragraphs, generated block and float
boxes, empty standalone hosts and general boxed inline layouts remain outside
this coverage. CRE treats generated URLs and other unsupported tokens as
placeholders. Reader bounds captured host attributes and inline boundaries per
chapter, and caps generated text at 1,024 hosts and 64 KiB per chapter.
[Flattened note replay](in-page-footnotes.md#generated-text-in-flattened-notes) ·
[Stock generated content parser](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L1493-L1678)

The native replay runs KOReader’s actual toggle on the same chapter. It
compares five source pointers and painted positions, confirms generated text
is absent from stock search and copy, and restores the initial geometry.
Reader exercises converted inline positions, copied text, original link
targets, and a long generated prefix and suffix that occupy their own pages.
[Shared chapter](../tests/fixtures/pseudo-generated.xhtml) ·
[Stock replay](../scripts/koreader-pseudo-generated-check.lua) ·
[Reader tests](../src/parley_layout/pseudo_generated_tests.rs)

| Style | Reader on Mac | KOReader on Mac |
| --- | --- | --- |
| Publisher decoration | [Page](screenshots/reader-pseudo-generated-publisher.png) | [Page](screenshots/stock-pseudo-generated-publisher.png) |
| Before and after disabled | [Page](screenshots/reader-pseudo-generated-off.png) | [Page](screenshots/stock-pseudo-generated-off.png) |

Run the stock replay from the pinned native KOReader runtime with absolute
paths to the chapter, an empty CRE cache, the Noto Sans Regular fixture font,
and a capture folder. Run the Reader checks with
`cargo test --locked --features kindle --lib pseudo_generated`.
Use `READER_PSEUDO_GENERATED_CAPTURES` to save the reading frames and
`READER_SELECTION_VIEWPORT_CAPTURES` to save the menu frame.
