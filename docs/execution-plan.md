# Reader execution plan

Start with P0, the EPUB feasibility milestone. Use the same Kindle and books
for both candidate implementations and the KOReader baseline. Record the engine
decision before building the full reading interface. Requirement IDs refer to
the [KOReader parity matrix](requirements.md). Component boundaries are defined
in the [architecture](architecture.md).

The [EPUB feasibility results](epub-probe-results.md) record the completed
desktop and Oasis 3 experiments and the remaining acceptance checks.
The [reading parity plan](reading-parity-plan.md) breaks out each reading
workflow and format, with prerequisites and side-by-side acceptance checks.

| Stage | Work | Exit evidence |
|---|---|---|
| P0 · EPUB feasibility | Test package loading, layout, text positions, and a minimal book view on desktop and Oasis 3. K01–K05. | Corpus results, ARMHF build, device measurements, and a documented engine decision |
| P1 · Daily EPUB | Build the library, Continue Reading, typography and navigation, bookmarks, highlights, search, and Files integration. K06–K11. | Desktop workflows and an automated Kindle reading session, including reopening a book and returning to Launcher. |
| P2 · Offline parity | Add the fixed-page PDF workflow and the other offline formats and reading tools in scope. K12–K16. | Documents and annotations verified against format-specific fixtures on desktop and Kindle |
| P3 · Remaining formats, integrations, and import | Deliver the remaining parity requirements by feature, including advanced PDF, library integrations, and KOReader data import. K17–K24. | Each Kindle workflow has an acceptance result. Import reports matched and unmatched records. |

## Verification on existing Kindle documents

Use the original file, the same view and the same input action in both readers when comparing behavior. Separate opening a document, painting a page, navigating to a passage, resuming after exit and retaining an annotation. A successful open alone did not reveal the lost PDF destinations, chapter-end blank pages or missing list markers found in this collection.

