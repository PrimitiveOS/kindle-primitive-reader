# EPUB paragraph indentation

Reader honors publisher `text-indent` when laying out horizontal EPUB paragraphs. The starting edge follows the paragraph direction. An `em` value uses the computed font size of the element declaring it, so changing a descendant's font size preserves the inherited distance. Percentages use the width of the current text column. An explicit zero or `initial` clears an inherited indent. The `hanging` keyword applies the distance to wrapped lines. Turning off publisher styles restores flush paragraphs.

Negative lengths can move the first line outside its normal starting edge. At a reading-column boundary Reader keeps that line visible and moves the following lines inward. Oversized lengths leave room for at least a glyph. Reader supports `em`, `rem`, percentages, CSS absolute lengths and unitless zero. Full publisher margin/padding geometry, vertical indentation, `calc()` and the optional `each-line` keyword remain separate work.

KOReader v2026.07.1's pinned CREngine parses signed lengths, percentages and the hanging modifier, computes a per-paragraph indent against the available width and protects the page boundary when overflow is disabled. Reader passes line geometry to Parley. The EPUB text, search terms and saved byte offsets stay intact. [CREngine CSS parser](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L4051-L4080) · [Block geometry](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8854-L8902) · [Line formatting](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L5935-L5951) · [CSS Text](https://www.w3.org/TR/css-text-3/#text-indent-property)

When the publisher supplies padding around a paragraph, a negative indent can draw into that leading space. The same rule applies at the right edge of RTL text. [Publisher box padding](publisher-box-padding.md)

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

Two further positive controls covered the other two books with nonzero indentation. A 7% rule placed the first line at x = 62 in a 552 px column with a 24 px margin. A 1em rule placed it at x = 56. Across all 19 measurements in seven books, seven positive, six negative and six explicit-zero cases matched their expected direction or reset. All 323,472 nonspace characters mapped in the 17 distinct chapters. The one-font desktop fixture left two glyphs missing in one additional chapter.

## Oasis 3 verification

Three automated ARM tests passed, covering the stylesheet limit, signed geometry, right-to-left alignment, tap selection, search and saved positions. On two original EPUB files, measured line starts matched the Mac for positive, negative, zero and the later citation rule. Two authored controls matched as well. Reader restored a position written by the 0.1.10 ARM probe in both a genuine EPUB and the authored example. Both page images changed after applying indentation and all text in both test chapters mapped.

The authentic EPUB sample used a peak 4,348 KiB with both builds. The authored sample used 3,356 KiB on the baseline and 3,500 KiB on the candidate. These are two short offscreen probes with one font. The installed-library sweep ran concurrently. Source document hashes, the Documents listing, the installed Reader executable and checked live library files matched before and after the successful run. The temporary device workspace was removed.

## Publisher rules for a paragraph after a heading

