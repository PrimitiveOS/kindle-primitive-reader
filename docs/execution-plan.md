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

## Reader 0.1.50 verification plan

Exercise absolute, relative and hidden-flow page navigation with source
position and Return. Keep printed-label, percentage and chapter-and-page
entry working, and cancel stale prepared counts after sleep or geometry
changes. Compare DjVu's native raster with DjVuLibre, check the same word
at fractional zoom, and verify the portrait round trip. Confirm that the
reading footer and toolbar hit areas move with the prepared reflow and
fixed-page frames in simulation. Rotation enablement still requires the
negotiated Canvas and the Launcher and device checks.
[Go to page](go-to-page.md) · [Viewport verification](reading-edition-orientation.md)

## Reader 0.1.49 verification plan

Verify chapter, saved-mark, bookmark and numeric shortcuts from Skim in each
placement. Check held boundaries, one return location, Cancel, and that only
the pressed control changes. Compare partial profile inclusion with pinned
KOReader action selection. Confirm that applying profiles preserves excluded
settings and the reading position in reflow, PDF, DjVu and comics. Compare
the publisher-box position and raster bands against the pinned native
rendering. Verify source anchors, gap retention and the returned portrait
pixels for continuous PDF, comics and fixed-layout EPUB staging. Device
landscape remains gated on the runtime and Launcher handoff. The integrated
Mac build passed 1,239 checks. The focused captures cover three Skim placements,
selective profiles in light and dark, and the continuous PDF round trip.
[Skim controls](navigation-preferences.md#skim-position) · [Reading profiles](reading-profiles.md) · [Publisher geometry](render-modes.md#physical-space-and-paint) · [Fixed-page viewports](reading-edition-orientation.md#fixed-page-viewport-preparation)

## Reader 0.1.48 verification plan

Exercise named text and PDF profiles, including updates, duplicate names,
format gating and changes to a PDF's fit. Verify that applying a profile
keeps the source position and annotations, and that backups preserve both
the older text records and the new fixed-page settings. Compare publisher
breaks around empty elements across the four pinned modes. Keep the same
source anchor when returning from a blank slot to text. Verify the fixed
PDF stage with a link, a selected word and the portrait return frame.
The integrated Mac build passed 1,229 checks. Three profile captures and
two PDF viewport captures map to the same published simulator fixtures.
Landscape controls remain gated pending the app-wide viewport handoff.
[Reading profiles](reading-profiles.md) · [Publisher breaks](render-modes.md#forced-breaks-through-empty-boxes) · [Fixed-page viewports](reading-edition-orientation.md#fixed-page-viewport-preparation)

## Reader 0.1.47 verification plan

Compare article image settings, contextual wrapping and caption actions with
the pinned KOReader lookup. Verify that images arriving after the article
preserve the visible paragraph and that failed requests leave the article
readable. Check optional Parley margin collapse and float placement against
the pinned CREngine fixtures. Verify that preparing new reflow geometry
preserves the source passage, selection, and portrait rendering on return.
The combined Mac build passed 1,219 checks. Repeat the Wikimedia fixtures
and compare the seven captured frames on Oasis. Both paged and scrolling
reflow returned to pixel-identical portrait frames in simulation.
Landscape support still requires the coordinated Launcher handoff and
foreground Kindle verification.
[Wikipedia lookups](online-lookups.md) · [Render modes](render-modes.md)

## Reader 0.1.46 verification plan

Compare the restored opening-page callback and all three Skim positions
with pinned KOReader. Verify EPUB and PDF resume, the +/-10 actions,
progress-track drag, Cancel, Read here and Exit. Check legacy settings,
saved placement and that a preview leaves the reading origin intact until
it is accepted.
[Navigation settings and simulator frames](navigation-preferences.md)

## Reader 0.1.45 verification plan

Compare guide geometry, shades, settings, and the inward step against the
pinned overlay. Check the EPUB and PDF reflow captures, footer clearance,
disabled guides, and resume. Compare selection markup for identical quotes
in different source elements, validate escaped entities and original chapter
identity, and confirm Back restores the same selection. Keep the four-mode
CREngine reference as evidence for the remaining provider work.
[Reading guides](reading-guides.md) · [EPUB selection markup](selection-html.md) · [Render modes](render-modes.md)

## Reader 0.1.44 verification plan

Compare scan and text reflow spacing with the pinned PDF provider. Verify
that changing spacing preserves the source anchor and Cancel restores the
page. Check annotation folders, filename substitutions, selection scope,
and duplicate filenames. Reset a book for a selected day or period and
verify cutoff boundaries, preserved visits and failed writes. Test portrait
defaults and per-book choices with both supported and earlier Launchers.
[Scan reflow](scan-controls.md) · [Annotation export](library-highlights-export.md) · [Statistics](reading-statistics.md#reset-one-book-for-a-day-week-or-month) · [Portrait orientation](reading-edition-orientation.md)

## Reader 0.1.43 verification plan

Compare detected PDF crop padding and fit to the pinned native provider.
Change a preset and a custom margin on two adjacent scrolling pages. Check
saved source links and annotations, and compare Manual and full-page fit.
Verify all eight Time range choices, cutoff inclusion, New Year week groups,
day and book drill-down, page restoration, and an unchanged reading journal.
[Crop margins](continuous-semi-auto-crop.md) · [Time range reports](reading-statistics.md#time-range-reports)

## Reader 0.1.42 verification plan

Compare scan contrast presets and a fine step against the pinned grayscale
ramp. Verify defaults, old book settings and source text coordinates. Move
an authored EPUB and PDF into an archive and verify confirmation, collision
handling, failed writes, exact resume and preserved marks. Compare the
latest-bookmark swipe with stock link precedence and EPUB/PDF destinations.
Exercise touch lock, the return gesture and saved preferences.
[Scan controls](scan-controls.md) · [Archive workflow](document-end.md#move-a-book-to-archive) · [Latest bookmark](link-taps.md#swipe-to-the-latest-saved-bookmark)

The Mac suite passed 1,168 checks. The Oasis passed 1,153 Reader library
tests, 14 EPUB package tests and four layout probes. Both skipped 65 opt-in
cases. Five of six Oasis captures matched their Mac simulator frames exactly.
The post-archive Library label differed by two pixels. Foreground touch and
panel refresh have separate acceptance checks.

## Reader 0.1.41 verification plan

Scroll width-fit PDFs across pages with different crops in both directions.
Compare Manual, Auto and Semi-auto against the pinned KOReader PDF runtime.
Check original coordinates on links and annotations before and after Apply,
Cancel, rotation, zoom and reopening. Sort folders by last read and progress,
then verify persistent order, search, next-book behavior and legacy records.
Reset authored statistics by current book, another book and strict duration
cutoffs. Check confirmation, Cancel, failed writes, resume and pace.
[Continuous crop](continuous-semi-auto-crop.md) · [Folder order](document-end.md) · [Statistics reset](reading-statistics.md#reset-statistics)

The Mac suite passed 1,152 checks. The Oasis passed 1,137 Reader library
tests, 14 EPUB package tests and four layout probes. Both skipped 65 opt-in
cases. Five native captures matched their Mac simulator frames exactly. The
folder picker differed by two label pixels. Foreground touch and panel refresh
have separate acceptance checks.

## Reader 0.1.40 verification plan

Add and remove bookmarks from PDF and EPUB thumbnails. Keep the reading
location and jump history, including when an EPUB bookmark repaginates.
Check publisher image maximum dimensions in portrait and landscape; follow
an inline passage through the change and open the full illustration. Compare
PDF crop bounds for edge specks, rotated pages, shaded paper and sparse ink
with the pinned KOReader detector. Retest the existing default reading footer.
[Thumbnail bookmarks](bookmark-browser.md) · [Publisher illustrations](publisher-image-maxima.md) · [Scan crop sample](scan-margin-specks.md)

The Mac suite passed 1,143 checks. The Oasis passed 1,128 Reader library
tests, 14 EPUB package tests and four layout probes. Both skipped 65 opt-in
cases. All seven native frames matched the Mac simulator, including the
publisher illustration in both orientations. Foreground touch and physical
refresh behavior have separate checks.

## Reader 0.1.39 verification plan

Cycle enabled custom footer items through the hidden step in EPUB and PDF.
Check the default chapter-page label, ticks and Exit, locked taps, and clock
refresh. Open a footnote with a relative and a fixed size, page through a
large note, and return to the original passage. Verify saved choices after
reopen and capture the settings and reading views. Hold a PDF thumbnail to
add, rename or remove its chapter. Confirm Save and Cancel return to the
same thumbnails and keep the current page, bookmark and jump history.
[Edit chapters from thumbnails](contents-list-settings.md#edit-a-pdf-chapter-from-page-browser)
[Footer items](footer-item-cycle.md) · [Footnote size](footnote-export-roots.md#preview-font-size)

### Folder order

Compare natural name, file name, file type, last read, progress and reverse
order with the pinned KOReader file chooser. Test the saved choice after
reopen, physical pagination, retained search ranking and page, and the folder
traversal and end prompt under both saved reading orders. Use a mixed library
of unopened, active, on-hold and finished titles, including older records
with no saved percentage. [Folder order controls](document-end.md#choose-folder-order)

The exact Mac candidate passed 1,134 checks. The ARMHF bundle passed
1,119 Reader library tests, 14 EPUB package tests and four layout probes
on Oasis 3. Both skipped 65 opt-in tests. Eight native test frames match
the Mac simulator pixel for pixel. The folder picker differs by two label
pixels. These captures exercise the Reader software path on Kindle.

## Book archive verification

Check folder selection and confirmation using authored EPUB and PDF files.
Verify resumed source locations, bookmarks, progress, status and collections
after reopening. Exercise an existing target, a changed source, a replaced
folder and a failed state write. Check **Continue reading** when source
removal fails and both complete copies remain. [Archive workflow](document-end.md#move-a-book-to-archive)

## Reader 0.1.38 verification plan

Compare the optional top header’s published chapter marks across an entire
EPUB, and check both footnote font choices on the same passage. Switch
between publications in a folder and check that saved positions, bookmarks,
and encrypted-PDF cancellation work in both directions. Exercise PDF crop
regions across exact, odd and even pages, verify the source coordinates of
annotations and links, and compare an authored scan with pinned KOReader.
[Chapter marks](whole-book-screen-pages.md) · [Footnote previews](footnote-export-roots.md) · [Folder navigation](document-end.md) · [Semi-automatic crop](semi-auto-pdf-crop.md)

The Mac build passed 1,124 checks. The ARMHF candidate passed 1,109 Reader
library tests, 14 EPUB package tests
and four layout probes on the Oasis 3. It skipped 65 opt-in cases. The top
header, adjacent-file action grid and target, crop editor and book-font
footnote matched their 1,264 × 1,680 Mac simulator frames pixel for pixel.
An original 30-section EPUB produced 430 screen pages and 111 publisher
marks. Its first frame was ready in 1,763 ms and the full index in 9,401 ms.
The crop test detected content on page one of an original PDF. Both original
checks verified source checksums in place. Foreground touch and E Ink timing
have separate acceptance checks.

## Reader 0.1.37 verification plan

Verify folder return for nested EPUBs and scans with saved reading data, and
return from a deep dictionary lookup with the selected passage intact.
Compare column and row zoom against the reference overlap calculation and
reopen a saved scan viewport. Verify local Kindle clippings export from an
open book and from History, including the original clippings file on the
Oasis with a separate Reader profile.
[Folder return](document-end.md) · [Dictionary dismissal](dictionary-popup-display.md) · [Scan zoom](fixed-tile-zoom.md) · [Clippings export](library-highlights-export.md)

## Reader 0.1.35 verification plan

Compare the optional EPUB screen-page header with the pinned KOReader
page count and preserve the default chapter footer in Kindle-sized renders.
Test the first frame and completed count on an authored EPUB and an authorized
original, including positioning and refresh when typography or the viewport
changes. Record elapsed time and peak memory. [Screen page checks](whole-book-screen-pages.md)

Verify narrow and large dictionary popups, justification and nested lookup
backtracking in EPUB and PDF. Exercise link-tap changes in both formats and
check explicit link access after disabling direct taps.
[Dictionary popup comparisons](dictionary-popup-display.md) · [Link tap settings](link-taps.md)

## Current host and device integration gate

On 23 November 2026, legacy pixel-size font taps were aligned to KOReader's
±0.5 logical-size step using each book's captured screen/DPI context. The
chapter/page caption now always uses the requested hyphen, even when a saved
custom separator is a bullet. Device-side tests exercised a saved bullet
separator and confirmed the caption remains `Chapter x of y - Page m of n`.
The simulator shows that caption above the horizontal progress line with
vertical chapter ticks below it and Exit at the right. [Default footer](screenshots/reading-footer-below.png) ·
[Large EPUB footer](screenshots/footer-font-large-epub.png) ·
[Large PDF footer](screenshots/footer-font-large-fixed.png)

On 24 November 2026, `cargo test --locked --offline --features kindle`
passed 979 Reader library tests (62 ignored), one Kindle binary test, 14 EPUB
package tests, and four layout probes on macOS. The run used the published
[shared tools revision](https://github.com/PrimitiveOS/kindle-primtitive-tools/commit/f580c81e5389e880abde76e9f1f86981a0667105)
pinned by Cargo. It covers physical button inversion, footer text and separator,
named status bar presets, footer spacing and progress geometry, fixed and
reflowable captions, and custom TTF/OTF faces. The renderer and device changes
also passed the tools workspace tests and Clippy. The earlier local tools
integration run is archived [here](device-tests/reader-host-suite-20261124.log).

The pinned source inventory matched all 324 identifiers against KOReader
commit `9192014d8bd82a91dc1012473be0f238dedfdb54`. Simulator captures accompany
the [footer spacing](footer-container-layout.md), [custom text](footer-custom-text.md),
and [page-button mapping](page-turn-inversion.md) checks.

The ARMHF 0.1.26 test bundle ran natively on the Oasis 3. The first run passed
978 Reader tests and failed one because its symbol-font fixture still used a
Mac build path. Both the 14 EPUB package tests and four layout probes passed.
After [the fixture fix](https://github.com/PrimitiveOS/kindle-primitive-reader/commit/e4ddd7b),
the full Reader library suite passed 979 tests with 62 ignored in 294.49 seconds.
The fixture fix affects tests only. The corrected test executable SHA-256 is
`3a1bf563c532bc6d7a9eaa94dff4726a319e23b24730d3a9ea12df872ee7eeae`.
The Mac and Oasis both generated the same 1264 × 1680 footer frame from the same
authored EPUB, with SHA-256
`d388a1442b2156f815ce5a0fa0e0c62ffc4421d4a953c9f941325fc07aba5a50`.
This compares software rendering without measuring E Ink refresh quality.

The latest test-only ARMHF bundle built from the prior v0.1.25 Reader source
passed 960 Reader tests (62 ignored), 14 EPUB package tests, and four layout
probes on the Oasis. Its log is
[`footer-bold-kindleoasis-20261124.log`](device-tests/footer-bold-kindleoasis-20261124.log)
(SHA-256
`cc7995bc84968042081b4233f8be659eb04bc97e4abe6e0e5c4032cfd50f29b3`). The
static ARM EABI5 binary from the verified v0.1.25 GitHub release ZIP (ZIP
SHA-256
`678f42f3e44b1f32a08526c819fda4c7e3cf8efc9a714f4613383a59f274e4db`, 21,252,010
bytes). Its executable SHA-256 is
`04ac6a55c96982737bfff8e41652d7a4a3761cdd5ce7f5ccfe324c89ef4a6c06`. This
release binary is installed at `/mnt/us/reader/bin/reader-kindle`; the previous
local candidate is kept as
`/mnt/us/reader/bin/reader-kindle.pre-release-0.1.25-20261124`. Its
`--check-document` diagnostic opened a real EPUB from Documents, rendered its
first frame in 2,174 ms, advanced to a changed page and resumed from scratch
state, with a reported 51,908 KiB peak. The diagnostic scratch was removed.
The test bundle and log were also removed from the Kindle after the suite; 3.9
GiB remains free and the keep-awake flag is on. The battery reported 16% and
was not charging.
Launcher, user documents, and reading data were left unchanged. No foreground
E Ink screenshot was captured, so visual acceptance remains open. Simulator
captures show the [footer symbol modes](screenshots/footer-item-symbols.png),
[their settings](screenshots/footer-item-symbols-settings.png), and
[left](screenshots/footer-alignment-left.png),
[center](screenshots/footer-alignment-center.png), and
[right](screenshots/footer-alignment-right.png) alignment.
The two cross-page PDF selection regressions pass in both directions, after
save and reopen, and with an unreadable intermediate page. The
isolated shared-input worktree passed 140 `kl-core`, eight `primitive-app`, and
four `primitive-input` tests. Reader tests used a temporary local Cargo patch
to that worktree at the time; subsequent tests now use the same revision pinned
in Cargo.toml and Cargo.lock. The optional KOReader-style
highlight prompt now offers Off, Style,
Color, and Style and color. Desktop workflows verify persistence, cancel,
per-mark appearance, EPUB and PDF saves, and that the book defaults remain
unchanged. The current suite also verifies page/date/reverse bookmark ordering,
stable saved-location targets, settings persistence, all three annotation-row
text modes, and captures the list, annotation and settings screens. An
additional opt-in original-publication check passed on the
redistributable `build/reading/real-corpus/Alice.epub`: it saved a styled
highlight and confirmed the input bytes were unchanged. Both chooser screenshots
are in [`docs/screenshots`](screenshots).

The current local worktree adds cross-page PDF Select mode using the existing
per-page annotation-continuation format and selected-text Copy with an
in-process Reader clipboard and Paste into Reader text fields. The [cross-page
PDF capture](screenshots/pdf-cross-page-selection.png) shows the range on its
current page; the [Copy capture](screenshots/selection-copy.png) shows the
selection action grid. The custom footer now offers a selectable local clock
and battery percentage/charging state. Clock and battery refresh use the
default-Off minute refresh, which pauses during sleep and resumes on wake. The
focused Mac workflow tests and [clock](screenshots/current-time-footer.png) and
[battery setting](screenshots/battery-footer-settings.png) captures pass. The
previous ARMHF real-document candidate was a stripped, static ARM EABI5
executable, 32 MiB, SHA-256
`d28379bb12cffe9b7906e195465b7c890df4535a3c4bfdd1d1ff11d39c536322`; it is the
binary used for the 183-entry Documents sweep below. On 23 November 2026, the
pre-overlap test-only ARMHF bundle passed 927 Reader tests, 14 EPUB package
tests, and four layout probes on the Oasis; 62 Reader tests remained ignored
because they require private or device-provided inputs. The
bundle's Reader test binary SHA-256 is
`e37d6fcad932068377ca04c6eaaab19842e3b63139dab9bf2238644e6d618a58`. This
bundle was copied only to a temporary test folder, not installed over the
current Reader package. A fresh ARMHF bundle from the current feature source
also passed on the Oasis: 946 Reader tests (62 ignored), 14 package tests and
four layout probes. The Reader suite took 283.73 seconds with no failures. Its
SHA-256 was
`ceeab4b2869ad594aad26c88084f44677491ac663eb43e6c30b67abd1f0dfd83`; package
and layout binaries were
`22e99336737484dcada83e708e1c8b048e243b69759c922109a5999c0c166039` and
`7c0125bbc85934f1f764e41621649655e78c378b9c9544ccca9f5eec77b7c569`.
This bundle was built before the 0.1.24 package-version-only bump and ran from
a separate test directory; it did not replace Reader or Launcher. The suite
tests app logic and document engines offscreen, not physical touch, E Ink
refresh, or foreground visual acceptance. An offscreen software profile also
ran against three EPUB chapters; the figures below are not E Ink or
interactive UI timings.

The versioned 0.1.24 ARMHF candidate has now passed the same device suite from a
separate directory: 946 Reader tests (62 ignored), 14 package tests and four
layout probes. Its binary SHA-256 is
`fd55cce42e733bbe5ac3a2304df226e1684198df4e9a868998db76bc9678078b`. The new
candidate also opened and resumed 74 of 88 supported file candidates from the
owner's Documents tree. Three AZW and ten KFX files returned the expected
Kindle Library guidance; one PDF stopped at a possible password
prompt. The aggregate run and its limits are recorded in
[real-document verification](real-document-verification.md#reader-0124-candidate-on-the-oasis).
The installed Reader, Launcher, user documents and reading data were not
replaced or changed. Foreground UI acceptance and publication remain open.

On 23 November 2026, the current uncommitted Reader worktree based on commit
`8e956e8` was rebuilt as a test-only static ARMHF bundle and run on the Oasis 3
from `/mnt/us/reader-test-bundle`. It passed 952 Reader tests (62 ignored), 14
EPUB package tests, and four layout probes. The Reader test binary SHA-256 is
`9f0ee43c3f688b363095fce0fc2bd0d0ed196ae15125bb49b26ef14a9c6ca3f8`; the EPUB
package and layout probes are
`894da8849bec7db89d1f75cdc8d54935e82cf59d6de00644612a447fe801e8db` and
`fcd411b2d67498769c852c6b91e0baef5b91510ebab69f11bbbe06227e80e293`. The
reader-probe binary is
`dbb171732d60d12e4104a110cd6a13e6e5d3d7dd6e3ed93e8f2602e0adb8f091`. Remote
and local hashes matched. The Kindle's keep-awake setting was already enabled;
the test bundle used a temporary profile and did not replace Reader or Launcher,
or alter user documents or saved reading data. The temporary bundle has since
been removed. This native run validates ARM behavior, not foreground rendering,
physical buttons or E Ink refresh for the new status controls.

On 23 November 2026, the battery-footer threshold update was tested on the Mac
and on the Oasis 3 from a test-only ARMHF bundle built from the worktree based
on `37799e3`. The Mac suite passed 953 Reader tests (62 ignored), one Kindle
binary test, 14 EPUB package tests, and four layout probes. On-device ARMHF
results were 953 Reader tests (62 ignored), 14 package tests, and four layout
probes, with no failures. The Reader, EPUB package, layout probe, and
reader-probe binary SHA-256 values are respectively
`d210d744b65a57a2bfc9edae4936309ab6372a8a04c2292429e8afb2c6dfcccd`,
`9ba3f7e77e273302bd669a082cc7511edf09086f7f8f9324fea45716116259a6`,
`e590a66fa40acc58870ff3eec152ceb5a10d3b954c5d1e123b48caa59db77f7c`, and
`7e65db3e04852e6ff41ebb10f8a31aa9d3b31ae351b2e9d9af8222d7f89becfc`.
The test archive's SHA-256 matched on the Mac and Kindle before extraction.
The Kindle remained awake; its available storage returned to 4.0 GiB after the
temporary bundle and profile were removed. These suites validate ARM execution
of app logic and document engines only. The installed Reader and Launcher,
reading data, and owner documents were not changed, and foreground visual or
physical E Ink acceptance for the battery value remains open.

| ARMHF chapter | Characters / pages | Median first frame | Median layout | Median warm raster p95 | Peak RSS | Missing glyphs |
|---|---:|---:|---:|---:|---:|---:|
| Shore | 401 / 1 | 123.4 ms | 85.0 ms | 11.4 ms | 7,808 KiB | 43 |
| Long Chapter | 36,277 / 41 | 419.7 ms | 378.4 ms | 17.5 ms | 11,336 KiB | 0 |
| Vertical | 1,929 / 3 | 106.7 ms | 70.6 ms | 16.2 ms | 6,820 KiB | 0 |

The Shore fixture includes Arabic and Japanese glyphs; this isolated profile
used NotoSans-Regular without the device's complete fallback stack, so its 43
missing glyphs are a font-configuration limitation of this probe, not a verified
Kindle rendering result. At this checkpoint, the installed app remains
unchanged. The source changes are awaiting commit and the release package and
Store listing have not yet been published.

The test-only `--check-document` path then exercised 183 entries from the
owner's Kindle Documents tree in fresh scratch profiles. All 134 selectable
supported entries opened and resumed after a fresh start: 31 EPUB, 16 AZW3,
two MOBI, 20 PDF, four TXT, 37 JPEG, one PNG, and 23 XHTML documents. Three
protected AZW files and ten KFX files showed the expected Kindle Library
guidance. Thirty-six image paths inside sidecar or packaged-book directories
were absent from the Reader picker and were recorded as not selectable, not as
open failures. One PDF's first sweep attempt did not enter a document; two fresh
scratch-profile reruns opened and resumed it. The source documents and the
installed Reader data were left unchanged. These checks exercise the software
picker/render/resume path on the ARM Kindle; they do not cover foreground touch,
physical page keys, E Ink refresh timing, or the current installed app UI.

The previous committed ARMHF candidate was built from Reader commit
`8d5de2b62792531ccf29a5a83913ca2eca6ee1f1`, with the shared-input dependency
pinned to `d3caf14241facf307699d3ecc37414671496bd1a`.
`file` identifies it as a 32-bit ARM EABI5 statically linked, stripped ELF; its
size is 33,517,404 bytes and SHA-256 is
`17651122ade859ac21525ecf4e0ad263f0c4403e091aa7f56198c94695a4da84`.
It is a development candidate, not a release.
`rust-objcopy` warned that this Rust
installation cannot load `libLLVM.dylib`; `file` confirms the resulting ELF is
stripped. The progress footer matches KOReader's default placement, thickness,
minimum width, margin and chapter-tick width. Placement, thickness, 5–90%
minimum width, 0–140 px margins, lock-at-minimum width and 1/2/3 px chapter ticks
are saved for EPUB/PDF and covered by desktop geometry tests and light/dark
simulator captures. The optional initial-position marker is captured at open,
scoped to book or chapter progress, hidden across chapter/flow boundaries, and
reset on wake; EPUB/PDF lifecycle tests and a simulator capture cover the behavior.
The footer captures are
[`reading-footer-locked.png`](screenshots/reading-footer-locked.png) and
[`reading-footer-initial.png`](screenshots/reading-footer-initial.png). Manual
fixed-page buttons and tap zones pan through zoomed documents using the saved
overlap and traversal order; their functional tests and simulator captures are
recorded. OPDS acquisition and remote progress-sync code and worker entry
points have been removed from this source candidate. The Kindle is online and
its keep-awake setting remains enabled. Native ARM tests and the Documents
corpus replay pass as recorded above; foreground UI acceptance, current-package
installation, release packaging, and publication remain open. Older release
and corpus results below describe their recorded revisions.

The long-press update adds all nine text-hold choices and the stock single-word
dictionary default/opt-out. Desktop unit and workflow tests cover each action,
preference persistence and selection return. Select mode now uses a compact
floating Cancel/Highlight sheet and extends reflowable text or multi-page PDF
selections; desktop workflows cover forward/reverse extension, save, restart,
and unreadable intermediate pages. The optional
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
and EPUB/PDF interaction tests are current. Kindle timing, corner hit regions,
cross-page selection, Copy/Paste and owner-document acceptance still need
device comparison. The current desktop tests and ARMHF development candidate
use the Git-pinned shared-input revision above. The baseline through `f30fd78`
is committed on the parity feature branch; all current changes are local and
uncommitted. Kindle acceptance and publication remain open.

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
Use `--fixtures-only` with a new output directory when staging existing
matching ARM test binaries. It copies the fixture tree and the packaged
symbol font used by the footer and font regressions, and writes `run.sh`.
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
