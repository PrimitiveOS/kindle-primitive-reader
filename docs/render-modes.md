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
| Flat | `0x03030031` | `0x00030031` | Enhanced blocks, vertical margin collapse with individual negative margins, prepared float boxes rendered inline, inline-block boxes and anonymous table repair |
| Book | `0x03375131` | `0x00375131` | Flat features, W3C content-box sizing, automatic horizontal margin alignment, limited CSS width enforcement, floating float boxes, own-float handling and exact float footprints |
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
| Flat | Extend the measured empty-box geometry below to nested and margin-only boxes, and extend margin grouping to the remaining formatting contexts, inline floats, inline-block containers with complex content, and anonymous boxes beyond the simple CSS table cases below. |
| Book | Extend the horizontal float model below to complex boxes and inline floats, and implement the remaining style-width and auto-margin behavior |
| Web | Extend signed margins and float geometry, implement full publisher box sizing and automatic placement, and extend physical break-page painting to complex boxes |

The four options depend on independent box geometry, pagination and
source mappings. Reader’s diagnostic policies combine the measured cases
below and exposes them through the reading Layout panel.

## Composed policies in Reader

Open the bottom reading bar and choose **Layout › Render mode** on a
horizontal, left-to-right, one-page EPUB with Publisher CSS on. Tap
**Legacy**, **Flat**, **Book**, or **Web** to reflow the current edition.
Tap **Reader** to return to the established Reader layout. The setting
uses the same four composed policies as the Parley probe.

**New-book mode** picks a default for EPUBs that Reader has never opened.
Existing editions continue with their saved mode or the established Reader
layout. Publisher CSS off keeps the selected mode, as KOReader does: its
render flags are set independently of the embedded stylesheet switch
(`readertypeset.lua` `setBlockRenderingMode`). Continuous scrolling keeps the
mode too: on the pinned KOReader, the scroll full height of
`render-mode-composition.epub` is 1655 px for Legacy and 1580 px for Flat. Custom
hidden flows keep the mode as well; the flow tests keep their markers resolved
under Web. Two-page spreads keep the mode on each leaf, at the leaf width.
No publisher-render pause is left in the Reader layout; RTL and vertical text
still use Reader's layout.

Scroll view was compared on the first screen of the same fixture, Legacy and
Web, with the pinned KOReader and Reader (`docs/screenshots/render-mode-scroll-*.png`).
Both engines keep blocks left-aligned in Legacy. In Web, the centred box starts
at the same horizontal position (about 389 px), and the floating card keeps its
text to the right. Margins and font metrics differ between the two setups, so
this is a structural match, not a pixel comparison. Hidden-flow frames are not
compared yet. EPUBs with right-to-left spines and
vertical text keep their existing layout.

[Reader Layout on Mac](screenshots/render-mode-reader-layout.png) ·
[Landscape Layout](screenshots/render-mode-reader-layout-landscape.png) ·
[New-book default](screenshots/render-mode-reader-new-book-default.png)

| Policy | Legacy | Flat | Book | Web |
| --- | --- | --- | --- | --- |
| Vertical margins | Separate | Collapse | Collapse | Signed collapse |
| Incoming margin at eligible forced openings | Ordinary | Preserve | Preserve | Preserve |
| Inline blocks | Flowing | Declared box | Content-box sizing | Content-box sizing |
| Incomplete CSS table boxes | Source order | Repaired | Repaired | Repaired with supported width |
| Ordinary defined or constrained width and height | Natural | Natural | Natural | Publisher |
| Text-sized boxes | Natural | Natural | Bounded fit-content | Signed fit-content |
| Float footprints | Source order | Source order | Content height | Publisher height |
| Empty forced breaks | Separate edges | Coalesce | Coalesce | Keep supported sized boxes |

The authored [composition EPUB](../tests/fixtures/render-mode-composition.epub)
includes margins, floats, a repaired table, an inline block, a signed-margin
link and empty forced breaks. It also includes a 400-pixel box capped at
260 pixels in the spine item with a float. The pinned Web replay and Reader
paint that box 260 pixels wide. In Reader an eligible ordinary width limit
now stays active throughout the same chapter as floats. A box with its own
column clears active float footprints, so the separately sized line uses
its own text and hit area.

The native replay switches from Web through the other policies and returns
to Web at the same XPointer. The Mac reading session preserves the same
serialized anchor and restores its original Web coordinates and frame.
Tests also check a tappable link extending into the reading margin, the
repaired row, and painted empty pages. The Mac UI replay checks the
current and global choices, reopen, restoring the original frame and
graying the setting out when unavailable. Broader box composition, the
older-DOM compatibility path and Kindle hardware behavior remain open.
Legacy keeps orphan table cell text readable in source order;
KOReader’s Legacy layout omits those cells on this specimen. Full-width
wrapper backgrounds outside the supported Web sizing path remain open.

[Reader combined checks](../src/publisher_render_mode/tests.rs) ·
[Native KOReader replay](../scripts/koreader-render-modes-check.lua) ·
[Stock Web](screenshots/render-composition-stock-web.png) ·
[Reader Legacy](screenshots/render-composition-reader-legacy-main.png) ·
[Reader Flat](screenshots/render-composition-reader-flat-main.png) ·
[Reader Book](screenshots/render-composition-reader-book-main.png) ·
[Reader Web](screenshots/render-composition-reader-web-main.png) ·
[Reader Web forced page](screenshots/render-composition-reader-web-break.png)

Run the same composed policies with the Reader diagnostic CLI.

```sh
cargo run --locked --bin reader-probe -- parley tests/fixtures/render-mode-composition.epub \
  --font tests/fixtures/fonts/NotoSans-Regular.ttf --render-mode web \
  --height 1606 --size 40 --line-height 1.2 --output web.png
```

Run the focused test set with `cargo test --locked --features kindle --lib publisher_render_mode`.
Set `READER_RENDER_MODE_CAPTURES` to an existing directory to reproduce the
Mac frames. Generate the same EPUB with `python3 scripts/generate-render-modes-fixture.py /tmp/composition.epub --composition` and pass
it to the native Render Mode probe documented below.

## Text-only inline blocks

