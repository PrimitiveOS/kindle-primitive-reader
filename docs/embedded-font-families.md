# Publisher font families

An EPUB can give regular, bold, italic and bold italic separate font files under one family name. Reader registers the files together and shapes each span with the weight and style that its text requests. A CSS font rule can assign the same file to multiple faces and families. Switching off publisher styles or overriding the font still uses the reader’s chosen family. [CSS font face descriptors](https://www.w3.org/TR/css-fonts-4/#font-face-rule)

The comparison uses KOReader v2026.07.1. Its EPUB parser passes each resource, family, bold flag and italic flag to the document font list. KOReader’s Lua font control toggles embedded faces and its native interface distinguishes registered and instantiated document families. Reader uses Parley’s font matcher, including numeric CSS weights. [CREngine EPUB parser at the pinned revision](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1043) · [KOReader font control](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/credocument.lua#L1341) · [KOReader native face inventory](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L1095)

## Reading checks

The authored letterpress EPUB assigns the two bundled OFL fonts to four faces and another display family. The normal text has round forms and the printer’s headings and imprint have square forms. A second source for the normal face and a missing final URL exercise CSS precedence and fallback. The test checks the actual font bytes used by each glyph run, toggles publisher styles and reopens the original source anchor. Each font’s original license travels inside the fixture. [Simulator page](screenshots/publisher-faces.png) · [Fixture](../tests/fixtures/publisher-faces.epub)

Two original EPUBs each declared four individual faces totaling over 5.5 MiB for their body font. In two chapters, 27,530 and 3,728 Latin glyph clusters respectively used precisely the font file assigned by the publisher. Each chapter exercised three styles. The reference independently resolved the four original CSS URLs. Reading and comparing them happened in memory. No original publication assets are included here.

## Limits

Reader currently loads plain sfnt TrueType and OpenType fonts from local EPUB resources. Each file is capped at 2 MiB, with six local resource reads, 64 face rules and 8 MiB total per chapter. Repeated paths share the same underlying font bytes. Files over budget, unavailable formats, and corrupt sources fall through to available faces. WOFF, variable descriptor ranges, CSS unicode-range subsetting and standard EPUB font obfuscation remain separate work.

The 48-edition audit also found five Kindle-format originals with 34 embedded FONT records. These resources require support in the KF8 importer. One additional EPUB supplies five standard IDPF-obfuscated fonts. A different Kindle-format book uses eight private-use code points without a font or character mapping. The intended symbols in that source cannot be reconstructed from their Unicode values alone.

To repeat the authored comparison, run the following commands from the Reader repository.

```sh
python3 scripts/generate-fixtures.py --check
cargo test --locked --features kindle --lib publisher_faces_choose_declared_variants
cargo run --locked --bin reader-probe -- parley tests/fixtures/publisher-faces.epub \
  --font tests/fixtures/fonts/NotoSans-Regular.ttf --width 650 --height 620 \
  --size 32 --margin 40 --output publisher-faces.png
```
