# Kindle publication fonts

Reader indexes up to 128 FONT records after the first resource in an unprotected KF8 book. Each index entry holds its position and length in the original file. When a chapter’s CSS requests a Kindle font URL, Reader reads that record from the same open file, checks the source bounds and inflates zlib data in 16 KiB input chunks. It can also read a stored record and the FONT format’s optional publisher-font mask. The encoded record, decoded face, and aggregate chapter memory each have separate size limits. A missing or malformed face leaves the text readable with the available font. The existing document protection checks run before the font index is built. [calibre’s FONT reader and writer](https://github.com/kovidgoyal/calibre/blob/a03eefa6b684af0c08c44b06c9f2d94345c41255/src/calibre/ebooks/mobi/utils.py#L437)

KOReader’s pinned CREngine Palm database importer registers JPEG, PNG and GIF resources in this part of its MOBI handler. Reader also supports publisher fonts in standalone KF8. The reference for the 34 original FONT records used an independent Palm record-table parser and a zlib implementation. Twenty-nine decompressed to valid TrueType or OpenType data. Their checksums also matched the corresponding EPUB font assets. The integrated test compares the actual font bytes used to shape text. [CREngine Palm resource loop](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L931)

The remaining five records in one converted KF8 still contain the obfuscated payload from its EPUB counterpart. The KF8 records declare compression alone. Its standard resource metadata contains no publication identifier for the EPUB font transformation. Reader falls back to available fonts for this edition. A second original supplies three valid OpenType faces in one section without assigning their families to any text in that Kindle edition. Explicit selection in the test controls uses those three faces.

### Oasis 3 verification

The same five original KF8 books were read directly from Documents with separate test state. Reader checked 34 FONT records. It used every one of the 29 valid faces in explicit shaping controls. Across 13 original spine sections, 119,725 shaped text clusters rendered in the eight publisher faces actually selected by three editions. A fourth provided three unused OpenType faces. The fifth rendered all three sampled chapters with its fallback face. Every original checksum was identical before and after the check.

The installed KOReader's CREngine identified all five files as PDB when invoked directly. It rendered an interior page from each and reported zero active embedded font families in all five. Its page counts ranged from 32 to 1,507 at the probe dimensions. KOReader's normal file registry lists its CREngine provider for AZW and MOBI extensions. Reader opens these standalone AZW3 files directly from its normal library. [KOReader format registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1585-L1590) · [CREngine format names](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4639-L4673)

The authored regression assigns Silkscreen to one heading and reads the rest of the book in the configured text face. Replacing the typeface with a damaged copy keeps the passage and its source anchor available. Silkscreen’s original OFL notice is in the fixture bundle. [Simulator capture](screenshots/kf8-publisher-font.png)

```sh
cargo test --locked --features kindle --lib kf8_font
```

### Font family details

Under **Text › Rendering**, tap **Details** or hold **Embedded fonts**.
For a KF8 edition within Reader’s whole-book inspection limits, the report
uses the same publisher layouts and local FONT records as ordinary reading.
It uses an immutable copy of the publication while checking the chapters.
This can expose a font used only in a later section. Declared families with
no observed use are marked **Not used** after the complete scan. Resource
records that fail validation cannot supply a family. With the switch off,
registered families remain visible and none are marked used.
[Reader limits and controls](embedded-font-families.md#font-family-details) ·
[Authored Kindle-format simulator capture](screenshots/embedded-fonts-report-kf8.png)

The authored KF8 test exercises a compressed and masked font used only in
the second section, one valid family with no uses, and a damaged font
record. It opens and dismisses the report from normal Reader controls and
checks the same reading location, jump history, and source bytes. The pinned
KOReader runtime opened the authored specimen and five original KF8
editions. Its Embedded Fonts callback had no registered families to show
for these six files. Reader uses the registered and instantiated distinction
from the KOReader EPUB report. [Pinned UI callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/creoptions.lua#L700-L715) · [Native KF8 replay](../scripts/koreader-kf8-font-report-check.lua)

On the Mac, the normal library and help control completed whole-book
inspection for all five original editions. Across them the reports found
four used family entries, two unused entries, and one book with no
readable family. Each report preserved the reading position and jump
history, and the five original files matched their initial bytes. The
isolated debug test took 193 seconds for all five editions, including
opening and laying out the books.

The authored test writes a reproducible KF8 when
`READER_EMBEDDED_FONT_REPORT_KF8_FIXTURE` points to a scratch path. Run
`cargo test --locked --features kindle --lib kf8_embedded_font_report_scans`
to generate it. The stock script accepts a scratch cache directory, the
Noto Sans test font, and one or more local KF8s.
