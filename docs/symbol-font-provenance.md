# Private-use characters and font identity

One original Kindle-format book has 134 private-use characters across eight code points in U+F000–U+F0FF. These values do not identify standard Unicode symbols. Their meaning belongs to the font that created them.

An independent Palm record-table reader decompressed the unchanged KF8 source into 154,748 bytes. All 134 private-use values occur literally in that text. Reader's two reconstructed chapters contain exactly the same values in exactly the same order. The source contains no `font-family`, `face` attribute, `@font-face`, stylesheet, CSS flow, FONT record or recognizable standalone font payload. Its RESC metadata contains no font declaration. The import did not lose a publisher font assignment.

## What the font engines do

CREngine first requests a Unicode character map when loading a face. If a private-use value in U+F000–U+F0FF has no glyph, it tries that face's Microsoft Symbol map. This reads glyphs from an actual font. It does not turn every private-use value into a standard character. Its general replacement table supplies no replacement for these eight values. [CREngine face loading and Symbol fallback](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvfntman.cpp#L2279-L2313) · [Glyph lookup](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvfntman.cpp#L2393-L2414) · [Replacement table](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvfntman.cpp#L453-L562)

Reader's existing Fontique and Skrifa paths also support Microsoft Symbol maps. An authored desktop control assigned eight private-use values to the A–H outlines in a temporary font with a Windows platform 3, encoding 0 map. With ordinary Noto Sans the control reported eight missing glyphs. Adding the authored Symbol font reduced the count to zero. All eight source characters and the saved source anchor remained identical. This control proves font-map support. It does not assign meaning to the original book's characters. The temporary font remains outside the application package.

The tested Mac Symbol font has Unicode and Macintosh maps, with no Microsoft Symbol map and no mappings for these eight private-use values. Its legacy low-byte glyphs are not evidence that this unnamed publisher font had the same identity. STIX and the other desktop fallback fonts also have no mappings for the eight values. Adding these fonts to the original therefore leaves the 134 missing glyphs unchanged.

KOReader's configured fallback list does not automatically select every installed font. Its Kindle font inventory also excludes the firmware's Symbol and MonospacedSymbol files. The earlier device coverage probe checked the default map of 87 firmware faces and 17 configured fallback faces and found no coverage for the eight values. It did not explicitly retry a secondary Microsoft Symbol map. That narrower native comparison remains pending; this desktop investigation makes no new claim that the installed KOReader repairs the source. [KOReader fallback selection](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/credocument.lua#L1098-L1127) · [Kindle font exclusions](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/fontlist.lua#L23-L36)

## Result and remaining acceptance

The original still needs an identified source font or an explicit character mapping from its creator before Reader can reconstruct these symbols reliably. The provider preserves the existing text and anchors. No source substitution, font addition or license change follows from this audit.

When the Kindle is available, repeat the stock coverage comparison with explicit Unicode and Microsoft Symbol selection for each face, then compare a known affected passage using the actual active face and fallback settings. Keep that evidence separate from the already resolved [numeric quotation reference](numeric-character-references.md), which has a defined CREngine conversion table.