The authored [inline-block specimen](../tests/fixtures/render-inline-blocks.xhtml)
compares the same Noto Sans text at 600 × 720, 96 content DPI, 36-pixel
reading margins and nominal 32-pixel text. Gray band widths below come from
the native captures. In Legacy the bands follow unboxed inline text.
Flat includes padding in a declared width. Book and Web apply the
declared box model, adding padding to content-box widths.

| Native specimen | Legacy | Flat | Book | Web |
| --- | ---: | ---: | ---: | ---: |
| 192 px content box with 12 px side padding | 91 px | 192 px | 216 px | 216 px |
| Intrinsic span containing styled text | 160 px | 164 px | 164 px | 164 px |
| 104 px content box with 8 px side padding | 177 px | 104 px | 120 px | 120 px |
| 40% border box with 10 px side padding | 127 px | 211 px | 211 px | 211 px |
| Text after the 192 px box, screen x | 216 px | 316 px | 340 px | 340 px |

The internal Parley candidate implements these width and padding rules in
horizontal, text-only paragraphs. It shapes each box separately, uses its
last line's baseline, and places the whole box in one host line. A narrow
box can wrap its own text. A host line containing a box moves to the next
page as a unit. Links, text selection, and anchors use the same source
bytes as ordinary inline layout, including when a box is the entire
paragraph. Native and Reader text metrics can produce different line cuts.

The default Reader configuration continues ordinary inline layout.
A paragraph flows as ordinary text when it contains an unsupported box,
such as an empty or nested box, a box mixed with floats, images, ruby or
table cells, constrained dimensions, publisher heights or borders, or a
right-to-left host. These cases and
the older-DOM flag selection still need acceptance for the full setting.
[Native Web capture](screenshots/render-inline-blocks-stock-web.png) ·
[Reader Book/Web capture](screenshots/render-inline-blocks-reader-book-web.png) ·
[Native assertions](../scripts/koreader-render-inline-blocks-check.lua)

Generate the replay EPUB with
`python3 scripts/generate-render-inline-blocks-fixture.py /tmp/inline-boxes.epub`.
Pass that EPUB, a scratch cache directory, the fixture Noto Sans regular
font and a capture directory to the native Lua assertions. Run
`cargo test --locked --lib inline_blocks` for the Reader geometry, source
and page checks. Set `READER_INLINE_BLOCK_SCREENS` to an existing directory
to save its three candidate captures.

## Repairing missing CSS table rows and roots

The pinned table-completion flag groups CSS table cells into missing rows
and groups orphan rows into a table. A supported explicit `display:table`
can mix those repaired rows with ordinary CSS row groups. Reader's internal
horizontal repair path reads the computed display roles and projects its
row and column positions without rewriting the publication's source.
A `display:none` separator in an explicit table splits adjacent direct-cell
runs in the native fixture. An ordinary paragraph between two orphan
runs stays outside both grids.

| Authored anchor at 1264 × 1606 and 96 DPI | Stock Flat and Book | Stock Web | Reader with the corresponding internal policies |
| --- | ---: | ---: | ---: |
| First explicit grid, second column | x = 642 | x = 469 | x = 642 or 468–469 |
| Grouped row, second column | x = 642 | x = 469 | x = 642 or 468–469 |
| Orphan row with 50% cell widths, second column | x = 642 | x = 642 | x = 642 |
| Orphan cells with intrinsic widths, second column | x = 146 | x = 146 | Measured text and publisher padding |

Web uses the authored 70% width for the explicit grids. Flat and Book
use the available column width. Legacy omits invalid direct cells in an
explicit table and places the tested orphan cells on separate lines.
Reader's ordinary layout keeps both kinds of cells readable in source
order. Line pitch and the vertical gaps between row backgrounds follow
each engine's text metrics.

The Mac checks restore an exact source quote after repair, hit-test text
in every table, and follow an in-cell link in the reconstructed geometry.
They also check paired cells across multiple pages and compare ordinary
native tables and nested fallback boxes byte for byte. The repair path covers bounded horizontal text grids. Spanning
CSS cells, inline and nested table boxes, row group styling, and complete
table border and pagination behavior remain open.

Generate the authored EPUB with
`python3 scripts/generate-render-css-tables-fixture.py`. Run the
[native oracle](../scripts/koreader-render-css-tables-check.lua) in the
pinned runtime with the fixture path, an empty cache directory, Noto Sans
and a capture directory. Run `cargo test --locked --lib repair_css_tables`
for the Reader comparison. Set `READER_CSS_TABLE_CAPTURES` to an existing
directory to save Reader's frames.
[CREngine missing-parent construction](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L7973-L8170) ·
[KOReader Flat](screenshots/render-css-tables-stock-flat.png) ·
[Reader full-width policy](screenshots/render-css-tables-reader-flat.png) ·
[KOReader Web](screenshots/render-css-tables-stock-web.png) ·
[Reader authored-width policy](screenshots/render-css-tables-reader-web.png)

## Publisher widths on ordinary paragraphs

An internal horizontal Parley width policy lays out a direct body paragraph
at its supported publisher pixel or percentage width. The existing box
coordinates set the line width, text hit areas, and painted background.
Auto margins consume the available space and resolve through the normal CSS
cascade, including the `margin` shorthand. A declared width must fit its
parent and leave room for text. Reader uses its existing layout by default.

| Authored paragraph | Reader default and stock Legacy/Flat/Book | Reader width policy and stock Web |
| --- | ---: | ---: |
| Centered at 42% | x = 54, width = 1,156 px | x = 389, width = 485 px |
| Left aligned at 260 px | x = 54, width = 1,156 px | x = 54, width = 260 px |

The comparisons use a 1264 × 1606 viewport and a 1,156-pixel content width.
The native oracle asserts the stock word origin and the width and origin of
each solid paint band. Reader tests additionally check line wrapping,
restored source quotes and touch hit testing. Reader wraps the 260-pixel
sample in two lines under its own text metrics. Remaining box sizing work
includes complex ancestor boxes, fixed widths with negative margins,
overflow beyond the physical screen, and boxes with images, tables, or floats.
[CREngine width resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7909-L7982) · [CREngine auto margins](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8178-L8202) · [Native oracle](../scripts/koreader-render-modes-check.lua) · [Reader capture](screenshots/render-modes-reader-web-width.png)

