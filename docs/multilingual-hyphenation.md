# Multilingual hyphenation

Choose **Appearance › Typography rules › Hyphenation** while reading. The
dictionary row names the main language's dictionary. **Hyphenation limits**
shows its default character counts, and a saved global override applies to
all supported dictionaries. Embedded language tags select their own rules
inside a mixed-language book when **Respect embedded language tags** is on.

| Dictionary | Before / after | Pinned rules |
| --- | ---: | ---: |
| Spanish | 2 / 2 | 4,692 |
| Italian | 2 / 2 | 326 |
| Portuguese (Portugal) | 2 / 3 | 308 |
| Estonian | 2 / 3 | 3,691 |
| Finnish | 2 / 2 | 284 |

Reader compiles upstream TeX patterns into immutable tables. The build
normalizes word-edge markers, removes patterns for punctuation outside the
supported word runs, and gives two Portuguese exceptions the same weights as
CREngine. Fingerprints of the compiled rule lists match the pinned
CREngine patterns that can apply to a single word. CREngine reports 285 Finnish
patterns. It combines the two TeX entries `r2as l2as` into one pattern
with an internal word boundary. The matcher adds boundaries only at each
word's edges, so Reader compiles the 284 reachable patterns. The existing US
and UK English tables retain their fingerprints. Brazilian Portuguese,
French, and German remain open.
[Pinned language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L56-L111) ·
[Spanish, Italian, and Portuguese sources](licenses/HYPHENATION-ES-IT-PT.txt) ·
[Estonian and Finnish sources](licenses/HYPHENATION-ET-FI.txt)

The matcher composes accented letters for lookup and derives each break from
the original grapheme's byte range. Layout paints a hyphen only at a fitted
line break. Searching or selecting a split word returns its original text
and coordinates. Publisher CSS, soft hyphens, limits, and user exceptions use
the existing rules. These dictionaries apply to horizontal reflowable text.

## Mac comparison

The pinned native replay verifies nineteen Spanish, Italian, Portuguese,
Estonian, and Finnish words by measuring where CREngine breaks each one over a
range of screen widths. It verifies language defaults, repeats the measurement
with a 4 / 4 override, and checks that no word breaks with hyphenation off.
CREngine's dictionary API also confirms the candidate `lü-` in
`lühiülevaade`. In the width sweep, the stock page layout uses `lühi-` and
`lühiüle-`. The Reader screenshot at 574 px uses `lü-`. The Rust tests
verify dictionary candidates, decomposed accents, mixed inline languages,
search, hit testing, and saved anchors.
[Spanish, Italian, and Portuguese specimen](../tests/fixtures/multilingual-hyphenation.xhtml) ·
[Estonian and Finnish specimen](../tests/fixtures/estonian-finnish-hyphenation.xhtml) ·
[Native replay](../scripts/koreader-multilingual-hyphenation-check.lua) ·
[Rust dictionary tests](../src/hyphenation.rs)

Run the Lua replay from the pinned KOReader runtime. Pass the XHTML specimen,
a new CRE cache directory, `NotoSans-Regular.ttf`, and a capture directory.
Pass `estonian-finnish-hyphenation` as the fifth argument for that specimen.
The Mac Reader rehearsal runs with the `multilingual_hyphenation` test filter.
It uses the same specimens and font at 570 × 864 and 574 × 864 px respectively.
Set `READER_MULTILINGUAL_HYPHENATION_CAPTURE` for the pages and
`READER_SELECTION_VIEWPORT_CAPTURES` for the in-app settings screenshots.

| Mode | Reader | KOReader |
| --- | --- | --- |
| Dictionary defaults | [Page](screenshots/reader-multilingual-hyphenation.png) | [Page](screenshots/stock-multilingual-hyphenation.png) |
| 4 / 4 override | [Page](screenshots/reader-multilingual-hyphenation-limits.png) | [Page](screenshots/stock-multilingual-hyphenation-limits.png) |
| Hyphenation off | [Page](screenshots/reader-multilingual-hyphenation-off.png) | [Page](screenshots/stock-multilingual-hyphenation-off.png) |

[Reader settings for Spanish](screenshots/multilingual-hyphenation-settings.png)

| Estonian and Finnish mode | Reader | KOReader |
| --- | --- | --- |
| Dictionary defaults | [Page](screenshots/reader-estonian-finnish-hyphenation.png) | [Page](screenshots/stock-estonian-finnish-hyphenation.png) |
| 4 / 4 override | [Page](screenshots/reader-estonian-finnish-hyphenation-limits.png) | [Page](screenshots/stock-estonian-finnish-hyphenation-limits.png) |
| Hyphenation off | [Page](screenshots/reader-estonian-finnish-hyphenation-off.png) | [Page](screenshots/stock-estonian-finnish-hyphenation-off.png) |

[Reader settings for Estonian](screenshots/et-hyphenation-settings.png) ·
[Reader settings for Finnish](screenshots/fi-hyphenation-settings.png)
