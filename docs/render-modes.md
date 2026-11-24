# KOReader publisher block rendering modes

KOReader’s four **View → Render Mode** choices change CREngine’s box
construction, layout, painting and pagination flags. The pinned ReaderTypeset
handler was exercised against its native CREngine in the Mac SDL runtime.
The probe records the chosen bitmask and checks the **UpdatePos** event.
[Stock setting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L317-L334) ·
[ReaderTypeset flags](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypeset.lua#L378-L447)

| Mode | Native mask | Mask with older-DOM compatibility | Enabled features |
| --- | --- | --- | --- |
| Legacy | `0x00000000` | `0x00000000` | Original block rendering path |
| Flat | `0x03030031` | `0x00030031` | Enhanced blocks, vertical margin collapse with individual negative margins, W3C content-box sizing, prepared float boxes rendered inline, inline-block boxes and anonymous table repair |
| Book | `0x03375131` | `0x00375131` | Flat features, automatic horizontal margin alignment, limited CSS width enforcement, floating float boxes, own-float handling and exact float footprints |
| Web | `0x7FFFFFFF` | `0x7CFFFFFF` | All bits through bit 30, including empty forced breaks, net negative collapsed margins, negative horizontal margins, block and page overflow, and absolute CSS width and height |

The older-DOM path removes bits `0x01000000` and `0x02000000` for
inline-block and anonymous table boxing. The saved per-book option takes
precedence when opening a book. A previously opened book with a saved
XPointer and no explicit mode uses Legacy when its book settings have
never been reset. A new book uses the global
setting, with Web as the runtime fallback. The config-dialog row also
declares an option default of Book. [Native flag definitions](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/include/lvrend.h#L199-L237) ·
[Book-open defaults](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypeset.lua#L49-L64)

## Authored fixture results

The same two-spine EPUB was opened in all four modes at 1264 × 1606,
96 content DPI, 54-pixel reading margins and nominal 40-pixel text.
Line distances use source XPointer positions. Box widths use painted
gray bands, with a distinct shade for each test box. The measurements
below apply to these authored declarations.

| Test | Legacy | Flat | Book | Web |
| --- | ---: | ---: | ---: | ---: |
| Joined positive margins, line pitch | 184 px | 144 px | 144 px | 144 px |
| Positive and negative margin pair, line pitch | 136 px | 103 px | 103 px | 103 px |
| Net negative collapsed gap, line pitch | 71 px | 64 px | 64 px | 46 px |
| Centered 42% container, painted width | 1,156 px | 1,156 px | 1,156 px | 485 px |
| Fixed 260 px box, painted width | 1,156 px | 1,156 px | 1,156 px | 260 px |
| Fixed-height box to following text, line pitch | 64 px | 64 px | 64 px | 128 px |
| Following negative horizontal margin, screen x | 54 px | 53 px | 53 px | 33 px |
| Neighbor offset relative to floated card, x and y | (0, 80) px | (0, 64) px | (416, 0) px | (416, 0) px |
| Following text after the inline-block, screen x | 817 px | 936 px | 936 px | 936 px |
| Incomplete CSS table | Cells omitted | Same row | Same row | Same row, narrower |
| Page advance across two empty forced breaks | 3 | 1 | 1 | 3 |
| Whole EPUB pages | 6 | 4 | 4 | 6 |

The same XPointer for the text next to the floated card survived live
mode changes from Web through Legacy, Flat and Book and back to Web.
Its screen x coordinate went from 470 to 54, 54, 470 and 470,
and the final Web rectangle matched the original. The audit uses one
stable passage. The older-DOM mask is checked separately.
[Legacy](screenshots/render-modes-stock-legacy.png) ·
[Flat](screenshots/render-modes-stock-flat.png) ·
[Book](screenshots/render-modes-stock-book.png) ·
[Web](screenshots/render-modes-stock-web.png)

## Reader comparison

Reader’s existing Parley normal reading path drew the two positive-margin
markers one regular line apart at 68 pixels. The floated text stayed on
separate lines, both positioned at x=54. The explicit-width gray band
was 1,156 pixels. The CSS cells stayed visible in separate paragraphs.
The empty breaks advanced one page with the following forced break
advancing another page. Parley provided usable source selection
rectangles during these measurements.
[Parley capture](screenshots/render-modes-reader-parley.png)

The diagnostic Blitz renderer produced 486-pixel centered boxes and an
exact 260-pixel absolute-width band. The float neighbor remained below
the floated card, as seen in the capture. Blitz put the second spine’s
forced breaks on one viewport page and returned zero of the 22 selection
rectangles on this specimen. Reader’s existing Blitz path builds anchor
lines when it can pair text, but touch source hit-testing and
source-selection rectangles use the Parley or vertical engines.
[Blitz capture](screenshots/render-modes-reader-blitz.png) ·
[Reader layout engines](../src/lib.rs) · [Selection geometry](../src/selection.rs)

| Mechanism | Reader work required for the observed mode behavior |
| --- | --- |
| Legacy | Match the legacy inline model and the old handling of incomplete CSS tables. Separate publisher margins are measured below. |
| Flat | Extend the measured empty-box geometry below to nested and margin-only boxes, and add the remaining parent collapse, inline float, and anonymous box behavior. |
| Book | Extend the horizontal float model below to complex boxes and inline floats, and implement the remaining style-width and auto-margin behavior |
| Web | Extend signed margins and float geometry, implement full publisher box sizing and automatic placement, and extend physical break-page painting to complex boxes |

The four options depend on independent box geometry, pagination and
source mappings. A Reader renderer choice would currently change several
of these behaviors together. The **Render Mode** matrix item remains open.

## Horizontal block spacing

The horizontal Parley provider now has three internal margin policies. A
fourth policy, `Ignore`, remains the Reader default. The policies use the
cascaded top and bottom margins of source elements, resolve relative units
with the existing font and box metrics, and assign each outer edge once. A
section with several paragraphs receives its margin at the edges of the
section. Suppressed elements contribute no spacing, while
`visibility:hidden` still occupies space.

| Stock workflow | Reader policy | Added pitch for positive / mixed / net-negative pairs | Reader measured pitch | Pinned stock pitch |
| --- | --- | ---: | ---: | ---: |
| Legacy | `Separate` | 120 / 72 / 7 px | 187.8 / 139.8 / 74.8 px | 184 / 136 / 71 px |
| Flat or Book | `Collapse` | 80 / 39 / 0 px | 147.8 / 106.8 / 67.8 px | 144 / 103 / 64 px |
| Web | `CollapseNegative` | 80 / 39 / −18 px | 147.8 / 106.8 / 49.8 px | 144 / 103 / 46 px |

The margin contributions match the pinned executable for each pair.
Parley measures 67.8 pixels between the control lines at this font size.
CREngine measures 64 pixels. The provider preserves Parley’s existing
ordinary text metrics.
[Separate margins capture](screenshots/render-modes-reader-separate-margin.png) ·
[Signed collapse capture](screenshots/render-modes-reader-collapsed-margin.png)

Collapse combines the largest positive edge and the most negative edge of
adjacent sibling blocks. `Collapse` floors a net negative result at zero.
`CollapseNegative` allows that gap to reduce the pitch. Parent and child
edges in the current provider remain independent. An intervening visible
empty element or a table ends a sibling pair. Table cell tracks retain
their existing layout, and a positive boundary gap can push the entire
following grid down. The provider bounds a gap to half a viewport, prevents
extreme negative values from reversing text origins, and drops terminal
spine margins. Collapsing through empty or nested boxes, complex float and overflow
layout, and vertical writing remain pending. Reader has no saved Render
Mode preference or control yet.

The tests reopen a serialized text selection in all policies, hit-test the
same word at its new geometry, compare the default frames, and resolve a
saved source range after it moves to a later screen page. The margin
projection preserves source revisions and text offsets.
[Provider and tests](../src/block_margins.rs)

## Horizontal floats and clearance

Reader has internal `ContentHeight` and `PublisherHeight` float policies
for horizontal Parley flow. The default is `Ignore`. A supported float uses
the physical left or right edge of the reading column and reserves its
space in the text around it. Parley shapes each paragraph once and breaks
each line against the current footprint. A long paragraph returns to full
width within the same paragraph when the float ends. `clear:left`,
`clear:right`, and `clear:both` move the next block below the relevant
footprints, while the opposite side can continue excluding text.

The pinned Book and Web modes both place the 36%-wide card at the same
edge. Book gives it the height of its content. Web honors the explicit
CSS height in the fixture. For two opposing Web floats, clearing left
leaves the right exclusion in place. The coordinate and painted-height
assertions run through the actual pinned macOS SDL renderer.
[CREngine float and clearance code](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6777-L6830) ·
[Stock probe](../scripts/koreader-render-floats-check.lua)

| Measurement at 1264 × 1606 | Stock Book | Reader `ContentHeight` | Stock Web | Reader `PublisherHeight` |
| --- | ---: | ---: | ---: | ---: |
| Left card and adjacent prose, screen x | 54 / 470 | 54 / 470 | 54 / 470 | 54 / 470 |
| Right card and adjacent prose, screen x | 794 / 54 | 794 / 54 | 794 / 54 | 794 / 54 |
| Card and adjacent prose, first screen y | 54 / 54 | 54 / 54 | 54 / 54 | 54 / 54 |
| Single card painted height | 64 px | 48 px | 80 px | 80 px |
| Matching clearance after the single card, screen y | 118 | 121 | 134 | 134 |
| Lines using the left exclusion in the long paragraph | 1 | 1 | 2 | 2 |

For the opposing 160- and 200-pixel floats, each Web-style provider
places `clear:left` at screen y=214 and limits that line to the remaining
right edge. With both sides cleared, KOReader puts the next line at y=278
and Parley puts it at y=281. Parley retains its own line metrics and
hyphenation through the exclusion. The Web specimen wraps the long
paragraph into six lines in KOReader and seven in Reader.
[KOReader opposing floats](screenshots/render-floats-stock-opposed.png) ·
[Reader opposing floats](screenshots/render-floats-reader-opposed.png) ·
[Reader right float](screenshots/render-floats-reader-right.png) ·
[Reader width recovery](screenshots/render-floats-reader-wrap.png)

The current provider supports simple text cards with explicit widths and
at most one simultaneous float on each side. It uses horizontal physical
sides. It keeps publisher-defined percentages for width and uses a
definite height in `PublisherHeight`. Nested and multiblock floats, inline
floats, percentage heights, independent formatting contexts and interactions
with collapsed outer margins require further work. Tables, illustrated,
indented, decorated and ruby blocks start below active floats. A chapter
with an unsupported float shape uses normal flow for that float. If an
otherwise supported placement is too tall to paginate safely, Reader
retries the chapter in ordinary flow and retains the original anchors.
The fixture checks link destinations, caret and quote positions, restored
coordinates, and an authored overflow that forces this rollback.
[Provider and tests](../src/float_layout.rs)

## Forced breaks through empty boxes

The horizontal paginator projects empty-block edges from the source tree
onto the boundary before following content. It groups adjoining siblings
and attaches an empty wrapper’s leading and trailing rules to its first
and last leaf. `display:none` removes the empty box and its break rules.
Breaks around actual text still use the existing line and keep rules.

Reader's `Current` default and `Coalesce` policy use a single forced page
at the following text when empty blocks contribute adjacent directives.
`PreserveStructure` retains distinct leading and trailing breaks on a
zero-size leaf. `PreserveSized` retains both edges on a leaf with padding,
vertical borders, or a definite positive height. It folds two edges of a
zero-size leaf into one. An adjoining sibling can share an outgoing edge
as measured below. These are internal Parley policies.

| Authored case | Legacy / `PreserveStructure` | Flat and Book / `Coalesce` | Web / `PreserveSized` |
| --- | ---: | ---: | ---: |
| Two bare empty boxes with forced breaks on both edges | 3 | 1 | 2 |
| Two padded or bordered empty boxes | 3 | 1 | 3 |
| Two boxes with only margins or only backgrounds | 3 | 1 | 2 |
| Bare box followed by padded box | 3 | 1 | 3 |
| Padded box followed by bare box | 3 | 1 | 2 |
| Two branches of nested, padded empty boxes | 4 | 1 | 4 |
| A padded `after` box, a bare unforced sibling, a padded `before` box | 1 | 1 | 1 |
| Two suppressed boxes | 0 | 0 | 0 |
| Real paragraph with forced breaks on both edges | 2 | 2 | 2 |

Values are page advances between text markers. The pinned SDL runner
checks 25 authored structures in each of the four modes. On the original
Render Mode specimen, the next text advances three pages in Legacy and
Web and one in Flat and Book. The subsequent nonempty forced break
advances one page in all four modes. [Stock comparison](../scripts/koreader-empty-breaks-check.lua) ·
[Source topology and tests](../src/publisher_breaks/empty.rs)

The additional slots in Reader are blank physical pages in paged mode.
They have no fabricated text anchor. Reopening a blank slot uses the
stored page index, and reopening the following text uses its original
source anchor after a policy change. Page intervals use the same document
coordinate for adjacent blank slots. Continuous reading follows source
coordinates, so it does not stop on these slots. Reader bounds the number
of additional slots per chapter.

The topology-only policy places following text at the ordinary top margin
and leaves the inserted slots white. The separate physical policy below
positions and paints supported empty boxes. Leading and trailing empty
runs, boxes inside tables, floats and lists, and more complex forced,
keep and collapsed-margin interactions remain out of scope.
[Stock decorated intermediate page](screenshots/empty-break-stock-web-intermediate.png) ·
[Reader intermediate page](screenshots/empty-break-reader-intermediate.png) ·
[Reader following text](screenshots/empty-break-reader-following.png)

### Physical space and paint

The physical Parley policy reserves publisher padding, supported borders,
margins and definite height for direct sibling empty boxes between two
ordinary text blocks. It positions the existing forced boundaries in that
space, then draws opaque solid top and bottom borders and grayscale
backgrounds. Text offsets, fragment identifiers and selections still use
the original source. The reader can paint an intermediate page that has
no source text anchor.

The legacy policy uses separate margins and the older empty-block height
rule. The compact policy collapses adjoining margins, keeps the space on
the surviving page, and uses the box’s padding and borders for its height.
The sized policy honors content height and drops the outgoing margin when
a sized box forces a break after itself. The page starts where the pinned
renderer places the surviving separator, so top margins can still move a
border or the following text down on screen.

| Authored run | Legacy space and following y | Flat or Book space and following y | Web space and following y |
| --- | ---: | ---: | ---: |
| Two original decorated boxes | 124 px / 54 | 104 px / 158 | 84 px / 54 |
| Two asymmetric boxes with both edges forced | 94 px / 54 | 81 px / 135 | 62 px / 54 |
| Same boxes with definite publisher heights | 130 px / 54 | 81 px / 135 | 114 px / 54 |

The space column is measured separately from the ordinary text pitch.
Reader's pixel snap adds 0.20 pixels at the start of these authored runs.
The measured page-relative bands and following text coordinates match
the pinned renderer in the supported cases.
Coordinates are native screen positions in the 1264 × 1606 specimen.
The asymmetric fixture uses distinct grayscale tones for each solid
border and background. The native probe checks the position, thickness,
and tone of each uninterrupted band at screen x=1201 with the same setup
used for the pagination probe. It also checks before-only and after-only
rules and preserves the original 25 structures.

Reader currently projects full-width direct siblings with nonnegative
margins and compatible text neighbors. Nested empty boxes, unsized
collapsed-margin runs in the enhanced policies, side and patterned
borders, percentage heights, and complex keeps still use the existing
topology. Physical cuts also depend on safe intervals in the existing
paginator. The provider decisions are opt-in, and the settings interface
does not expose the unfinished Render Mode choices.
[Physical layout provider](../src/publisher_breaks/empty/physical.rs) ·
[Pinned comparison](../scripts/koreader-empty-breaks-check.lua) ·
[KOReader Book](screenshots/empty-geometry-stock-book.png) ·
[Reader compact](screenshots/empty-geometry-reader-compact.png) ·
[KOReader Web](screenshots/empty-geometry-stock-web.png) ·
[Reader sized](screenshots/empty-geometry-reader-sized.png)

## Reproduce

The EPUB uses authored CSS and text. Generate it with
`python3 scripts/generate-render-modes-fixture.py`. Run the stock probe
with the pinned SDL runtime working directory, a fresh KOReader settings
directory, the EPUB path, an empty native cache directory, the Noto
Sans font path and a capture directory. The script prints the flags,
source positions, bands and live-switch results.
[Stock probe](../scripts/koreader-render-modes-check.lua) ·
[Pinned runtime setup](koreader-sdl-reference.md#scope-and-reproducibility)

Run the Reader comparison from the repository root.

```sh
cargo run --locked --example verify_render_modes -- \
  tests/fixtures/render-modes.epub tests/fixtures/fonts/NotoSans-Regular.ttf
```

Pass a final directory argument to write the two Reader viewport captures
as grayscale PGM. [Fixture](../tests/fixtures/render-modes.epub) ·
[Reader verifier](../examples/verify_render_modes.rs)

Run `cargo test --locked --lib block_margins -- --nocapture` for the block
spacing and resume assertions. Set `READER_BLOCK_MARGINS_CAPTURE_DIR` to an
existing directory to write all four Parley viewport captures as PNG.

Generate the float specimen with `python3 scripts/generate-render-floats-fixture.py`.
Pass `tests/fixtures/render-floats.epub` to the float stock probe with the
same cache, font and capture arguments. Run
`cargo test --locked --lib float_layout -- --nocapture` for the Reader
comparison. Set `READER_FLOAT_CAPTURE_DIR` to an existing directory to
write the simulated reading pages.

Generate the empty-box comparison with
`python3 scripts/generate-render-empty-breaks-fixture.py`. Pass
`tests/fixtures/render-empty-breaks.epub` to the empty-break stock probe
with the same cache, font and capture arguments. Run
`cargo test --locked --lib publisher_breaks -- --nocapture` for the Reader
page assignments and resume checks. Set `READER_EMPTY_BREAK_CAPTURE_DIR`
to an existing directory to save the original specimen’s page sequence.
Set `READER_EMPTY_GEOMETRY_CAPTURE_DIR` to save the physical box pages
from the asymmetric specimen.
