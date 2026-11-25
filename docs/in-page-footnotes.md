# In-page EPUB footnote pagination

Open **Settings › Style tweaks › In-page EPUB footnotes** in a reflowable
EPUB. Choose **On for this book**, **Off for this book**, or **Use default**.
**Use on all books** enables the live default for EPUBs using it. The master
style switch suspends the effect and retains the choices.

In supported single-page horizontal reading, Reader reserves space for the note
and its separator at the bottom and carries long notes across page turns. The original
note remains in the book flow. The in-page copy uses the same source positions,
so word selection, highlighting, backlink hits and saved locations use the
original book text. Repeated references to the same note share one copy on a
page. Letter spacing, the reading font and margins still come from the normal
book layout. [Page composer](../src/footnotes/in_page.rs) · [Reading session](../src/session.rs)

The pinned KOReader `In-page EPUB footnotes` tweak keeps each original note in
its canonical position and adds a separate copy to the pages that refer to it.
On the test book, KOReader keeps the first short note in the bottom band and
wraps the long note onto continuation pages, then resumes the chapter body.
Reader tests the same behavior with its own shaper, including restoration on a
note-only page while the canonical fragment continues to jump to the original
location. Line wraps and exact page breaks reflect each engine's layout.
[KOReader's EPUB tweak](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L920-L943) ·
[Pinned page splitter](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvpagesplitter.cpp#L810-L1340)

The compositor covers explicit EPUB and ARIA notes referenced from the same
spine chapter, including notes between paragraphs. Standalone and inline
illustrations remain in their original body or note flow. Each artwork area
stays on one page, its links and opening position use its source identity, and
ordinary forced page breaks still start a new page.
Overlapping glyphs within a text band stay together. A note whose boundaries
overlap adjacent content, an illustration without a source line, nested or
cross-spine notes, tables, complex publisher breaks,
FictionBook, facing pages, continuous reading and custom flows use the normal
layout. The reading position uses the same source anchor when the layout changes.

The Mac workflow exercises the visible controls, book defaults and overrides,
reopen, master switch, and facing and continuous layout fallback. The pinned
KOReader replay invokes the stock tap and long-press actions and reloads the
saved edition override before checking the native rendered note bands.
[Stock style controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerstyletweak.lua#L509-L610) ·
[Mac controls](screenshots/in-page-footnote-style.png) ·
[Mac reading page](screenshots/in-page-footnote-reading.png)

[Reader short note](screenshots/in-page-reader-band.png) ·
[KOReader short note](screenshots/in-page-stock-band.png) ·
[Reader continuation](screenshots/in-page-reader-note-only.png) ·
[KOReader continuation](screenshots/in-page-stock-note-only.png)

The authored comparison also places an explicit note between narrative
paragraphs and includes linked artwork in a separate chapter. Both engines
show the interleaved note at its reference and keep the original note before
the following prose. Reader keeps the illustrated note and the linked diagram
on a single page at the measured viewport. The native engine uses a different
artwork scale and places the illustrated note and diagram on adjacent pages.
[Reader interleaved note](screenshots/in-page-reader-note-interleaved-band.png) ·
[KOReader interleaved note](screenshots/in-page-stock-note-interleaved-band.png) ·
[Reader illustrated page](screenshots/in-page-reader-note-illustrated-page.png) ·
[KOReader note band](screenshots/in-page-stock-note-illustrated-band.png) ·
[KOReader diagram](screenshots/in-page-stock-note-illustrated-page.png)

Mac verification covered 14 chapters across two unchanged original EPUBs.
Reader composed 70 note bands across eight chapters, including all six
illustrated chapters, and painted the first band in each of those chapters.
All 63 visible canonical note anchors matched the ordinary layout. The pinned
KOReader Book-mode run also observed bands in all eight chapters. Both
engines kept the other six chapters in the ordinary layout because the
publisher stylesheet hides their note targets. The source-file hashes matched
before and after both runs. [Private-corpus test](../src/footnotes/in_page.rs)

## Footnote size

In **Style tweaks › In-page footnote size**, choose the book’s own size or use
the live default. The choices include the 80% legacy setting, publisher sizes,
and block sizes of 100%, 90%, 85%, 80%, 75%, 70%, and 65%. Changing the default
keeps a book’s explicit selection. The style switch retains the preference
when notes return to ordinary reading.

For the supported EPUB notes, the legacy setting changes the note container
and lets explicit sizes on nested blocks take precedence. The percentage
options apply to note blocks, with relative sizing preserved for inline text
such as superscripts. The same layout supplies each bottom band and the
canonical note. The normal body stays at the reading size.
[Reader size controls](screenshots/in-page-footnote-size-choices.png) ·
[Size controls in landscape and dark mode](screenshots/in-page-footnote-size-landscape.png)

The pinned KOReader menu replay compares every size on the same authored
EPUB. Its selected note text measures 229 px at 100% and 149 px at 65%,
with ordinary body text at 220 px in both cases. Reader measures 230 px,
149 px, and 221 px. An inline note label scales with its block, and the
publisher’s explicit block size survives the legacy setting. The Mac
tests show the note in both locations and check save and reopen through
the actual size menu.
Other note formats and physical E Ink checks remain open for this control.
[Reader 100%](screenshots/in-page-reader-note-size-100.png) ·
[Reader 65%](screenshots/in-page-reader-note-size-65.png) ·
[KOReader 100%](screenshots/in-page-stock-note-size-100.png) ·
[KOReader 65%](screenshots/in-page-stock-note-size-65.png)

## Classic markers and note formatting

In **Style tweaks › More footnote options**, enable **In-page classic
footnotes** for an EPUB that uses class names such as `footnote`, `fn`,
`fnote`, `duokan-footnote-item`, `przypis`, or `voetnoten`. Reader follows
the source ID inside a marked container and copies the whole note into the
page band. The source link still targets its original position. A class
name must match a recognized token. An unreferenced class alone does not
create a band. [Reader controls](screenshots/classic-footnote-options.png)

**Extend notes until next entry** includes consecutive supported sibling
blocks before the next marked note. **Extend notes until next heading**
also stops before a following heading. These choices are mutually
exclusive for the book. **No footnote indentation**, **Justify footnote
text**, **Remove footnote block spacing**, and **Regularize footnote text**
apply to the note and its continuation. Regularizing uses the surrounding
note text’s size, line height and baseline. It removes superscript sizing
and positioning. The controls save the choice for the edition or as a live
default.

The stock replay exercises the actual tweak callbacks and the same 80%
legacy size as Reader. The note’s first word moved from x=96 to x=32 in
both engines when the fixes were applied. The raised label measured
57→104 px in KOReader and 60→111 px in Reader. The ordinary body stayed the
same size. Reader checks both continuation boundaries, the canonical IDs,
and reopening at the original passage. The authored EPUB remains read-only.
[Reader baseline](screenshots/in-page-reader-classic-note-base.png) ·
[Reader fix-up](screenshots/in-page-reader-classic-note-fixed.png) ·
[KOReader baseline](screenshots/in-page-stock-classic-note-base.png) ·
[KOReader fix-up](screenshots/in-page-stock-classic-note-fixed.png) ·
[Pinned continuation detection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L5644-L5674)

Run the replay below with `classic` as its fifth argument to exercise this
chapter. Physical E Ink verification remains pending.

**Remove cosmetic footnote spacing** removes margins and padding on inline
note elements. Book-mode layouts honor authored inline boxes, so a padded
label takes up less horizontal and vertical space when the option is on.
The box keeps its text and backlink hit target. The reference paragraph and
body inline boxes keep their original geometry. The word after the box
moves 88 px in both engines. Reader measures x=285→197 using its saved
legacy note size. KOReader measures x=346→258 using publisher sizes.
[Reader before](screenshots/reader-note-cosmetic-before.png) ·
[Reader after](screenshots/reader-note-cosmetic-after.png) ·
[KOReader before](screenshots/stock-note-cosmetic-before.png) ·
[KOReader after](screenshots/stock-note-cosmetic-after.png)

## Saved Wikipedia EPUBs

**Style tweaks › More footnote options › In-page Wikipedia footnotes** uses
the reference pattern from a saved EPUB. A link to a direct ordered list item
in `ol.references` displays that numbered item in the page band. Direct
children marked `noprint` or `mw-cite-backlink` are omitted. Nested text and
other links in the reference stay selectable. The original list stays
accessible through source links, including after saving and reopening.
The tests use an authored EPUB with the Wikipedia reference markup. A
chapter with an unsupported reference table returns to its ordinary layout,
including its backlinks. Local Wikipedia exports with different structures
can still be read with ordinary layout.
[Pinned KOReader selectors](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L945-L960) ·
[Reader controls](screenshots/advanced-note-options.png) ·
[Reader reference band](screenshots/reader-wikipedia-note-band.png) ·
[KOReader reference band](screenshots/stock-wikipedia-note-band.png) ·
[Reader canonical list](screenshots/reader-wikipedia-note-canonical.png) ·
[KOReader canonical list](screenshots/stock-wikipedia-note-canonical.png)

## Combined non-linear footnotes

With in-page EPUB footnotes enabled, use **Style tweaks › More footnote
options › Combine footnotes in a non-linear flow**. In **Navigation**,
turn on **Hide non-linear fragments**. Consecutive original note blocks
within the same source group form a single supplemental range. Regular
page turns skip the original note pages while their copies remain in the
footnote bands. Links and explicit Go to page jumps can still reach the
original pages. Turn from an original to return to the adjacent regular
page or use the reading history to return to the reference.

The toggle preserves the source position and the selected state is saved
for the edition. The chapter footer, reading time and progress skip the
originals in supplemental note ranges. Whole-book progress
uses Reader’s existing weighted chapter estimate. Custom hidden flows
take precedence, and setup is available from chapters with supported
note bands. The change applies to supported horizontal single-page
EPUB layouts using same-chapter links. Continuous, facing-page and
unsupported note layouts retain their ordinary source pagination.
[KOReader note grouping](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L1168-L1179) ·
[KOReader navigation](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L392-L460)

The pinned native replay enables both stock tweaks and runs the stock
flow toggle. It verifies six regular pages out of nine total, adjacent
notes sharing a flow, separate note groups, direct canonical locations,
and forward and backward turns over the intervening originals. Reader’s
simulator verifies the corresponding routes, selectable note copies,
the original position after reopening, and unchanged EPUB bytes.
[Reader reference band](screenshots/reader-combined-note-band.png) ·
[Reader originals](screenshots/reader-combined-note-originals.png) ·
[KOReader reference band](screenshots/stock-combined-note-band.png) ·
[KOReader originals](screenshots/stock-combined-note-originals.png)

## Inline all footnote content

Open **Style tweaks › More footnote options › Inline all footnote content**.
The note number, paragraphs and text list items share the same flow inside
the note. This removes the extra line for a number stored in its own
paragraph. Nested list items and items containing several paragraphs use
the same flow. Generated markers disappear from flattened list items.
Reader uses a space between adjacent source blocks.
The page band and canonical note use the same layout. Tapping or selecting
text on their shared lines resolves to the original block and byte offset.
Selection spanning the two paragraphs keeps their original text and newline,
and backlinks keep their targets.
[KOReader inline note tweak](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L1129-L1148) ·
[Reader setting](screenshots/whole-note-inline-settings.png) ·
[Reader before](screenshots/reader-note-inline-before.png) ·
[Reader after](screenshots/reader-note-inline-after.png) ·
[Reader original note](screenshots/reader-note-inline-original.png)

Formatting applies to supported horizontal in-page notes. Inline and
standalone pictures can flow with the surrounding note text. Tables, ruby,
inline boxes, preformatted blocks, and authored page breaks inside a note
keep their original layout. Reader also keeps the block layout when the
note’s own container is a list item. In that case, KOReader flattens the
paragraphs and moves the generated marker to the start of the shared
line, outside searchable text. The original backlink still resolves.
[Stock inside-marker rendering](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L3915-L3935)
If the chapter cannot fit the joined notes, Reader retries the original
layout. The publication stays unchanged.
Reader stores this option with the edition's existing style settings.

The pinned native replay uses the actual KOReader style callback. On the
authored EPUB, its canonical note number and text change from y=72 and
y=112 to the same line at y=72. The body text stays at y=32.
Run the replay below with `inline-all` as its fifth argument. Physical
E Ink verification remains pending.
[KOReader before](screenshots/stock-note-inline-before.png) ·
[KOReader after](screenshots/stock-note-inline-after.png) ·
[KOReader original note](screenshots/stock-note-inline-original.png)

The pinned `inline-list` replay measures a paragraph and two list items at
y=72, 112 and 152 in the original note. With the tweak on, all three start
at y=72 and the generated bullets disappear. The ordinary body stays at
y=32. The Reader simulator verifies shared lines in the reference band
and the original, selection across list items, the canonical fragment IDs,
and the backlink hit from the second item.
[Reader list before](screenshots/reader-note-inline-list-before.png) ·
[Reader list after](screenshots/reader-note-inline-list-after.png) ·
[Reader original list](screenshots/reader-note-inline-list-original.png) ·
[KOReader list before](screenshots/stock-note-inline-list-before.png) ·
[KOReader list after](screenshots/stock-note-inline-list-after.png) ·
[KOReader original list](screenshots/stock-note-inline-list-original.png)

The pinned `inline-nested` replay checks a bulleted item with two
paragraphs and a nested numbered item. The five source blocks start at
y=72, 112, 152, 192 and 232 before flattening. With the tweak on, they
share y=72 in their original order. The body stays at y=32. The native
replay taps the original item's backlink and checks its reference target.
The Reader simulator checks search matches, selection across the parent
paragraphs and child item, and the child's backlink in both locations.
[Reader nested list before](screenshots/reader-note-inline-nested-before.png) ·
[Reader nested list after](screenshots/reader-note-inline-nested-after.png) ·
[Reader original nested list](screenshots/reader-note-inline-nested-original.png) ·
[KOReader nested list before](screenshots/stock-note-inline-nested-before.png) ·
[KOReader nested list after](screenshots/stock-note-inline-nested-after.png) ·
[KOReader original nested list](screenshots/stock-note-inline-nested-original.png)

The pinned `inline-image` replay joins a legend with a paragraph containing
a linked diagram. Its caption moves from y=112 to the legend's line at
y=72, with the body unchanged at y=32. The diagram moves from x=81 to x=366.
The replay verifies all 56 × 36 painted pixels before and after, the same
pixels in the bottom-band copy, and that tapping the original diagram
returns the same link and source PNG. Reader's simulator also checks
the image and its link in both locations, along with the canonical image
location, source text and word selection.
[Reader illustrated note before](screenshots/reader-note-inline-image-before.png) ·
[Reader illustrated note after](screenshots/reader-note-inline-image-after.png) ·
[Reader canonical illustration](screenshots/reader-note-inline-image-original.png) ·
[KOReader illustrated note before](screenshots/stock-note-inline-image-before.png) ·
[KOReader illustrated note after](screenshots/stock-note-inline-image-after.png) ·
[KOReader canonical illustration](screenshots/stock-note-inline-image-original.png)

The `inline-block-image` replay uses a linked picture in its own source
paragraph. KOReader paints both standalone copies at 216 × 138 pixels
and matches all 29,808 pixels between them. Whole-note flattening places
both copies inline at 108 × 69 pixels. In both modes, tapping the
canonical picture returns its link and the original 248-byte PNG.
The stock strip copy is paint-only. Reader lets you open the picture
and follow its link from both locations. Reader keeps its usual standalone
centering and uses the authored 108 × 69 size in both modes. The Mac tests
resolve the default first picture and the explicit ordinals of two
pictures that share a source block.
[Reader standalone note](screenshots/reader-note-inline-block-image-before.png) ·
[Reader flattened note](screenshots/reader-note-inline-block-image-after.png) ·
[Reader canonical picture](screenshots/reader-note-inline-block-image-original.png) ·
[KOReader standalone note](screenshots/stock-note-inline-block-image-before.png) ·
[KOReader flattened note](screenshots/stock-note-inline-block-image-after.png) ·
[KOReader canonical picture](screenshots/stock-note-inline-block-image-original.png) ·
[Stock strip rendering](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L2172-L2204) ·
[Stock main-page hit testing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L2597-L2666)

## Replay on macOS

Run the Reader tests on the [deterministic authored EPUB](../tests/fixtures/in-page-footnotes.epub).
Set `stock` to the absolute directory of the pinned native KOReader build with
`runtime/koreader` and `build/staging` beneath it. The native script enables the
actual stock Book-mode CSS tweak and checks separator positions with the same
book, font, viewport and margins. Both runs use only local files.

```sh
reader="$PWD"
scratch=$(mktemp -d)
mkdir -p "$scratch/profile" "$scratch/cache" "$scratch/captures"
READER_SELECTION_VIEWPORT_CAPTURES="$scratch/captures" \
  READER_IN_PAGE_FOOTNOTE_CAPTURES="$scratch/captures" \
  cargo test --locked --features kindle --lib in_page
READER_SELECTION_VIEWPORT_CAPTURES="$scratch/captures" \
  READER_IN_PAGE_FOOTNOTE_CAPTURES="$scratch/captures" \
  cargo test --locked --features kindle --lib advanced_note
READER_SELECTION_VIEWPORT_CAPTURES="$scratch/captures" \
  READER_IN_PAGE_FOOTNOTE_CAPTURES="$scratch/captures" \
  cargo test --locked --features kindle --lib whole_note_inline
cd "$stock/runtime/koreader"
KO_HOME="$scratch/profile" KO_DYLD_PREFIX="$stock/build/staging" \
  SDL_VIDEODRIVER=dummy ./luajit "$reader/scripts/koreader-in-page-footnotes-check.lua" \
  "$reader/tests/fixtures/in-page-footnotes.epub" "$scratch/cache" \
  "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf" "$scratch/captures"
```

Pass `cosmetic`, `wikipedia`, `inline-all`, `inline-list`, `inline-nested`,
`inline-image`, or `inline-block-image` as the fifth argument to select an
advanced style replay.

Run the combined note flow checks with the same stock runtime and scratch
paths.

```sh
cd "$reader"
READER_NONLINEAR_NOTE_BAND="$scratch/captures/reader-combined-note-band.png" \
  READER_NONLINEAR_NOTE_ORIGINALS="$scratch/captures/reader-combined-note-originals.png" \
  cargo test --locked --features kindle --lib combined_footnote_flows
cd "$stock/runtime/koreader"
KO_HOME="$scratch/profile" KO_DYLD_PREFIX="$stock/build/staging" \
  SDL_VIDEODRIVER=dummy ./luajit "$reader/scripts/koreader-nonlinear-note-flows-check.lua" \
  "$reader/tests/fixtures/nonlinear-note-flows.epub" "$scratch/cache" \
  "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf" "$scratch/captures"
```

For local originals, set `READER_IN_PAGE_FOOTNOTE_ORIGINALS` to a private JSON
array of book records. Each book record contains its `path` and a `chapters`
array. Each chapter entry contains its archive `href`, an array of note IDs in
`notes`, and the numeric `media` count. Run the opt-in test with
`cargo test --locked --features kindle --lib original_epub_in_page_notes -- --ignored`.
The test reports aggregate counts and skips screenshot capture for private books.
