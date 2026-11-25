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
| Portuguese (Brazil, shared rules) | 2 / 3 | 308 shared Portuguese rules; dedicated Brazilian dictionary remains open |
| Estonian | 2 / 3 | 3,691 |
| Finnish | 2 / 2 | 284 |
| Danish | 2 / 2 | 1,144 |
| Dutch | 2 / 2 | 12,764 |
| French | 2 / 1 | 1,074 upstream rules; stock comparison pending |
| German, reformed spelling | 2 / 2 | 36,709 upstream rules; stock comparison pending |
| Bulgarian | 2 / 2 | 6,883 upstream word rules; stock comparison pending |
| Galician | 2 / 2 | 2,321 upstream word rules; stock comparison pending |
| Irish | 2 / 3 | 6,033 upstream word rules and 46 exceptions; stock comparison pending |
| Ukrainian | 2 / 2 | 1,989 upstream word and apostrophe rules; stock comparison pending |
| Polish | 2 / 2 | 4,053 upstream word rules and 20 exceptions; stock comparison pending |
| Russian | 2 / 2 | Upstream single-word rules and exceptions; stock comparison pending |
| Swedish | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Russian + English (UK) | 2 / 2 | Combined existing Russian and UK English tries; stock comparison pending |
| Russian + English (US) | 2 / 2 | Combined existing Russian and US English tries; stock comparison pending |
| Slovak | 2 / 3 | 2,467 upstream word patterns and four active exceptions; stock comparison pending |
| Slovenian | 2 / 2 | 1,068 upstream word patterns; stock comparison pending |

Slovak and Slovenian follow the pinned CREngine language minima and use
MIT-licensed upstream sources also identified by its pattern files. The
Slovak source's unhyphenated `dosť` exception is excluded because the pinned
CREngine table explicitly leaves it disabled; four hyphenated exceptions are
included. Regional language tags retain the selected language and explicit
non-Latin scripts do not use these Latin dictionaries. Existing automatic,
soft-hyphen and user-limit switches continue to apply. Neither language has
yet been built or behaviorally compared on Mac or Kindle. [Language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L99-L100) · [Slovak stock exceptions](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/Slovak.pattern) · [Slovenian stock patterns](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/Slovenian.pattern)

