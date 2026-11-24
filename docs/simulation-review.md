# Reader workflow simulation review

## Current Mac simulator checkpoint

Reader composes book-opening profiles against the accepted screen turn
when Launcher offers a verified viewport. The Mac transactions preserve
the EPUB anchor, resolve competing metadata and orientation rules in list
order, and match the committed reading frame to the candidate byte for
byte. The PDF test rejects a landscape opening, turns a page in the
fallback, then commits against the verified fallback turn. The desktop
API and older Launchers continue to use the saved orientation preference.
[Opening profiles and screenshots](reading-profiles.md#apply-a-profile-on-book-opening)

Opening rules can match seven publisher metadata fields. The pinned stock
profile callbacks and native EPUB, PDF, DjVu, and comic providers were
checked against authored examples. Reader can fill a condition from the
current book, including Unicode characters, and uses a filename title
when the provider has none.
[Metadata fields](screenshots/landscape-profile-opening-metadata.png) ·
[Native provider comparison](../scripts/koreader-profile-metadata-providers.lua)

The portrait calendar can jump to a typed month, the first recorded month,
or the current month. Returning from a day timeline opens the month of the
day reached. The pinned stock callbacks and Mac interaction tests cover
these paths while preserving the reading position and statistics.
[Calendar controls and captures](reading-statistics.md#jumping-to-a-month)

The signed EPUB layout policy matches KOReader Web's paint bands for
two negative sides, a paragraph in width-neutral wrappers, and text and
background that extend beyond either physical screen edge. Reader clips
the displayed selection while restoring the complete source quote.
[Screen-edge clipping and comparisons](render-modes.md#two-negative-sides-and-clipping-at-the-screen-edge)

The four-thread Mac suite passed 1,385 library tests, one Kindle entry
test, 14 EPUB package tests, and four layout tests. Sixty-seven opt-in
tests were skipped in the main run. The normal picker opened original
EPUB, PDF, AZW3 and MOBI files, turned a page, and resumed at the same
saved position with identical central page pixels and unchanged source
hashes. The separate original-file statistics and positive-margin tests
also passed.

The Mac ARMHF release build produced a stripped, statically linked
37,361,684-byte executable with SHA-256
`b01170ba291c7ea852d50e43760d095844f1fae22352dd7ffd15b144d6e332b7`.
The local hard-float stage assembled 28 files totaling 46,935,533 bytes
uncompressed and verified the SQLite notices, embedded Launcher icon,
and OCR model. Kindle foreground acceptance and the Store release remain
open.

## Opening conditions and grouped dictionary history checkpoint

The opening-profile simulator checks conditional file paths, collections,
preferred orientation and first-time reading. The pinned KOReader callbacks
confirm that any configured condition can match and that the new-book gate
applies separately. Reader keeps its alphabetical profile order and can fill
the current book's path directly, including characters outside the Kindle
keyboard. Matching the actual display turn remains open.
[Opening profiles and captures](reading-profiles.md#apply-a-profile-on-book-opening)

The fixed-page statistics importer can browse nested Documents folders for
a KOReader SQLite snapshot. The simulator checks that the chosen database
stays associated with its preview and that the original reading anchor and
source files survive the import. Dictionary history groups local lookups by
book and time and has its own recording and clear controls. The pinned Lua
comparison checks logging, dates, repeat lookup and privacy.
[Statistics import and captures](reading-statistics.md#import-koreader-statistics-for-a-pdf-or-djvu-book) ·
[Dictionary history and captures](shared-dictionaries.md#dictionary-history)

The opt-in EPUB renderer matches the pinned Web mode's 20-pixel extensions
into either user margin for two authored paragraphs. The simulator checks
visible link hits, text selection, source anchors and the painted result.
[Signed margins and captures](render-modes.md#one-negative-physical-side-on-an-auto-width-paragraph)

The four-thread Mac suite passed 1,376 library tests, one Kindle entry
test, 14 EPUB package tests, and four layout tests. Sixty-seven opt-in
tests were skipped in the main run. Separate original-file checks passed
for the dated statistics workflow and positive publisher margins. The
normal picker opened an original EPUB, PDF, AZW3, and MOBI, turned a page,
and resumed at the saved position with identical pixels in the central
reading area and unchanged source hashes.

The Mac ARMHF release build produced a stripped, statically linked
37,298,996-byte executable with SHA-256
`ce0f519a8706abb7a6d92f206ca96601181f9efd250f2537fc3db0573c95f4a3`.
The local hard-float stage assembled 28 files totaling 46,872,845 bytes
uncompressed and verified the SQLite notices, embedded Launcher icon,
and OCR model. This checkpoint used the Mac only. Kindle foreground
acceptance and the Store release remain open.

## DjVu history and positive EPUB margins checkpoint

Reader imports reading-history visits for self-contained DjVu books.
Pinned KOReader's four visits mapped to the same physical pages in
Reader and totaled 75 seconds across three distinct pages. Import kept
the zoomed reading position, and reopening the same book as `.djv`
identified all four records as duplicates. The simulator also checked
the previously supported PDF flow.
[Fixed-page history import](reading-statistics.md#import-koreader-statistics-for-a-pdf-or-djvu-book)

Reading profiles can apply when a matching book opens. The Mac tests
check composition, missing-font recovery, the saved EPUB passage and
the PDF's physical page and tiled zoom. Dictionary presets now report
unavailable titles and can cycle through saved sets from the list or
an assigned reading gesture.
[Opening profiles](reading-profiles.md#apply-a-profile-on-book-opening) ·
[Dictionary presets](shared-dictionaries.md#named-dictionary-presets)

The opt-in EPUB layout policy matches KOReader's measured positive
side margins on supported auto-width paragraphs. Two passages in
an original EPUB matched the pinned native measurements. The Reader
test restored their saved quotes and link targets against unchanged
source bytes. [Publisher margins and captures](render-modes.md#positive-physical-margins-on-auto-width-paragraphs)

The four-thread Mac suite passed 1,365 library tests, one Kindle entry
test, 14 EPUB package tests, and four layout tests. Sixty-seven opt-in
tests were skipped. Separate original-file checks passed for the
publisher margins and the reading-history workflow. The normal picker
opened an original EPUB, PDF, AZW3, and MOBI document, turned a page, and
reopened the same rendered page with unchanged source hashes.

The Mac ARMHF release build produced a stripped, statically linked
37,212,644-byte executable with SHA-256
`2d515fef108c64d2587c2959b1e717d9794b27cfcc1055cb6a150bc348dbe379`.
The local stage passed the hard-float binary check and assembled 28
files totaling 46,786,493 bytes uncompressed, including the SQLite
notices, Launcher entry and OCR model. Kindle foreground testing and
the Store release remain open.

## PDF history import checkpoint

Reader previewed and imported the three visits written by pinned KOReader
for an authored PDF. The 60 seconds stayed attached to the same two
physical pages. Duplicate and conflicting records were reported before
Apply, and tests checked cancellation, source replacement, a full journal
and concurrent writes.
The dictionary picker applies a saved set to one query, including the
existing spelling worker and nested definitions. The source comparison
exercises typed lookup, misses and definition edits.
[PDF statistics import](reading-statistics.md#import-koreader-statistics-for-a-pdf-or-djvu-book) ·
[Dictionary lookup and light/dark captures](shared-dictionaries.md#searching-with-a-preset-once)

Saved reflowable profiles now edit hinting, kerning, publisher styles and
fonts, and daytime and night image rendering. The opt-in EPUB layout
policy matched KOReader Web's 485-pixel centered container at x = 389
on the authored EPUB fixture. The Rust tests check wrapped text,
source selection and a background that continues across pages.
[Profile comparisons](reading-profiles.md#editing-saved-values) ·
[Centered containers](render-modes.md#sized-wrappers-around-ordinary-paragraphs)

The four-thread Mac suite passed 1,351 library tests, one Kindle entry
test, 14 EPUB package tests, and four layout tests. Sixty-six opt-in
tests were skipped. The Reader picker opened cached original EPUB, PDF,
AZW3, and MOBI files, turned a page, and resumed with identical page
pixels and source hashes. The separate original EPUB/PDF reading-history
workflow also passed.

The Mac ARMHF release build from the same source produced a stripped,
statically linked ARM EABI5 executable of 37,188,196 bytes. Its SHA-256 is
`c7c519dfa4c4d8d1afe7c4c179485843a0fbc1a3357847b486a9defaccd338db`.
The local staging check assembled 28 files totaling 46,762,045 bytes
uncompressed and checked the SQLite version marker, SQLite dependency
notices, launcher entry, and OCR model. Kindle foreground acceptance
and the Store release remain open.

## Duration and named dictionary preset checkpoint

Reader's page-duration sheet matches the pinned KOReader whole-second
bounds, tap and hold steps, defaults, and accounting for ordinary visits.
Reader saves both values with Apply and keeps failed edits for retry.
Saved PDF profiles now edit crop margin, and PDF and comic profiles edit
scan contrast. The opt-in EPUB renderer uses the measured opening break
priority for supported keep groups and keeps the saved source anchor.
Offline dictionary presets can be created, applied, updated, renamed, and
deleted from Dictionaries. Presets record which sources to enable.
[Duration controls and frames](reading-statistics.md#page-duration-limits) ·
[Saved scan profile controls](reading-profiles.md#editing-saved-values) ·
[Opening-break comparison](render-modes.md) ·
[Offline dictionary presets](shared-dictionaries.md#named-dictionary-presets)

The normal picker opened cached original EPUB, PDF, AZW3, and MOBI files.
Each turned a page and reopened with identical resumed pixels and an
unchanged source hash. An additional EPUB/PDF run verified that the
statistics views leave the reading position and original bytes intact.
The four-thread Mac suite passed 1,338 library tests, one Kindle entry
test, 14 EPUB package tests, and four layout tests. Sixty-six opt-in tests
were skipped. Two OCR fixtures timed out in the higher-concurrency run,
then passed in isolation and in the complete four-thread run.
Kindle foreground acceptance and the Store release remain open.

## Seven-day report checkpoint

The seven-day reading progress report reproduces stock weekly, daily, and
session totals on the same authored visits. The actual pinned SDL widget
and Reader's portrait and landscape report have reviewed frame captures.
Reader restores the statistics menu and the same passage when dismissed.
The saved profile editor adds font weight and contrast. The internal EPUB
renderer retains the incoming paragraph margin at ordinary forced breaks.
Per-book dictionary exclusions apply across exact lookup, word forms,
fuzzy retry, and automatic CJK selection.
[Seven-day report](reading-statistics.md#seven-day-reading-progress) ·
[Saved profile values](reading-profiles.md#editing-saved-values) ·
[Forced-break comparison](render-modes.md) · [Dictionary exclusions](shared-dictionaries.md#excluding-a-source-for-one-book)

The normal Reader picker opened four cached original files on the Mac in
EPUB, PDF, AZW3, and MOBI formats. Each turned a page, saved, and resumed with
the same page pixels. Their source SHA-256 values matched before and after.
The separate original EPUB/PDF progress workflow passed. The combined Mac
suite passed 1,326 library tests, one Kindle entry test, 14 EPUB package
tests, and four layout checks. Sixty-six opt-in tests were skipped by the
standard run. Kindle foreground testing and the Store release remain open.

## Previous Mac simulator checkpoint

This pass exercises calendar book spans, a minute-based timeline start and a
separate month shift. The reviewed calendar and time editor use original
fixtures. The stock callback harness also executes the ranking query and week
span calculation. An offline dictionary miss offers a one-shot fuzzy retry
when fuzzy search is disabled. Saved profiles accept word spacing and expansion
before Apply, and the internal EPUB keep policy uses available space for an
oversized text group. [Calendar controls](reading-statistics.md#calendar-and-timeline-settings) ·
[One-shot dictionary retry](dictionary-word-forms.md#fuzzy-retry-after-a-miss) ·
[Saved profile values](reading-profiles.md#editing-saved-values) ·
[Publisher keep comparison](render-modes.md)

The latest original-file verifier opened one cached EPUB, PDF, and AZW3 through
the file picker, turned a page, and resumed at identical page pixels. The
source SHA-256 values matched before and after. The reviewed dictionary retry
and time editor frames were reproduced byte for byte. The exact combined Mac
run passed 1,318 library tests, one Kindle entry test, 14 EPUB package tests,
and four layout checks. Sixty-six opt-in tests were skipped. Kindle foreground
acceptance and the Store release remain open.

## Earlier Mac simulator checkpoint

This pass added the Reader Back gesture, independent fuzzy dictionary
search, saved profile margin editing, and nested publisher margin collapse. In
the simulator, Back restores exact within-page EPUB scroll positions and PDF pan
offsets. The real-file picker opened one cached original EPUB, PDF, and AZW3,
turned a page, and restored the page pixels through Continue reading. Each
original's SHA-256 was identical before and after. [Back navigation](back-navigation.md) ·
[Dictionary preferences](dictionary-word-forms.md) · [Saved profiles](reading-profiles.md#editing-saved-values) ·
[Nested EPUB margins](render-modes.md)

The combined Mac run passed 1,309 library tests, one Kindle entry test,
14 EPUB package tests, and four layout checks. Sixty-six opt-in cases were
skipped. The Skim placement workflow retained its existing options and order,
and the exit confirmation matched the reviewed frame byte for byte. Physical
Kindle acceptance for this candidate remains open. [Back settings capture](screenshots/back-navigation-settings.png)

## Initial viewport simulator checkpoint

The desktop example opened an authored EPUB and PDF in separate simulator
profiles. It rendered the ordinary Reader surfaces, top and bottom toolbars,
Contents and footer at the Kindle's 1264 × 1680 resolution. The EPUB preview
used the Mac's Japanese fallback for the mixed-script fixture. Functional
captures separately exercise landscape dictionary result navigation and
editing saved font size and content zoom. The renderer comparison uses
authored text-box fixtures against pinned KOReader Web mode.

The integrated Mac run passed 1,299 library tests, one Kindle entry test,
14 EPUB package tests and four layout checks. Sixty-six opt-in cases were
skipped. The latest integration reproduced the reviewed dictionary result,
profile font size and fixed overlap frames byte for byte.

[Dictionary navigation](dictionary-popup-display.md#first-and-last-results) ·
[Saved profile values](reading-profiles.md#editing-saved-values) ·
[Publisher geometry](render-modes.md)

The 22 November 2026 Mac review covers the combined bookmark browser and publisher metadata, mark creation dates, library highlight exports, publisher and custom hidden flows, vertical table padding, highlight opacity and line height, numeric font controls, contextual vocabulary review, character-reference compatibility and the corrected document-resume checker. It also covers daily reading calendars, named highlight colors and bulk edits, Japanese lookup and shared UI font fallback, catalog acquisition, opt-in progress sync and the lower toolbar label adjustment. Kindle execution and release publication of this combined candidate remain pending.

This record describes an earlier candidate. Catalog acquisition and remote progress sync are excluded from the active local-reading goal and have been removed from the current source candidate; their former checks are historical only.

The combined checkpoint at `31612701` passes **724 library tests, 14 EPUB package tests and four system tests**. Its 59 opt-in cases are omitted from that ordinary run. Separate exact-binary runs pass the original EPUB/PDF backup and default-reset workflows, content-DPI controls on original EPUB/AZW3/MOBI, Japanese public-edition lookup, original-file sync and highlights, and actual catalog subprocess acquisition. A cross-feature regression restores defaults and verifies that new editions receive them while known editions retain their own settings, anchors and notes. Source hashes remain unchanged. Initial harness failures and their corrected reruns are retained privately. These checks establish the cases they exercise; the active comparison follows [K01–K20 and K23–K24](requirements.md) and the [KOReader source inventory](koreader-source-inventory.md).

The later supported user-style slice at `4649b6ca` passes 739 library, 14 package and four system tests, plus 15 focused cases and three original-file apply/reopen/restore workflows. Its full default-style replay covers the same 48 editions, 1,509 sections, 28,075 frames, 28,065 exact page anchors and 33,405 fragment destinations as `b8ab53a6`, with no recorded spine-result changes and no modified originals. The separate combined source at `464dc69a` adds the projection foundation and passes [GitHub CI](https://github.com/PrimitiveOS/kindle-primitive-reader/actions/runs/45002113965). Renderer, search and selection migration to CSS flow remains in progress; the foundation does not yet enable hiding controls. The native package checkpoint below remains tied to `31612701`.

## Compared workflows

| Workflow | Candidate evidence | Further acceptance |
| --- | --- | --- |
| Find saved marks across books | The bookmark browser groups books, combines sources and filters, searches quotations and notes, previews marks and follows a saved target with return history. Creation dates survive import and editing. Publisher property selection and descriptions passed on 46 original documents, including independent checks of 182 fields and 19 descriptions. [Browser behavior](bookmark-browser.md) · [Metadata comparison](bookmark-metadata.md) | Additional metadata providers retain separate comparisons. Confirm the combined build on Kindle. |
| Export reading annotations | History, selected books and collections produce KOReader JSON, text, Markdown and HTML. Tests cover identity, quotations, notes, style filters and unchanged original inputs. [Export contract](library-highlights-export.md) | Compare additional upstream exporters individually and verify the combined native workflow. |
| Publisher and custom hidden content | Publisher defaults, per-book overrides and source-anchored custom markers preserve explicit hidden destinations and return history. Desktop checks include paged, scrolling and facing views, five original EPUB/PDF inputs, hidden final-page resume and comic continuation. [Publisher flows](nonlinear-reading.md) · [Custom flows](custom-hidden-flows.md) | Exercise the combined native workflow and retain the documented upstream final-regular-page defect as a comparison limit. |
| Numeric font controls | All 57 gamma tables match 14,592 values from the pinned C++ generator. Signed quarter-step weight settings retain source anchors, hit geometry and publisher face roles. Contrast repaints the retained spread or scroll view. [Font control evidence](font-contrast-and-weight.md) | FreeType and Kurbo synthesized-outline pixel equality remains unverified. Current Kindle rendering and timing remain pending. |
| Vocabulary review | Manual and opt-in automatic saving retain source context, due dates, source/search filters, explicit existing-word choices and per-word undo. The scheduler matches 2,720 pinned Lua transitions. Two original EPUBs and two PDFs retain source bytes and resume positions. [Vocabulary acceptance](vocabulary-review.md) | Native execution, SQLite migration/synchronization and source-title catalog editing have separate acceptance work. |
| Numeric reference compatibility | Defined Windows-1252 numeric references follow pinned CREngine behavior. Literal controls and CDATA retain their text. Older positions, Ruby revisions and block identities stay resolvable. One real EPUB's U+0092 glyph report is resolved without changing its source file. [Character references](numeric-character-references.md) | The fresh corpus replay is complete; current native checks remain. Private-use font symbols retain the separate scope below. |
| Publisher table padding | Layout, source selection and saved anchors follow supported top and bottom cell padding. [Padding measurements](publisher-vertical-padding.md) | Broader publisher CSS and vertical cell alignment retain separate evidence and limits. |
| Saved highlight appearance | Opacity, line height, named colors, four drawing styles and margin note markers have independent provider checks. The aggregate also verifies confirmed bulk edits, intersection filters and exported colors while retaining quotations, notes and creation dates. [Drawing comparisons](highlight-drawing-styles.md) · [Colors and markers](highlight-colors-and-note-markers.md) · [Bulk workflows](highlight-workflows.md) | Interface mirroring, retained stock frames and current native touch remain separate checks. |
| Daily reading history | Dated intervals, the monthly calendar and day timeline preserve reading positions. Deterministic accounting includes rereads, sleep, uncertain clocks and 23/25-hour local days. [Statistics](reading-statistics.md) | Full range reports, migration/sync and current native acceptance remain separate. |
| Japanese lookup | Productive forms and a cancellable longest-prefix dictionary pass retain source graphemes. Supplied font fallbacks now paint the headword correctly. Four real W3C Japanese passages and an unchanged owner English book provide separate controls. [CJK lookup](cjk-dictionary-selection.md) | Broader lexical classes, conversion options and native execution remain open. |
| Catalog acquisition | Earlier OPDS Atom/JSON, search, pagination, format choices and validated downloads passed component and real calibre 9.19 checks. [Historical acquisition record](opds-acquisition.md) | Removed from current source and excluded from the reading goal. No new acquisition/provider work is planned. |
| Progress sync | Earlier WebDAV exchange covered conflict choice, conditional writes, interrupted replies and durable clears. The two-Reader original EPUB/PDF scenario passed. [Historical sync record](progress-sync.md) | Removed from current source and excluded from the reading goal. No production provider or interoperability work is planned. |
| Document defaults | Independent reflow and fixed-page templates apply to previously unseen editions. Explicit reset retains the source position and annotations; failed reflow/persistence retains the prior view. Nonfinite typed inputs cannot replace the saved file. [Defaults](document-defaults.md) | Current native behavior, broader profile actions and model-specific viewport constraints retain separate checks. |
| Complete local backup | Versioned component preview supports existing-data preservation, explicit replacement, legacy import, process-interruption recovery and six-document cross-feature integrity. Credentials and running jobs are excluded. [Backup contract](reading-data-backups.md) | Physical storage/power and large-library memory/latency require native evidence. |
| Content DPI | Six reference presets scale supported absolute CSS boxes and images independently of reader font size. The 48-edition style inventory preserves 96-DPI arithmetic, and three original first frames match every baseline pixel. [DPI semantics and limits](render-dpi.md) | Native display checks and documented author-font/legacy-strut differences remain open. |
| User styles | Eighteen supported presets, live global defaults, per-book choices, reviewed CSS editing/import and atomic reflow have focused desktop checks. Three original formats verify changed ink and exact baseline restoration. The default-style full-corpus replay has no spine-result changes. [Supported CSS and captures](style-tweaks.md) | Full CSS rendering and integrated native behavior remain open. Unsupported input is rejected rather than accepted as an inert setting. |
| Resume from a final page | The checker closes the optional end-of-document prompt through the ordinary Back input before comparing saved document pixels. The JPEG and XHTML regression confirms the prompt appears and closes before Continue reading restores the content. [Resume checker](../src/document_check.rs) · [Regression](../src/app/end_of_book.rs) | The earlier installed 0.1.22 sweep reported 37 prompt-versus-content comparisons as failures. The corrected original-file replay and the final installed-package sweep remain pending. |
| Compact and large reading controls | Eight authored EPUB/PDF captures cover both themes and toolbar sizes. Independent OCR recognizes all 60 complete shortcut labels. The functional test opens Bookmarks through its actual touch rectangle in both toolbar sizes and document formats. [Toolbar layout and regression](../src/app/reading_bars.rs) | These are framebuffer simulations. Physical touch, orientation, e-ink update completion and page-button timing require current Kindle evidence. |

## Visual inspection

The reviewed large EPUB toolbar keeps Contents, Bookmarks, Highlights, Find, Go to, Text and Exit distinct. The large dark PDF toolbar also fits its View and Scan shortcuts. The document remains visible above the configuration panel. The bookmark results and saved-mark preview show their source, location and visible return controls. [Bookmark results](screenshots/bookmark-browser.png) · [Mark preview](screenshots/bookmark-details.png)

The initial Latin-only font set showed missing-glyph boxes in the authored Arabic passage. The updated Mac preview uses Noto Sans with the Mac's installed Geeza Pro and Arial Unicode fallback fonts. A separate render of this authored passage maps all 401 source characters and reports zero missing glyphs. These system fonts stay on the Mac. Later device comparisons will use the Kindle's own font stack.

<img src="screenshots/reader-toolbar-large.png" width="390" alt="Reader's large lower toolbar over a document containing Vietnamese, Arabic and Japanese">
<img src="screenshots/reader-toolbar-pdf-dark.png" width="390" alt="Reader's large PDF controls in dark mode with complete shortcut labels">

The earlier audit describes seven full Oasis KOReader states covering upper categories, lower settings and the dictionary overlay. Those original image files could not be located in the retained working folders during this review. An image-by-image comparison of those seven frames remains unverified. The pinned Lua handlers and earlier written observations remain available for the functional comparison. The comparison baseline is KOReader `9192014d8bd82a91dc1012473be0f238dedfdb54`. [Earlier observations](reading-control-evidence.md) · [Pinned upstream source](https://github.com/koreader/koreader/tree/9192014d8bd82a91dc1012473be0f238dedfdb54)

## Open acceptance across the full plan

K03, K04 and K11 still require current device lifecycle, latency, memory, package-update and host-return checks for this candidate. The fresh b8ab53a6 replay passes all 48 cached reflow editions, 1,509 sections, 28,075 painted frames and 28,065 exact text anchors. It maps all 26,592,055 tested non-whitespace source characters and locates all 33,405 extracted fragments. Original source hashes are unchanged. The damaged original still uses its documented recovery chapter and 31 unavailable-image placeholders; two KF8 sections with markup and an empty layout and two literal empty EPUB bodies remain distinguishable from reading content. This verifies the stated workflows and source coverage, not whole-engine pixel equality with KOReader. Additional workflow checks combine hidden ranges, bookmark navigation, exports and numeric typography on authored documents and original EPUB/PDF inputs. All checked source hashes remain unchanged.

Font fallback changes the coverage result. The baseline two-font replay reported 892 missing glyphs across 11 editions. Mac fallback fonts reduced that result to 135. The numeric-reference correction resolves one further report. The remaining 134 characters are private-use scalars in a Kindle book that supplies no font identity or font payload. Raw and converted sequences match. Reader already honors a genuinely supplied Windows Symbol cmap in an authored control. The earlier firmware-font query only checked default charmaps; its secondary-cmap and actual KOReader comparison remain pending. [Symbol provenance](symbol-font-provenance.md)

K08 and K19 retain broader publisher, scan and typography cases. Global defaults, complete local backup and content DPI pass the combined desktop gate; the supported user-style slice has its separate checks above. Canonical-source-preserving CSS suppression is undergoing consumer integration. Network acquisition and synchronization are outside the current parity goal; dated reading statistics remain an in-scope workflow with separate native acceptance. The exact `31612701` source cross-compiles into a static ARM32 hard-float production binary and five native test executables. Its 20,614,502-byte draft package verifies all 26 manifest files and 325 dependency notices. Those binaries have not run on the disconnected Kindle, and this candidate is not a published release or Store update. Completing this simulation review leaves the reading-parity goal active.
