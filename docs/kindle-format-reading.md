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

The debug build on this Apple Silicon Mac took 0.206–6.201 seconds from Reader startup through the first painted frame, with a median of 0.490 seconds. The measurements include browsing to each book in the picker. The Kindle hardware timings are listed separately below. This desktop pass sampled the opening and resumed pages of each book. The published evidence contains aggregate results only.

## Interior navigation and illustrations

A second Mac pass checked the entire import structure and selected interior pages of the same 17 books. It included the first, middle and final spine section in every book, plus sections containing illustrations, direct artwork links and numerous fragment destinations. Section numbers refer to Reader's layout sections, including continuations of long chapters.

| Check | Result |
| --- | --- |
| Local references across the full book | All 3,639 local links resolved across 501 sections. The books contained 716 image placements, 406 distinct image resources counted per book, and 72 local CSS resources. All resources passed decoding or loading checks. Adobe XPGT page templates appeared in 61 references and were counted separately from CSS. |
| Interior rendering | Reader rendered 195 selected pages in 83 sections. All 194 pages that supplied a source anchor mapped it back to the same page. All 707 sampled fragment destinations mapped to a page. |
| Artwork inspection | Reader opened all 88 sampled placed illustrations in its original-image viewer. Four links pointed directly to image resources. Both visible link targets opened in the viewer. The other two links occurred inside publisher-hidden list entries. |
| Text coverage | Layout covered all 1,142,259 non-whitespace source characters in the sampled sections. The single desktop Noto Sans test font produced 142 missing-glyph occurrences. Glyph coverage with the Kindle's additional fallback fonts needs its own measurement. |

The aggregate verifier prints counts and numeric section indices. It accepts additional reading fonts as trailing arguments.

```sh
cargo run --locked --example verify_kindle_sections -- book.azw3 path/to/reading-font.ttf
```

## Oasis 3 device verification

The physical Oasis 3 batch used the installed Documents collection and a fresh scratch profile for each file. All 16 AZW3 books and both legacy MOBI files opened from the picker, changed frame after a hardware page-key input event, and restored the saved page after a Reader restart. The extra MOBI was part of the device collection.

| Device sample | First readable frame | Peak process memory |
| --- | --- | --- |
| 16 AZW3 books | 433–6,833 ms, median 997 ms | Up to 64,548 KiB |
| Two legacy MOBI books | 1,189 ms and 2,986 ms | Up to 50,160 KiB |

Elapsed times cover Reader startup through a software-rendered first frame. The device harness sent page-key events to Reader and compared saved and resumed reading pixels. Physical e-ink panel timing requires a separate measurement.

## How KOReader opens these formats

The comparison uses KOReader v2026.07.1 at commit `9192014d`. Its `base` submodule points to `6e4bc81a`, which pins the KOReader fork of CREngine at `b32a88ff`. These are the revisions compiled for this comparison. [KOReader base pin](https://github.com/koreader/koreader/tree/9192014d8bd82a91dc1012473be0f238dedfdb54/base) · [CREngine pin](https://github.com/koreader/koreader-base/tree/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/kpvcrlib/crengine)

| Stage | What the pinned source does |
| --- | --- |
| Choosing a reader | KOReader checks filename suffixes and ranks registered providers. CREngine has weight 90 for `.mobi` and `.azw`, and handles `.prc`. MuPDF is another `.mobi` provider at weight 80. The `.azw3` suffix is absent from both registration tables. A forced provider or the unknown-extension fallback is a separate test from normal library discovery. [Registry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/documentregistry.lua#L91-L145) · [CREngine registrations](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1585-L1618) · [MuPDF registrations](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L372-L390) |
| Loading legacy content | Lua calls the CREngine document view. The native bridge passes the file to `LVDocView::LoadDocument`. Its PDB path calls `DetectPDBFormat` and `ImportPDBDocument`. The importer exposes the decompressed records as a stream, detects the contained markup, attaches an in-memory resource container, and invokes its HTML or text parser. [Lua entry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L298-L310) · [Native bridge](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L690-L705) · [Document dispatch](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4442-L4469) · [PDB import](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L1282-L1336) |
| Metadata and images | CREngine reads EXTH metadata, walks the image records after the first-image index, recognizes JPEG, PNG and GIF by their bytes, and associates the cover offset with a local resource. The DOM maps the legacy HTML `recindex` attribute to that numbered resource. [MOBI metadata and resources](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L806-L953) · [DOM image lookup](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L21756-L21775) |
| Compression and KF8 | The CREngine MOBI header accepts uncompressed records or PalmDOC and rejects encrypted MOBI. It reads a single logical legacy text stream. FDST flows, skeleton/fragment indexes and `kindle:pos` reconstruction are absent from its importer. The registered MIME type is a dispatch hint. Reader's native KF8 importer reconstructs these elements explicitly. [Header and validation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L106-L203) · [Record decoding and trailers](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/pdbfmt.cpp#L458-L610) |
| Alternate MuPDF provider | KOReader instantiates MuPDF through its PDF document wrapper and pins MuPDF 1.27.2. Its MOBI extractor accepts uncompressed or PalmDOC text, writes one HTML entry, and collects recognized image records into an archive. The compression check rejects HUFF/CDIC. Both the CREngine and MuPDF routes use their own Palm database parser. [KOReader wrapper](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L23-L42) · [MuPDF pin](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/mupdf/CMakeLists.txt#L125-L130) · [MuPDF record handling](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/html/mobi.c#L113-L175) · [HTML and images](https://github.com/ArtifexSoftware/mupdf/blob/73d3100d46d8a9ad634f6ef035bbe78f0f947886/source/html/mobi.c#L295-L332) |

Source inspection establishes the registered routes and parser behavior. A Kindle file can contain more than one representation. For hybrids, the legacy view and the reconstructed KF8 layout require separate checks of reading order, auxiliary flows and cross-chapter destinations.

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