| Reading workflow | Observed result | Remaining comparison |
|---|---|---|
| Files and resume | The installed 0.1.10 release opened and resumed all 117 readable entries in a 130-entry Oasis collection. Thirteen protected AZW or KFX entries displayed Kindle Library guidance. | Wider file variants. This collection contains no original DjVu or comic to compare. [Original-file results](real-document-verification.md) |
| Chapter navigation | Both Contents controls showed the current section in three original EPUBs on the Oasis. All six returns reproduced the 1264 × 1680 reading frame. | Reflow chapter metrics when a reader-defined chapter crosses spine files. [Contents and status](reading-control-evidence.md) |
| PDF links and text destinations | On the 1,066-page source PDF, all 50 valid links showed their target within one pixel at 300% in both reading modes on the Oasis. Zoomed search and reopened annotations have a separate comparison on two original files. | Combine crop, page rotation and annotations in additional editions. [PDF measurements](real-document-verification.md) |
| Language and typography | The Oasis preserved older bookmarks and mapped all 82,308 tested characters in three original Kindle books after applying the publication and passage language to English word breaks. | Original paragraph evidence covers positive indents, hanging paragraphs, explicit zero and structural resets after headings. Remaining comparisons include mixed writing directions and other hyphenation languages. [Regional typography results](english-hyphenation.md) |
| Saved marks and notes | Saved highlights extend across EPUB chapters and fixed pages. The original EPUB and PDF kept their notes after reopening. Cancelling restored the previous frame on the Oasis. | Three original EPUBs offered their existing marks from adjacent KOReader metadata and repeated imports kept a single record. The next interaction check covers tapping overlapping highlights on the page. [Annotation actions](reading-control-evidence.md) |
| Resume a book previously read in KOReader | Six original EPUBs with saved CREngine positions reopened at the passages identified by the installed KOReader. All twelve surrounding text samples matched. Adjacent PDF metadata uses the original page count and edition fingerprint. | Check other saved DOM versions and fixed-page metadata from additional editions. [Position comparison](real-document-verification.md#restoring-koreader-epub-positions) |
| DjVu Contents | Chapter names, nesting and named page destinations matched 391 headings in five distributed DjVuLibre documents. The Oasis exercised the outline and returned to the previous scan. | Test independent editions with external indexes. The personal Documents collection contained no DjVu files. [DjVu contents](reading-control-evidence.md) |

## Numeric-reference compatibility

Reader now applies CREngine's defined Windows-1252 numeric-reference substitutions without modifying literal controls or CDATA. Desktop regressions cover variable-width saved positions, both older ruby text revisions, normalized attributes, search, selection and painting. A real EPUB chapter is checked with its unchanged source and the same local font stack. The independent Symbol private-use font gap remains open. Kindle acceptance is pending the next combined build. [Implementation and limits](numeric-character-references.md)

## Private-use source font audit

The remaining 134 private-use characters are already present in an original KF8 source with no font declaration or embedded font. Its reconstructed chapters preserve the exact character sequence. An authored Microsoft Symbol cmap control renders all eight tested values with unchanged source anchors through the existing pipeline. Do not infer the original symbols from low bytes or an unrelated font. Explicit secondary Symbol-map coverage and an actual affected-passage comparison in stock KOReader remain pending device availability. [Provenance and limits](symbol-font-provenance.md)

## P0.1 Prepare the corpus

Record each fixture's source, redistribution terms, and checksum. Use original
fixtures and individual W3C tests for standards cases. Use local copies of
additional representative books for both engines. Keep personal books and
reading history out of the repo. Save expected chapters, landmarks, and text
ranges so layout and position checks can be repeated.
[W3C EPUB tests](https://github.com/w3c/epub-tests) ·
[EPUB 3.3](https://www.w3.org/TR/epub-33/)

| Case | Evidence |
|---|---|
| EPUB 2 and 3 with multiple chapters | Spine order, table of contents, chapter links, and end-of-book navigation |
| Stylesheets, embedded fonts, and images | Publisher styling, fallback fonts, and resource resolution with a grayscale screenshot |
| Vietnamese, Arabic or Hebrew, and CJK | Diacritics, direction and shaping, wrapping, and selection aligned to the displayed text |
| Long chapter and a book with many chapters | Correct pagination across chapter boundaries and bounded memory across a sustained session |
| Layout settings | Same source passage after changes to font size, margins, line spacing, and orientation |
| Missing, malformed, or oversized resources | A clear result, bounded processing, and recovery to the picker or Launcher |

## P0.2 Compare the engines on desktop

1. Build one EPUB loading interface and test reading order, navigation, and
   resource limits. Evaluate `rbook` for package parsing and inspect its
   dependency tree. Pin the selected parser for a comparable renderer test.
   [rbook](https://github.com/DevinSterling/rbook)
2. Exercise two layouts with the same chapter documents and viewport. Candidate
   A uses the offline Blitz layout path already explored in Browser. Candidate
   B uses a smaller chapter layout with Parley for text shaping and line
   breaking. Each candidate needs explicit pagination, page painting, and a
   mapping from rendered text to book locations.
   [Browser architecture](https://github.com/PrimitiveOS/kindle-primitive-browser/blob/main/docs/02-architecture.md) ·
   [Blitz](https://github.com/DioxusLabs/blitz) ·
   [Parley](https://github.com/linebender/parley)
3. Capture pages at the Oasis 3 viewport and check text continuity, glyph
   coverage, line and image placement, and repeatable passage selection. Save
   and reload a location across process restart and repagination. Advance
   candidates that meet the layout checks to the device build.

## P0.3 Verify on Kindle

The first device target is the Oasis 3 on firmware 5.18.2, using
`armv7-unknown-linux-musleabihf`. The development Mac has a Rust toolchain,
the ARMHF target libraries, Zig, and `cargo zigbuild`. Record the exact tool and
dependency versions with each build. Use the existing application package and
supervisor conventions to run the probe as a separate app.
[PrimitiveOS build pipeline](https://github.com/PrimitiveOS/kindle-primitive/blob/main/docs/handoff/PIPELINE.md) ·
[Oasis 3 device record](https://github.com/PrimitiveOS/kindle-primitive/blob/main/docs/handoff/STATE.md#kindle-oasis-3)

| Measure | Procedure |
|---|---|
| First readable page | Time from opening a book to a visible page. Restart the app for each of five runs per scenario and record the cache conditions. Report the median. |
| Page turns | Measure from physical button or swipe to the visible page with the same E Ink refresh settings. Record at least 30 warm turns and a separate set of chapter transitions. Report median and p95, and record how the visible update was measured. |
| Memory | Sample process-tree resident memory and device memory headroom during open, long-chapter pagination, and repeated chapter turns. Record the peak and check for sustained growth across repeated runs. |
| Correctness and lifecycle | Verify complete text in order across page boundaries, both page buttons, suspend and resume, process restart, return to Launcher, and USB export and eject. Verify the same passage after each layout change. |

Repeat the baseline and candidate measurements with the same books, viewport,
font characteristics, and device conditions. Record the KOReader version and
settings. Use fixed refresh conditions and keep instrumentation comparable.
Inspect the visible update alongside logs so paint timings can be distinguished
from the actual screen transition.

## P0 exit decision

Progress when one candidate meets K01–K05 for the required corpus. In particular,
check that text remains complete and positions resolve across layout changes,
that repeated runs stay within the measured Kindle memory headroom, and that
the pinned ARMHF build starts and exits reliably. Proposed prototype latency
targets are a median first page within 1.5× KOReader and warm p95 turns within
1.25× on the comparison corpus. Review these targets against the measured
baseline before scoring and record any adjustments with their reasons.

Save the corpus results, logs, screenshots, dependency licence review, and
engine choice with the benchmark run. If both candidates pass, select using
rendering coverage, Kindle performance, and maintenance cost. If neither passes,
record the failing cases, revise the layout or resource strategy, and rerun P0
before advancing to the full reading app.

## Publication and parity tracking

Run desktop checks and record the results during implementation. Once the core
reader is ready, use the existing Launcher for one automated deployment and
Kindle verification batch. Package the verified ARMHF build with the project
licences and third-party notices, publish the GitHub release, and verify the
Store listing and install path. Record evidence against each requirement.
Review KOReader reference updates when expanding the parity matrix so each
comparison is reproducible.

Run `cargo test --locked --release --features kindle` on the computer. To
prepare the same native Rust tests and fixtures for the Oasis 3, run
`python3 scripts/prepare-device-tests.py build/device-tests`. The bundle's
`run.sh` executes the suites in a private temporary directory. Its results
cover ARM execution of the document engines and simulated app inputs.
Run the bundle's `profile.sh` to measure five cold processes per EPUB fixture
and 30 warm rasterizations per process using the normal Reader or Launcher font.
It covers the multilingual, long and vertical-page fixtures and writes one
software timing and peak-memory record per process. The
hardware batch still verifies the actual panel, Launcher return and storage
ownership through the Kindle runtime.

## Daily reading history and calendar

Daily history now has a separate bounded, atomic interval journal. It retains older screen-time aggregates as undated data, records filtered reading visits, distinguishes uncapped reread time from capped distinct-screen time, and exposes a calendar and day timeline from Tools, Reading data and book information. Desktop acceptance covers deterministic time accounting, local DST boundaries, source-position preservation in real EPUB/PDF workflows and UI feedback. Native acceptance is pending. The full statistics plugin remains open for page-count rescaling, broader time-range reports, advanced calendar controls and history import/sync. [Implementation, evidence and exact limits](reading-statistics.md)
