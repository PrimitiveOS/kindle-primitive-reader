# Kindle publication fonts

Reader indexes up to 128 FONT records after the first resource in an unprotected KF8 book. Each index entry holds its position and length in the original file. When a chapter’s CSS requests a Kindle font URL, Reader reads that record from the same open file, checks the source bounds and inflates zlib data in 16 KiB input chunks. It can also read a stored record and the FONT format’s optional publisher-font mask. The encoded record, decoded face, and aggregate chapter memory each have separate size limits. A missing or malformed face leaves the text readable with the available font. The existing document protection checks run before the font index is built. [calibre’s FONT reader and writer](https://github.com/kovidgoyal/calibre/blob/a03eefa6b684af0c08c44b06c9f2d94345c41255/src/calibre/ebooks/mobi/utils.py#L437)

KOReader’s pinned CREngine Palm database importer registers JPEG, PNG and GIF resources in this part of its MOBI handler. Reader also supports publisher fonts in standalone KF8. The reference for the 34 original FONT records used an independent Palm record-table parser and a zlib implementation. Twenty-nine decompressed to valid TrueType or OpenType data. Their checksums also matched the corresponding EPUB font assets. The integrated test compares the actual font bytes used to shape text. [CREngine Palm resource loop](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L931)

The remaining five records in one converted KF8 still contain the obfuscated payload from its EPUB counterpart. The KF8 records declare compression alone. Its standard resource metadata contains no publication identifier for the EPUB font transformation. Reader falls back to available fonts for this edition. A second original supplies three valid OpenType faces in one section without assigning their families to any text in that Kindle edition. Explicit selection in the test controls uses those three faces.

The authored regression assigns Silkscreen to one heading and reads the rest of the book in the configured text face. Replacing the typeface with a damaged copy keeps the passage and its source anchor available. Silkscreen’s original OFL notice is in the fixture bundle. [Simulator capture](screenshots/kf8-publisher-font.png)

```sh
cargo test --locked --features kindle --lib kf8_font
```