Reader compiles upstream TeX patterns into immutable tables. The build
normalizes word-edge markers, removes patterns for punctuation outside the
supported word runs, and gives two Portuguese exceptions the same weights as
CREngine. Fingerprints of the compiled rule lists match the pinned
CREngine patterns that can apply to a single word. CREngine reports 285 Finnish
patterns. It combines the two TeX entries `r2as l2as` into one pattern
with an internal word boundary. The matcher adds boundaries only at each
word's edges, so Reader compiles the 284 reachable patterns. The Dutch table
includes 40 exceptions from the upstream TeX source. Sharing identical weight
sequences keeps that table within its 16-bit weight indexes. The US and UK
English tables retain their rule fingerprints. French uses the upstream 2016
MIT patterns and German the 2024 reformed-spelling MIT patterns. KOReader's
pinned French dictionary contains additional adaptations whose distribution
terms are not established in that file, and its German dictionary uses a
different 2023 pattern revision. These implementations provide automatic
hyphenation but do not yet claim exact KOReader word-break parity. The
comparison of real books and the associated Kindle checks remain pending.
Bulgarian and Ukrainian (including explicit Cyrillic tags), Galician and Irish
use licensed upstream patterns and KOReader's default minima. Ukrainian
normalizes typographic apostrophes for matching, preserving source offsets;
patterns containing literal hyphens are not supported by the current word
matcher. Their normalized rule sets and Irish exceptions have not yet been
compared exhaustively with the pinned KOReader dictionaries, and simulator and
device checks remain pending. [Pattern provenance and notices](licenses/HYPHENATION-BG-GL-GA.txt)
Russian uses Alexander Lebedev's upstream TeX patterns and exceptions, also
identified as the origin of KOReader's pinned Russian dictionary. Reader
filters patterns that require punctuation outside its single-word matcher,
including hyphen/dash rules, and uses 2 / 2 minima. Russian + English (UK)
and Russian + English (US) merge the Russian and the corresponding bundled
English pattern weights before selecting breaks, including for mixed-script
words. Both use 2 / 2 minima, as in the pinned KOReader language defaults;
custom limits still override them. This is not a claim of
exact KOReader parity: the compiled candidate, whole-document behavior and
computer/Kindle comparison remain unverified. The original source, LPPL 1.3c
and provenance are included in release packages. [Russian terms](licenses/HYPHENATION-RU.txt)
Polish uses the 1995 MIT-licensed upstream rules and exceptions with 2 / 2
minima. The compiled rules have not yet been compared exhaustively with
KOReader or tested on the computer or Kindle. [Polish source and notice](licenses/HYPHENATION-PL.txt) · [KOReader Polish selection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypography.lua#L83)
Brazilian Portuguese now uses the same BSD-licensed Portuguese TeX rules as a
fallback. KOReader selects a separate Brazilian dictionary with the same 2 / 3
limits; its word breaks can differ. Reader labels the shared rules in its
dictionary picker and does not claim Brazilian dictionary parity. This fallback
has not yet been compiled or tested. [KOReader language selection](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertypography.lua#L84-L85) · [KOReader's Brazilian pattern provenance](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/Portuguese_BR.pattern)
[Pinned language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L56-L111) ·
[Spanish, Italian, and Portuguese sources](licenses/HYPHENATION-ES-IT-PT.txt) ·
[Estonian and Finnish sources](licenses/HYPHENATION-ET-FI.txt) ·
[French and German sources](licenses/HYPHENATION-FR-DE.txt) ·
[Danish and Dutch sources](licenses/HYPHENATION-DA-NL.txt)

The matcher composes accented letters for lookup and derives each break from
the original grapheme's byte range. Layout paints a hyphen only at a fitted
line break. Searching or selecting a split word returns its original text
and coordinates. Publisher CSS, soft hyphens, limits, and user exceptions use
the existing rules. These dictionaries apply to horizontal reflowable text.

## Mac comparison

The pinned native replay verifies 29 words in seven languages by measuring
where CREngine breaks each one over a range of screen widths. It verifies
language defaults, repeats the measurement with a 4 / 4 override, and checks
that no word breaks with hyphenation off.
CREngine's dictionary API also confirms the candidate `lü-` in
`lühiülevaade`. In the width sweep, the stock page layout uses `lühi-` and
`lühiüle-`. The Reader screenshot at 574 px uses `lü-`. The native replay
also checks the dictionary candidates for each Danish and Dutch word.
For `bibliotekernes` and `bibliotheek`, the native dictionary offers `bi-`,
and the native page layout uses later breaks in the width sweep. At 578 px,
Reader uses `bi-` for both and KOReader uses `bibli-`. The Rust tests verify
dictionary candidates, decomposed accents, mixed inline languages, search,
hit testing, and saved anchors.
[Spanish, Italian, and Portuguese specimen](../tests/fixtures/multilingual-hyphenation.xhtml) ·
[Estonian and Finnish specimen](../tests/fixtures/estonian-finnish-hyphenation.xhtml) ·
[Danish and Dutch specimen](../tests/fixtures/danish-dutch-hyphenation.xhtml) ·
[Native replay](../scripts/koreader-multilingual-hyphenation-check.lua) ·
[Rust dictionary tests](../src/hyphenation.rs)

Run the Lua replay from the pinned KOReader runtime. Pass the XHTML specimen,
a new CRE cache directory, `NotoSans-Regular.ttf`, and a capture directory.
Pass `estonian-finnish-hyphenation` or `danish-dutch-hyphenation` as the fifth
argument for either of those specimens.
The Mac Reader rehearsal runs with the `multilingual_hyphenation` test filter.
It uses the same specimens and font at 570 × 864, 574 × 864, and 578 × 864 px
respectively.
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

| Danish and Dutch mode | Reader | KOReader |
| --- | --- | --- |
| Dictionary defaults | [Page](screenshots/reader-danish-dutch-hyphenation.png) | [Page](screenshots/stock-danish-dutch-hyphenation.png) |
| 4 / 4 override | [Page](screenshots/reader-danish-dutch-hyphenation-limits.png) | [Page](screenshots/stock-danish-dutch-hyphenation-limits.png) |
| Hyphenation off | [Page](screenshots/reader-danish-dutch-hyphenation-off.png) | [Page](screenshots/stock-danish-dutch-hyphenation-off.png) |

[Reader settings for Danish](screenshots/da-hyphenation-settings.png) ·
[Reader settings for Dutch](screenshots/nl-hyphenation-settings.png)

The combined Russian/English candidate reuses the existing licensed tables;
it has not been built or compared against KOReader's combined dictionaries,
and neither the simulator nor Kindle has verified its behavior. The existing
Russian and English rule-source and punctuation limitations continue to apply.

Swedish uses Jan Michael Rynning's upstream TeX patterns and KOReader's
2 / 2 minima. The unmodified source, LPPL 1.3c and provenance are included in
release packages. The compiled rules and their word breaks have not been
compared with the pinned KOReader dictionary, built, or tested on the computer
or Kindle. [Swedish terms](licenses/HYPHENATION-SV.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)
