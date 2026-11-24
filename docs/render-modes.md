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
| Legacy | Preserve a consistent legacy block and inline model, including uncollapsed publisher margins and the old handling of incomplete CSS tables |
| Flat | Add a source-owned block margin model with CSS collapse, individual signed margins, a nonnegative final gap, prepared inline float boxes, anonymous box repair, and page-break coalescing through empty boxes |
| Book | Add anchored float exclusion geometry and clearing to the Flat model, with applicable style-width and auto-margin behavior |
| Web | Add signed final margins, full publisher box sizing and automatic placement, overflow-aware painting and hit-testing, and consecutive forced breaks through empty content |

The four options depend on independent box geometry, pagination and
source mappings. A Reader renderer choice would currently change several
of these behaviors together. The **Render Mode** matrix item remains open.

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
