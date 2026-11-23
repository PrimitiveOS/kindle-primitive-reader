# Reader execution plan

Start with P0, the EPUB feasibility milestone. Use the same Kindle and books
for both candidate implementations and the KOReader baseline. Record the engine
decision before building the full reading interface. Requirement IDs refer to
the [KOReader parity matrix](requirements.md). Component boundaries are defined
in the [architecture](architecture.md).

The [EPUB feasibility results](epub-probe-results.md) record the completed
desktop and Oasis 3 experiments and the remaining acceptance checks.
The [reading parity plan](reading-parity-plan.md) breaks out each in-scope
reading workflow and format, with prerequisites and side-by-side acceptance
checks. Every implementation task must support a reading workflow. Generic
extension/plugin systems, Terminal/shell, device administration, and unrelated
utilities stay outside the Reader implementation, parity checklist, tests, and
release gates.

| Stage | Work | Exit evidence |
|---|---|---|
| P0 · EPUB feasibility | Test package loading, layout, text positions, and a minimal book view on desktop and Oasis 3. K01–K05. | Corpus results, ARMHF build, device measurements, and a documented engine decision |
| P1 · Daily EPUB | Build the library, Continue Reading, typography and navigation, bookmarks, highlights, search, and Files integration. K06–K11. | Desktop workflows and an automated Kindle reading session, including reopening a book and returning to Launcher. |
| P2 · Offline reading parity | Add fixed-page PDF, other offline formats, and format-specific reading controls. K12–K16. | Reading and annotation workflows verified against format-specific fixtures on desktop and Kindle |
| P3 · Advanced reading, formats, and import | Deliver the remaining local-reading and reading-data requirements, including advanced documents, organization, accessibility, and KOReader data import. K17–K20 and K23–K24. | Each in-scope Kindle workflow has an acceptance result. Import reports matched and unmatched records. |

