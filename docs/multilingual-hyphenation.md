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
| German, reformed spelling | 2 / 2 | 36,729 pinned KOReader rules; behavior comparison pending |
| Bulgarian | 2 / 2 | 6,883 upstream word rules; stock comparison pending |
| Galician | 2 / 2 | 2,321 upstream word rules; stock comparison pending |
| Irish | 2 / 3 | 6,033 upstream word rules and 46 exceptions; stock comparison pending |
| Ukrainian | 2 / 2 | 1,897 weighted source rules match pinned KOReader; behavior pending |
| Polish | 2 / 2 | 4,053 upstream word rules and 20 exceptions; stock comparison pending |
| Russian | 2 / 2 | 4,808 source rules match pinned KOReader; behavior pending |
| Swedish | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Catalan | 2 / 2 | Upstream single-word patterns and exceptions; stock comparison pending |
| Turkish | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Croatian | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Norwegian | 2 / 2 | Upstream single-word patterns and exceptions; stock comparison pending |
| Lithuanian | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Icelandic | 2 / 2 | Upstream single-word patterns; stock comparison pending |
| Occitan | 2 / 2 | Upstream alphabetic patterns; stock comparison pending |
| Welsh | 2 / 3 | Upstream single-word patterns; stock comparison pending |
| Piedmontese | 2 / 2 | Upstream alphabetic patterns; stock comparison pending |
| Romansh | 2 / 2 | Upstream version 1.1 alphabetic patterns; stock comparison pending |
| Basque | 2 / 2 | Upstream alphabetic subset; stock comparison pending |
| Friulian | 2 / 2 | Upstream alphabetic patterns; stock comparison pending |
| Esperanto | 2 / 2 | Upstream 2020 pattern set matches pinned stock entries; behavior comparison pending |
| Georgian | 1 / 2 | Upstream version 0.3 single-word patterns; stock comparison pending |
| Greek | 1 / 1 | KOReader Ancient Greek source; punctuation and behavior comparison pending |
| Malayalam | 2 / 3 | Matching 88-pattern MIT source; grapheme and stock behavior comparison pending |
| Hungarian | 2 / 2 | Matching upstream source; wide trie, distribution and behavior review pending |
| Latin | 2 / 2 | Classical upstream patterns; combining-mark behavior unverified |
| Latin (liturgical) | 2 / 2 | Liturgical upstream patterns; combining-mark behavior unverified |
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
MIT patterns; KOReader's pinned French dictionary contains additional
adaptations whose distribution terms are not established in that file. German
uses the exact 2023 KOReader pattern file, including all 36,729 active rules,
with word-edge notation converted during compilation. Its original MIT and
historical LPPL notices are preserved; mixed-notice distribution review remains
open. Rule loading, real-book behavior and Kindle checks remain unverified, so
full KOReader word-break parity is not claimed.
Bulgarian and Ukrainian (including explicit Cyrillic tags), Galician and Irish
use licensed upstream patterns and KOReader's default minima. Ukrainian compiles
the alphabetic source-only subset: its 1,897 effective weighted rules match
the pinned KOReader set, whose remaining unweighted entry cannot add a break.
Straight and curly apostrophes split lookup input without changing source
offsets. U+02BC modifier-letter segmentation, actual wrapping and real-book
behavior still need comparison. Bulgarian, Galician and Irish normalized rule
sets and Irish exceptions have not yet been compared exhaustively, and
simulator and device checks remain pending. [Pattern provenance and notices](licenses/HYPHENATION-BG-GL-GA.txt)
Russian uses Alexander Lebedev's upstream TeX patterns, also identified as
the origin of KOReader's pinned Russian dictionary. The 4,808 selected source
rules match pinned KOReader; 184 TeX exceptions are omitted because the pinned
dictionary omits them. Reader filters punctuation outside its single-word
matcher and uses 2 / 2 minima. Russian + English (UK) and Russian + English
(US) merge the corresponding bundled pattern weights before selecting breaks,
including for mixed-script words. Their pinned source sets equal the Russian
and corresponding pinned English set unions, but Reader's compiled composites
and actual breaks still need comparison. Both use 2 / 2 minima; custom limits
still override them. Whole-document behavior and computer/Kindle comparison
remain unverified. The original source, LPPL 1.3c and provenance are included
in release packages. [Russian terms](licenses/HYPHENATION-RU.txt)
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

