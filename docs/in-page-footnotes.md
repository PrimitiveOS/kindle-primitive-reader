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

The compositor covers explicit EPUB and ARIA notes in a trailing rendered
region referenced from the same spine chapter. All targets must resolve in a
horizontal, left-to-right layout. Overlapping glyphs within a text band stay
together, and overlapping note boundaries use the normal layout. Interleaved,
nested and cross-spine notes, complex publisher breaks, images and tables,
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
  cargo test --locked --features kindle --lib in_page_footnote_style
READER_IN_PAGE_FOOTNOTE_CAPTURES="$scratch/captures" \
  cargo test --locked --features kindle --lib footnotes::in_page::tests
cd "$stock/runtime/koreader"
KO_HOME="$scratch/profile" KO_DYLD_PREFIX="$stock/build/staging" \
  SDL_VIDEODRIVER=dummy ./luajit "$reader/scripts/koreader-in-page-footnotes-check.lua" \
  "$reader/tests/fixtures/in-page-footnotes.epub" "$scratch/cache" \
  "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf" "$scratch/captures"
```