Run `cargo test --locked --lib publisher_width` for the Reader comparison.
Set `READER_WEB_WIDTH_CAPTURE` to a PNG path to save a capture.

### Explicit widths, constraints and side placement

The opt-in `PublisherWithConstraints` policy resolves `width`, `max-width`,
and `min-width` for ordinary paragraphs directly in the document body.
It applies the maximum first, then the minimum. An automatic width that
hits a constraint uses the same margin placement as a definite width.
The resulting box controls text wrapping, background, selection rectangles,
and link taps. Reader keeps its existing default width policy.

| Authored case, 1264 px viewport at 96 DPI | KOReader Legacy, Flat and Book band | KOReader Web and Reader constrained band |
| --- | ---: | ---: |
| 320 px width and physical margins 40 px left, 24 px right | x = 94, width = 1,092 px | x = 94, width = 320 px |
| 40% width, left auto, right 56 px | x = 54, width = 1,100 px | x = 692, width = 462 px |
| 520 px width capped at 380 px, centered | x = 54, width = 1,156 px | x = 442, width = 380 px |
| 36% minimum overrides a 210 px maximum, centered | x = 54, width = 1,156 px | x = 424, width = 416 px |
| Auto width capped at 420 px, centered | x = 54, width = 1,156 px | x = 422, width = 420 px |
| 1,000 px width with competing LTR sides | x = 134, width = 956 px | x = 134, width = 1,000 px |
| 320 px RTL box with two physical sides | x = 94, width = 1,092 px | x = 890, width = 320 px |
| Auto width with unused maximum | x = 106, width = 1,092 px | x = 106, width = 1,092 px |

The native probe asserts each band, text edge, and linked word. Saved source
quotes survive repagination in Reader and mode changes in KOReader. Reader's
continued text can wrap differently because the engines shape with different
metrics. The nonnegative policy covers boxes that fit the containing column
with room for text. Complex ancestry, floats, and decorations on explicit
widths remain outside this policy.

Regenerate the authored EPUB with
`python3 scripts/generate-render-width-margins-fixture.py`. Run the Lua
oracle with the Render Mode probe arguments. Set
`READER_CONSTRAINED_WIDTH_CAPTURE_DIR` to an existing directory and run
`cargo test --locked --lib publisher_width_constraints` for the Reader comparison.
[CREngine width limits](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7985-L8021) ·
[Direction and automatic margins](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8160-L8247) ·
[Native oracle](../scripts/koreader-render-width-margins-check.lua) ·
[KOReader Web](screenshots/render-width-margins-stock-web.png) ·
[Reader default](screenshots/render-width-margins-reader-default.png) ·
[Reader with constraints](screenshots/render-width-margins-reader.png)

#### Width constraints with negative side margins

With both experimental policies enabled, Reader resolves explicit and
constrained widths alongside negative margins. When a definite width and
its sides exceed the column width, the placement drops the right margin
for left-to-right text or the left margin for right-to-left text. If the box
still overflows, auto margins stay unresolved. Reader shapes the full box
and clips the physical paint, underline and selection to the screen.

| Authored case at 1264 px and 96 DPI | Legacy, Flat and Book paint | Web and Reader paint |
| --- | ---: | ---: |
| 400 px width with a negative left side | x = 54, width = 1,144 px | x = 30, width = 400 px |
| 480 px width, left auto and right −24 px | x = 54, width = 1,156 px | x = 754, width = 480 px |
| 1,300 px width with overflow after dropping a margin | x = 54, width = 1,156 px | x = 54, width = 1,210 px |
| 1,240 px width starting off the left edge | x = 54, width = 1,116 px | x = 0, width = 1,220 px |
| 1,100 px right-to-left box with competing sides | x = 94, width = 1,116 px | x = 110, width = 1,100 px |
| Auto width capped at 420 px with a negative left side | x = 54, width = 1,156 px | x = 22, width = 420 px |
| 112% minimum width overriding a 380 px maximum | x = 54, width = 1,148 px | x = 20, width = 1,244 px |
| 104% width, left auto and right −84 px | x = 54, width = 1,156 px | x = 92, width = 1,172 px |

The native probe checks the unclipped source-word coordinates and the
visible paint bands. Reader wraps the right-beacon sentence onto two lines
under its own text metrics. Reader restores the entire source quote and lets the
reader tap the visible portion of a link, including where it crosses a
user margin. Either policy on its own leaves these authored frames
unchanged. The combined policy applies to ordinary horizontal paragraphs
in the body or width-neutral wrappers and limits extreme geometry to one
extra viewport per side.

Generate the EPUB with `python3 scripts/generate-render-width-margins-fixture.py --signed`.
Pass `signed` after the capture directory to the same Lua oracle. Run
`cargo test --locked --lib publisher_signed_widths` with
`READER_SIGNED_MARGIN_CAPTURES` pointing to an existing output directory.
[KOReader Web](screenshots/render-signed-widths-stock-web.png) ·
[Reader default](screenshots/render-signed-widths-reader-default.png) ·
[Reader with both policies](screenshots/render-signed-widths-reader.png)

### Paragraphs that fit their text

The internal `FitContentMode` policy measures the styled text of ordinary
horizontal `width: fit-content` paragraphs. Book and Web both limit the
measured width to the available column, using absolute minimum and maximum
widths during measurement. Book clamps negative margins to zero. Web uses
signed margins and reapplies the resolved constraints, which lets a
percentage minimum extend paint to the screen edge. The default preserves
the existing cascade, including a valid width declared before `fit-content`.

| Authored case at 1264 px and 96 DPI | KOReader Book x / width | KOReader Web x / width |
| --- | ---: | ---: |
| Content-sized paragraph centered with auto margins | 491 / 282 | 491 / 282 |
| Left auto and 56 px right margin | 871 / 283 | 871 / 283 |
| Centered paragraph capped at 150 px | 557 / 150 | 557 / 150 |
| Centered paragraph with 420 px minimum | 422 / 420 | 422 / 420 |
| Linked text with −24 px left margin | 54 / 328 | 30 / 328 |
| Long text with physical side margins | 74 / 1,124 | 74 / 1,124 |
| Centered 30% minimum overrides 22% maximum | 491 / 281 | 459 / 346 |
| 110% minimum with −24 px left and auto right | 54 / 275 | 30 / 1,234 |