The combined Russian/English candidate reuses the existing licensed tables.
Pinned KOReader composite source sets match the unions of its Russian and
English sets; compiled composite behavior, punctuation, and simulator/Kindle
checks remain pending.

Swedish uses Jan Michael Rynning's upstream TeX patterns and KOReader's
2 / 2 minima. The unmodified source, LPPL 1.3c and provenance are included in
release packages. The compiled rules and their word breaks have not been
compared with the pinned KOReader dictionary, built, or tested on the computer
or Kindle. [Swedish terms](licenses/HYPHENATION-SV.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Catalan uses the upstream TeX patterns and exceptions compiled by Gonçal
Badenes and Francina Turon, with KOReader's 2 / 2 minima. The unmodified
source, LPPL 1.3c and provenance are included in release packages. For dictionary lookup only, Reader treats Catalan's U+00B7 middle dot as a
boundary between alphabetic runs and keeps each resulting break at its original
UTF-8 source offset. This follows the pinned CREngine's alphabetic word-bound
selection without changing selection or search. It does not add a break at the
dot or implement the Catalan spelling change that may be needed when a line
actually breaks there; precomposed middle-dot letters also remain open. The
compiled rules and their word breaks have not been compared with the pinned
KOReader dictionary, built, or tested on the computer or Kindle.
[Catalan terms](licenses/HYPHENATION-CA.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Turkish uses the upstream TeX patterns with KOReader's 2 / 2 minima. Lookup
uses Turkish-specific casing: uppercase I maps to dotless ı, while uppercase İ
(including its decomposed spelling) maps to i. Break positions continue to use
the original UTF-8 source boundaries. CREngine uses locale-neutral lowercase
in its pinned matcher, so uppercase behavior may differ; exact stock and
real-document comparisons remain open. The unmodified upstream source,
LPPL 1.3c and provenance are included in release packages. These changes have
not been built or tested on the computer or Kindle.
[Turkish terms](licenses/HYPHENATION-TR.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Croatian uses Igor Marinović's upstream TeX patterns with KOReader's 2 / 2
minima. The original source, LPPL 1.3c and provenance are included in release
packages. The compiled rules and their word breaks have not been compared
with the pinned KOReader dictionary, built, or tested on the computer or Kindle.
[Croatian terms](licenses/HYPHENATION-HR.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Norwegian (`no`) uses Rune Kleveland and Ole Michael Selberg's upstream TeX
patterns and exceptions with KOReader's 2 / 2 minima. The original source and
its copyright and permission notice are included in release packages. The
separately tagged Bokmål (`nb`) and Nynorsk (`nn`) choices are not mapped to
this dictionary yet. The pinned KOReader menu lists `nor` as an alias for `no`,
but does not list `nb` or `nn`; its native dictionary lookup falls back to US
English for unknown tags. Reader leaves those tags without a dictionary rather
than applying Norwegian rules without supporting source evidence. Its compiled rules, word breaks and memory footprint have
not been compared with the pinned KOReader dictionary, built, or tested on the
computer or Kindle. [Norwegian terms](licenses/HYPHENATION-NO.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Lithuanian (`lt`) uses the upstream TeX patterns with KOReader's 2 / 2 minima.
The original source and MIT terms are included in release packages. The
existing grapheme normalization preserves original source offsets; Lithuanian-
specific casing, exact rule equivalence and real-document behavior remain
unverified. The change has not been built or tested on the computer or Kindle.
[Lithuanian terms](licenses/HYPHENATION-LT.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Icelandic (`is`) uses Jörgen Pind's upstream TeX patterns with KOReader's
2 / 2 minima. The unmodified source, LPPL 1.3c and provenance are included in
release packages. The compiled rules and word breaks have not been compared
with the pinned KOReader dictionary, built, or tested on the computer or Kindle.
[Icelandic terms](licenses/HYPHENATION-IS.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)

Occitan (`oc`) uses Claudio Beccari's upstream TeX patterns with KOReader's
2 / 2 minima. Reader compiles the supported single-word alphabetic patterns.
The upstream TeX source also contains 29 apostrophe-bearing tokens; Reader does
not apply them, and the pinned KOReader Occitan pattern file does not contain
them either. KOReader's hyphenation word-boundary code selects alphabetic runs.
Reader likewise splits straight and curly apostrophes for this dictionary
without changing the original source text or UTF-8 offsets. The original source
and MIT terms are included in release packages. Exact stock comparison,
apostrophe behavior and real-document checks remain open; the change has not
been built or tested on the computer or Kindle.
[Pinned Occitan patterns](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/Occitan.pattern) ·
[Pinned word boundaries](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvstring.cpp) [Occitan terms](licenses/HYPHENATION-OC.txt) ·
[KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Welsh (`cy`, including the `cym` language alias) uses Yannis Haralambous's
upstream TeX patterns with KOReader's 2 / 3 minima. Reader compiles supported
single-word patterns and preserves original UTF-8 source offsets. The unmodified
source and MIT terms are included in release packages. Exact comparison with
the pinned KOReader patterns and real-document behavior remain open; this
change has not been built or tested on the computer or Kindle.
[Welsh terms](licenses/HYPHENATION-CY.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Piedmontese (`pms`) uses Claudio Beccari's upstream TeX patterns with
KOReader's 2 / 2 minima. Reader compiles supported single-word alphabetic
patterns and preserves original UTF-8 source offsets; upstream rules containing
apostrophes are excluded. The unmodified source and MIT terms are included in
release packages. Exact comparison with the pinned KOReader patterns and
real-document behavior remain open; this change has not been built or tested
on the computer or Kindle.
[Piedmontese terms](licenses/HYPHENATION-PMS.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Romansh (`rm`, including the `roh` language alias) uses Claudio Beccari's
upstream TeX patterns, version 1.1, with KOReader's 2 / 2 minima. Reader
compiles supported single-word alphabetic patterns and preserves original UTF-8
source offsets; upstream rules containing apostrophes are excluded. The
unmodified source and MIT terms are included in release packages. Exact
comparison with the pinned KOReader patterns and real-document behavior remain
open; this change has not been built or tested on the computer or Kindle.
[Romansh terms](licenses/HYPHENATION-RM.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Basque (`eu`) uses Juan M. Aguirregabiria's upstream TeX patterns with
KOReader's 2 / 2 minima. The generated pattern data is modified from the source:
Reader selects the supported single-word alphabetic rules and compiles them to
a native lookup table; the source file itself is unmodified. Original UTF-8
source offsets are preserved. The upstream source and full copyright and
permission notice are included in release packages. The generated Rust and
normalized development artifacts also carry the notice, and the runtime keeps
the notice alongside the compiled pattern data. This is a conservative
attribution measure, not a legal determination; distribution review remains
open. Exact comparison with the pinned KOReader patterns and real-document
behavior remain open; this change has not been built or tested on the computer
or Kindle.
[Basque terms](licenses/HYPHENATION-EU.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Friulian (`fur`) uses Claudio Beccari's upstream TeX patterns, version 1.1,
with KOReader's 2 / 2 minima. Reader compiles supported single-word alphabetic
patterns and preserves original UTF-8 source offsets; rules containing
apostrophes are excluded. The unmodified source and MIT terms are included in
release packages. Exact comparison with pinned KOReader patterns and
real-document behavior remain open; this change has not been built or tested
on the computer or Kindle.
[Friulian terms](licenses/HYPHENATION-FUR.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Esperanto (`eo`, including `epo`) uses Sergio B. Pokrovskij's upstream
2020-10-02 pattern data with KOReader's 2 / 2 minima. The original XML pattern
file and Latin-3 m4 source are preserved along with the LPPL terms. Upstream
and pinned KOReader contain the same 2,009 pattern entries, in a different
order; Reader excludes the one unweighted entry, `uzulinterfaco`, which cannot
indicate a hyphenation point. Edge spaces become word-boundary markers during
compilation, preserving source-text UTF-8 offsets. Build and real-document
behavior comparison remain pending on the computer and Kindle.
[Esperanto terms](licenses/HYPHENATION-EO.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Georgian (`ka`, optionally `ka-Geor`) uses Levan Shoshiashvili's upstream
version 0.3 patterns and KOReader's 1 / 2 minima. Reader rejects explicitly
incompatible scripts and compiles the supported single-word alphabetic rules,
preserving original UTF-8 source offsets. The unmodified upstream source and
MIT terms are included in release packages. Exact stock comparison and
real-document behavior remain open; this change has not been built or tested
on the computer or Kindle.
[Georgian terms](licenses/HYPHENATION-KA.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Classical Latin (`la`, `lat`) and liturgical Latin (`la-lit`, `lat-lit`,
`la-x-liturgic`) use separate upstream patterns and KOReader's 2 / 2 minima.
The selected locale is carried separately from the `la` shaping language,
and respected embedded language tags can change or clear it. Original UTF-8
source offsets and reading anchors are unchanged. The source files and MIT
terms are included in release packages. Reader includes combining-mark rules
and uses canonical decomposition for Latin pattern matching while emitting
only original UTF-8 grapheme boundaries. Latin words exceeding 64 graphemes,
256 decomposed scalars, or the overall word-byte limit are skipped.
User-defined hyphenation rules still use the `la` shaping language for both
Latin variants. Exact stock rule/behavior comparison, canonical combining-mark
cases, mixed-language behavior and real-document checks remain open; this
change has not been built or tested on the computer or Kindle.
[Latin terms](licenses/HYPHENATION-LA.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Greek (`el` / `ell`, optionally `Grek`) uses the 2011 Ancient Greek pattern
source selected by the pinned KOReader dictionary, with 1 / 1 minima. The
unmodified LPPL source is included in release packages. The upstream and
pinned KOReader contain the same 4,296 pattern strings after converting their
word-boundary notation. Reader includes patterns with modifier-letter
apostrophe, Greek koronis and Greek psili and follows the pinned word scanner
by splitting at ASCII and right quotation apostrophes; its 137 rules containing
those separators remain excluded. The bounded NFD matcher emits only original
UTF-8 grapheme boundaries and keeps Greek modifiers with the preceding
grapheme. Greek capital sigma is lowercased per scalar to ordinary sigma, like
the pinned engine; existing final sigma remains unchanged. Exact punctuation
segmentation, compiled-rule comparison, and real-document behavior remain open;
this change has not been built or tested on the computer or Kindle.
[Greek terms](licenses/HYPHENATION-EL.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Malayalam (`ml` / `mal`, optionally `Mlym`) uses Santhosh Thottingal's
upstream v0.9.0 MIT patterns. The upstream and pinned KOReader sources have
the same 88 pattern strings after word-boundary conversion; the original
source and MIT notice are included in release packages. KOReader uses 2 / 3
minima in code-point positions; Reader applies 2 / 3 to grapheme boundaries
and preserves source UTF-8 offsets. The bounded NFD matcher retains vowel
marks, virama and join controls inside graphemes, and fails closed on unsupported
segmentation or length. Exact wrapping, join-control handling, minima and
real-document behavior still need comparison; this change has not been built
or tested on the computer or Kindle.
[Malayalam terms](licenses/HYPHENATION-ML.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


Hungarian (`hu` / `hun`, optionally `Latn`) uses Bence Nagy's upstream
2011-08-15 patterns and KOReader's 2 / 2 minima. The upstream and pinned
KOReader sources contain the same 62,851 pattern strings after word-boundary
conversion. The original source, attribution and MPL 1.1 are included in
release packages. Hungarian alone uses a bounded wide-index trie; the other
dictionaries retain their compact indexes. Words longer than 64 graphemes or
the overall word-byte limit, and graphemes that NFC does not compose to one
character, are skipped. MPL distribution obligations, device memory use,
compiled-rule comparison, and real-document behavior remain open; this change
has not been built or tested on the computer or Kindle.
[Hungarian terms](licenses/HYPHENATION-HU.txt) · [KOReader language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp)


### Optional Armenian patterns

Reader does not include Armenian hyphenation pattern data. If you have a
permitted `hyph-hy.tex` file, create `reader/data/hyphenation/`, copy it there
as `hyph-hy.tex`, and close and reopen Reader. In an Armenian book, Text ›
Hyphenation reports whether the pattern file was loaded, missing or invalid.
Missing or invalid patterns leave Armenian automatic hyphenation unavailable;
the book remains readable, and existing user hyphenation rules remain separate.
The file is read locally as inert data; Reader neither downloads nor executes it.

The loader accepts one `\patterns{...}` group, `%` comments, Armenian
lowercase pattern letters, ASCII digits and optional word-edge dots. It rejects
other TeX commands, malformed groups, symlinked files or directories, files
over 256 KiB, more than 4,096 rules and tries above 8,192 nodes. It uses
KOReader's Armenian default 1 / 2 minima and emits breaks at original UTF-8
grapheme boundaries; automatic behavior and real-book comparisons remain
unverified. Restart Reader after replacing the file. The original
[upstream Armenian patterns](https://github.com/hyphenation/tex-hyphen/blob/5684c0f51c0b81133db2efbe60a408b4155a3ff5/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-hy.tex)
identify LGPL terms. Obtaining or redistributing that optional file has its own
licensing obligations; separating it from Reader does not establish legal
compliance. License and distribution review remains open.


### Optional Zulu patterns

Reader does not include Zulu hyphenation pattern data. If you have a permitted
`hyph_zu_ZA.dic` file, create `reader/data/hyphenation/`, copy it there as
`hyph-zu.dic`, and close and reopen Reader. In a Zulu book, Text › Hyphenation
reports whether the file loaded, is missing, or is invalid. The book remains
readable when no valid patterns are present. Reader reads the file locally as
inert data and does not download it.

The loader accepts the original ASCII-compatible `ISO8859-1` dictionary header,
`%` comments, lowercase ASCII weighted patterns and optional word-edge dots.
The input is bounded to 256 KiB, 4,096 rules and 8,192 trie nodes; symlinks,
other encodings, malformed rules and unsupported characters are rejected. The
source's apostrophe-only `'1` rule is omitted, matching KOReader's pinned
129-rule pattern file. Reader applies the 2 / 1 minima to original UTF-8
grapheme boundaries. Reopen Reader after replacing the file. Behavior and
real-book comparisons remain unverified.

The [upstream Zulu file](https://github.com/LibreOffice/dictionaries/blob/91c039a37e66094bc666878bd1f0d5477affb07f/zu_ZA/hyph_zu_ZA.dic)
identifies LGPL 2.1 terms. Obtaining or redistributing the optional file has
separate licensing obligations; keeping it separate from Reader does not
establish legal compliance. Distribution review remains open.


### Optional Serbian patterns

Reader does not include Serbian hyphenation pattern data. The original
[Serbian pattern archive](https://devbase.net/dict-sr/hyphen-sr-20100818.zip)
contains separate Latin `hyph_sh.dic` (ISO-8859-2) and Cyrillic `hyph_sr.dic`
(ISO-8859-5) files. If you have permission to use them, create
`reader/data/hyphenation/`, copy either or both there as `hyph-sr-latn.dic`
and `hyph-sr-cyrl.dic`, then close and reopen Reader. In a Serbian book,
Text › Hyphenation shows the loaded, missing or invalid status of each file.
Reader reads the files locally as inert data; the book remains readable when
a pattern file is absent.

Each file is limited to 256 KiB, 4,096 rules and 8,192 trie nodes. The loader
requires its correct encoding and 2 / 2 minima headers, rejects symlinks and
malformed or mismatched-script patterns, and converts the upstream Cyrillic
`д2р` rule to KOReader's pinned `д3р` variant. The two upstream files otherwise
match the pinned KOReader pattern set. An explicit `sr-Latn` or `sr-Cyrl`
language selects its respective file; for `sr` without a script, Reader
selects by each candidate word's script and skips mixed-script words.
Hyphenation emits only original UTF-8 grapheme boundaries. Real-book behavior,
encoding and stock comparison remain unverified; reopen Reader after replacing
a file.

The upstream archive identifies LGPL 2.1-or-later terms. Obtaining or
redistributing either optional file has separate licensing obligations;
keeping files separate from Reader does not establish legal compliance.
Distribution review remains open.


### Optional Czech patterns

Reader does not include Czech hyphenation pattern data. If you have a permitted
`hyph-cs.tex` file, create `reader/data/hyphenation/`, copy it there, and close
and reopen Reader. In a Czech book, Text › Hyphenation reports whether the file
was loaded, missing or invalid. Missing or invalid patterns leave Czech automatic
hyphenation unavailable; the book remains readable. Reader reads the file
locally as inert data and does not download or execute it.

The loader accepts one `\patterns{...}` group followed by one
`\hyphenation{...}` exception group, `%` comments, lowercase Latin-script
letters, ASCII digits and optional word-edge dots. It converts hyphenated
exceptions to KOReader's weight-5 patterns. Files over 256 KiB, more than
4,096 combined rules, tries above 8,192 nodes, symlinks, malformed syntax
and unsupported characters are rejected. The documented upstream file has
3,636 patterns and five exceptions matching KOReader's pinned set. Reader
uses 2 / 3 minima and emits only original UTF-8 grapheme boundaries. Czech
line-breaking tweaks, pattern behavior and real-book comparisons remain
unverified. Reopen Reader after replacing the file.

The [upstream Czech file](https://github.com/hyphenation/tex-hyphen/blob/5684c0f51c0b81133db2efbe60a408b4155a3ff5/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-cs.tex)
identifies GPL 2-or-later terms. Obtaining or redistributing the optional file
has separate licensing obligations; keeping it separate from Reader does not
establish legal compliance. Distribution review remains open.


### Optional Romanian patterns

Reader does not include Romanian hyphenation pattern data. If you already have
a `hyph-ro.tex` file that you have permission to use, create
`reader/data/hyphenation/`, copy it there, and close and reopen Reader. In a
Romanian book, Text › Hyphenation reports whether the file was loaded, missing
or invalid. Missing or invalid patterns leave Romanian automatic hyphenation
unavailable; the book remains readable. Reader reads the file locally as inert
data and does not download or execute it.

The loader accepts a single `\patterns{...}` group, `%` comments, lowercase
Latin-script letters, ASCII digits and optional word-edge dots. It rejects
files over 256 KiB, more than 4,096 rules, tries above 8,192 nodes, symlinks,
malformed syntax and unsupported characters. Reader uses 2 / 2 minima and
emits only original UTF-8 grapheme boundaries. The pattern strings in the
pinned KOReader file match those in the 1996 upstream source, but actual
behavior and real-book comparisons remain unverified. Reopen Reader after
replacing the file.

The [upstream source header](https://github.com/hyphenation/tex-hyphen/blob/5684c0f51c0b81133db2efbe60a408b4155a3ff5/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-ro.tex)
identifies copyright 1995–1996 Adrian Rezus and specifies no license. This
link records provenance, not permission to use or redistribute the file.
Rights and distribution review remain unresolved; keeping a file separate
from Reader does not establish legal compliance.


### Optional Latvian patterns

Reader does not include Latvian hyphenation pattern data. If you have a
permitted `hyph-lv.tex` file, create `reader/data/hyphenation/`, copy it there,
and close and reopen Reader. In a Latvian book, Text › Hyphenation reports
whether the file was loaded, missing or invalid. Missing or invalid patterns
leave Latvian automatic hyphenation unavailable; the book remains readable.
Reader reads the file locally as inert data and does not download or execute it.

The loader accepts one `\patterns{...}` group, `%` comments, lowercase
Latin-script letters, ASCII digits and optional word-edge dots. It rejects
files over 256 KiB, more than 16,384 rules, tries above 32,768 nodes, symlinks,
malformed syntax and unsupported characters. Reader uses 2 / 2 minima and
emits only original UTF-8 grapheme boundaries. The documented upstream file's
11,583 pattern strings match pinned KOReader's Latvian pattern set, but
behavior, memory consumption and real-book comparisons remain unverified.
Reopen Reader after replacing the file.

The [upstream Latvian source](https://github.com/hyphenation/tex-hyphen/blob/5684c0f51c0b81133db2efbe60a408b4155a3ff5/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-lv.tex)
identifies LGPL 2.1 or GPL 2-or-later terms. Obtaining or redistributing the
optional file has separate licensing obligations; keeping it separate from
Reader does not establish legal compliance. Distribution review remains open.


### Optional French pattern override

Reader includes a French fallback dictionary. If you have permission to use a
KOReader-format French pattern file, create `reader/data/hyphenation/`, copy
it there as `french.pattern`, and close and reopen Reader. In a French book,
Text › Hyphenation reports whether the override is loaded, missing or invalid.
A valid file replaces the built-in French rules; a missing or invalid file
leaves the built-in dictionary active. Reader reads the file locally as inert
data and does not download or execute it.

The bounded parser accepts a `HyphenationDescription` XML root containing
weighted lowercase Latin `<pattern>` elements and optional comments. Comments
are ignored, word-edge spaces become boundary markers, and document type
declarations are disabled. Files over 256 KiB, more than 4,096 patterns, more
than 16,384 XML nodes, tries above 8,192 nodes, symlinks, unsupported elements
and malformed rules are rejected. Reader applies 2 / 1 minima and emits only
original UTF-8 grapheme boundaries. The pinned KOReader file has 1,748 active
rules and fits these bounds; exact behavior, real-book comparisons and memory
use remain unverified. Reopen Reader after replacing the file.

The [pinned KOReader French file](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/cr3gui/data/hyph/French.pattern)
describes adaptations from several sources without establishing a complete
license for those adaptations. Its link records provenance, not permission to
use or redistribute it. Obtain any optional file only with the necessary
permission. Source-rights and distribution review remain unresolved; keeping
a file separate from Reader does not establish legal compliance.
