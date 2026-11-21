# English hyphenation in horizontal reading

In the lower Text panel, **Automatic** uses the book’s language and publisher
CSS. **Authored** uses discretionary soft hyphens supplied in the text. **Off**
disables both. Reader saves this choice per book and in typography profiles.
An existing saved book that disabled hyphenation still opens with it off.

![Two Reader captures of the same narrow English page. The left column keeps whole words. The right column breaks words at dictionary boundaries.](screenshots/english-hyphenation.png)

The two simulator pages use the same Noto Sans face, 24 px text, 11 px margins
and 234 × 540 viewport. The fixture contains no authored soft hyphens. Both
modes mapped all 223 nonspace characters and reported zero missing glyphs.
The first paragraph took ten lines with Authored and eight with Automatic.
An independent comparison of both pattern tables found the same 35 possible
break positions across the fixture’s 19 eligible words.

```sh
for mode in authored automatic; do
    cargo run --locked --bin reader-probe -- parley tests/fixtures/english-hyphenation.epub \
        --font tests/fixtures/fonts/NotoSans-Regular.ttf --width 234 --height 540 \
        --size 24 --margin 11 --hyphenation "$mode" --output "$mode.png"
done
```

## How the two readers choose breaks

KOReader enables embedded language tags and hyphenation by default, with
book-specific settings taking precedence over global settings. Its Lua bridge
initializes the installed hyphenation directory and passes the reading options
to CREngine. [Reader typography](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypography.lua#L698-L735) · [Dictionary initialization](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L115-L120) · [Native settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1149-L1175)

The pinned CREngine chooses the full language tag, then its primary language.
Its English table gives British English its own patterns and uses American
English for generic English, with at least two letters before and three
after a break. It also has a US fallback for unrecognized languages.
Its US dictionary adds the TUGboat correction list to the same 4,938 base patterns used by Reader. [CREngine US dictionary](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/English_US.pattern#L1-L9)

The formatter uses the CSS permission, measured advances and unused line
space when deciding whether to try a hyphen. [Language table](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L61-L66) · [Language selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L178-L199) · [TeX pattern matching](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/hyphman.cpp#L944-L1034) · [Line fit](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtextfm.cpp#L5381-L5475)

Reader uses the EPUB package language when the XHTML omits a language.
A nearer HTML `lang` or `xml:lang` takes precedence. Automatic applies to
generic or US English, including explicit Latin script. Other regions,
unknown languages and explicit empty tags retain authored opportunities.
Publisher `hyphens: none`, `manual` and `auto` inherit across inline markup.
A word with an explicit soft hyphen uses its publisher’s breaks.

Reader builds the equivalent 6,708 weighted rules from 4,938 original TeX patterns,
14 original exceptions, 1,753 TUGboat corrections and three doubled-consonant
corrections. A higher matching weight takes precedence. This preserves extra
base-pattern opportunities where the published dictionary adds a preferred
break. The sorted normalized rules match the pinned CREngine dictionary
byte for byte (SHA-256 `9185d734d23492ac9872496e617b2a8e6ef19612a74e8f5d9a8e56817ee8b817`).
The Rust test also checks the complete set of resulting source offsets for
all 1,766 distinct exception spellings against an independent evaluator.
[TeX Users Group correction list](https://ctan.org/pkg/hyphenex) · [Reference verifier](../scripts/verify-english-hyphenation.py)

Its matcher checks ASCII words of 5–64 letters and supplies up
to 4,096 offsets per paragraph. The shaper accepts boundaries after complete
single-character LTR shaping atoms. It measures the face’s U+2010 or U+002D,
and paints it only when a line actually ends there. The source word remains
intact for search, selection, annotation and position storage. British English
patterns, user exceptions, spelling-changing breaks and controls for
CREngine’s unused-space threshold remain open. [Publisher source and terms](licenses/ENGLISH-HYPHENATION.txt) · [Shaper contract](../vendor/parley/PRIMITIVEOS-PATCH.md)

## Tests with existing books

Eleven private EPUB copies, including one duplicate, declared English in ten
packages and Vietnamese in one. Using Reader’s normal archive member loader,
309 of 344 spine records had an English section root after language inheritance,
11 Vietnamese and 23 an undetermined language. One missing spine member
produced the existing recovery placeholder. The counts measure declared language tags.

On a US English source chapter, Reader mapped all 13,306 nonspace characters
over 14 pages with Noto Sans Regular and Bold on the Mac. The audit checked
the EPUB package and every text run in the sampled source block. A single
eight-letter source word occupied one line in Authored and two lines in
Automatic. Tapping either fragment selected the complete word and source
search returned the same UTF-8 endpoints. The two page images differed in
165,541 pixels, including 160 in the 24-pixel strip at the chosen line end.
[Reflow comparison harness](../examples/verify_english_hyphenation.rs)

A Vietnamese chapter provided a negative control. Both modes mapped all
42,733 nonspace characters over 49 pages and produced identical first-page
pixels. A second control with an explicitly empty language tag mapped all
4,682 characters across seven pages and also produced identical first-page pixels. Six fresh Mac debug processes per mode alternated
run order, using Noto Sans Regular at 43 px on a 1,264 × 1,680 viewport.
Median layout time was 3.69 s for Authored and 3.64 s for Automatic.
Median time through the first painted frame was 3.71 s and 3.66 s,
respectively. Other desktop build work overlapped part of the measurement. The simulator test also taps
each fragment of a split word and restores the same serialized anchor after
repagination. [Authored fixture](../tests/fixtures/english-hyphenation.epub) · [Layout and selection tests](../src/parley_layout.rs)
