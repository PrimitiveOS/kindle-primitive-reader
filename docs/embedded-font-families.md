# Publisher font families

An EPUB can give regular, bold, italic and bold italic separate font files under one family name. Reader registers the files together and shapes each span with the weight and style that its text requests. A CSS font rule can assign the same file to multiple faces and families. Switching off publisher styles or overriding the font still uses the reader’s chosen family. [CSS font face descriptors](https://www.w3.org/TR/css-fonts-4/#font-face-rule)

The comparison uses KOReader v2026.07.1. Its EPUB parser passes each resource, family, bold flag and italic flag to the document font list. KOReader’s Lua font control toggles embedded faces and its native interface distinguishes registered and instantiated document families. Reader uses Parley’s font matcher, including numeric CSS weights. [CREngine EPUB parser at the pinned revision](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1043) · [KOReader font control](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/credocument.lua#L1341) · [KOReader native face inventory](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L1095)

## Reading checks

The authored letterpress EPUB assigns the two bundled OFL fonts to four faces and another display family. The normal text has round forms and the printer’s headings and imprint have square forms. A second source for the normal face and a missing final URL exercise CSS precedence and fallback. The test checks the actual font bytes used by each glyph run, toggles publisher styles and reopens the original source anchor. Each font’s original license travels inside the fixture. [Simulator page](screenshots/publisher-faces.png) · [Fixture](../tests/fixtures/publisher-faces.epub)

Two original EPUBs each declared four individual faces totaling over 5.5 MiB for their body font. In two chapters, 27,530 and 3,728 Latin glyph clusters respectively used precisely the font file assigned by the publisher. Each chapter exercised three styles. The reference independently resolved the four original CSS URLs and compared the registered font bytes in memory. At the same source locations, the previous revision chose the assigned face for 450 of the 31,258 clusters. The corrected Reader chose it for all 31,258 on both the Mac and the Kindle Oasis 3. Installed KOReader instantiated the publisher family in both books. Disabling embedded fonts changed the rendered frame at each sampled source anchor. Re-enabling them restored both original frames.

### Missing-glyph inventory

A separate 48-edition Mac scan reported 892 missing glyphs in 56 sections of 11 editions with Launcher Noto Sans regular and bold. Sixty-three text-style variation selectors shared a cluster with an arrow, so these glyphs corresponded to 955 source-scalar occurrences across 27 distinct Unicode scalars. On the physical Oasis, Reader reshaped all 56 sections from the original files using its normal five-font configuration. Nine editions had complete glyph coverage. The remaining two yielded 134 private-use glyphs and one C1 control.

KOReader's installed FreeType independently inspected 87 firmware font files and the 17 installed files belonging to its configured fallback families. Both groups provided all 18 ordinarily represented scalars. Neither default character map provided the eight private-use mappings or the C1 control. This probe did not separately select secondary Microsoft Symbol maps. KOReader ships 21 font files in total on this device. No user fonts were installed. The checks used isolated scratch storage and verified the original book checksums. [KOReader fallback ordering](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/credocument.lua#L44)

## Limits

Reader loads sfnt TrueType and OpenType fonts from local EPUB resources, including the standardized IDPF embedding method. Each file is capped at 2 MiB, with six local resource reads, 64 face rules and 8 MiB total per chapter. Repeated paths share the same underlying font bytes. Files over budget, unavailable formats, and corrupt sources fall through to available faces. WOFF, variable descriptor ranges and CSS unicode-range subsetting remain separate work.

For the IDPF method, Reader requires a font MIME type in the OPF manifest, the matching font URI with the IDPF method in the container metadata, and the identifier named by that package. Only the bounded font-reading path applies the transformation. Reading raw archive members, including chapter text, retains their original bytes. Unknown algorithms, ambiguous mappings and absent identifiers leave the resource untouched. The existing ring dependency supplies the specification’s SHA-1 digest of the public identifier. [W3C EPUB 3.3 font obfuscation](https://www.w3.org/TR/epub-33/#sec-font-obfuscation) · [CREngine’s font container](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L807)

The authored regression independently masks its OFL font, inserts XML whitespace into the package identifier, and reopens the EPUB. Its first-page pixels exactly match the plain-font original. An unknown method, a missing identifier and a duplicate declaration all retain readable fallback text and the same source passage.

The 48-edition audit also found five Kindle-format originals with 34 embedded FONT records. The [KF8 font comparison](kf8-publication-fonts.md) covers their record decoding and actual use in rendered passages. One additional EPUB supplies five standard IDPF-obfuscated fonts. An independent decoder verified all five font assets. Reader selected all five across 11,540 shaped clusters in the two original spine sections. KOReader instantiated two embedded families in this book, though switching fonts at its sampled midpoint left those particular page pixels unchanged. A different Kindle-format book uses eight private-use code points without a font or character mapping. The intended symbols in that source cannot be reconstructed from their Unicode values alone. An independent raw-source audit confirms that the provider retains all 134 private-use values in order. The [font provenance review](symbol-font-provenance.md) records the missing font identity and the remaining secondary-map stock comparison.

To repeat the authored comparison, run the following commands from the Reader repository.

```sh
python3 scripts/generate-fixtures.py --check
cargo test --locked --features kindle --lib publisher_faces_choose_declared_variants
cargo run --locked --bin reader-probe -- parley tests/fixtures/publisher-faces.epub \
  --font tests/fixtures/fonts/NotoSans-Regular.ttf --width 650 --height 620 \
  --size 32 --margin 40 --output publisher-faces.png
```

On the Oasis 3, Reader independently matched all five deobfuscated OpenType or TrueType assets from the original EPUB and selected all five across 11,540 shaped clusters. It painted both sampled original sections. The authored IDPF and ordinary-font renderings also matched on the device, and the original EPUB checksum stayed the same. The combined physical EPUB/KF8 check completed in 139 seconds. [Kindle publication font results](kf8-publication-fonts.md)
