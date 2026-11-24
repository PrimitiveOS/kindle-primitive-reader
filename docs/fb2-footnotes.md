# FictionBook note previews

With **Footnote previews** enabled, Reader opens a linked FictionBook
`section` in a note popup when its original `body` is named `notes` or
`comments`. Body names are matched case-insensitively. A reference can
precede or follow its note, and a standalone link can open a note.
Reader preserves the provider's authored section IDs separately from the
XHTML it uses for layout. The metadata adds no markup, so existing
chapter fingerprints, anchors, and book positions keep their identity.
Ambiguous duplicate target IDs and links to note paragraphs use the
existing link rules.

The pinned KOReader ReaderUI ran four actual link taps on the same
[authored FictionBook](../tests/fixtures/workshop-notes.fb2). Both section
targets passed its native trusted-container branch with Prefer more
footnotes off. Each popup retained the second paragraph and excluded
unrelated prose. Both ordinary links followed their destinations. The Mac
Reader test checks note content, the backlink, identical source-page pixels
after closing either popup, location history, and reopen from the saved
position. The parser test checks nested entries, case-insensitive body
names, excluded paragraph targets, and duplicate IDs.
[Native trusted section detection](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L3109-L3133) ·
[Native popup comparison driver](../scripts/koreader-fb2-footnotes-check.lua) ·
[Reader workflow test](../src/app/footnote_popup/tests.rs)

[KOReader notes popup](screenshots/fb2-stock-notes.png) ·
[Reader notes popup](screenshots/fb2-reader-notes.png) ·
[KOReader comments popup](screenshots/fb2-stock-comments.png) ·
[Reader comments popup](screenshots/fb2-reader-comments.png)

KOReader's independent CSS tweaks for in-page notes and endnotes use a
footnote-aware page splitter. It reserves part of the reading page,
deduplicates repeated references on the page, and can carry a long note
across page boundaries. That pagination and the associated styling
controls remain open in Reader. The popup comparison used a Mac SDL
runtime and has no physical Kindle evidence.
[In-page style rules](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L851-L947) ·
[Native footnote page splitter](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvpagesplitter.cpp#L679-L810)

## Replay

Capture the Reader frames by setting `READER_FOOTNOTE_ROOT_SCREENS` to an
output directory and running
`cargo test --locked --features kindle --lib fb2_named_note_sections`.
The stock check runs as a priority-2 patch in an isolated profile. From
this repository, set `stock` to the absolute pinned Mac reference
directory with build output in `build/staging` and the runtime in
`runtime/koreader`. Run the following command.

```sh
scratch=$(mktemp -d)
mkdir -p "$scratch/profile/patches"
cp scripts/koreader-fb2-footnotes-check.lua "$scratch/profile/patches/2-fb2-footnotes.lua"
cp tests/fixtures/workshop-notes.fb2 "$scratch/"
printf '%s\n' 'return {language="en", emulator_fake_wifi_connected=false, color_rendering=false, use_xtext=true, footnote_link_in_popup=true, link_prefer_footnote=false}' > "$scratch/profile/settings.reader.lua"
cd "$stock/runtime/koreader"
KO_HOME="$scratch/profile" KO_DYLD_PREFIX="$stock/build/staging" \
  SDL_VIDEODRIVER=dummy READER_FB2_STOCK_PROOF="$scratch" \
  EMULATE_READER_W=1264 EMULATE_READER_H=1680 EMULATE_READER_DPI=300 \
  EMULATE_BW_SCREEN=1 EMULATE_BB_TYPE=BB8 ./luajit reader.lua "$scratch/workshop-notes.fb2"
```
