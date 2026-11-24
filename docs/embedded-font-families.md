# Publisher font families

An EPUB can give regular, bold, italic and bold italic separate font files under one family name. Reader registers the files together and shapes each span with the weight and style that its text requests. A CSS font rule can assign the same file to multiple faces and families. Switching off publisher styles or overriding the font still uses the reader’s chosen family. [CSS font face descriptors](https://www.w3.org/TR/css-fonts-4/#font-face-rule)

The comparison uses KOReader v2026.07.1. Its EPUB parser passes each resource, family, bold flag and italic flag to the document font list. KOReader’s Lua font control toggles embedded faces and its native interface distinguishes registered and instantiated document families. Reader uses Parley’s font matcher, including numeric CSS weights. [CREngine EPUB parser at the pinned revision](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1043) · [KOReader font control](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/credocument.lua#L1341) · [KOReader native face inventory](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L1095)

## Embedded fonts control

Open the bottom toolbar and choose **Text › Rendering › Embedded fonts**.
It defaults to **On** and is saved for the book. **Off** skips registration
of the book's font files. Publisher CSS can still select an installed
family or specify sizes and other appearance. The existing **Use selected
font** action overrides publisher family choices.

Reader checks for a supported declared font before showing the On and Off
choices. The choice is unavailable while Publisher CSS is off. Turning CSS
back on restores the book's previous embedded-font choice.

A positive registration in the current chapter is enough to enable the
control. Reader can also check up to 64 stylesheets and the current and
first 96 spine items within a 2 MiB chapter-markup budget when the Rendering
panel opens.
It uses the normal CSS parser and bounded font loader to distinguish a
usable declaration from an unused font asset or a missing source URL. The
availability result is cached for the open book. If discovery finds no
usable declarations, Reader checks new chapters when Rendering opens there.
It can find a bundled face beyond the initial discovery window while the
embedded-font setting is off.


The pinned KOReader replay executes the actual control availability and
change callbacks against the authored Harbor lettering EPUB. Changing
the bundled face preserves the installed Silkscreen sample and publisher
inset, and switching back restores the original pixels. The equivalent
Reader tests exercise a later spine item, no usable faces, independent
CSS/font choices, reopen, and source selection geometry.

[Available control](screenshots/embedded-fonts-available.png) ·
[Saved Off in landscape](screenshots/embedded-fonts-off-landscape.png) ·
[Unavailable control](screenshots/embedded-fonts-unavailable.png) ·
[KOReader On](screenshots/embedded-fonts-stock-on.png) ·
[KOReader Off](screenshots/embedded-fonts-stock-off.png) ·
[Reader On](screenshots/embedded-fonts-reader-on.png) ·
[Reader Off](screenshots/embedded-fonts-reader-off.png) ·
[Native KOReader replay](../scripts/koreader-embedded-font-toggle-check.lua) ·
[Pinned control](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L693-L717)

### Font family details

Hold the **Embedded fonts** label or tap **Details** to open the family
report over Rendering. The report groups font faces by their declared
family and marks those used by Parley’s shaped glyph runs. It includes
bundled families registered from local CSS even if the current chapter
does not use them. The report is available with either font setting and
with Publisher CSS off. Done returns to the same Rendering controls
and reading location.

For packaged EPUBs with a regular layout, up to 96 spine items, an 8 MiB
archive and 2 MiB of spine markup, Reader checks the book in the background
one chapter at a time. Installed fonts and publisher styles use the active
layout settings. The worker checks the book and font source stamps before
accepting its result. Larger books, extracted EPUBs, facing pages and other
providers show current-chapter usage and label unchecked families explicitly.
The existing publisher CSS and font resource limits still apply.

The pinned KOReader help replay verifies both names for a two-chapter EPUB,
including a family that is registered but unused, a family instantiated
only after the preface, and disabled font use. The Mac workflow also
checks the touch and hold entry points, rotation, sleep cancellation,
previous and next controls, and source and history preservation.
[Native help replay](../scripts/koreader-embedded-font-report-check.lua) ·
[Whole-book report](screenshots/embedded-fonts-report.png) ·
[Disabled in landscape](screenshots/embedded-fonts-report-off-landscape.png) ·
[Current-chapter report](screenshots/embedded-fonts-report-unchecked.png)

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
