# Kindle reading parity implementation plan

**Goal:** complete the PrimitiveOS Rust Reader as a focused Kindle reading app, matching KOReader for supported reading workflows. Scope is limited to opening and rendering documents, reading navigation and layout, search and lookup, bookmarks and annotations, accessibility, offline scan reading, and local reading data. Compare these capabilities against the same KOReader version, documents, and Kindle. A proposed feature must directly support one of those reading tasks to enter implementation or the parity checklist. The [requirements](requirements.md) define acceptance checks; the [execution plan](execution-plan.md) tracks milestones; the [architecture](architecture.md) defines component boundaries.

Compare against the installed KOReader release for hardware measurements. Record the actual edition, format provider, options, and interaction being compared in each run. Keep only capabilities that directly support local reading in the parity backlog. [KOReader user guide](https://koreader.rocks/user_guide/) · [KOReader main features](https://github.com/koreader/koreader#main-features)

The [pinned native KOReader interface inventory](koreader-sdl-reference.md) adds
128 complete desktop frames, including six lower configuration tabs, selection
and dictionary popups. Its [774-control source matrix](koreader-sdl-reference-controls.json)
pairs controls with pinned handlers and workflow, partial, gap, audit,
system-owned or excluded states. Excluded controls stay as source references,
not parity tasks. The capture is a stock macOS SDL reference, separate from
Oasis acceptance. Use only in-scope reading controls to schedule implementation;
inventory counts do not measure completed parity.

## Scope and completion checklist

Use the upstream inventory to identify reading behavior. It is not a request to
recreate KOReader's extension architecture or every utility in its menus.
Before implementation, map each feature to opening, reading, navigating,
searching, understanding, annotating, or resuming a local document. Features
that support one of those tasks can be implemented as native Reader modules;
the Lua plugin ABI and a general extension host are not part of the goal. File
extensions remain in scope only as document-format identifiers.

| Scope decision | Status |
| --- | --- |
| Implement document opening and rendering, reading navigation and layout, search and lookup, bookmarks and annotations, accessibility, offline scan reading, and local reading data. | Core reading scope · K01–K20, K23–K24 |
| Keep a capability only when it directly supports reading, finding or managing a book or the reader's local data. Implement an in-scope capability natively, even when KOReader delivers it through a plug-in. | Reading-only acceptance rule |
| Do not implement or count a general plugin/extension runtime, manager or catalogue, Terminal/shell, RSS/news tools, HTTP inspectors or servers, patch management, unrelated developer utilities, online book acquisition, remote progress/annotation sync, remote annotation services, or Wikipedia article-to-EPUB export. | Explicitly excluded from this goal, tests, and release gates |
| Keep battery, Wi-Fi, clock, sleep, wallpaper and other Kindle-wide settings in PrimitiveOS Launcher. | Explicitly excluded from Reader |

Only the in-scope reading requirements contribute to parity tracking. Excluded
items are closed scope: they are not unimplemented features, release blockers,
test requirements, or part of the completion denominator. The KOReader source
and screenshot inventories remain comparison references; their full menu and
module counts are not Reader requirements.

Completion order: finish the P0–P2 local reading workflow, close the remaining
in-scope document and local reading-data gaps in P3, then validate those
workflows on the Oasis. Keep the feature-by-feature reading checklist in
[requirements](requirements.md) and use the [control matrix](koreader-sdl-reference-controls.json)
only to find and compare relevant controls.

### Execution checklist

**Milestone status:** 50 of the 56 checklist items have desktop, scope, or
device evidence. The control inventory and foreground Kindle acceptance
checks below track the remaining feature and device work.

**Pinned control inventory:** 230 of 536 in-scope reading-control rows have
an implemented workflow, 236 are partial, 46 are gaps, and 24 need audit. The
[statistics reset comparison](reading-statistics.md#reset-statistics) updates
seven rows with an executable stock check and Mac verification.
Reader 0.1.32 adds [search from selection](selection-search.md),
[results density and context](search-result-presentation.md),
[temporary PDF fit](fixed-search-fit.md),
[bookmark appearance](bookmark-list-appearance.md), and
[help for the top status header](top-header-about.md).
Reader 0.1.33 adds [flexible search-result rows](flexible-search-results.md),
[focus and return from the results list](search-result-navigation.md), and
[a full-page preview of fixed-layout matches](fixed-search-result-preview.md).
Exact whole-book rendered reflow counts and foreground Kindle checks remain
separate gates.

The negotiated orientation candidate has Mac coverage for EPUB and PDF
handoff and rollback, two-page reflow in both spine directions, vertical
CJK, the Documents browser, Contents, Go to page, and selection and saved
marks. The Oasis accepted the monotonic event clock on its physical page
keys. Keep the foreground landscape milestone open through physical
refresh, touch, routed page keys, sleep, USB and exit tests.
[Orientation comparison and screenshots](reading-edition-orientation.md) ·
[Document and navigation screens](landscape-navigation.md) ·
[Selection and lookup screens](landscape-selection.md)

Mac coverage includes exact-value editors, OCR cancellation, scan text
and illustration inspection in both viewport sizes. The scan and
illustration tests preserve the source location through a rejected turn and
a return to the original view. The first live Oasis session verified portrait, both landscape turns,
injected page-key events and Exit. Later checks restored the reading frame
byte for byte after Quick Settings in both landscape directions. Mechanical
input, E Ink refresh, and sleep and USB resume checks remain open. [Exact-value editors](landscape-numeric-editors.md) ·
[Scans and OCR](landscape-scan-text.md) ·
[Illustrations](epub-gif-illustrations.md) ·
[Oasis test sequence](reading-orientation-contract.md#foreground-acceptance-on-oasis-3)

Reader now edits saved line spacing for reflowable profiles and page overlap
for fixed profiles, plus saved font size and content zoom. Dictionary result
arrows now support held jumps to the first and last definition and preserve
an armed highlight while switching sources. [Dictionary result navigation](dictionary-popup-display.md#first-and-last-results)

The experimental Parley policies match the pinned KOReader Web mode's
width and horizontal placement for two authored paragraphs. They also match
the measured definite box heights and preserve text and source positions when
a box's background fills across a page break. Full Render Mode behavior and its user-facing choices remain
open. [Saved reading profiles](reading-profiles.md) · [Publisher widths](render-modes.md)


- [x] Set the active Reader goal to KOReader's core Kindle reading workflows: document open/render, navigation/layout, search/lookup, bookmarks/annotations, reading progress, accessibility, offline scans, and local reading data.
- [x] Audit all four native CREngine Render Mode masks against authored CSS and compare Reader’s existing Parley and Blitz paths. The mode control remains open for the measured layout and source-position work. [Reference and captures](render-modes.md)
- [x] Match the measured vertical margin contributions in the horizontal Parley provider behind an internal opt-in. Check direct sibling edges, structural boundaries, and source-range restoration after pagination changes. [Block spacing comparison](render-modes.md#horizontal-block-spacing)
- [x] Add physical left and right text float exclusions and side-specific clearance to the horizontal Parley provider behind an internal opt-in. Verify per-line width, source hit-testing and ordinary-flow rollback. [Float comparison](render-modes.md#horizontal-floats-and-clearance)
- [x] Match the measured forced-break page topology through empty horizontal native boxes behind an internal opt-in. Check blank-slot reopen and canonical text resume. [Empty-break comparison](render-modes.md#forced-breaks-through-empty-boxes)
- [x] Reserve and paint the supported empty sibling boxes at the measured forced-break coordinates while retaining canonical text anchors. [Physical box comparison](render-modes.md#physical-space-and-paint)
- [x] Keep general extension/plugin systems, Terminal/shell, unrelated utilities, book acquisition, and remote services out of implementation, parity counts, test requirements, and release gates. Reading-specific capabilities remain eligible when implemented as native Reader features.
- [x] Remove OPDS acquisition and WebDAV progress-sync modules, screens, and worker commands from the current source. Keep local progress, backup, restore, and migration workflows.
- [x] Apply saved overlap and axis-order preferences to manual fixed-page turns. Physical buttons and page-turn tap zones pan through the zoomed page before advancing, matching KOReader's reading order. Fixed-page and app-level regressions pass on the Mac; the device comparison remains open.
- [x] Save Small, Medium and Large line spacing per fixed document for embedded-text and original-pixel scan reflow. Desktop checks compare KOReader’s pitch ratios and preserve the source passage through Apply, Cancel and reopen. [Evidence](fixed-reflow-spacing.md)
- [x] Add overlap-aware **Columns** and **Rows** zoom for PDF and DjVu scans. Presets, custom tenths, rotated-page scale, page traversal, and saved viewport position have desktop coverage. Kindle comparison remains open. [Workflow and simulator capture](fixed-tile-zoom.md)
- [x] Add **Semi-auto crop** for paged PDFs, with original-coordinate seed priority, saved adjustments, preserved links and annotation geometry, and exact Cancel restoration. The authored four-page crop comparison runs against the pinned Mac provider. [Workflow and oracle](semi-auto-pdf-crop.md)
- [x] Save detected PDF crop margins per book. Desktop checks compare the stock range, independently scaled scroll strips and original annotation coordinates. [Margin evidence](continuous-semi-auto-crop.md#detected-crop-margins)
- [x] Filter isolated margin specks from semi-auto PDF crops. Authored stock comparisons cover rotated pages, faint strokes, shaded paper and sparse notes. [Scan evidence](scan-margin-specks.md)
- [x] Apply page-specific semi-auto crops in width-fit continuous PDF reading. Desktop checks cover a split scroll frame, original note and link coordinates, Cancel, Apply and reopen. [Scroll evidence](continuous-semi-auto-crop.md)
- [x] Resolve manual and automatic crop per page while scrolling continuous width-fit PDFs. Desktop checks cover crop precedence, switching modes, native link geometry and page-specific scaling. [Scroll evidence](continuous-semi-auto-crop.md)
- [x] Provide numeric contrast presets and fine levels for PDF and comic archives. Preserve DjVu’s adaptive contrast and saved settings. Desktop checks match stock PDF and archive tones and preserve PDF and DjVu selection geometry. [Scan controls](scan-controls.md)
- [x] Add KOReader's continuous-scroll page overlap: configurable 1–10 repeated lines, per-book and new-book settings, page-button and automatic-turn movement, and all five stock indicators. Focused desktop tests and a simulated reading-frame capture pass; compare line placement and E Ink refresh on Kindle. [Behavior and screenshot](scroll-page-overlap.md)
- [x] Add KOReader's selection-dialog placement choices with the stock Center default and a selected-text placement option. Unit and integrated desktop tests cover geometry and persistence; capture the simulator frame.
- [x] Add the nine KOReader default text-hold actions and default single-word dictionary behavior with a setting to disable it. Implement a dedicated Select mode with Highlight, Copy and Cancel, tap or long-hold extension, reflowable selection, and cross-page PDF selection with saved continuations. Copy uses Reader's in-process clipboard and supports pasting selected EPUB/PDF text into Reader fields. [Select-mode and Copy screenshots, plus remaining device checks](long-press-behavior.md)
- [x] Add KOReader's optional highlight style/color prompts to text-selection highlighting, with per-mark appearance and no change to future book defaults. Cover Off, Style, Color and Style and color settings, cancellation, EPUB, PDF, touch targets and simulator screenshots. [Prompt behavior and screenshots](long-press-behavior.md)
- [x] Implement temporary highlight, hold-release/hold-pan selection, configurable very-long-press timing and the stock On/Off corner auto-scroll preference (On by default; active for continuous reflow). Desktop EPUB/PDF tests cover both preference states and the temporary-selection screenshot passes. PDF Select mode extends in both directions across pages when enabled, persists page continuations and rejects unreadable intermediate pages; see the [cross-page selection capture](screenshots/pdf-cross-page-selection.png).
- [x] Add KOReader's dictionary-popup Highlight/Unhighlight toggle. Save the selected EPUB or PDF passage as a persistent annotation when the lookup closes; toggling back cancels the save. Automated EPUB/PDF interaction tests pass and the desktop popup screenshot has been inspected. Kindle acceptance remains open. [Dictionary evidence](reading-control-evidence.md#dictionary-navigation-checks)
- [x] Add KOReader's dictionary pencil: tap edits the original query and long press edits the active result headword. Submitting creates a nested lookup; Back restores the prior result and source passage. EPUB interaction tests pass and the editor screenshot has been inspected. Kindle acceptance remains open. [Dictionary evidence](reading-control-evidence.md#dictionary-navigation-checks)
- [x] Add KOReader's saved, opt-in “Show romanizations” translation setting. Google requests include the romanization response field only when enabled, and results show source text, Latin romanization when returned, and translation. Keep MyMemory unchanged and older settings Off by default. The persistence and provider-response tests pass; inspect the [Off](screenshots/translation-settings-romanization-off.png) and [On](screenshots/translation-settings-romanization-on.png) settings captures. Device acceptance remains open. [Romanization behavior](translation-romanization.md)
- [x] Match KOReader's global physical page-turn inversion and status-footer indicator. Apply inversion only to hardware page-button inputs, preserve touch and gesture behavior, default Off, and expose Page turning inverted in custom-footer Icons/Letters/Compact modes. Unit and app-level tests pass; inspect the light/dark [settings](screenshots/page-turn-inversion-settings.png) and [footer](screenshots/page-turn-inversion-footer.png) captures. Foreground Kindle acceptance remains open. [Behavior and source comparison](page-turn-inversion.md)
- [ ] Compare selection placement, hold timing, corner zones and cross-page PDF selection behavior against KOReader on Kindle.
- [ ] Compare selection-dialog placement, long-press behavior, highlight appearance and dismissal against KOReader on the Kindle.
- [x] Run the desktop library, binary, package and layout suites with the published shared tools pin. The Mac run passed 979 Reader tests (62 ignored), one Kindle binary test, 14 EPUB package tests and four layout probes. [Verification record](execution-plan.md#current-host-and-device-integration-gate)
- [x] Run the 0.1.26 ARMHF test bundle on the Oasis. The corrected native suite passed 979 Reader tests (62 ignored). The 14 EPUB package tests and four layout probes passed. The Mac and Oasis footer captures match byte for byte. [Verification record](execution-plan.md#current-host-and-device-integration-gate)
- [x] Install the verified 0.1.26 release through the Store package transaction. Verify its executable checksum and run real EPUB, PDF, AZW3 and MOBI files through the release diagnostic. [Release and original-file checks](real-document-verification.md#reader-0126-release-on-the-oasis)
- [ ] Compare the 0.1.26 foreground reading footer on Kindle. [Verification record](execution-plan.md#current-host-and-device-integration-gate)
- [x] Run the previous v0.1.25 static ARMHF footer candidate on the Oasis and install it in place. This earlier device result does not cover the later romanization, button-inversion, footer separator, presets, or container settings. [Previous-device result and build hashes](execution-plan.md#current-host-and-device-integration-gate)
- [x] Add user-reorderable status metrics, KOReader-style footer separator and Icons/Letters/Compact symbol choices, “pages left includes current page,” and 0–2 decimal-place progress percentages. Verify settings persistence, item movement, and actual combined/custom captions; capture and inspect the [footer](screenshots/footer-item-symbols.png) and [settings](screenshots/footer-item-symbols-settings.png) simulator frames. Kindle acceptance remains open.
- [x] Add KOReader's Left, Center, and Right footer alignment choices. Preserve Left as the default, persist the selection, keep the progress track and Exit target fixed, and inspect the [Left](screenshots/footer-alignment-left.png), [Center](screenshots/footer-alignment-center.png), and [Right](screenshots/footer-alignment-right.png) simulator captures. The ARMHF suite passes; foreground visual acceptance remains open.
- [x] Add a selectable local current-time footer item and optional minute-boundary refresh, default Off. Verify local-time rendering, settings persistence, timer scheduling, sleep/wake pause and resume, and inspect the simulator capture; Kindle acceptance remains open. [Clock capture](screenshots/current-time-footer.png)
- [x] Verify 0.1.27 Kindle time zone behavior on the Oasis. Footer clocks, alarms, saved-mark dates and history calendar days matched the device clock. Vocabulary review dates share the same local date conversion. The system 12-hour format preference remains a separate comparison. [Device clock behavior and tests](device-local-time.md)
- [x] Add an optional battery percentage/charging item to the custom reading footer. Reuse the existing auto-refresh setting, update on minute boundaries, pause during sleep, and stop polling when hidden. Verify formatting, option persistence and scheduling; inspect the settings capture. Kindle battery reporting remains open. [Battery footer behavior and capture](battery-footer.md)
- [x] Match KOReader's battery hide threshold: saved 0–100% value, default 100%, and hide only when the battery level is higher than the configured threshold. Verify the exact boundary, clamping, persistence and conditional setting row. Kindle battery reporting remains open. [Battery footer behavior and capture](battery-footer.md)
- [x] Add KOReader's optional top status header for paged reflow reading, with title, author, clock, page count, progress, chapter marks, battery state and adjustable font size. Preserve the source anchor on relayout; hide the header in continuous scroll; verify battery/clock refresh scheduling, light/night screenshots and desktop behavior. Kindle appearance and battery checks remain open. [Header behavior and screenshots](top-status-header.md)
- [x] Match KOReader's progress-track positions and thickness choices, with an owner-requested default below the chapter/page caption, Thick 7 px, and 20% minimum width. Keep the horizontal track and vertical section ticks clear of the Exit target; expose adjustable widths, persist them, and verify EPUB/PDF layouts and light/dark simulator captures. Kindle acceptance remains open.
- [x] Add KOReader's optional horizontal footer separator, default Off. Persist it and draw the hairline at the reading-page/footer boundary for EPUB and fixed-page documents without moving the caption, progress track, chapter ticks, or Exit target. Verify the boundary pixels, settings persistence, EPUB/PDF render paths, and light/dark simulator captures. Kindle appearance remains open. [Source comparison and screenshots](footer-separator.md)
- [x] Add named status bar presets with KOReader-style create, apply, update, rename, and delete operations. Preserve non-footer preferences, enforce bounded case-insensitive unique names, and verify persistence with unit and application workflow tests. Inspect the settings, list, actions, and confirmation simulator captures. [Source comparison and screenshots](status-bar-presets.md)
- [x] Add KOReader's editable custom footer text item with its token set, a visible editor and token guide, 1–99 repetitions, whitespace-only separator merging, preference backup, and preset capture/apply. Verify save/reopen behavior, bounded expansion, battery glyph levels, settings, editor and reading screenshots in the Mac simulator. Kindle acceptance and battery refresh on hardware remain open. [Source comparison and screenshots](footer-custom-text.md)
- [x] Add KOReader's adjustable progress-track inset (0–140 px, 10 px default) and Thin/Medium/Thick chapter ticks (1/2/3 px, 2 px default). Persist both; test their EPUB/PDF geometry and ensure the track stays clear of Zoom, Exit, and the Next Comic control.
- [x] Add KOReader's status item container height (7–98, default 14) and bottom margin (0–49, default 1). Save them in Reader controls and footer presets, keep the default caption/track/Exit layout intact, and clip document pixels under larger footer settings. Verify minimum/maximum and invalid values, persistence, EPUB/PDF drawing, light/dark simulator captures, and screenshots. Kindle appearance remains open. [Behavior and source comparison](footer-container-layout.md)
- [x] Add KOReader's optional lock-at-minimum width for Alongside progress tracks. Persist it; verify short and long captions and EPUB/PDF geometry, and capture the simulator frame. Kindle acceptance remains open.
- [x] Add adjustable footer item text size using KOReader's 8–36 range and 14 default, plus the independently persisted bold toggle (default Off). Measure the caption at the selected weight so the adjacent progress track remains clear of Exit. Verify setting persistence, exact hyphen caption, EPUB rendering and the bold/regular [simulator captures](footer-font-size.md). Foreground Kindle appearance remains open. [Footer text size and weight](footer-font-size.md)
- [x] Add KOReader's selectable footer item font face using local TTF/OTF files, defaulting to Noto Sans. Persist the choice, include it in named status-bar presets, and use the same registered face for caption measurement and drawing in EPUB and PDF. Verify selection, persistence, preset restore and both render paths with the host workflow test; inspect the [chooser](screenshots/footer-font-faces.png), [selected setting](screenshots/footer-font-face-settings.png), [EPUB](screenshots/footer-font-face.png) and [PDF](screenshots/footer-font-face-pdf.png) captures. Foreground Kindle appearance remains open. [Font and footer controls](footer-font-size.md)
- [x] Match KOReader's ±0.5 logical font-size tap interval for books with legacy pixel-size preferences. Scale the interval through each book's captured screen/DPI context in both Text › Type and Text settings, preserve the legacy saved unit and reading anchor, and capture the simulator panel. Kindle raster comparison remains open. [Logical font behavior](logical-font-size.md)
- [x] Add KOReader's starting-position marker to book and chapter progress. Keep it anchored to the opened position, hide it after changing books, chapters, or hidden flows as appropriate, and reset it on wake. Verify its settings, EPUB/PDF behavior, and simulator capture. Kindle acceptance remains open.
- [x] Add per-book base stylesheet choices for Auto, None, Traditional, HTML Standard, and format-appropriate FictionBook. Author native CSS subsets, preserve publisher and inline cascade precedence, cover the choices with focused tests, and capture the Rendering panel. Exact CREngine CSS parity and Kindle appearance remain open. [Base stylesheet behavior and screenshot](base-stylesheets.md)
- [ ] Compare and implement every remaining in-scope reading requirement against the pinned KOReader source and interface evidence. Keep device-owned and excluded controls out of the implementation backlog; update statuses with evidence instead of treating the source inventory as a feature list.
- [x] Run the owner-Documents system sweep on the ARMHF candidate alongside the existing unit, functional, and simulator checks. The candidate checked 183 entries in isolated profiles: 134 selectable supported files passed open/resume; three protected AZW and ten KFX entries showed Kindle Library guidance; 36 sidecar or packaged-book image paths were not selectable. One PDF passed two fresh-profile reruns after a transient first attempt, and all original files remained unchanged. [Device run details](execution-plan.md#current-host-and-device-integration-gate)
- [ ] Complete foreground Kindle acceptance for document controls, touch and exit, sleep/wake, USB handoff, state preservation and the current installed app UI. Capture comparable device screenshots for the reading surface, top and bottom controls, selection/lookup, and reading settings.
- [ ] Publish and list the candidate after the desktop and Kindle gates pass, then record the release checksum, Store listing, screenshots, and device evidence.

The [release and original file log](real-document-verification.md) records
which build passed the automated Oasis checks and which package is installed.
The source matrix records measured workflows separately from foreground
checks. Use the current installed binary for physical input, E Ink timing,
sleep and wake, and USB acceptance.

The Text → Rendering panel now offers per-book base styles. Auto follows the
document format, publisher CSS and inline declarations remain higher in the
cascade, and the built-in sheets are native subsets. See the [behavior and
desktop capture](base-stylesheets.md); Kindle visual acceptance remains open.

The optional top status header appears in paged reflow documents and has a
per-book switch, an About panel, and its own font and display settings.
The native renderer has been exercised. Foreground appearance, E Ink updates,
and battery refresh on the panel remain open. [Behavior and screenshots](top-status-header.md)

## Search result navigation

Reader 0.1.34 adds **All results** and **Results in current chapter** to the
search result options. The selected result sequence drives the list, physical
paging, previews, first/last shortcuts, and counts while batches arrive.
Authored desktop EPUB and rotated PDF tests cover stable anchors, empty
results, history and viewport restoration. The native and release checks
remain open. [Search result navigation and screenshots](search-result-navigation.md)

Whole-book screen page counts remain a separate layout task. Keep the
current section caption and allow exact global numbering only when the layout
can supply a complete count.

## Implementation checkpoint

Library and Reading Data expose local reading and data workflows. OPDS
acquisition and WebDAV progress-sync modules, UI paths and worker entry points
have been removed from the current source candidate. Existing per-user config
files are not loaded or changed by Reader. Previously published packages are
unchanged. Contextual Wikipedia and translation remain available as confirmed
reading actions under K15; local reading-data backup and migration remain in
scope.

Custom hidden reading flows now have source-anchored boundary editing, explicit hidden-page destinations, main-flow navigation and separate page counts across paged, scroll and spread layouts. Desktop source/reference and private-original checks are recorded separately from pending aggregate native and foreground acceptance. [Custom-flow evidence](custom-hidden-flows.md)

The publisher-page workflow has independent checks for source order, exact destinations and the label shown at a reflowed passage. Generated character-count numbers have their own acceptance criteria. [Printed page controls](publisher-page-numbers.md)

Automatic turns accept and persist custom inactivity intervals from 1 second
to 24 hours and signed scroll distance from −20.0 to +20.0 pages. Reflowable
scroll views move by the selected fraction and paged views advance whole pages.
Fixed-page AutoTurn exposes per-book horizontal and vertical overlap (0–90%),
horizontal-first or vertical-first traversal, and top-to-bottom or
bottom-to-top reading. **Save current** includes these settings in fixed-page
defaults for new documents. Desktop checks pass; Kindle comparison of the new
controls and resumed preferences remains open.
[Automatic page-turn parity](auto-turn-parity.md)

| Area | Development coverage | Remaining work |
| --- | --- | --- |
| EPUB reading | Saved positions, page and scroll modes, reading bars, printed page references, whole-book percentage jumps, generated contents from headings or chapter starts, publisher CSS with first-line and hanging indents and supported structural selectors, selectable native base stylesheet subsets, local fonts and illustrations, numbered and nested lists, supported tables with column widths and merged cells, footnote previews, bounded static SVG, ruby, authored soft hyphens, automatic US and British English hyphenation, inherited language, horizontal writing direction, and publisher line-breaking rules. Supported vertical chapters use right-to-left columns and the same text anchors. Fixed pages include publisher-defined spreads and the XHTML viewport directives. | More advanced stylesheet cases and exact CREngine stylesheet fidelity, additional dictionary languages and mixed writing modes within one chapter, and broader live typography checks |
| PDF | Fixed pages and continuous scroll, printed labels, page thumbnails, outlines and links, passwords, page, width and height fit, crop, contrast, search across wrapped text lines and local annotations. Visible embedded highlights import and annotated-copy export for intact unencrypted PDFs up to 16 MiB. | [Standard password revisions 2–6 and both roles](protected-pdf.md) compared with installed KOReader. Broader international input, certificate handlers, outline and annotation types, and sustained large-scan tests |
| Other offline reading | Fixed-layout EPUB, DjVu, TXT, HTML, single-document HTMLZ, Markdown, FB2, supported MOBI/PalmDOC and KF8, DOCX, CHM, RTF, TCR, CBZ, uncompressed USTAR CBT, selected CBR, JPEG, PNG, BMP, GIF first frame, portable anymaps, static SVG, selected WebP and TIFF, and compressed StarDict with synonyms | Wider format variants and larger live document samples |
| Scanned documents | Embedded and recognized text reflow across readable source pages. Original word-image reflow for scans with complete horizontal left-to-right OCR and a column view for clear two-column scans. English and Vietnamese recognized by the worker on desktop. The ARMHF package's English OCR worker produced word boxes on Oasis 3. | Broader layout and language cases, and sustained tests with real scans |
| Library and controls | Folder navigation with file types, sizes and support labels, whole-library search, favorites, reading status, collections, author and recent views, EPUB cover previews and series, [daily reading history and a calendar](reading-statistics.md), separately retained undated screen time, reusable text profiles, night pages, adjustable toolbar sizes, mirrored or disabled tap turns, touch lock, optional E Ink full refresh intervals, chapter-spanning page thumbnails, transient EPUB and fixed-page Skim previews with queued turns, origin restore, cancel and one-entry return history, location history and zoom for framed comic panels in single-page views | More configurable gestures, broader accessibility, physical Skim panel interaction and dialog preferences, outline editing for additional provider types and excluded sections, and irregular comic panels |
| Reading data | Backup and restore with preview, text/Markdown/HTML annotation exports, adjacent KOReader EPUB bookmarks and source-verified highlighted passages, KOReader JSON annotations by unique quotation, and sampled-fingerprint checks for fixed-page sidecar progress and supported reflowable EPUB CFIs and normalized CREngine DocFragment positions | Additional CREngine DOM versions, broader CFI and sidecar variants, and migration measurements with larger libraries |
| Kindle verification | The installed Reader 0.1.42 release opened, advanced and resumed the original EPUB, PDF, AZW3 and MOBI test books. The Oasis passed 1,171 automated checks. Five of six Oasis software captures matched the Mac frames, and one Library label differed by two pixels. [Original documents and package receipt](real-document-verification.md#reader-0142-release-on-the-oasis) | Foreground touch, actual E Ink refresh, and interactive sleep, USB and Launcher handoff still need acceptance. [Execution plan](execution-plan.md#current-host-and-device-integration-gate) · [Reading evidence](reading-control-evidence.md) |

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
| K01 · Publication structure | [EPUB 3 Contents fallbacks and nested landmark headings](epub-landmarks.md) follow the installed CREngine on five authored publications. Retest EPUB 2 NCX, EPUB 3 navigation, publisher CSS, embedded TTF/OTF, image placement, and links. Add samples for real books with nested and mixed content, notes in separate files, and a legacy XHTML header. Record remaining format limitations. | On Kindle, activate an inline footnote, read it, and return to the originating passage. Verify actual glyphs, publisher styles, and artwork with named font files and known text. |
| K02 · Positions | Extend the existing chapter and text position with a versioned publication identity and context for recovery. Distinguish displayed page number from the stored text location. | Save both occurrences of a repeated quote. Reopen after changing font size, margins and viewport, and resolve both to their original passages. Confirm a different book with identical chapter text keeps its own state. |
| K03–K04 · Device behavior and performance | Measure button and touch input through physical panel update. Retest rapid direction changes, both available portrait orientations, abnormal input files, power and USB transitions, and return to Launcher. | Keep separate software and physical timing results under the same refresh settings. Verify rapid presses each move the reading position, the USB graphic appears during export, and the session resumes at the same passage after eject. Record timing and memory on the multilingual, image-heavy and long-chapter samples. |
| K05 · Build and distribution | Lock the ARMHF inputs and exact licence obligations. Record source versions of the tools, fonts and assets used by the release. | Rebuild from a clean workspace, review dependency and artwork notices, and verify the normal Launcher entry on Oasis 3 without reinstalling Launcher. |

## P1 · Daily EPUB reader

Build the durable document model first. Navigation, search, bookmarks, notes, and saved progress all use that model. Keep the page visible when the reader taps near the upper center to open its top and bottom bars. Put the Library, quick bookmark and Exit Reader on the top bar. Put Contents, Bookmarks, Highlights, Find and Text settings on the bottom bar, with a separate compact Done control in the normal reading view. Physical and touch inputs invoke the same reading actions. [KOReader navigation and formatting guide](https://koreader.rocks/user_guide/)

| Feature and requirement | Implementation and dependencies | Acceptance on Kindle |
| --- | --- | --- |
| Library, current book and progress · K06 | Read files from supported library locations and Files. Index title, author, cover, series if present, and reading status. Track recent books, resumable state, and per-book settings in a versioned local store. Reconcile moves and replacements using stable identity and content revision. | Resume from Launcher and Reader, then rename or move a book and retain the saved state. Keep separate positions for books with identical names. Show a repair path when a book is missing or changed. |
| Page and scroll controls · K03, K07 | Implement page and continuous scroll modes, directional tap zones, swipes, physical page buttons, chapter transitions, previous location, and touch lock. Reuse the proven page refresh pacing. The Mac now verifies the separate Back gesture for recent views, jump history, parent folder and configurable exit. [Back navigation](back-navigation.md) | Compare forward and back behavior against KOReader at chapter and book boundaries. Preserve the active location when switching from page to scroll, and verify that touch lock keeps the chosen exit or unlock control reachable. Exercise the Back gesture and the physical page buttons independently. |
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
| Navigation depth and display controls · K07, K20 | Skim mode, book map, page browser/thumbnails, history, publisher page references and stable calculated references, custom contents and ignored sections. Skim's EPUB/PDF preview, origin return, cancel, commit history and Exit path pass four focused desktop tests. Offer reading direction, page refresh, and per-book layout profiles with reset. | Compare Skim's controls and dismissal with KOReader on Kindle. Move between a bookmarked passage, a thumbnail, a printed-page label, and an inline link without losing return history. Verify profile precedence, skipped sections, and reading controls on each target model. |
| Reading library and accessibility · K20 | Add collections, favorites, search/sort/filter, reading statuses, per-book statistics and configurable gestures. The Mac simulator now verifies calendar book spans and independent month and timeline boundaries. Extend accessibility with larger selections and alternate interactions. Evaluate read-aloud or media-overlay playback separately against available Kindle audio and documented reference behavior. [Calendar controls](reading-statistics.md#calendar-and-timeline-settings) | Validate library edits and search offline with thousands of files and multiple editions. Verify a timed session excludes sleep, the calendar time editor and month drill-down work in the foreground, a custom gesture avoids unwanted navigation, and any supported audio or read-aloud workflow has a model-specific record. |
| Annotation formats · K09, K13 | Extend local export to human-readable HTML, Markdown and Kindle Clippings-compatible text where verified. Add patterns or grayscale marks for different highlight modes and bulk export. | Confirm overlapping highlights remain distinct, notes and quotations round-trip, and incomplete local exports can be retried. [KOReader annotation export guide](https://koreader.rocks/user_guide/) · [W3C Web Annotation Data Model](https://www.w3.org/TR/annotation-model/) |
| KOReader import, metadata backup and file associations · K23–K24 | Import known export formats and sidecar snapshots as bounded data. Match publication and revision before mapping source pointers. Show resolvable, ambiguous and missing targets. Add Reader export/restore with duplicate protection and use the existing Files handler choice for shared formats. | Import known progress and highlights twice without duplicate records, produce a useful unresolved list for missing or edited books, verify source hashes, and restore Reader metadata on a fresh test installation. Confirm Reader, Viewer and KOReader all continue to open files through their chosen handlers. |

Online book acquisition, remote progress sync, service-based annotation export or sharing, and article-to-EPUB export are outside this goal. Contextual Wikipedia and translation requests launched from the reading view are in scope as reading actions; they require explicit confirmation and must preserve local reading state. Local annotation and reading-data export/import remain in scope.

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
| MOBI, PRC, DRM-free AZW and KF8 variants | Native legacy MOBI and PalmDOC. Reflowable KF8 accepts plain, PalmDOC and HUFF/CDIC text. Reader reconstructs chapter order, KindleGen markup and auxiliary flows, local links, stylesheets, JPEG, PNG, first-frame GIF and supported SVG artwork. Long or image-heavy chapters retain their original destinations across layout sections. Legacy MOBI preserves images, source-byte links, publisher paragraph alignment and inline font sizes. Reader’s Contents panel opens publisher navigation at verified subject headings and preserves nested entries. Exact destinations match all 194 publisher entries in the two original legacy MOBIs. Fourteen original AZW3s expose 664 publisher headings with verified KF8 fragment addresses, including eight later-fragment headings that use a zero offset. Readable hybrid legacy editions keep their original chapter paths and positions. A validated unencrypted KF8 section can take over if the primary compression or encoding is unsupported. The HUFF/CDIC decoder verifies all 58 text records of three independent public books against libmobi. Fixed-layout KF8 and links into arbitrary legacy text offsets remain open. | Metadata, text encoding, links, images and mixed layouts, location recovery, and clear messages for encrypted inputs. |
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

## CJK dictionary selection and Japanese forms

The desktop candidate enables Parley's supported complex-script segmentation models, adds bounded independently authored Japanese lookup forms and NFKC candidates, and checks the longest CJK source prefix through a cancellable background dictionary pass. Grapheme-safe source endpoints retain voicing marks, variation sequences and original anchors. Shared UI fonts now use supplied script fallbacks while preserving ASCII pixels. Authored regressions, real public Japanese passages and an unchanged owner EPUB cover the scoped behavior. Native acceptance, broader lexical classes, honorific/colloquial forms and the reference plugin's complete conversion/settings surface remain open. [Behavior, source comparison and limits](cjk-dictionary-selection.md)

## Global document defaults

Separate reflow and fixed-page templates now apply only to unseen content. Explicit save/reset/clear controls use the existing icon language. Older saved books retain their settings; reset keeps source position and annotations and retains fixed crop/pan within valid view bounds. Desktop regressions inject reflow/write failures, reopen both formats, preserve unavailable-font recovery and exercise four original EPUB/PDF publications. The optional versioned64KiB configuration shares validators with backup import. Native acceptance remains pending. [Behavior, source comparison and storage](document-defaults.md)