Reader supports CSS child (`>`), adjacent-element (`+`) and general-sibling (`~`) relationships, along with `:first-child` and `:first-of-type`. Text, comments and Reader's own anonymous layout boxes do not interrupt sibling order. A hidden element still participates in sibling order, including when finding the first element of a type. General-sibling selectors use a bounded backtracking search of at most 128 candidate nodes per match; if that budget is exhausted, that selector does not match. Other selectors retain their existing matcher. Reader keeps search and bookmark positions tied to the original text. An unsupported selector in an ordinary comma-separated selector list invalidates that group. A following independent rule can still apply. This follows the pinned engine's selector-list error handling. General-sibling behavior is an unverified implementation candidate. [CREngine combinators](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L5999-L6077) · [First-of-type matching](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L6306-L6332) · [Selector-list parsing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L7466-L7547) · [W3C selector relationships](https://www.w3.org/TR/selectors-4/#adjacent-sibling-combinators)

Reader also recognizes `:empty` and `:dir(ltr)` / `:dir(rtl)`, including inside supported selector functions. It records direct element or nonempty text children and inherits the nearest explicit HTML `dir` value; an invalid or `auto` value stops that inheritance and matches neither direction, as in the pinned CRE selector. Comments are ignored, retained whitespace counts as text, and CSS `direction` does not set this selector value. Source extraction records these fields in linear DOM work without changing saved text or anchors. Reader and CRE may differ where their XML parsers normalize or discard whitespace; automatic-direction inference is not implemented for this selector. This implementation is unverified. [CRE selector matching](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L6172-L6193)

`:root` matches the chapter body under HTML or DocFragment, following the pinned CREngine selector rather than a browser's HTML-root match. It contributes ordinary pseudo-class specificity and works in the supported selector functions. This enables directly supported publisher properties on the chapter body; CSS custom properties and `var()` remain unsupported. Reader's existing root-relative font-size calculation also remains tied to its outer parsed element, so `rem` behavior after `:root` font-size changes may differ. Source identity and saved positions are unchanged. This implementation is unverified. [CRE root selector](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstsheet.cpp#L6144-L6169)

The native selector candidate also supports `:last-child`, `:only-child`,
`:last-of-type`, `:only-of-type` and numeric or `an+b` forms of `:nth-child()`,
`:nth-last-child()`, `:nth-of-type()` and `:nth-last-of-type()`. Sibling
positions are computed in a linear pass; type comparisons retain namespace
identity, and non-element nodes and anonymous wrappers do not count. The
`of <selector>` variants remain open.
These additions have not
yet been behaviorally tested in simulation or on Kindle.

Publisher selectors can also use `:not(...)`, `:is(...)` and `:where(...)`
with lists of supported single compounds and up to four nested functions.
`:not` and `:is` use the most specific argument; `:where` adds no specificity.
Complex combinators and pseudo-elements inside these functions remain
unsupported. Unsupported list alternatives invalidate the function rather
than receiving CSS's forgiving-list behavior. This is an unverified candidate;
existing selector and source-position bounds apply.

The bounded attribute-selector candidate also recognizes `class`, `id` and
`href` presence and value operators alongside the existing `type` support.
It reuses Reader's CRE-compatible local-name and comparison behavior. Other
local attribute names, such as `lang`, `role` and `data-*`, are captured with
a 64 KiB accounted name/value and tuple-size budget, an 8,192-entry chapter
limit, at most 32 attributes per element and a 4 KiB value limit. Allocator and
temporary-vector overhead are additional. On incomplete capture, a compound consulting those generic
attributes is declined, including inside nested negation or alternatives.
CRE's special inner-text `_` selector and CSS namespace bindings remain unsupported. The
source extractor retains these attribute values for selector matching without
changing the publication text or saved anchors. Multiple attributes sharing
the same local name are treated as ambiguous by the new value selectors. Any
compound consulting an ambiguous value is declined, including through nested
negation or alternatives. Existing `[href]` presence and link handling retain their prior
behavior. CRE's empty prefix/suffix comparisons can match an empty operand,
which differs from standard CSS substring semantics. Runtime validation is pending.

In anonymous EPUB 061, the first paragraph in spine item 57 follows a second-level heading. A grouped publisher rule resets its indent to zero. The first three selection rectangles previously began at x = 56, 24 and 24. They now begin at x = 24, 24 and 24. The 311-byte paragraph matched its source exactly, with 5,947 of 5,947 nonspace characters mapped across the chapter. The authored comparison below shows an opening paragraph and its indented continuation in two sections. [Fixture](../tests/fixtures/paragraph-opener.xhtml)

![Opening paragraphs align with their headings and the next paragraph starts farther in](screenshots/paragraph-opener.png)

The same original was checked headlessly on the Oasis 3 with ARM builds immediately before and after structural selector support. The heading reset changed from 56 / 24 / 24 to 24 / 24 / 24. The separate hanging citation stayed at 24 / 56 / 56. All four paragraphs in the authored fixture followed their expected rules. The earlier build saved the real publisher fragment for the opening paragraph, and the new build restored and reserialized the identical anchor. Both mapped all 5,947 nonspace characters in that spine item. The original EPUB's SHA-256 and the checked live Reader files matched before and after the test, and both temporary device directories were removed.
