# Kindle reading parity implementation plan

Build Reader into a daily replacement for KOReader's reading workflows on Kindle. Use the same publications and device to verify the results, with Reader's own Rust implementation and interface. Keep terminal, RSS, server tools and other unrelated utilities outside the reading scope. The [requirements](requirements.md) define K01–K24. The [execution plan](execution-plan.md) tracks milestones and the [architecture](architecture.md) defines component boundaries.

Compare against the installed KOReader release for hardware measurements. Record the actual edition, format provider, options, and interaction being compared in each run. Use the current guide and provider inventory to plan capabilities introduced since the benchmark release. Keep those additional capabilities in the parity backlog until they have their own verification. [KOReader user guide](https://koreader.rocks/user_guide/) · [KOReader main features](https://github.com/koreader/koreader#main-features)

## Implementation checkpoint

The publisher-page workflow has independent checks for source order, exact destinations and the label shown at a reflowed passage. Generated character-count numbers have their own acceptance criteria. [Printed page controls](publisher-page-numbers.md)

| Area | Development coverage | Remaining work |
| --- | --- | --- |
| EPUB reading | Saved positions, page and scroll modes, reading bars, printed page references, whole-book percentage jumps, generated contents from headings or chapter starts, publisher CSS with first-line and hanging indents and supported structural selectors, local fonts and illustrations, numbered and nested lists, supported tables with column widths and merged cells, footnote previews, bounded static SVG, ruby, authored soft hyphens, automatic US and British English hyphenation, inherited language, horizontal writing direction, and publisher line-breaking rules. Supported vertical chapters use right-to-left columns and the same text anchors. Fixed pages include publisher-defined spreads and the XHTML viewport directives. | More advanced stylesheet cases, other hyphenation dictionaries and user exceptions, mixed writing modes in one chapter, and broader live typography checks |
| PDF | Fixed pages and continuous scroll, printed labels, page thumbnails, outlines and links, passwords, page, width and height fit, crop, contrast, search across wrapped text lines and local annotations. Visible embedded highlights import and annotated-copy export for intact unencrypted PDFs up to 16 MiB. | Broader password, outline and annotation types, and sustained large-scan tests |
| Other offline reading | Fixed-layout EPUB, DjVu, TXT, HTML, single-document HTMLZ, Markdown, FB2, supported MOBI/PalmDOC and KF8, DOCX, CHM, RTF, TCR, CBZ, uncompressed USTAR CBT, selected CBR, JPEG, PNG, BMP, GIF first frame, portable anymaps, static SVG, selected WebP and TIFF, and compressed StarDict with synonyms | Wider format variants and larger live document samples |
| Scanned documents | Embedded and recognized text reflow across readable source pages. Original word-image reflow for scans with complete horizontal left-to-right OCR and a column view for clear two-column scans. English and Vietnamese recognized by the worker on desktop. The ARMHF package's English OCR worker produced word boxes on Oasis 3. | Broader layout and language cases, and sustained tests with real scans |
| Library and controls | Folder navigation with file types, sizes and support labels, whole-library search, favorites, reading status, collections, author and recent views, EPUB cover previews and series, reading time, reusable text profiles, night pages, adjustable toolbar sizes, mirrored or disabled tap turns, touch lock, optional E Ink full refresh intervals, chapter-spanning page thumbnails, location history and zoom for framed comic panels in single-page views | More configurable gestures, broader accessibility, outline editing for additional provider types and excluded sections, and irregular comic panels |
| Reading data | Backup and restore with preview, text/Markdown/HTML annotation exports, adjacent KOReader EPUB bookmarks and source-verified highlighted passages, KOReader JSON annotations by unique quotation, and sampled-fingerprint checks for fixed-page sidecar progress and supported reflowable EPUB CFIs and normalized CREngine DocFragment positions | Additional CREngine DOM versions, broader CFI and sidecar variants, and migration measurements with larger libraries |
| Kindle verification | Reader 0.1.20 passed 515 native and Mac tests. Installed KOReader served as reference for source-selected EPUB layout, dictionary suggestions and result context. The earlier signed 0.1.18 package opened and resumed all 117 readable entries. | Verify the latest package through Launcher and continue comparing original documents across formats. [Reading evidence](reading-control-evidence.md) · [Original documents](real-document-verification.md) |

The initial feasibility run passed 249 desktop Reader tests, six vendored Parley tests, and 16 Files tests.
Reader rendered three public-domain DjVu samples and 12 narrative chapters from
the Standard Ebooks edition of *Alice’s Adventures in Wonderland*.

The Oasis 3 session opened Markdown and DOCX from Files, sent forward and back
events through the Kindle's page button input, reopened at the exact saved page,
and used both Done and Exit Reader to return to Launcher. The installed OCR
worker recognised 46 structured rows from a grayscale PGM using the bundled
English model. The verification session used the Kindle's sleep override.

On the same Oasis 3, KOReader v2026.07.1 exposed navigation and general actions
through top tabs and formatting controls through a separate bottom panel.
Reader 0.1.1 keeps frequent document actions visible in one lower row and uses
an anchored panel for related adjustments. Its upper bar holds Library, Pages,
Controls and Exit. Opening these controls preserves the document layout.
[KOReader user guide](https://koreader.rocks/user_guide/)

Reader 0.1.2 extends the comparison using the exact KOReader v2026.07.1 source.
Its icon menus use the Primitive UI skill and the Primitive Browser action grid.
The top and bottom gestures open separate groups, including live typography,
facing pages, fixed-page adjustments, and device display controls.
The checklist records the remaining provider and hardware checks.
[KOReader controls checklist](koreader-controls-parity.md)

Each EPUB profile ran five times, with 30 warm rasterizations per run. The
median first frame took 75.4 ms on Field Notes, 300.0 ms on Long Chapter, and
75.7 ms on the vertical chapter. The median warm p95 was 10.2, 13.1, and 16.0 ms
respectively. A separate Field Notes run using the device's normal Reader
fallback fonts covered all 401 source characters with zero missing glyphs.
These measurements end when the grayscale frame is ready in memory.

## Publisher lists in horizontal reflow

The reading engine draws outside list markers from HTML list order, `start`, `reversed`, `li value`, HTML `type` and common publisher `list-style-type` declarations. It supports decimal, leading-zero, Latin alphabetic and Roman numbering, filled discs, rings, squares and suppressed markers. A continued paragraph stays under its item. Nesting adds the width of the parent list gutter. An author-supplied matching number stays selectable and is displayed once. Heading and chapter navigation, selection, search and saved locations use the original publication text. Marker shapes for bullets are independent of font glyph coverage.

<img src="screenshots/publisher-lists.png" width="340" alt="A numbered Kindle checklist with a continued paragraph, nested ring bullets, an author-supplied number and reversed letter markers">

One existing EPUB contained 1,597 ordered items. Of those, 1,581 had no literal ordinal at the start of their text. Before this change, the source page and two controls produced identical 1264 × 1680 frames on the Mac. One control changed the first list start to 81 and the other changed ordered lists to unordered. The corrected renderer produces distinct frames and finds the same source passage in all three variants. Its native layout covered all 201,498 source characters in that section with zero missing glyphs.

A second Mac check opened 19 list-bearing sections in five existing EPUB and KF8 files. The layouts covered 485,623 of 485,623 source characters across 667 pages with zero missing glyphs. Two of those files are editions of the same work. Its long numbered section occupies one EPUB spine entry and five KF8 sections. Both cover the same 201,498 characters. These totals count editions separately and measure layout coverage. The screenshot comparison above separately verifies the presence of markers.

The fixture also exercises repagination with larger type and checks selection geometry. The KOReader comparison uses the pinned CREngine implementation for outside marker placement and text indentation. The current horizontal renderer supports outside marker geometry. Custom CSS counter styles, image markers and placement inside a line remain open. [Authored test publication](../tests/fixtures/publisher-lists.xhtml) · [KOReader marker placement](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvrend.cpp#L8684-L8705)

## P0 · Close the feasibility gate

Use Parley with direct grayscale glyph rendering as the starting EPUB engine. Keep Blitz as a reference for publisher-style differences. The Oasis 3 software tests measured 65 ms first frame and 9.1 ms warm p95 for Field Notes, and 276 ms first frame and 13.5 ms warm p95 for Long Chapter. The corresponding CREngine warm p95 measurements were 10 ms and 13 ms. Those tests stopped when a frame was ready in memory. Use the device report for the status of interactive links, actual screen timing, and USB artwork verification. [EPUB feasibility results](epub-probe-results.md)

| Requirement | Work to complete | Exit evidence |
| --- | --- | --- |
| K01 · Publication structure | Retest EPUB 2 NCX, EPUB 3 navigation, publisher CSS, embedded TTF/OTF, image placement, and links. Add samples for real books with nested and mixed content, notes in separate files, and a legacy XHTML header. Record remaining format limitations. | On Kindle, activate an inline footnote, read it, and return to the originating passage. Verify actual glyphs, publisher styles, and artwork with named font files and known text. |
| K02 · Positions | Extend the existing chapter and text position with a versioned publication identity and context for recovery. Distinguish displayed page number from the stored text location. | Save both occurrences of a repeated quote. Reopen after changing font size, margins and viewport, and resolve both to their original passages. Confirm a different book with identical chapter text keeps its own state. |
| K03–K04 · Device behavior and performance | Measure button and touch input through physical panel update. Retest rapid direction changes, both available portrait orientations, abnormal input files, power and USB transitions, and return to Launcher. | Keep separate software and physical timing results under the same refresh settings. Verify rapid presses each move the reading position, the USB graphic appears during export, and the session resumes at the same passage after eject. Record timing and memory on the multilingual, image-heavy and long-chapter samples. |
| K05 · Build and distribution | Lock the ARMHF inputs and exact licence obligations. Record source versions of the tools, fonts and assets used by the release. | Rebuild from a clean workspace, review dependency and artwork notices, and verify the normal Launcher entry on Oasis 3 without reinstalling Launcher. |

## P1 · Daily EPUB reader

Build the durable document model first. Navigation, search, bookmarks, notes, and saved progress all use that model. Keep the page visible when the reader taps near the upper center to open its top and bottom bars. Put the Library, quick bookmark and Exit Reader on the top bar. Put Contents, Bookmarks, Highlights, Find and Text settings on the bottom bar, with a separate compact Done control in the normal reading view. Physical and touch inputs invoke the same reading actions. [KOReader navigation and formatting guide](https://koreader.rocks/user_guide/)

| Feature and requirement | Implementation and dependencies | Acceptance on Kindle |
| --- | --- | --- |
| Library, current book and progress · K06 | Read files from supported library locations and Files. Index title, author, cover, series if present, and reading status. Track recent books, resumable state, and per-book settings in a versioned local store. Reconcile moves and replacements using stable identity and content revision. | Resume from Launcher and Reader, then rename or move a book and retain the saved state. Keep separate positions for books with identical names. Show a repair path when a book is missing or changed. |
| Page and scroll controls · K03, K07 | Implement page and continuous scroll modes, directional tap zones, swipes, physical page buttons, chapter transitions, previous location, and touch lock. Reuse the proven page refresh pacing. | Compare forward and back behavior against KOReader at chapter and book boundaries. Preserve the active location when switching from page to scroll, and verify touch lock keeps the chosen exit or unlock control reachable. |
| Contents, links and bookmarks · K07 | Open EPUB 3 nav and EPUB 2 NCX as nested contents. Support exact heading and fragment jumps, inline links, accessible footnotes with a back action, page or percentage jumps, and bookmarks. Confirm before handing an external link to Browser. | Jump to a nested section, open a footnote in another spine item, return to the same sentence, then reopen an added bookmark after process restart. Check invalid targets leave the page readable. |
| Everyday typography · K08 | Support reader defaults and per-book override for fonts, scaling, weight, margins, line height and text alignment. Preserve or override supported publisher fonts and styles. Validate local custom fonts and maintain script fallback. Add image inspection and readable tables to the supported reflow subset. | Verify embedded and substituted fonts, Vietnamese accents, Arabic reading direction and CJK wrapping. Compare glyphs and text order against known samples. Change typography and verify images, tables, hyperlinks and the anchored passage still work. |
| Selection, highlights and notes · K09 | Map glyph clusters to source ranges. Provide press-and-drag selection, adjustable endpoints, cross-line and cross-page ranges, note editing and deletion, and a bookmarks/highlights/notes view. Export local UTF-8 text and versioned JSON first. | Capture one passage spanning pages, add a note, edit it, repaginate, and restore the original range after restart. Verify export text, ordering and identity. Verify that deletion remains deleted after update. |
| Search in the book · K10 | Search normalized text offline and paginate exact results on demand. Provide case options, snippets, next and previous match, cancellation, and return to the starting passage. Preserve mapping from normalized search strings to original text ranges. | Find repeated terms across chapters and a term with diacritics. Long-running search can be cancelled while navigation remains responsive. The selected result lands on the intended instance. |
| Reading accessibility · K03, K08 | Support readable minimum control sizes, larger UI text, high contrast, stable contrast in night mode, and hardware or touch access to core navigation. Expose left-handed zones, a usable selection mode, and controls that work in both portrait orientations. | Verify every daily workflow with larger UI text, high contrast and both orientations. Operate navigation with page buttons, including picker and contents lists, and verify that contrast keeps text distinguishable on book artwork. |

**P1 acceptance.** Complete a continuous reading session with Wi-Fi off, including chapter navigation, a footnote round-trip, search, note creation, sleep and wake, and USB export and eject. Verify package install, update and removal against a populated library and confirm books and exports survive. Run these checks with the other reading phases during the automated Kindle verification. [PrimitiveOS package and catalog contract](https://github.com/PrimitiveOS/kindle-primitive/blob/main/docs/contracts.md)

## P2 · Fixed-page and offline reading

Add a fixed-page path beside EPUB reflow. Keep fixed-page coordinates independent of viewport zoom and crop, then reuse Library, bookmarks, annotation storage and navigation contracts where the format exposes them. Check the Viewer renderer and the existing PrimitiveOS handoff before selecting the PDF component. [PrimitiveOS Viewer](https://github.com/PrimitiveOS/kindle-primitive-viewer) · [KOReader PDF and comics guide](https://koreader.rocks/user_guide/)

| Feature and requirement | Implementation and dependencies | Acceptance on Kindle |
| --- | --- | --- |
| PDF reading · K12 | Evaluate the renderer used in Viewer for PDF rendering, document outlines, links, and text access. Build fit page, width, height and crop-to-content, zoom, pan, rotation, scroll and page modes, page labels, and jump-to-page. Support ordinary PDF passwords if the selected parser can validate them locally. Bound tile and thumbnail cache usage. | Use vector, image-heavy, large, mixed-orientation, malformed and locally password-protected samples. Preserve page and zoom state across reopening. Check crisp text, valid password behavior, page labels, both page buttons and memory while zoomed. |
| PDF text, links and annotations · K13 | Map extracted text and quadrilaterals to page space. Support a visible selection, page links, context search, dictionary entry hooks, and highlights/notes stored as Reader data. Preserve rotation and crop transforms. | Select text on rotated and cropped pages, follow an internal link, and find a repeated phrase. Restore overlay alignment after changing zoom and orientation. Show a useful message when a page has no embedded text. Compare the source file hash throughout. |
| PDF annotation interchange · K13 | Read existing PDF highlight and note annotations. Add explicit export to an annotated copy once write support passes engine and licence review. Permit inspection of unsupported annotation types so an exported copy retains original content. | Import one file with pre-existing annotations. Export a copy and reopen it in Viewer and KOReader, checking text, page geometry, notes and source file integrity. Verify export cancellation or failure leaves the original intact. |
| Images, comics and manga · K14 | Support indexed image sequences and CBZ first. Add natural sorting, right-to-left reading, single and spread views, fit/zoom, page refresh frequency, grayscale controls, and next-volume handoff. Add panel zoom after testing ordered crops. | Read `1`, `2`, `10` image pages in order in both directions. Verify zoom pan and page turn behavior, resume a large archive, open the next volume, and record ghosting and peak memory. |
| Offline dictionary · K15 | Add user-installed StarDict with bounded index/data reads. Support multiple dictionaries, preset order, tap and selection lookup, a smaller lookup popup, history controls, and a language-aware exact match before fuzzy or deinflection fallback. Read local formatted definitions safely. | With the radio off, look up known and unknown terms, switch dictionaries, and dismiss to the exact passage. Check a missing index, repeated terms and multiple languages. Repeat on PDF text. [KOReader dictionary setup](https://github.com/koreader/koreader/blob/master/frontend/ui/quickstart.lua) |
| Common reflow adapters · K16 | Adapt TXT, FB2, and offline HTML/XHTML to shared structural blocks, metadata, links and source locators. Keep parsing format-specific. Preserve original inputs and document character encodings and local resource rules. | Use representative fixtures per format, including a legacy encoding, structured chapters, styled text and a broken resource. Verify search, progress, annotation, and navigation on the device for each declared variant. |

**P2 acceptance.** Verify PDF, comics and images through normal open and resume flows plus a sustained offline reading run. Confirm inputs with missing glyph data or scan text are handled safely. Document each verified format variant and its feature support.

## P3 · Advanced reading and specialist formats

P3 includes work that depends on a suitable rendering component or on evidence from specialist document collections. A blocked component leaves its capability open in the parity plan until an alternate implementation can meet the acceptance criteria. [KOReader guide](https://koreader.rocks/user_guide/) · [KOReader reference engines](https://github.com/koreader/koreader-base)

| Feature and requirement | Implementation and dependencies | Acceptance on Kindle |
| --- | --- | --- |
| DjVu and scanned documents · K17 | Evaluate an ARMHF DjVu decoder, text-layer extraction, and page outlines. Add a scan pipeline that can crop margins, rotate or deskew, adjust contrast, segment columns, and apply configurable reflow. Compare native document reflow, image segmentation, and OCR-assisted reading against the same scans. | Use clean and skewed scans, scans with and without text layers, and multi-column pages. Verify text order and page regions, navigate and reopen at the intended passage, and check latency and memory on large files. |
| Offline OCR · K17 | Evaluate a compatible engine and separately installable language packs. Run bounded and cancellable recognition, retain page-coordinate results and language metadata, and reuse selection, search and dictionary workflows. Permit a user to clear OCR data or change language packs. | Recognize known text in a scanned PDF and DjVu sample, search for a recognized word, highlight and look it up, cancel a slow page, recover from a missing pack, and confirm normal reading remains usable. |
| Advanced EPUB and typography · K19 | Add fixed-layout EPUB reading through the fixed-page contract, including viewport/pagination and spread metadata. Extend reflow to more publisher CSS, inherited styles, custom CSS overrides, hyphenation rules and user exceptions, ligatures, ruby, vertical writing, RTL mixed-script layout, tables, SVG and supported complex content. Evaluate nonvisual media one capability at a time on Kindle. | Check EPUB fixtures for mixed scripts, contextual shaping, ruby, fixed spreads, vertical text, and viewport rotation. Compare positions and text selection across repagination. List which reading-system test cases pass and record outstanding script, layout and media gaps. [EPUB reading systems requirements](https://www.w3.org/TR/epub-rs-33/) · [KOReader typography reference](https://github.com/koreader/koreader/blob/master/frontend/apps/reader/modules/readertypography.lua) |
| Navigation depth and display controls · K07, K20 | Add a skim mode, book map, page browser/thumbnails, history, publisher page references and stable calculated references, custom contents and ignored sections. Offer reading direction, page refresh, frontlight controls where supported, and per-book layout profiles with reset. | Move between a bookmarked passage, a thumbnail, a printed-page label, and an inline link without losing return history. Verify profile precedence, skipped sections, and available hardware controls on each target model. |
| Reading library and accessibility · K20 | Add collections, favorites, search/sort/filter, reading statuses, per-book statistics and configurable gestures. Extend accessibility with larger selections and alternate interactions. Evaluate read-aloud or media-overlay playback separately against available Kindle audio and documented reference behavior. | Validate library edits and search offline with thousands of files and multiple editions. Verify a timed session excludes sleep, a custom gesture avoids unwanted navigation, and any supported audio or read-aloud workflow has a model-specific record. |
| Annotation formats and services · K09, K13 | Extend export to human-readable HTML, Markdown and Kindle Clippings-compatible text where verified. Add patterns or grayscale marks for different highlight modes, bulk export, and optional reading-annotation service adapters subject to documented interface and credentials handling. | Confirm overlapping highlights remain distinct, notes and quotations round-trip, and incomplete exports can be retried. Verify offline use without credentials and confirm service exporters transmit only selected records. [KOReader annotation export guide](https://koreader.rocks/user_guide/) · [W3C Web Annotation Data Model](https://www.w3.org/TR/annotation-model/) |
| Lookup beyond local dictionaries · K15, K21 | Compare contextual language tools such as deinflection, Wikipedia and translation. Keep network lookups opt-in and specify the selected text and service before sending it. Evaluate local language data separately. | Enable a network lookup and return to the original selection. In airplane mode, continue local lookup. Share only the intended selection, and recover from an unavailable service or an unsupported language. |
| Book acquisition · K21 | Implement versioned OPDS browsing, search and acquisition with documented authentication and calibre content integration. Download to a temporary file, validate the book and atomically move it to the chosen library folder. | Browse and acquire from representative OPDS and calibre libraries, cancel a download, retry a partial transfer, and open the downloaded book offline. Handle expired credentials and filename collisions. [OPDS specifications](https://github.com/opds-community/specs) |
| Progress and annotation sync · K22 | Implement opt-in progress synchronization with document identity separate from viewport layout. Verify KOReader progress interoperability against its public API and distinguish numeric fixed-page positions from CREngine text pointers. Treat Reader annotation sync as a separate compatible client contract. Record conflicts without silently overwriting manual jumps or edits. | Use two clients with matching and differing editions. Test offline queues, later and earlier moves, concurrent edits and deletion, pending transfers after reboot, and service authentication. With sync disabled or disconnected, local use continues. [KOReader progress client](https://github.com/koreader/koreader/blob/master/plugins/kosync.koplugin/main.lua) · [KOReader progress service](https://github.com/koreader/koreader-sync-server) |
| KOReader import, metadata backup and file associations · K23–K24 | Import known export formats and sidecar snapshots as bounded data. Match publication and revision before mapping source pointers. Show resolvable, ambiguous and missing targets. Add Reader export/restore with duplicate protection and use the existing Files handler choice for shared formats. | Import known progress and highlights twice without duplicate records, produce a useful unresolved list for missing or edited books, verify source hashes, and restore Reader metadata on a fresh test installation. Confirm Reader, Viewer and KOReader all continue to open files through their chosen handlers. |

**Optional network integrations.** Network lookup, OPDS and calibre acquisition, and progress sync can follow the offline reader. These services have their own acceptance criteria and sit outside the core reading release.

**P3 acceptance.** For each native format, record its variant coverage, reference version, constraints and Kindle evidence. Complete tests for advanced EPUB, scan reflow, OCR and interoperation before declaring reading parity. Keep unresolved capabilities visible in the status record.

## Format disposition and acceptance

Use this table as the initial format inventory. A decoder that opens a file qualifies only for the capabilities exercised on documented variants. A conversion or an explicit handoff provides access while native parity for the input format remains open. Record the reference provider and result for each target, since some formats have more than one provider. [KOReader reflow providers](https://github.com/koreader/koreader/blob/master/frontend/document/credocument.lua) · [KOReader fixed-page providers](https://github.com/koreader/koreader/blob/master/frontend/document/pdfdocument.lua) · [KOReader image providers](https://github.com/koreader/koreader/blob/master/frontend/document/picdocument.lua) · [KOReader DjVu provider](https://github.com/koreader/koreader/blob/master/frontend/document/djvudocument.lua)

| Format or family | Stage and planned disposition | Checks before a support claim |
| --- | --- | --- |
| Reflowable EPUB 2 and 3 | P0 subset measured. Native daily reading in P1, broader standards cases in P3. | Content order, nav documents, CSS and fonts, links and footnotes, glyph coverage, selection, repagination and resume. |
| Fixed-layout EPUB 3 | P3 native static XHTML adapter with facing-page placement and publisher direction. | Viewport metadata, embedded content, spreads and direction, pan and zoom, links, and per-page resume. |
| PDF | P2 native fixed pages and text. P3 scanned PDF tools. | Text and image-only PDFs, outlines, passwords, rotation, crop, source preservation, and PDF annotation interchange. |
| DjVu/DJV | P3 native decoder evaluation followed by scan tools. | Multipage containers, single pages, outlines, embedded text, coordinate accuracy and scanned reflow. |
| JPEG/PNG and standalone image sequences | P2 native using the fixed-page surface. | EXIF orientation, alpha, grayscale, zoom, dimension limits, invalid input, and resume in image sequences. |
| GIF, BMP, WebP, TIFF, SVG, JPEG 2000, JPEG XR, and portable bitmap images | Native BMP, GIF first frame, PAM/PBM/PGM/PNM/PPM, bounded static SVG and single-image lossy VP8 WebP. TIFF accepts one top-left page of uncompressed 1-bit or 8-bit grayscale strips. JP2 containers and raw J2K codestreams open in Library and inside supported comics. JPEG XR, HDP, WDP and broader variants remain to evaluate. | Animation/static policy, multi-image behavior, compression variants, orientation, alpha, external SVG resource rules, and bounded decode. |
| CBZ, CBT, CBR | Native CBZ and uncompressed USTAR CBT. CBR supports bounded single-volume, unencrypted, non-solid RAR4 and RAR5 with independent page extraction and checksum checks. Opening an installed KOReader remains available in Files for wider RAR variants. | Entry order, paths, archive variants and signatures, grayscale displays, reading direction, spreads and large archive limits. |
| TXT, FB2/FB2.ZIP, FB3 | P2 native TXT/FB2. P3 verifies ZIP packaging and FB3 packaging. | BOM and encoding variants, FB2 metadata/sections/notes/images, embedded identifiers, and bounded archive resources. |
| HTML, XHTML, HTM, HTMLZ, Markdown, XML, and plain text aliases | Native offline HTML/XHTML and single-document Calibre-style HTMLZ with one root HTML file, local PNG/JPEG and CSS, and optional OPF metadata. Evaluate other markup and archive variants in P3. | Links, images and anchors within permitted book resources, conversion fidelity, text selection, and explicit handling of unsupported script or remote content. |
| MOBI, PRC, DRM-free AZW and KF8 variants | Native legacy MOBI and PalmDOC. Reflowable KF8 accepts plain, PalmDOC and HUFF/CDIC text. Reader reconstructs chapter order, KindleGen markup and auxiliary flows, local links, stylesheets, JPEG, PNG, first-frame GIF and supported SVG artwork. Long or image-heavy chapters retain their original destinations across layout sections. Legacy MOBI preserves images, source-byte links, publisher paragraph alignment and inline font sizes. Reader’s Contents panel opens publisher navigation at verified subject headings and preserves nested entries. Exact destinations match all 194 publisher entries in the two original legacy MOBIs. Readable hybrid legacy editions keep their original chapter paths and positions. A validated unencrypted KF8 section can take over if the primary compression or encoding is unsupported. The HUFF/CDIC decoder verifies all 58 text records of three independent public books against libmobi. Fixed-layout KF8 and links into arbitrary legacy text offsets remain open. | Metadata, text encoding, links, images and mixed layouts, location recovery, and clear messages for encrypted inputs. |
| RTF, legacy DOC, CHM, PalmDOC/PDB, and TCR | Native RTF, PalmDOC and bounded TCR text. CHM opens static local help topics and nested contents through a bounded ITSF/LZX adapter. Merged help files, active content and legacy binary DOC remain open. | Structural fidelity, character sets, images, links, nested archive handling, and reading state tied to the original revision. |
| DOCX/DOCM, ODT, PPTX, XLSX, XPS and compound files | Native DOCX reading subset for macro-free Strict and Transitional Word files. Supports title and author metadata, heading contents, paragraphs, basic emphasis, and table cell order. Files offers XPS to an installed KOReader. Native XPS would require a fixed-page renderer for OpenXPS resources, font obfuscation, glyph runs and package links. [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/) · [KOReader XPS provider](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/document/pdfdocument.lua) | TOC or sheet order, displayed content, text search/selection where available, malformed or unsupported variants, and bounded document processing. |
| ZIP and TAR as document wrappers | P3 validate book, image and comic payload combinations individually. | Entry recognition, collision and storage limits, format selection, and position after reopening a verified payload. |

The DjVu reader uses the pinned Rust decoder for embedded text and NAVM publisher contents. The Mac comparison verified all 391 headings in five DjVuLibre reference books. [DjVu destinations and evidence](djvu-contents.md)
Desktop proof includes rotated text selection and rendering real public-domain
scans. Reader caps the vendored BZZ decoder's expanded output at 8 MiB and
checks the declared IW44 coefficient allocation before rendering.
Large-file memory, additional OCR languages and scan-reflow performance still
need Kindle evidence. [Offline OCR build](ocr-build.md)

The [Kindle format reading report](kindle-format-reading.md) records the Mac picker, page turn and resume results for 16 AZW3 books and one legacy MOBI, together with the synthetic format regression tests.

The Mac Reader rendered an independently generated macOS DOCX with its bold
heading and italic paragraph intact. A Calibre-generated standalone KF8 sample
produced seven pages and covered all 3,312 extracted characters. That sample
uses PalmDOC compression and contains obfuscated fonts, which Reader leaves
unused. [Pinned KF8 reference sample](https://github.com/bfabiszewski/libmobi/blob/906274205c11944b628da1c553b255acb1af7c55/tests/samples/sample-obfuscated-fonts.mobi)

Reader rendered all 567 topics from the official PuTTY 0.83 CHM on the Mac
with complete coverage of 489,427 source characters and zero missing glyphs.
The pinned CHM dependency read all 570 ordinary members of that archive.
The package retains the upstream MIT notice and limits archive directory,
topic, LZX window and frame allocations. [PuTTY 0.83 release](https://www.chiark.greenend.org.uk/~sgtatham/putty/releases/0.83.html)

KOReader registers both `.jp2` and `.j2k` with its fixed-page provider. Reader
opens single-image JP2 containers and raw JPEG 2000 Part 1 codestreams from
Library and comic pages. The decoder accepts opaque unsigned grayscale, sRGB
and sYCC with equal component depths from 1 to 16 bits. Palette colors, ICC
profiles, CMYK, alpha, signed or mixed-depth samples, and JPX or motion
containers remain outside this implementation. [KOReader provider registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L401-L402) · [Hayro JPEG 2000 decoder](https://docs.rs/hayro-jpeg2000/0.5.0/hayro_jpeg2000/)

Reader checks the encoded source and metadata before allocating wavelet or
color planes. Each file or archive page is limited to 8 MiB, 4,096 pixels
per side, 4,194,304 pixels, 8,388,608 component samples and 1,024 tiles. The
box metadata and codestream main header are each capped at 1 MiB. Independent
OpenJPEG comparisons cover tiled images, lossless grayscale and RGB values,
16-bit downconversion, and lossy decoding with subsampled chroma. The desktop
workflow opens both extensions through Library, turns a mixed comic page and
resumes it after restart. [Fixtures and verification](../tests/fixtures/README.md#jpeg-2000-pages) · [Simulator capture](screenshots/jpeg2000-beacon.png)

The desktop Reader rendered the same first page from independently prepared
compressed RAR4 and RAR5 comics. Tests cover natural ordering, independent
page access, invalid checksums and unsupported archive variants. CBR limits
the tracked decoder workspace to 32 MiB, the RAR5 dictionary to 16 MiB,
and each encoded or packed image page to 16 MiB or the format's smaller
limit. [Pinned RAR decoder](https://docs.rs/rars/0.12.3/rars/)

The horizontal line breaker supports automatic US and British English and authored soft
hyphens. The Text panel saves Automatic, Authored or Off per book. A chosen
hyphen must fit in the original font. Search, tap selection and bookmarks use
the same UTF-8 positions across all three settings. A side-by-side fixture
and the actual-book measurements are available in the [hyphenation comparison](english-hyphenation.md).

Eligible `vertical-rl` chapters shape upright glyphs with HarfRust and paint
sideways Latin from horizontal glyphs rotated clockwise. Supported ruby
pronunciations sit next to their base character or phrase. Anchored links,
selections and chapter positions use the original source. Vertical chapters
use pages and preserve the book's scroll preference for horizontal chapters.
Mixed writing modes, tables, artwork and unavailable vertical font coverage
use the horizontal rendering path.
One Japanese title showed placeholder glyphs in the desktop toolbar while the
book text rendered correctly. Verify shared UI font fallback on Kindle.
[CSS Writing Modes](https://www.w3.org/TR/css-writing-modes-3/#block-flow)

Reader rendered all seven pages at the expected dimensions across four W3C
fixed-layout publications covering device dimensions, unit suffixes, repeated
dimensions, and multiple viewport declarations.
[EPUB 3.3 fixed-layout requirements](https://www.w3.org/TR/epub-rs-33/#sec-fxl-icb)
· [W3C EPUB 3.3 test suite](https://w3c.github.io/epub-tests/epub33/)

The EPUB outline scans one bounded spine item at a time. Its heading links use
the same anchors as bookmarks and include a chapter-start fallback. Comic panel
zoom checks a small grayscale sample for enclosed panels and opens a separate
view so dismissal restores the prior view exactly. [KOReader contents tools](https://koreader.rocks/user_guide/#table-of-contents-tools)
· [KOReader panel zoom](https://koreader.rocks/user_guide/#panel-zoom)

[Parley shaping](https://github.com/linebender/parley/blob/6e1143dda817360dd3a2a64f9133c6e5dfabd881/parley_engine/src/shape/shaper.rs)
· [Parley line breaking](https://github.com/linebender/parley/blob/6e1143dda817360dd3a2a64f9133c6e5dfabd881/parley/src/layout/line_break.rs)
· [Blitz writing modes](https://github.com/dioxuslabs/blitz/blob/92b11f1bd8ab7975411328b0406a95a09445b4e7/packages/blitz-dom/src/layout/writing_mode.rs)
[DjVu codec source](https://github.com/matyushkin/djvu-rs/tree/c8e86c93214f9f10093c2c696945cfef0c9e500d)

## Evidence and release checkpoints

1. Assign each capability above its stage, verified formats, and current disposition. Mark a capability **verified** only when device evidence demonstrates its workflow. Use **partial** for a narrower working subset and **open** for planned or blocked work. Record conversion and external handlers separately from native support.
2. Build a reproducible, redistributable corpus with a manifest, hashes, expected locations, and precise file variants. Use representative full books and scanned documents for performance checks. Use focused fixtures for malformed input, uncommon scripts, repeated anchors, mixed writing directions, unsupported codecs and failure recovery. [Reader EPUB fixtures](../tests/fixtures/README.md) · [W3C EPUB test suite](https://github.com/w3c/epub-tests)
3. Run implementation and regression checks on the computer first. After the core reading build is ready, deploy it once to the existing Launcher and run the Kindle verification as a single automated batch. Track cold first readable screen, warm p95 across at least 30 turns, chapter or volume changes, peak RSS, and physical e-ink updates separately. Start EPUB comparisons with the P0 1.5× first-frame and 1.25× warm p95 software budgets and record equivalent KOReader baselines before setting targets for PDF, images or other formats.
4. Require rollback or recovery for corrupted state, interrupted exports, directory moves, missing fonts, and removed application files. Test releases against an existing populated Reader install. Verify source books stay intact through app update or removal and confirm official package contents, licences and dependency notices from the clean ARMHF build.
5. Publish each verified release to GitHub, validate the Store catalog entry and package checksums, and document device results. Publish a reading parity summary only after every claimed capability and format has verification or an explicit open disposition. Keep the outstanding features visible as follow-up work until resolved. [PrimitiveOS Store publication workflow](https://github.com/PrimitiveOS/kindle-primitive/blob/main/docs/store-submit-app.md)


## EPUB footnote preview

The Navigation panel offers **Footnote previews** for reflowable books. Its initial value is off, as in KOReader. With previews enabled, tapping a recognized reference opens a panel at the bottom of the page. Short notes use their text height. Longer notes use up to one third of the display, with tap, swipe and physical page controls. The original chapter, reading anchor and jump history stay in place while the panel is open. Tapping a second reference can replace the current note directly. A hold opens the dictionary for a word in the note. After closing the dictionary, tap another word to extend the selection and use **Look up** for the phrase. [Simulated Oasis reading screen](screenshots/footnote-preview.png)

**Open in book** or a leftward swipe follows the full target and records the source in location history. **Previous location** returns to the source. A link within the note follows its own destination, including a publisher's backlink to an exact inline reference. Closing the preview or tapping outside resumes the source page. A rightward swipe also closes it. A downward swipe closes a short note, or a longer note when started above the text.

| Behavior compared with KOReader v2026.07.1 | Rust evidence and scope |
| --- | --- |
| Recognizing a note link and choosing when to follow it | Publisher `epub:type` and document roles take precedence. Unmarked superscript, subscript or short letter/number references can open notes when embedded in prose. Heading and contents destinations and ordinary backlinks keep normal navigation. **Preview more links** can also open plausible unmarked links pointing earlier in the book. [Reader link policy](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L1406-L1514) · [Rust detection and extraction](../src/footnotes.rs) |
| Rendering a note apart from the book | A separate Parley layout renders the target container's XHTML with inherited language, direction and local resource paths. A semantic note can contain multiple paragraphs, emphasis, images and links. Oversized or unreadable excerpts fall back to full-page navigation. Rust limits a preview to 10,000 text characters and 96 KiB of serialized note markup. [KOReader popup layout](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/footnotewidget.lua#L260-L365) · [Rust panel and input](../src/app/footnote_popup.rs) |
| Detection and selection differences | KOReader also evaluates computed CSS and private engine hints, recognizes FictionBook note sections, and can infer an extended group of sibling paragraphs. Rust scopes an unmarked note to its nearest relevant text block and reads explicit or inline raised markers. Rust supports hold-to-lookup and a tap to extend the selection. KOReader also distinguishes a prolonged hold for Wikipedia. [Upstream detection](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/cre.cpp#L2962-L3670) · [Upstream note gestures](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/footnotewidget.lua#L140-L200) |

The original Night Landing EPUB passes EPUBCheck 5.4.0. The desktop workflow tests the opt-in control, ordinary full-page fallback, short and long cross-chapter notes, replacement and dismissal, light and dark paper, selection and dictionary return, internal backlinks, and both ways to follow a note to its full page. Run `cargo test --locked --features kindle --lib footnote`. Set `READER_FOOTNOTE_SCREENSHOT` to a PNG path and `READER_TEST_FONTS` to a path list of Noto Sans Regular and Bold to reproduce the screenshot.
