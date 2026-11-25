# Spanish, Italian, and Portuguese hyphenation

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

Reader compiles upstream TeX patterns into immutable tables. The build
normalizes word-edge markers, removes patterns for punctuation outside the
supported word runs, and gives two Portuguese exceptions the same weights as
CREngine. Fingerprints of all three compiled rule lists match the pinned
CREngine data. The existing US and UK English tables retain their fingerprints.
The Brazilian Portuguese dictionary is tracked separately.
[Pinned language defaults](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/textlang.cpp#L56-L111) ·
[Source pins and licenses](licenses/HYPHENATION-ES-IT-PT.txt)

The matcher composes accented letters for lookup and derives each break from
the original grapheme's byte range. Layout paints a hyphen only at a fitted
line break. Searching or selecting a split word returns its original text
and coordinates. Publisher CSS, soft hyphens, limits, and user exceptions use
the existing rules. These dictionaries apply to horizontal reflowable text.

## Mac comparison

The pinned native replay verifies ten Spanish, Italian, and Portuguese
words by measuring where CREngine breaks each one over a range of screen
widths. It verifies language defaults, repeats the measurement with a
4 / 4 override, and checks that no word breaks with hyphenation off. The Rust
tests compare the break vectors and check decomposed accents, mixed inline
languages, search, hit testing, and saved anchors.
[XHTML specimen](../tests/fixtures/multilingual-hyphenation.xhtml) ·
[Native replay](../scripts/koreader-multilingual-hyphenation-check.lua) ·
[Rust dictionary tests](../src/hyphenation.rs)

Run the Lua replay from the pinned KOReader runtime. Pass the XHTML specimen,
a new CRE cache directory, `NotoSans-Regular.ttf`, and a capture directory.
The Mac Reader rehearsal uses the same specimen and font at 570 × 864 px with
`READER_MULTILINGUAL_HYPHENATION_CAPTURE` pointing to a capture directory.

| Mode | Reader | KOReader |
| --- | --- | --- |
| Dictionary defaults | [Page](screenshots/reader-multilingual-hyphenation.png) | [Page](screenshots/stock-multilingual-hyphenation.png) |
| 4 / 4 override | [Page](screenshots/reader-multilingual-hyphenation-limits.png) | [Page](screenshots/stock-multilingual-hyphenation-limits.png) |
| Hyphenation off | [Page](screenshots/reader-multilingual-hyphenation-off.png) | [Page](screenshots/stock-multilingual-hyphenation-off.png) |

[Reader settings for Spanish](screenshots/multilingual-hyphenation-settings.png)
