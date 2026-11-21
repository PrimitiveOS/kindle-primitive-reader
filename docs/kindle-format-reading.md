# Reading MOBI, PalmDOC and AZW3

Reader 0.1.6 imports DRM-free legacy MOBI and PalmDOC text, and standalone reflowable Kindle Format 8 (KF8, commonly stored as AZW3). It uses its Rust importer and the same reading view, page controls and saved-position system as EPUB.

| Reading feature | Implementation |
| --- | --- |
| Text records | Read uncompressed and PalmDOC-compressed records, including multibyte overlap and trailing-data records. Legacy text supports UTF-8 and Windows-1252. KF8 markup uses UTF-8. |
| KF8 chapters | Reassemble skeletons and fragments in reading order. Use the FDST table to locate auxiliary flows when the PalmDOC header declares the length of the main markup flow. |
| Text and links | Accept standalone HTML with or without the XHTML namespace and ordinary XHTML document type declarations. Resolve Kindle fragment offsets against the original source bytes, including text with standard HTML character references. |
| Illustrations and style | Resolve local JPEG and PNG illustrations and first-frame GIFs. Recover supported raster covers from SVG wrappers and load local KindleGen stylesheet flows. The reading engine applies supported publisher styles. |
| Large chapters | Split long text and heavily illustrated sections for layout. Continuations retain nested markup, reading order and cross-section link destinations. Contents lists the original chapter once. |
| Import bounds | The container limit is 32 MiB and the uncompressed text limit is 12 MiB. Separate budgets apply to XML nodes, record counts, images, rendered sections and the accumulated resource bytes. |

HUFF/CDIC compression, fixed-layout KF8 and the KF8 portion of hybrid publications still need separate implementation and evidence. Encrypted input returns a protected-document error.

## Tests with books from a Kindle

On November 22, 2026, the Mac simulator exercised **16 standalone AZW3 books and one legacy MOBI** drawn from the owner's Kindle Documents. All 17 had failed to open during the earlier device audit. Each test began with a new Reader profile and used the visible library picker. All 17 opened, painted a first frame, changed the frame after a forward page-button event, saved the reading position and reopened through **Continue reading**. The central reading area matched the saved frame after restart, and the stored position matched.

The debug build on this Apple Silicon Mac took 0.206–6.201 seconds from Reader startup through the first painted frame, with a median of 0.490 seconds. The measurements include browsing to each book in the picker. Kindle hardware timings are tracked in the device verification. This desktop pass sampled the opening and resumed pages of each book. It did not inspect every page of the private collection. The published evidence contains aggregate results only.

## Regression fixtures

The repository builds synthetic fixtures in Rust. No files from the private collection are required to run these tests.

| Fixture group | Regression coverage |
| --- | --- |
| MOBI and PalmDOC | Encoding, record trailers, overlapping PalmDOC backreferences, malformed inputs, encryption detection and full-length MOBI. A generated legacy book retains all 26,102 paragraphs after partitioning. |
| KF8 | Fragment and skeleton reconstruction, cross-chapter links and local art, an external declaration, an auxiliary stylesheet beyond the primary text length, namespace-free HTML, misleading image MIME metadata, invalid FDST ranges and malformed index records. |
| Layout sections | A synthetic 30-image chapter retains every placement and links in both directions across sections. A long multibyte paragraph retains its text and character references across cuts. |
| XML preprocessing | Ordinary external declarations, built-in HTML character references in text and attributes, unchanged literal sections, source-byte mapping across expansions and rejection of document-defined entities. |

Run the regression groups from the Reader repository.

```sh
cargo test --locked --lib mobi::
cargo test --locked --lib kf8::
cargo test --locked --lib kindle_sections::
cargo test --locked --lib epub_xml::
```

The format implementation was checked against [calibre's Kindle 8 reader](https://github.com/kovidgoyal/calibre/blob/a03eefa6b684af0c08c44b06c9f2d94345c41255/src/calibre/ebooks/mobi/reader/mobi8.py) and [libmobi's raw markup processing](https://github.com/bfabiszewski/libmobi/blob/906274205c11944b628da1c553b255acb1af7c55/src/parse_rawml.c).
