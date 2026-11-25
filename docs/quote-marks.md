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
the original text. When a chapter has an active quote rule, Reader walks
earlier EPUB spine items one at a time. Normal forward and nearby backward
turns reuse the quote state already established during that reading session.
The state is tied to the active styles and the open ZIP file's identity,
size, modification time, and change time. Reader checks the file before and
after chapter layout and clears the state when USB storage returns.
Extracted EPUB folders use the same bounded scan without file reuse.

A foreground scan processes at most 24 preceding items and 768 KiB of
their XHTML. The previous range of up to 16 items and 512 KiB still
completes on slow devices. Outside that range Reader checks the 120 ms
deadline between items. An item already being read can finish after the
deadline, and a single predecessor larger than the byte budget cannot
advance the scan.

A cold jump beyond this budget uses default quote depth for that page.
For a local EPUB ZIP, Reader then prepares the exact prefix in the background
and repaints the current passage when it can preserve its anchor. Preparation
begins after the first frame and uses a verified snapshot of at most 8 MiB.
It permits at most 512 preceding spine entries and 16 MiB of decoded preceding
XHTML; repeated references count each time. A five-second deadline is checked
between entries, so an in-progress chapter can finish after that deadline.
Linked stylesheets are read only within their existing per-sheet and remaining
CSS budgets. Reader checks the file identity,
edition, active chapter and effective style before accepting the result.
Repaint waits while a selection, modal, scroll gesture or page turn is active.
Sleep, USB export, book changes and layout changes cancel stale work. On USB
return, Reader retries only if the original file identity still matches.

If the archive or prefix exceeds these limits, or a preceding item cannot be
read or parsed, the current page keeps the bounded foreground result. Reader
does not repeatedly retry the same failed preparation; a changed reading
marker or a lifecycle reset can request it again. Extracted EPUB folders
also keep the foreground path.
Larger archives need a bounded, storage-leased member reader for exact
background preparation. [Quote worker](../src/session/quote_prefixes/worker.rs) ·
[Repaint and lifecycle](../src/app/quote_repair.rs)

Whole-book preparation and background page counting reuse quote state
inside their immutable EPUB snapshot. Each style profile keeps two
checkpoints so page counting can check the visible chapter first and then
scan from the beginning. Note fallback and style changes use separate
profiles. Cancellation and incomplete reads leave no partial checkpoint.
Reader keeps a checkpoint only while it has at most 128 odd-depth language
keys. Larger sets continue with the languages in the requested chapter.
Foreground reading retains 12 recent exact boundaries per style profile.

The prefix uses the active EPUB note preferences, including Wikipedia
backlink suppression. An earlier page that cannot fit its note band may
have used an ordinary-flow fallback when displayed. Other languages use
the English pair. The horizontal generated-text limits still apply to
painting. [Reader tests](../src/book_style/pseudo/quotes/tests.rs)

## Mac simulator comparison

The native replay checks the computed marks and original source pointers,
calls the real style-tweak callback, restores the first frame, and opens a
second EPUB spine item with an incoming depth. A longer EPUB opens the
21st spine item after 621,448 bytes of preceding XHTML. Both engines draw
the inner marks in its first paragraph and return to the outer marks after
the matching close. Reader also checks source selection, a linked word
inside nested quotes, and snapshot reuse across witness-first indexing,
style retries, and cancellation. The foreground regression checks direct
jump fallback, resumption, back and forward turns, and file replacement.
Each engine uses its own line metrics and glyph spacing.
[XHTML fixture](../tests/fixtures/quote-marks.xhtml) ·
[EPUB spine fixture](../tests/fixtures/quote-spine.epub) ·
[Long EPUB spine fixture](../tests/fixtures/quote-spine-long.epub) ·
[Native replay](../scripts/koreader-quote-marks-check.lua)

| Reading style | Reader | KOReader |
| --- | --- | --- |
| Embedded languages | [Page](screenshots/reader-quote-marks-publisher.png) | [Page](screenshots/stock-quote-marks-publisher.png) |
| Main German with embedded languages off | [Page](screenshots/reader-quote-marks-main-de.png) | [Page](screenshots/stock-quote-marks-main-de.png) |
| Generated marks suppressed | [Page](screenshots/reader-quote-marks-pseudo-off.png) | [Page](screenshots/stock-quote-marks-pseudo-off.png) |
| Direct jump to spine item 21 | [Page](screenshots/reader-quote-marks-spine-long.png) | [Page](screenshots/stock-quote-marks-spine-long.png) |

Set `READER_QUOTE_MARK_CAPTURES` and run the `quote_marks` Rust test filter
to recreate Reader’s screenshots. The native replay takes the XHTML
fixture, a cache directory, Noto Sans Regular, a capture directory, and the
two EPUB spine fixtures, with the longer fixture last.
