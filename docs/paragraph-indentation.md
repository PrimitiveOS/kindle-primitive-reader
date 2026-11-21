# EPUB paragraph indentation

Reader honors publisher `text-indent` when laying out horizontal EPUB paragraphs. The starting edge follows the paragraph direction. An `em` value uses the computed font size of the element declaring it, so changing a descendant's font size preserves the inherited distance. Percentages use the width of the current text column. An explicit zero or `initial` clears an inherited indent. The `hanging` keyword applies the distance to wrapped lines. Turning off publisher styles restores flush paragraphs.

Negative lengths can move the first line outside its normal starting edge. At a reading-column boundary Reader keeps that line visible and moves the following lines inward. Oversized lengths leave room for at least a glyph. Reader supports `em`, `rem`, percentages, CSS absolute lengths and unitless zero. Full publisher margin/padding geometry, vertical indentation, `calc()` and the optional `each-line` keyword remain separate work.

KOReader v2026.07.1's pinned CREngine parses signed lengths, percentages and the hanging modifier, computes a per-paragraph indent against the available width and protects the page boundary when overflow is disabled. Reader passes line geometry to Parley. The EPUB text, search terms and saved byte offsets stay intact. [CREngine CSS parser](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L4051-L4080) · [Block geometry](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8854-L8902) · [Line formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L5935-L5951) · [CSS Text](https://www.w3.org/TR/css-text-3/#text-indent-property)

## Desktop comparison

Both images are complete 720 × 1000 captures of the same authored file with 31 px text and a 38 px margin. The first paragraph inherits a first-line indent. The second explicitly resets it. The last two exercise percentage and hanging indents. [Source fixture](../tests/fixtures/paragraph-indents.xhtml)

| Reader 0.1.10 | Publisher indentation |
|---|---|
| ![Four paragraphs with a common left edge](screenshots/paragraph-indents-0.1.10.png) | ![First-line indentation, an explicit zero and a hanging last paragraph](screenshots/paragraph-indents.png) |

## Original-file checks

A local stylesheet survey of ten distinct EPUBs found 256 `text-indent` declarations across eight books. Among visible paragraphs of at least 70 characters, seven books contained nonzero indents. Matched styles specified 8,117 positive and 787 negative indents. Seven damaged archive members were unreadable to this independent survey. The public fixture and screenshots contain only original project text.

Using the actual Reader engine at 600 × 800 with a 24 px margin and 32 px Noto Sans, the five positive samples below all started at x = 24 on each line in the previous implementation. The updated positions came from the highlight rectangles over the same source paragraphs. All 59,736 nonspace characters mapped across these five chapters.

| Anonymous book | Spine / paragraph (1-based) | Publisher indent | First / second / third line x |
|---|---|---|---|
| 035 | 4 / 4 | 1.4em | 68 / 24 / 24 |
| 042 | 7 / 34 | 1em | 56 / 24 / 24 |
| 046 | 7 / 3 | 1.5em | 72 / 24 / 24 |
| 052 | 6 / 2 | 1.64em | 72 / 24 / 24 |
| 053 | 13 / 4 | 1.8em | 67 / 24 / 24 |

The independent source and geometry comparison also found a stylesheet-budget issue. One EPUB's five stylesheets included 398 top-level rules. An applicable citation rule appeared after 256 accepted rules. Reader now skips selectors whose class or ID cannot occur in the loaded chapter before counting the retained rules. It keeps the existing per-sheet parser and chapter rule bounds and caps the name index. A citation that previously started at x = 56 now starts at x = 24, with later lines at x = 56. [Geometry verifier](../examples/verify_paragraph_indent.rs)

To measure the authored fixture locally, run the following command from the Reader repository.

```sh
cargo run --locked --example verify_paragraph_indent -- \
  tests/fixtures/paragraph-indents.xhtml 1 1 tests/fixtures/fonts/NotoSans-Regular.ttf
```