The active goal is a focused Kindle reading app: document access and rendering,
navigation and layout, search and lookup, bookmarks and annotations,
accessibility, offline scan reading, and local reading data. Retain an optional
capability only when it directly supports reading, finding a book, or managing
local reading data; implement that behavior natively when KOReader delivers it
through a plug-in. OPDS or calibre acquisition (K21), remote progress or
annotation sync (K22), Wikipedia article-to-EPUB export, remote annotation
services, a general extension/plugin host, manager or catalogue, Terminal or
shell, RSS/news tools, HTTP inspectors or servers, patch management, and
unrelated utilities are closed scope. Do not implement, test, or count them as
parity gaps or release gates. File extensions identify supported document
formats. Keep device-wide controls in Launcher. See the
[scope checklist](reading-parity-plan.md#scope-and-completion-checklist).

The [pinned native KOReader interface inventory](koreader-sdl-reference.md) adds
128 complete desktop frames, including six lower configuration tabs, selection
and dictionary popups. Its [774-control matrix](koreader-sdl-reference-controls.json)
pairs visible controls with pinned source handlers and explicit workflow, partial,
gap, excluded or audit states. The capture is a stock macOS SDL reference,
separate from Oasis acceptance. Schedule only relevant reading controls from
this inventory; excluded controls are not implementation tasks, and inventory
counts do not measure completed parity.

## Current desktop integration gate

The current worktree passes `cargo-zigbuild test --locked --features kindle --bins --tests --examples`:
909 library tests passed and 62 remain ignored for private or device-provided
inputs, as did the Kindle binary test, 14 EPUB package tests, four layout
probes and all example targets. The isolated shared-input worktree passed 140 `kl-core`, eight
`primitive-app`, and four `primitive-input` tests. Reader tests used a temporary
local Cargo patch to that worktree at the time; subsequent tests now use the
same revision pinned in Cargo.toml and Cargo.lock. The optional KOReader-style
highlight prompt now offers Off, Style,
Color, and Style and color. Desktop workflows verify persistence, cancel,
per-mark appearance, EPUB and PDF saves, and that the book defaults remain
unchanged. An additional opt-in original-publication check passed on the
redistributable `build/reading/real-corpus/Alice.epub`: it saved a styled
highlight and confirmed the input bytes were unchanged. This is not the owner's
Kindle Documents corpus, which remains pending. Both chooser screenshots are
in [`docs/screenshots`](screenshots).
The current static ARMHF candidate was built from Reader commit
`0caaeb6a3f20d5a22dcee949a329cf2a798b1b22`, with the shared-input dependency
pinned to `d3caf14241facf307699d3ecc37414671496bd1a`.
`file` identifies it as a 32-bit ARM EABI5 statically linked, stripped ELF; its
size is 33,407,700 bytes and SHA-256 is
`5adb135b6d9cb42320a94f5b7b65a758a38c55e4b656f00f8ae1c431c58b3bb3`.
It is a development candidate, not a release.
`rust-objcopy` warned that this Rust
installation cannot load `libLLVM.dylib`; `file` confirms the resulting ELF is
stripped. The progress footer matches KOReader's default placement, thickness,
minimum width, margin and chapter-tick width. Placement, thickness, 5–90%
minimum width, 0–140 px margins and 1/2/3 px chapter ticks are saved for EPUB/PDF
and covered by desktop geometry tests and light/dark simulator captures. Manual
fixed-page buttons and tap zones pan through zoomed documents using the saved
overlap and traversal order; their functional tests and simulator captures are
recorded. OPDS acquisition and remote progress-sync code and worker entry
points have been removed from this source candidate. The Kindle is
disconnected, so device installation and acceptance remain open. Older release
and corpus results below describe their recorded revisions.

The long-press update adds all nine text-hold choices and the stock single-word
dictionary default/opt-out. Desktop unit and workflow tests cover each action,
preference persistence and selection return. Select mode now uses a compact
floating Cancel/Highlight sheet and extends reflowable text or same-page PDF
selections; two workflow tests cover extending, cancel and save. The optional
style/color chooser keeps the passage selected until a per-mark style or color
is confirmed. Its current selection is visible, Cancel leaves the range intact,
and Invert skips the color step. The simulator captures are
[`highlight-style-prompt.png`](screenshots/highlight-style-prompt.png) and
[`highlight-color-prompt.png`](screenshots/highlight-color-prompt.png). The
select-mode capture is [`select-mode.png`](screenshots/select-mode.png), and the
settings frame is [`long-press-settings.png`](screenshots/long-press-settings.png).
The candidate now paints a temporary held selection, extends it on paced drag,
applies the configured action at release, supports a 0.9–20.0 second very-long
threshold (3.0 second default), and scrolls at reading corners with leave and
re-enter gating. The [hold-selection capture](screenshots/hold-selection.png)
and EPUB/PDF interaction tests are current. Fixed PDF text selection remains
page-local. Kindle timing, corner hit regions, and owner-document acceptance
are open. The current desktop tests and ARMHF development candidate use the
Git-pinned shared-input revision above. Reader source and the current evidence
are committed on the parity feature branch; Kindle acceptance and publication
remain open.

Source `e79c7afe` passes 862 ordinary Mac tests, CI and eight original-document
settings, backup and DPI workflows. It repairs original EPUB 053's three rejected
media-only line ranges. Its full 48-edition replay opens/resumes every edition, checks all
1,509 sections and 28,071 frames, and restores all 28,061 inspected page anchors.
The source denominator remains 26,592,055 characters: 26,591,258 map and 797 are
explicitly omitted by publisher CSS. One eligible fragment in original 039,
zero-based chapter 7, remains unresolved alongside eight intentional CSS fragment
omissions; retain that destination gate until its geometry is repaired.

Paired spacing has separate component verification. The newer verifier captures
the actual resumed Reader session before checking all sections, so subsequent
corpus runs exercise the active spacing, font, style and hyphenation settings.
Aggregate/native acceptance for these newer changes remains open. The Kindle is
disconnected at the owner's request.
[Consumer integration and limits](css-flow-consumers.md) ·
[Session-bound verification](corpus-session-verification.md)

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
## Closed scope: acquisition and remote services · K21–K22

OPDS/calibre acquisition, remote progress or annotation sync, article export,
remote annotation services, and generic network utilities are excluded from the
current reading goal. Contextual Wikipedia and translation actions remain in
scope through K15 because they act on selected or visible reading text. Existing
acquisition and sync designs remain historical reference only; they are not
active milestones, release gates, or completion checklist items.

## Complete local reading-data backups · K24

The development candidate adds a bounded v2 envelope for non-secret reading controls, profiles/defaults, gestures, dictionary preferences, vocabulary, statistics/history, personal hyphenation, retained bookmark metadata, timer preferences and pace samples. Legacy library-only input keeps its additive merge. Each global component has an explicit preview choice; existing local components start skipped. A durable transaction journal recovers before normal startup loads data. Network accounts and jobs stay outside the envelope. [Scope, failure recovery and tests](reading-data-backups.md)

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

Daily history now has a separate bounded, atomic interval journal. It retains older screen-time aggregates as undated data, records filtered reading visits, distinguishes uncapped reread time from capped distinct-screen time, and exposes a calendar and day timeline from Tools, Reading data and book information. Desktop acceptance covers deterministic time accounting, local DST boundaries, source-position preservation in real EPUB/PDF workflows and UI feedback. Native acceptance is pending. Reading-related work remains open for page-count rescaling, broader time-range reports, advanced calendar controls and local history import or merge. [Implementation, evidence and exact limits](reading-statistics.md)

## CJK dictionary selection and Japanese forms

The desktop candidate enables Parley's supported complex-script segmentation models, adds bounded independently authored Japanese lookup forms and NFKC candidates, and checks the longest CJK source prefix through a cancellable background dictionary pass. Grapheme-safe source endpoints retain voicing marks, variation sequences and original anchors. Shared UI fonts now use supplied script fallbacks while preserving ASCII pixels. Authored regressions, real public Japanese passages and an unchanged owner EPUB cover the scoped behavior. Native acceptance, broader lexical classes, honorific/colloquial forms and the reference plugin's complete conversion/settings surface remain open. [Behavior, source comparison and limits](cjk-dictionary-selection.md)

## Global document defaults

Separate reflow and fixed-page templates now apply only to unseen content. Explicit save/reset/clear controls use the existing icon language. Older saved books retain their settings; reset keeps source position and annotations and retains fixed crop/pan within valid view bounds. Desktop regressions inject reflow/write failures, reopen both formats, preserve unavailable-font recovery and exercise four original EPUB/PDF publications. The optional versioned64KiB configuration shares validators with backup import. Native acceptance remains pending. [Behavior, source comparison and storage](document-defaults.md)

## Combined desktop and compiler checkpoint · 31612701

The aggregate passes 724 ordinary library tests, 14 package tests and four system tests, plus separate original-file/process workflows for backup, defaults, DPI, sync, lookup and acquisition. Static ARM32 hard-float production and five test executables compile; the private draft package verifies 26 files and 325 notices. Native execution, foreground/USB/power behavior, performance, the corrected 37-file resume sweep and release/Store publication remain pending while the owner is offline. User style origins/tweaks and CSS display/visibility projection retain separate implementation worktrees. [Current evidence and limits](simulation-review.md)

## User-style and source-projection desktop checkpoints

The supported style-tweak slice at `4649b6ca` passes 739 library, 14 package and four system tests. Fifteen focused cases cover user/author precedence, all 18 presets, strict diagnostics, editing/import, failure recovery and retained state. Three unchanged original EPUB/AZW3/MOBI workflows verify actual ink changes, chapter retention on reopen and restoration of baseline pixels. Its subsequent default-style replay passes all 48 original editions, 1,509 sections, 28,075 painted frames, 28,065 exact anchors and all 33,405 fragment targets, with all recorded spine results unchanged from `b8ab53a6`. [Supported behavior and limits](style-tweaks.md)

The independent `70f3b0e8` projection foundation passes 739 library, 14 package and four system tests, with 15 focused cases and a private-original semantic-footnote check. It records bounded layout provenance while preserving canonical bytes, revisions and saved anchors. It does not yet change rendering, search or selection, and exposes no hiding controls. The combined source at `464dc69a` passes [GitHub CI](https://github.com/PrimitiveOS/kindle-primitive-reader/actions/runs/45002113965); do not add the two component Mac test totals or relabel their separate original-file checks as a combined run.
