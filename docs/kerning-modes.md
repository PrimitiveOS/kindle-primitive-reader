# Font kerning modes

The pinned KOReader Text/Rendering control offers Off, Fast, Good and Best.
The Rust Reader now exposes the same four choices, saves the selected mode per
book, and applies OpenType shaping features during EPUB layout and annotation
layout. Legacy `font_kerning` booleans still load as Off or Best.

| Mode | KOReader behavior | Rust Reader behavior and remaining difference |
| --- | --- | --- |
| Off | No kerning. | Disables horizontal and vertical kerning features. |
| Fast | FreeType pair kerning without ligatures. | Applies classic horizontal font pairs with FreeType's whole-pixel fitting. Reader uses the result for wrapping, glyph positions, carets and intrinsic widths within eligible left-to-right runs. |
| Good | HarfBuzz-light shaping: kerning, mark/cursive positioning and localized forms, without ligatures; limited support for non-Western scripts. | Applies the pinned allowlist through Harfrust. The Rust and KOReader engines can still differ in glyph advances, fallback, and script behavior. |
| Best | Full HarfBuzz shaping, including ligatures and joined-script shaping. | Uses Parley's default full shaping through Harfrust. It is the closest conceptual equivalent, but is not pixel- or glyph-identical to KOReader's HarfBuzz/FreeType stack. |

The pinned KOReader configuration documents the four labels and their intended
tradeoffs in [`creoptions.lua`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L635-L649).
Its HarfBuzz-light allowlist is in
[`lvfntman.cpp`](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvfntman.cpp#L1770-L1815).
KOReader applies the selected mode and restores the reading position in
[`readerfont.lua`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfont.lua#L223-L227).

## Fast pair spacing

The pinned FreeType 2.14.3 build reads classic version-0, format-0
horizontal pairs. It scales the signed adjustment to the selected face
size, reduces it below 25 pixels per em and rounds it to a whole pixel.
Reader uses the same arithmetic on font data already available in its
layout engine. Modern fonts with kerning only in OpenType GPOS, including
the Noto Sans fixture, contribute zero pair spacing in Fast.
[CREngine pair lookup](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvfntman.cpp#L3286-L3365) ·
[FreeType grid fitting](https://github.com/freetype/freetype/blob/VER-2-14-3/src/base/ftobjs.c#L3603-L3688) ·
[Reader implementation](../vendor/parley/src/fast_kerning.rs)

The pair is applied only while both glyphs occupy the same line. A hidden
soft hyphen does not interrupt an eligible pair within a word. Supported
pairs cover horizontal, left-to-right single-glyph atoms inside one shaped
font run. Reader retains its own glyph selection, base advances and
rasterization. Vertical text, complex graphemes, right-to-left runs and
cross-run kerning remain outside this comparison.

The 42-pixel specimen reproduces the same signed changes in Reader and in
the stock CREngine caret position of the following word.

| Pair | Fast caret change |
| --- | ---: |
| AV | −10 px |
| VA | +5 px |
| To | −3 px |

The native replay invokes the stock font-mode callback. It also queries
the pinned FreeType library for the three pairs at 12, 16, 24, 25, 42 and
48 pixels per em. Reader tests check the same 18 adjustments in advances,
painted positions, caret geometry and intrinsic widths. A narrow wrap
and a hidden soft hyphen exercise the final line boundary.
[Native replay](../scripts/koreader-fast-kerning-check.lua) ·
[Pair-spacing fixture](../tests/fixtures/fast-pair-kerning.epub) ·
[Fast mode on KOReader](screenshots/fast-kerning-stock.png) ·
[Fast mode on Reader](screenshots/fast-kerning-reader.png)

## Verification status

Focused layout tests distinguish the feature sets using Noto Sans. The settings
workflow test selects each choice through the rendered bottom bar, verifies the
active mode and per-book saved preference, reopens the EPUB, and checks that the
mode survives. A separate workflow preserves the source anchor and confirms
that the source file remains byte-identical across reflow and reopening. It
passed with an authorized original EPUB. `READER_KERNING_SCREENSHOT` captures
the Rendering panel, and `READER_FAST_KERNING_SCREENSHOT` captures the
packaged specimen. Arabic, combining-mark and non-Western fallback
comparisons remain open, along with comparison against KOReader on the Kindle.

![Reader Rendering panel with all four kerning choices](screenshots/font-kerning-modes.png)
