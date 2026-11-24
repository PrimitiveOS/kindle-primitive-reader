# Exact-value settings in landscape

Reader keeps the selected screen turn while opening, editing, and closing
in-book numeric settings. Text controls keep their existing step, held
step, and default actions. Word spacing keeps separate Scaling and
Reduction drafts and saves them together. The footnote popup font editor
shows Relative and Fixed choices alongside the current value. Back or
the physical back-page action discards the draft.

The complete flow uses the active viewport, including the core settings
lists for Links, Page numbering, Reading guides, Auto turn, fixed-page
pan, and the reading status bar. In landscape the value field, range,
and validation message fit to the left of inline adjustment controls.
Both columns stay above the shared keyboard. Portrait uses its existing
field, controls, and keyboard positions.
[Landscape Word spacing](screenshots/landscape-numeric-editor.png)

## Pinned KOReader comparison

The KOReader v2026.07.1 Mac runtime opened an authored EPUB and executed
the real Word Spacing and Footnote popup font size dialogs. The word
spacing controls accept 10–500% Scaling and 25–100% Reduction, with a
held step of 10. Entering 123/67 updates both drafts. Canceling a second
entry leaves its previous draft in place, and Apply commits the pair.
The footnote editor starts at −2 in the relative range −10 to +5.
Switching to the fixed range of 12–255 leaves the saved preference intact.
[Word spacing options](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L458)
· [Footnote settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L361)

KOReader opens a separate numeric entry dialog when the spinner value
is tapped. Reader keeps both its inline controls and its shared keyboard
visible. Reader preserves its established Page Back cancellation and
returns from Set to the originating reading panel or settings list.
[Stock number entry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/numberpickerwidget.lua#L127)

## Host verification

Run `cargo test --locked --lib numeric_editor`. The host tests stage,
draw, abort, and commit 1680 × 1264 Reader viewports. They cover both
landscape turns, touch and page-key input, unchanged drafts after a
rejected viewport, saved per-book word spacing, invalid footnote values,
and a PDF pan preference at the same document location. Set
`READER_LANDSCAPE_TOOLS_CAPTURES` to a directory to render the screenshot.
