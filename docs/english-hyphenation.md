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

Reader inherits language from EPUB package metadata or MOBI/KF8 EXTH record
524 when a reading section omits it. A nearer XHTML `lang` or `xml:lang`
takes precedence. Automatic applies to generic, US and British English,
including explicit Latin script. Other English regions,
unknown languages and explicit empty tags retain authored opportunities.
Publisher `hyphens: none`, `manual` and `auto` inherit across inline markup.
A word with an explicit soft hyphen uses its publisher’s breaks.

For American English, Reader builds the equivalent 6,708 weighted rules from 4,938 original TeX patterns,
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
intact for search, selection, annotation and position storage. User exceptions,
spelling-changing breaks and controls for
CREngine’s unused-space threshold remain open. [Publisher source and terms](licenses/ENGLISH-HYPHENATION.txt) · [Shaper contract](../vendor/parley/PRIMITIVEOS-PATCH.md)

## British English and Kindle-format books

The original British pattern file contains 8,527 patterns and eight exceptions.
Reader builds 8,535 weighted rules. The complete normalized table matches the
pinned CREngine British dictionary byte for byte (SHA-256
`a0a2a0834a41bc7c0ce001de5c0d654b19a0475814b234d4dc4ed3d838b6499e`).
The Rust and independent Python tests check all eight corrected spellings.
A word split across style spans keeps its breaks when the spans agree on
the dictionary. If their dictionaries differ, only authored breaks apply.
[British pattern source and terms](licenses/ENGLISH-HYPHENATION.txt) · [CREngine British dictionary](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/English_GB.pattern) · [Reference verifier](../scripts/verify-english-hyphenation.py)

![Reader shows the same passage in two regions. American English breaks reciprocity after reciproc, British English after recipro.](screenshots/english-regions.png)

The two paragraphs in this single page use the same font and width.
Both regional declarations act inside a document tagged simply English.
The preview mapped all 127 nonspace characters. [Authored page](../tests/fixtures/english-regions.xhtml)

```sh
cargo run --locked --bin reader-probe -- parley tests/fixtures/english-regions.xhtml \
    --font tests/fixtures/fonts/NotoSans-Regular.ttf --width 235 --height 395 \
    --size 25 --margin 13 --output regional-lines.png
```

All 16 AZW3 and one legacy MOBI in the private Mac sample carried EXTH 524.
Ten declared English, five US English and two Chinese. Of the 501 reading
sections, 292 contained words without a markup language. Publication metadata
supplied English in 241 and Chinese in 51. One generically English AZW3
also contained 1,476 British-tagged text nodes in 44 sections. The pinned
CREngine PDB importer sets its publication language from that same EXTH
record. Reader accepts the first valid bounded BCP 47 record and preserves
the normalized XHTML and its source offsets. The pinned engine has no
Vietnamese dictionary. Reader preserves authored Vietnamese breaks. [CREngine PDB importer](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L897-L906) · [CREngine language table](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L61-L122)

In three Mac comparisons the earlier 0.1.9 layout saved the source anchor
from page two. Each current layout reopened it, with every source character
covered and zero missing glyphs. The simulator used Noto Sans Regular and
Bold at 32 px on a 600 × 800 viewport with 24 px margins.

| Section language | Nonspace characters | Previous pages | Automatic pages | Pixels changed at the saved position | Authored-mode pixels changed |
| --- | ---: | ---: | ---: | ---: | ---: |
| US English from AZW3 metadata | 15,614 | 35 | 34 | 54,964 | 0 |
| British English from AZW3 markup | 8,510 | 21 | 21 | 11,626 | 0 |
| Generic English from legacy MOBI metadata | 58,184 | 160 | 158 | 13,124 | 0 |

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

## Kindle Oasis 3

On the same 13,306-character English chapter, a separate static ARMHF build
from the Reader 0.1.8 tag saved a page-four bookmark. The candidate restored
that bookmark in both modes with the five fonts available to the installed
Reader. Four fresh headless processes ran in Authored, Automatic, Automatic,
Authored order after the other device tests completed.

| Oasis measurement | Authored | Automatic |
| --- | ---: | ---: |
| Source characters mapped across 14 pages | 13,306 / 13,306 | 13,306 / 13,306 |
| Missing glyphs | 0 | 0 |
| Lines occupied by the same eight-letter selection | 1 | 2 |
| Time through setup, selection and first painted frame | 4.50–5.31 s | 4.50–4.94 s |
| First paint alone | 27–28 ms | 27 ms |
| Process CPU through first frame, in kernel clock ticks | 429–501 | 432–440 |
| Peak resident memory | 36,924 KiB | 37,268 KiB |

Both taps on the split word returned its entire original spelling, and chapter
search returned its original start and end offsets in every run. Both
automatic renders differed from their control in 165,541 pixels overall and
160 pixels at the chosen line end. On this chapter the additional peak memory
was 344 KiB. The timings overlap across the two samples per mode.

The Vietnamese and explicitly undetermined chapters retained identical
first-page pixels on the Oasis. All three original files had matching SHA-256
digests before and after the comparison. The job used private scratch storage
and rendered its frames offscreen. [Reader 0.1.8](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.8)