The native fixture checks ten horizontal paint bands in all four stock modes.
Reader checks the Book and Web positions and widths with up to two pixels of
tolerance for text advance on four samples. Line wrapping and line height
still differ. Reader restores all ten source quotes and exercises link and
word taps in the left user margin in Web. A separate 50% indent test adds
30 pixels to the measured width at a 60-pixel paragraph font, matching
the stock intrinsic measurement. Coverage is limited to ordinary
paragraphs in the body or width-neutral wrappers. Negative and hanging
indents and extremely narrow width constraints use the ordinary layout
path. The [combined policies](#composed-policies-in-reader) cover supported EPUB layouts.

Generate the sample with `python3 scripts/generate-render-width-margins-fixture.py --fit-content`.
Pass `fit-content` after the capture directory to the Lua oracle. Run
`cargo test --locked --lib fit_content` and set `READER_FIT_CONTENT_CAPTURES`
to an existing directory to save Reader frames.
[CREngine width calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7909-L8021) ·
[Intrinsic text measurement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L12020-L12068) ·
[Native oracle](../scripts/koreader-render-width-margins-check.lua) ·
[KOReader Book](screenshots/render-fit-content-stock-book.png) ·
[KOReader Web](screenshots/render-fit-content-stock-web.png) ·
[Reader Book policy](screenshots/render-fit-content-reader-book.png) ·
[Reader Web policy](screenshots/render-fit-content-reader-web.png)

#### Horizontal padding and solid paragraph borders

The same opt-in modes size fitted paragraphs with horizontal padding and
solid borders. CREngine's intrinsic pass includes the horizontal decoration,
including the inverse intrinsic contribution for percentage padding. It
then clips that demand to the available column and adds the resolved
decoration during placement. `box-sizing` determines when the minimum and
maximum apply. Web also enforces the resolved limits after placement, and
Book clips overflow to the containing column.

| Authored case at 1264 px and 96 DPI | KOReader Book x / width | KOReader Web x / width |
| --- | ---: | ---: |
| Centered, 20 px left and 12 px right padding | 441 / 382 | 441 / 382 |
| Same padding, 168 px content-box maximum | 516 / 232 | 532 / 200 |
| Same padding, 168 px border-box maximum | 532 / 200 | 548 / 168 |
| Centered, 5% left and 3% right padding | 416 / 432 | 416 / 432 |
| 5 px and 7 px side strokes with a −24 px left margin | 54 / 409 | 30 / 409 |
| Horizontal padding, 30% minimum and 22% maximum | 471 / 321 | 449 / 366 |
| Four-sided 4 px stroke and 400 px border-box minimum | 766 / 420 | 766 / 420 |
| Long text with 32 px total horizontal padding | 54 / 1,156 | 54 / 1,188 |

The native captures assert the outer paint bands and text origins in all
four modes. Reader's Book and Web measurements agree within two pixels
where the font advances differ. Paint includes opaque solid strokes, with
square joins between different edge shades.
Selection and text or link taps use the inner text area, including visible
text in a Web margin. The verification covers exact source quotes,
default frame equivalence and border paint during fractional scrolling.
Nonsolid, oversized, or translucent strokes, patterned or complex ancestors,
and mixed boxes with artwork use the existing layout path.
Vertical spacing and line wrapping can differ between the engines.

Generate the specimen with `python3 scripts/generate-render-width-margins-fixture.py --fit-decorated`.
The native script takes the EPUB, scratch cache, Noto Sans Regular font and
capture directory. Use the same `fit_content` Reader test and capture option.
[Native decorated width oracle](../scripts/koreader-render-fit-decoration-check.lua) ·
[KOReader Web capture](screenshots/render-fit-decorated-stock-web.png) ·
[Reader Web capture](screenshots/render-fit-decorated-reader-web.png)

#### Vertical padding at page boundaries

Book and Web resolve vertical percentages against the containing column,
even when the paragraph is narrower. Font-relative padding uses the computed
paragraph font. Each painted edge follows its original stroke, padding, and
text order. The paragraph background fills the fitted width through its
padding. Leading and trailing edges stay with the first and last line where
they fit, while the middle of a long passage can turn at an ordinary line.
The continuation page paints the side strokes without creating a top or
bottom border at the page cut. Selection and resume still use the original
text coordinates.

| Authored case at 1264 px and 96 DPI | Leading edge | Trailing edge |
| --- | ---: | ---: |
| 26/19 px padding with 3/4 px strokes | 29 px | 23 px |
| 3%/2% with a 1,156 px containing column and `border-box` width | 34 px | 23 px |
| 0.5/1 em with a 50 px paragraph font | 25 px | 50 px |
| 2%/1% with a signed horizontal margin in Book and Web | 23 px | 11 px |
| 42 authored lines with 28/37 px padding and 3 px strokes | 31 px | 40 px |

Pinned KOReader and Reader keep the 42-line specimen between opening and
closing forced breaks and span two body pages. KOReader's second page starts
at `Sounding 23`, while Reader's starts at `Sounding 26` with its current
line metrics. Reader also keeps its own spacing between paragraphs. The
source links are tested on the text. A direct Reader tap on blank padding has
no text target. KOReader's nearby-link lookup can find a link from that
padding. Fit falls back to the normal layout path when an individual
padding edge exceeds a quarter of the reading height.

Generate the EPUB with `python3 scripts/generate-render-width-margins-fixture.py --fit-vertical`.
Use the same native oracle arguments and `READER_FIT_CONTENT_CAPTURES` for
the `fit_content_vertical` tests to reproduce the captures.
[Native vertical edge oracle](../scripts/koreader-render-fit-vertical-check.lua) ·
[KOReader Web first page](screenshots/render-fit-vertical-stock-web.png) ·
[Reader Web first page](screenshots/render-fit-vertical-reader-web.png) ·
[KOReader Web continuation](screenshots/render-fit-vertical-stock-continuation.png) ·
[Reader Web continuation](screenshots/render-fit-vertical-reader-continuation.png)

#### Height constraints on fitted paragraphs

Book fits the width and paints the natural text and edges. Web can reserve
extra height. It starts with `height`, applies `max-height` to that request,
and then applies `min-height`. The minimum can establish a height on its
own and can exceed the maximum. Long text keeps its full natural height.
Percentage heights are ignored for these paragraphs. Content-box heights
add the top and bottom padding and strokes. Border-box heights already
include those edges. Children use content-box by default and can explicitly
inherit the parent's sizing choice.

| Authored case at 96 DPI | Stock Book outer height | Stock Web outer height |
| --- | ---: | ---: |
| 180 px content box with 45 px of edges | 109 px | 225 px |
| Same declaration with border-box sizing | 109 px | 180 px |
| 220 px height capped at 100 px | 109 px | 145 px |
| Same cap with 155 px minimum | 109 px | 200 px |
| 136 px minimum alone | 109 px | 181 px |
| 3 em height and 4 rem minimum at 125% font | 126 px | 205 px |
| 160 px with a border-box parent and a default content-box child | 109 px | 205 px |
| 104.75 pt with edges | 109 px | 184 px |

The Web render adds blank space after the text and before the bottom padding
and stroke. It caps the added space at one reading page. A tall fitted box
can continue its side strokes across a page cut and paint its actual bottom
edge on the continuation. With a forced break after the box, the decorated
continuation remains visible before the next text page. It carries no
synthetic text anchor. Source quotes and text link coordinates are retained.
The same physical-unit sample measures 276 px with edges at 144 DPI.
Inter-paragraph spacing, natural text height, and antialiasing remain
engine-specific.
The fit policy still covers ordinary paragraphs and simple opaque strokes.

Generate the EPUB with `python3 scripts/generate-render-width-margins-fixture.py --fit-height`.
The native height oracle accepts the same four arguments as the vertical
probe and an optional DPI. Use `READER_FIT_CONTENT_CAPTURES` with
`cargo test --locked --lib fit_content_height` for Reader frames.
[Pinned height computation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7664-L7715) ·
[Native height oracle](../scripts/koreader-render-fit-height-check.lua) ·
[KOReader Web heights](screenshots/render-fit-height-stock-web.png) ·
[Reader Web heights](screenshots/render-fit-height-reader-web.png) ·
[KOReader Web closing edge](screenshots/render-fit-height-stock-tail.png) ·
[Reader Web closing edge](screenshots/render-fit-height-reader-tail.png)

### Sized wrappers around ordinary paragraphs

The internal `PublisherWithWrappers` policy extends publisher sizing
to a direct `div`, `section` or `article` in the document body when
the wrapper contains ordinary paragraphs. Its pixel or percentage
width constrains child wrapping, source hit boxes and the wrapper’s
painted background. The same margin calculation centers it within its
parent’s content width.

| The authored 42% wrapper at 1264 × 1606 | Child text starts at | Solid background band |
| --- | ---: | ---: |
| KOReader Legacy, Flat and Book | x = 54 | x = 54, width = 1,156 px |
| KOReader Web | x = 389 | x = 389, width = 485 px |
| Reader with the wrapper policy | x = 389 | x = 389, width = 485 px |
| Reader’s ordinary layout and the paragraph-only policy | x = 54 | Existing background behavior |

Wrapped paragraphs keep their canonical source positions, including on
subsequent pages of the same container. A hidden child removed by
`display:none` contributes no painted rows. The tests compare byte-identical
frames in unsupported boxes and with Publisher styles off. Nested wrappers,
children with their own dimensions, complex borders or spacing, floats
inside the wrapper, and media and tables use their existing layout.

Run `cargo test --locked --lib publisher_wrappers` for the Reader
comparison. Set `READER_PUBLISHER_WRAPPER_CAPTURES` to an existing
directory to capture the matching and continuation views. The original
Render Mode fixture also checks the same wrapper in the pinned native
oracle.
[CREngine’s width flag](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7909-L7923) ·
[Wrapper background](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L10253-L10285) ·
[Native Web capture](screenshots/render-modes-stock-web.png) ·
[Reader Web width and wrappers](screenshots/render-modes-reader-web-wrappers.png) ·
[Reader wrapper continued](screenshots/render-modes-reader-wrapper-continuation.png)

### Positive physical margins on auto-width paragraphs

The pinned renderer subtracts the resolved left and right margins from
an ordinary auto-width paragraph’s available width. Its left edge then
moves by the left margin. This lets publishers inset a paragraph evenly,
indent an entry from just one side, or leave extra room at the end of a
line. Legacy, Flat, Book and Web agree on the authored cases.

Reader’s internal policy applies the same geometry to plain paragraphs
directly in the document body. Its existing text bounds carry the
result through wrapping, background paint, saved selections and link
targets. Automatic sides contribute zero. With a nonzero content DPI,
a small positive fixed-point declaration may produce a one-pixel inset.

| Authored declaration, 1,156 px content width | Child text and band start | Painted band width |
| --- | ---: | ---: |
| Left 40 px, right 24 px | x = 94 | 1,092 px |
| Left 1.4 em, right 2% | x = 109 | 1,078 px |
| Left 0.01 em, right auto | x = 55 | 1,155 px |

The native and Reader tests assert all three horizontal measurements
at a 40-pixel font size and 96 DPI. Reader’s text metrics wrap the first
sample onto one additional line. Reader also tests scaled absolute lengths, narrower
wrapping, link targets and byte-identical frames with publisher styles
disabled. Explicit or maximum widths, decorated boxes, negative sides
and insets that leave less than one em of text keep their existing layout.
The policy is internal and defaults off.

Generate the authored specimen with
`python3 scripts/generate-render-horizontal-margins-fixture.py`.
Run the pinned oracle with the same arguments as the Render Mode probe.
For Reader, run `cargo test --locked --lib publisher_horizontal_margins`
and set `READER_HORIZONTAL_MARGIN_CAPTURES` to an existing folder to
save the frames. The optional original check takes a local EPUB and a
private list of source passages and prints only the measured shifts.
[CREngine’s physical sides](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7584-L7594) ·
[Auto-width calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7985-L7988) ·
[Native oracle](../scripts/koreader-render-horizontal-margins-check.lua) ·
[KOReader Web](screenshots/render-horizontal-margins-stock-web.png) ·
[Reader default](screenshots/render-horizontal-margins-reader-default.png) ·
[Reader with the policy](screenshots/render-horizontal-margins-reader-inset.png)

### One negative physical side on an auto-width paragraph

An independent internal policy shapes an ordinary horizontal body paragraph
against its signed publisher margins. Its background, glyphs, underlines,
link hits and selection rectangles use the same geometry. The first two
authored examples extend 20 px into either user margin. The default Reader
and the positive-inset policy keep their existing geometry.

| Authored declaration at 96 DPI and 40 px font | KOReader Legacy, Flat and Book band | KOReader Web and Reader signed band |
| --- | ---: | ---: |
| Left −0.5 em, right 12 px | x = 54, width = 1,144 px | x = 34, width = 1,164 px |
| Left 12 px, right −0.5 em | x = 66, width = 1,144 px | x = 66, width = 1,164 px |

The 1264 × 1606 stock Web frame places visible glyphs 20 px into the
left or right user margin. Its strict link lookup clips both user margins.
Reader uses the visible linked glyphs for taps in those margins. The native
fixture verifies both painted edges and quote restoration across the four
stock modes. Reader tests verify the matching paint bands, exact links and
quotes through page and scroll reading, search anchors and displayed
highlights.

#### Two negative sides and clipping at the screen edge

KOReader Web sizes each paragraph from its full signed box, including
parts outside the physical screen. Reader also shapes the full box, then
clips the background, characters, link underline, and highlight at the
screen edge. The source anchor and selected quote still refer to the whole
word. The policy also applies inside width-neutral `div`, `section`, and
`article` wrappers with ordinary flow and no horizontal box decorations.

| Authored case at 1264 px | Legacy, Flat and Book paint | Web and Reader with the signed policy |
| --- | ---: | ---: |
| Left −24 px, right −16 px | x = 54, width = 1,156 px | x = 30, width = 1,196 px |
| Left −74 px, right 12 px | x = 54, width = 1,144 px | x = 0, width = 1,198 px |
| Left 12 px, right −74 px | x = 66, width = 1,144 px | x = 66, width = 1,198 px |
| Nested paragraph, left −18 px, right −14 px | x = 54, width = 1,156 px | x = 36, width = 1,188 px |

Native source boxes extend to x = −20 on the left and x = 1284 on the
right. The matching visible Reader highlights stop at x = 0 and x = 1264.
The tests tap the partial words, follow their links, and restore the full
quotes. The signed policy limits off-screen extent to one viewport per
side and keeps its original eligibility restrictions for explicit widths,
decorated wrappers, floats, tables, inline images, and vertical text.

Run `python3 scripts/generate-render-signed-margins-fixture.py` to rebuild
the specimen. The Lua oracle takes the same arguments as the Render Mode
probe. Run `cargo test --locked --lib publisher_signed_margins` for Reader.
Set `READER_SIGNED_MARGIN_CAPTURES` to an existing folder to save the frames.
[CREngine signed margin and overflow flags](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/include/lvrend.h#L209-L214) ·
[CREngine auto-width placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7985-L7988) ·
[CREngine page overflow](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8251-L8284) ·
[CREngine strict link bounds](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L2597-L2671) ·
[Native oracle](../scripts/koreader-render-signed-margins-check.lua) ·
[KOReader Web](screenshots/render-signed-margins-stock-web.png) ·
[Reader default](screenshots/render-signed-margins-reader-default.png) ·
[Reader with the policy](screenshots/render-signed-margins-reader.png)

## Definite heights on ordinary paragraphs

KOReader Web honors a definite publisher height as a minimum for the used
text box. A shorter declaration retains every text line, and a larger one
adds blank space after the text. The new space participates in page breaks,
with extra space limited to one reading page. Legacy, Flat and Book use
the natural text height on the authored paragraphs. A percentage height
with an indefinite parent also uses the natural height in Web.

Reader’s opt-in Parley policy handles pixel and absolute physical lengths
on eligible direct body paragraphs. It retains Parley’s text metrics and
uses the existing geometry for background color and optional publisher
width. When a page ends inside added space, the background continues on
the next page and saved positions still resolve to source text. The
ordinary Reader setting keeps its previous layout.

| Authored fixture at 1264 × 1606, 96 DPI | Pinned KOReader Web | Reader height policy |
| --- | ---: | ---: |
| 164-pixel height band and following text pitch | 164 px / 164 px | 164 px / 168 px |
| 420-pixel width with 153-pixel height | 420 × 153 px | 420 × 153 px |
| 24-pixel declaration across two source lines | Both lines visible | Both lines visible |
| 45% height under an indefinite parent | Natural height | Natural height |
| Total pages and the page of the final text | 2 / 2 | 2 / 2 |

At 1264 × 690, the 600-pixel box reaches the bottom margin of page 2
and continues on page 3 in both engines. The stock split is 237 and
363 pixels. Reader splits it into 219 and 381 pixels because its
ordinary text starts at different vertical positions. The Reader test
measures both clipped bands and verifies that a tap in the blank space
selects no text.

Relative heights, nested boxes, padding, borders, maximum height, authored
keeps and text alongside active floats still use the existing Reader
behavior. The setting remains internal while other Render Mode mechanisms
are pending.
[CREngine height calculation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7665-L7683) ·
[CREngine bounded fill](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L9024-L9035) ·
[Native oracle](../scripts/koreader-render-text-heights-check.lua) ·
[Reader tests](../src/publisher_padding/height_tests.rs) ·
[Stock page 2](screenshots/render-text-height-stock-compact-page-2.png) ·
[Stock page 3](screenshots/render-text-height-stock-compact-page-3.png) ·
[Reader page 2](screenshots/render-text-height-reader-compact-page-2.png) ·
[Reader page 3](screenshots/render-text-height-reader-compact-page-3.png)

Generate the authored fixture with
`python3 scripts/generate-render-text-heights-fixture.py`. Run the pinned
oracle with the same arguments as the render modes probe below and an
optional final viewport height of `690` or `1606`.
Run `cargo test --locked --lib publisher_text_height` for the Reader
comparison. Set `READER_WEB_HEIGHT_CAPTURES` to an existing directory to
capture the comparison frames.

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
adjoining blocks. `Collapse` floors a net negative result at zero.
`CollapseNegative` allows that gap to reduce the pitch. An intervening
visible empty element or a table ends a sibling pair. Table cell tracks retain
their existing layout, and a positive boundary gap can push the entire
following grid down. The provider bounds a gap to half a viewport, prevents
extreme negative values from reversing text origins, and drops terminal
spine margins. Collapsing through empty boxes, complex float and overflow
layout, and vertical writing remain pending. These policies are internal.

### Adjoining parent and child margins

Transparent horizontal `div`, `section` and `article` wrappers can
share their top or bottom margin with the first or last in-flow block
child. The provider combines each eligible child edge with the parent
and then combines that group with the adjoining sibling margin. It
keeps both extrema through nested transparent wrappers, even when
their own margin is zero.

The first and last edges are checked independently. Parent-owned text
between the edge and child and physical padding or a recognized border
at that edge prevent the join. The existing spacing applies to sized or
painted wrappers, visible empty siblings, tables, list items, and
complex clearance, float or keep contexts. Reader uses supported
padding in the rendered box geometry. The stock border examples
measure the margin contribution separately from the border stroke.

| Authored gap at 1264 × 1606 | Pinned Legacy pitch | Pinned Flat / Book pitch | Pinned Web pitch | Reader extra space in Collapse / CollapseNegative |
| --- | ---: | ---: | ---: | ---: |
| Sibling, parent and first child | 190 px | 120 px | 120 px | 56 / 56 px |
| Last child, parent and next sibling | 178 px | 108 px | 108 px | 44 / 44 px |
| Negative group after the last child | 79 px | 64 px | 51 px | 0 / −13 px |
| Leading edge separated by 8 px of padding | 150 px | 135 px | 135 px | 63 / 63 px |
| Leading edge through a margin-free wrapper | 149 px | 102 px | 102 px | 38 / 38 px |
| Trailing edge through a margin-free wrapper | 137 px | 94 px | 94 px | 30 / 30 px |

The Reader column subtracts its ordinary 67.8-pixel text pitch and
independent box padding. The 64-pixel native pitch produces slightly
different absolute positions and page breaks in the same fixture. The test also restores a
serialized source selection and checks its live touch target after the
layout changes, including a new page assignment. The ordinary policy
preserves the frame byte for byte with or without the unused margin
declarations.

Generate the fixture with `python3 scripts/generate-render-parent-margins-fixture.py`.
Run the pinned oracle with the same arguments as the Render Mode probe.
Run `cargo test --locked --lib block_margins` to compare Reader, and set
`READER_PARENT_MARGIN_CAPTURES` to an existing folder to capture the
new layout.
[CREngine margin accumulation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6289-L6332) ·
[Native oracle](../scripts/koreader-render-parent-margins-check.lua) ·
[Reader tests](../src/block_margins/parent_tests.rs) ·
[Stock capture](screenshots/render-parent-margins-stock-web.png) ·
[Reader capture](screenshots/render-parent-margins-reader-web.png)

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

The provider supports simple text cards with explicit widths on the
horizontal physical sides. Consecutive cards can share either edge. It keeps publisher-defined percentages for width and uses a
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

### Consecutive floats and available width

Book and Web place a later float beside earlier floats when the gap is
wide enough. If the card does not fit, it drops to the first available
row and can stay beside a longer opposing float. The ordinary text keeps
its position beside the preceding card and wraps around the later one
when it reaches that row. Clearance on a floated card moves the card;
clearance on ordinary text moves the text below all cards on the specified
side.
[Pinned card placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6840-L6964)

The pinned native matrix asserts all 108 passage coordinates across
Legacy, Flat, Book and Web. Legacy and Flat leave the cards in ordinary
flow in this fixture. Book and Web place the two left cards at x=54 and
x=254 and the two right cards at x=970 and x=670. With a wide card on
the left, Book drops the next card to y=118 and Web drops it to y=150.
Both place the first line of ordinary text at x=734, y=54. Web still has
room beside the longer right float at y=150.

Reader's horizontal provider retains separate card positions and combines
the edges that intersect each text line's strut. Each paragraph uses
up to five individual rectangles. With six or more it combines the
widest edge and latest ending position on each side until another
paragraph starts. This matches the pinned footprint fallback. Reader bounds placement to 64
pending or active cards and falls back to normal text flow above that
limit. The two engines use different text metrics. Heights derived from
text can move, and the same 150-pixel card wraps “Far step” in Reader. [Native footprints](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L7003-L7163) · [Line strut](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L627-L686)

The Mac tests compare left and right placement, clearance, the contour
fallback, painted bands, selection and link targets. They reopen the
same chapter and compare the original source anchors and frame. To
repeat these captures, run the float tests with
`READER_FLOAT_STACKING_CAPTURES` set to an output directory.
[Native comparison script](../scripts/koreader-render-float-stacking-check.lua) ·
[KOReader stacked cards](screenshots/render-float-stacking-stock-web-shelf.png) ·
[Reader stacked cards](screenshots/render-float-stacking-reader-web-shelf.png) ·
[KOReader dropped card](screenshots/render-float-stacking-stock-web-dropped.png) ·
[Reader dropped card](screenshots/render-float-stacking-reader-web-dropped.png) ·
[KOReader six-card contour](screenshots/render-float-stacking-stock-web-dense.png) ·
[Reader six-card contour](screenshots/render-float-stacking-reader-web-dense.png)

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

## Oversized text groups with an authored keep

The native renderer measures a group before deciding whether to move it
to a fresh page. A group that fits there can stay together. When a group
is taller than a page, KOReader uses the space remaining below the
preceding text and wraps the group across page turns.

Reader’s internal policy now follows that behavior for transparent
horizontal wrappers directly in the document body. When their extent
exceeds one reading page, their ordinary paragraph children can flow
through the current page. A fitting wrapper still keeps its text
together. Explicit rules on the outer wrapper remain active.

| Authored group at 1264 × 720 | Pinned KOReader | Reader default | Reader with the policy |
| --- | --- | --- | --- |
| Three lines after seven preceding lines | Flat, Book and Web move all three to page 2. Legacy starts on page 1. | All three move to page 2. | All three move to page 2. |
| Twelve lines after four preceding lines | Starts on page 1 and ends on page 2 in all four modes. | Starts on page 2 and ends on page 3. | Starts on page 1 and ends on page 2. |

Page numbers are relative to each authored spine item. Tests recover a
serialized quote and its touch location after the opening line moves
back to page 1. Fitting groups and documents with publisher styles
disabled render byte-identical frames. Padded, bordered or decorated
wrappers, nested groups, active floats, images, special paragraph
rules and tables keep their existing path.

Generate the specimen with `python3 scripts/generate-render-text-keeps-fixture.py`.
Run the native oracle with the same arguments as the Render Mode probe.
Run `cargo test --locked --lib publisher_breaks` and set
`READER_TEXT_KEEP_CAPTURES` to an existing folder to save Reader frames.
[CREngine grouped page placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvpagesplitter.cpp#L921-L987) ·
[Native oracle](../scripts/koreader-render-text-keeps-check.lua) ·
[Reader tests](../src/publisher_breaks/text_keeps/tests.rs) ·
[KOReader Web](screenshots/render-text-keeps-stock-web.png) ·
[Reader default](screenshots/render-text-keeps-reader-default.png) ·
[Reader with the policy](screenshots/render-text-keeps-reader-bounded.png)

## Incoming margins at forced page breaks

In the pinned renderer, an ordinary forced break keeps the incoming
paragraph’s top margin on the new page. Flat, Book and Web discard
the preceding paragraph’s bottom margin at that edge. A normal edge
between the same paragraphs uses the collapsed margin.

Reader’s internal policy applies this rule to adjacent plain paragraphs
in the body when collapsed margins are enabled. It uses the incoming
margin for the document gap and positions the page boundary before
that space. The first anchor belongs to the paragraph that follows,
including when the top margin is zero. The ordinary layout continues
to use its existing page splitting.

| Authored case at 1264 × 720 | Flat, Book and Web | Reader with the policy |
| --- | --- | --- |
| Forced before, 94 px outgoing and 68 px incoming | 68 px at the top of page 2. Document pitch is 132 px. | 68 px at the top of page 2. Document pitch is 135.8 px. |
| Forced after, the same margins | The same top space and pitch. | The same top space and pitch. |
| Forced before, zero incoming margin | Text begins at the reading edge. Document pitch is 64 px. | Text begins at the reading edge. Document pitch is 67.8 px. |
| Ordinary edge with the same 94 px and 68 px margins | Both paragraphs share a page. Document pitch is 158 px. | Both paragraphs share a page. Document pitch is 161.8 px. |

The 3.8-pixel pitch difference follows each engine’s ordinary text
metrics. Reader’s painted band begins at screen y = 126 after the
68-pixel shift, compared with y = 122 in KOReader. The tests check
the painted shift, saved quote and touch recovery, and byte-identical
frames with Publisher styles off or outside the supported scope.
Reader applies its normal margin cap and resolves the cut between
complete lines. Legacy takes its own margin path. Nested or competing
keep rules, padded and bordered boxes, active floats, and negative
incoming margins use Reader’s existing path.

The stock oracle additionally checks a forced closing edge inside a
parent keep. Legacy breaks there. Flat, Book and Web retain both
paragraphs on one page. This is a separate case from the unambiguous
forced edges above.

Generate the specimen with
`python3 scripts/generate-render-forced-page-margins-fixture.py`.
Run the native oracle with the same arguments as the Render Mode probe.
Run `cargo test --locked --lib forced_page_margins` for Reader and
set `READER_FORCED_MARGIN_CAPTURES` to an existing folder to capture
its comparison frames.
[Forced margin accumulation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6240-L6288) ·
[Native page boundary](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6374-L6399) ·
[Native oracle](../scripts/koreader-render-forced-page-margins-check.lua) ·
[KOReader Web](screenshots/render-forced-margin-stock.png) ·
[Reader default](screenshots/render-forced-margin-reader-current.png) ·
[Reader with the policy](screenshots/render-forced-margin-reader-retained.png)

## Forced openings inside a kept group

KOReader’s enhanced block layout resolves a forced opening before the
final page splitter combines neighboring line flags. With an explicit
`page-break-before:always`, it can insert a zero-height boundary after
the preceding keep so the new paragraph starts another page. A
competing forced closing and keep can reach the final splitter, where
the keep has priority.

| Authored conflict at 1264 × 720 | Pinned Legacy | Pinned Flat, Book and Web | Reader with the opening policy |
| --- | --- | --- | --- |
| Forced opening inside a direct or nested parent keep | New page | New page | New page |
| Forced closing inside a direct or nested parent keep | New page | Same page | Same page |
| Forced opening after a sibling’s keep | Same page | New page | New page |
| Forced closing before a sibling’s keep | Same page | Same page | Same page |
| Unambiguous forced opening or closing | New page | New page | New page |

The internal policy applies the opening priority at a visible ordinary
paragraph with its own text interval. It accepts adjacent horizontal
paragraphs in the body or transparent `div`, `section` and `article`
wrappers without vertical spacing. Overlapping intervals, empty
separators, complex boxes and active floats keep their existing path.
The default continues to use the final splitter’s usual keep priority.

The tests restore a saved quote and its touch position after the
paragraph moves from page 1 to page 2. Cases where the policy leaves
the page assignment intact have byte-identical frames, including when
Publisher styles are disabled. The native comparison checks the
opening and closing targets through all four modes.

Generate the specimen with
`python3 scripts/generate-render-opening-breaks-fixture.py`.
Run the pinned oracle with the same arguments as the Render Mode probe.
Run `cargo test --locked --lib publisher_breaks` for Reader and set
`READER_OPENING_BREAK_CAPTURES` to an existing folder to save the
comparison frames.
[CREngine’s margin flags](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6230-L6272) ·
[Zero-height boundary](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L6484-L6502) ·
[Native oracle](../scripts/koreader-render-opening-breaks-check.lua) ·
[KOReader Web](screenshots/render-opening-break-stock-web.png) ·
[Reader default](screenshots/render-opening-break-reader-current.png) ·
[Reader with the policy](screenshots/render-opening-break-reader-enhanced.png)

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
