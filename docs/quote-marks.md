# Generated quotation marks

Reader adds quotation marks to `<q>` in the Traditional and HTML Standard
base stylesheets. Author and book CSS can set `open-quote`, `close-quote`,
`no-open-quote`, or `no-close-quote` on a before or after pseudo-element.
The two silent tokens change nesting depth without drawing a mark.
**Disable before/after pseudo elements** removes these marks along with other
generated text. [HTML defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/html5.css#L294-L300) · [Generated text controls](generated-reading-content.md)

Quote depth follows the publisher’s document order, from a before
pseudo-element through its descendants to the after pseudo-element. English,
French, Swiss French, and German have their verified pairs. Each effective
embedded language tag has its own counter. Turning embedded languages off
uses the selected reading language throughout. Deeper nesting alternates
between two pairs, and a word joiner attaches the mark to its neighboring
text. [Language selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L204-L254) · [Quote depth and spacing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L1216-L1259)

A `display: none` subtree or pseudo-element contributes no depth.
`visibility: hidden` still contributes depth while hiding its marks.
An unmatched close uses the inner pair at depth zero and lowers the depth.
Custom CSS `quotes` values leave the language pairs in use, as in the pinned
engine. Reader matches the observed short mixed declaration behavior where
quote tokens are resolved up to the first string or `attr(...)`.
[Pinned token resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L1708-L1782)

Generated marks leave search, copying, links, and saved anchors attached to
the original text. For reflowable EPUBs, Reader can calculate depth from the
preceding spine items before opening a later chapter directly. The prefix
pass runs when the current chapter has a winning quote rule and covers up to
16 preceding items and 512 KiB of preceding XHTML. Outside that budget or
when the prefix cannot be read, Reader starts at depth zero for the chapter.
The depth pass uses the active EPUB note preferences, including Wikipedia
backlink suppression. If an earlier page cannot fit its note band, its
rendering can fall back to ordinary flow. Each independent chapter layout recomputes its bounded
prefix, including layouts in the page-count worker. Other languages use the
English pair. The existing horizontal generated-text limits still apply to
painting. Joined notes keep their source text and currently omit generated
marks from their flattened layout. [Reader tests](../src/book_style/pseudo/quotes/tests.rs)

## Mac simulator comparison

The native replay checks the computed marks and original source pointers,
calls the real style-tweak callback, restores the first frame, and opens a
second EPUB spine item with an incoming depth. Reader checks the same quote
rules, direct-open placement, source selection, and a linked word inside
nested quotes. Each engine uses its own line metrics and glyph spacing.
[XHTML fixture](../tests/fixtures/quote-marks.xhtml) ·
[EPUB spine fixture](../tests/fixtures/quote-spine.epub) ·
[Native replay](../scripts/koreader-quote-marks-check.lua)

| Reading style | Reader | KOReader |
| --- | --- | --- |
| Embedded languages | [Page](screenshots/reader-quote-marks-publisher.png) | [Page](screenshots/stock-quote-marks-publisher.png) |
| Main German with embedded languages off | [Page](screenshots/reader-quote-marks-main-de.png) | [Page](screenshots/stock-quote-marks-main-de.png) |
| Generated marks suppressed | [Page](screenshots/reader-quote-marks-pseudo-off.png) | [Page](screenshots/stock-quote-marks-pseudo-off.png) |

Set `READER_QUOTE_MARK_CAPTURES` and run the `quote_marks` Rust test filter
to recreate Reader’s screenshots. The native replay takes the XHTML
fixture, a cache directory, Noto Sans Regular, a capture directory, and the
EPUB spine fixture.
