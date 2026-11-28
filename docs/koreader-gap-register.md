# KOReader gap register

Written for: the Reader maintainers, who use it to choose and verify the next parity items.

Reference: KOReader v2026.07.1, commit `9192014d8bd82a91dc1012473be0f238dedfdb54` (`/tmp/koreader-ref-src` in the review
session; re-clone to reproduce). Reader: `feature/reader-parity-workspace`. Each row gives the KOReader evidence (file:line),
the Reader evidence, a reading impact (high, medium, low), and the phase of the approved parity plan that owns it (the section headings give the phase numbers).

Limits: CREngine, mupdf, djvulibre and k2pdfopt are submodules and were not in the reference clone. Engine-level behaviour
(base CSS, hyphenation data, line breaking, `-cr-hint`) is not verified here. "Missing" means no handler or setting was found
by search; it is not a runtime test. The control matrix (`koreader-sdl-reference-controls.json`) is partly stale and is not
used as evidence.

Status values: **open** (no Reader counterpart), **partial** (some counterpart, behaviour differs), **verify** (the audit could not
decide; check before implementing), **intentional** (kept different on purpose, with the reason).

## Intentional divergences (recorded decisions)

| ID | Item | Reason |
| --- | --- | --- |
| I-1 | Footer default items: Reader enables current page, book progress and chapter pages-left; KOReader also enables time, battery, percentage and time-to-read (`readerfooter.lua` default_settings). | Owner decision (plan, owner decisions 3). |
| I-2 | Reflow font hinting default: Reader `Off` (`src/lib.rs:554`); KOReader `auto` (`creoptions.lua:634`). | Owner decision (plan, owner decisions 3). |
| I-3 | Reader front light and warmth controls and gesture bindings. | Quick Display follows the shared system contract (plan Phase 2). |

## Typography and fonts (plan 1.1, 1.5, 1.6)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| F-1 | User-chosen default, fallback and monospace fonts | `frontend/apps/reader/modules/readerfont.lua:242-275` | fallback font done: one Reader-wide fallback, after each book's font, chosen in the reading font chooser and saved to `fallback-font.json` (`src/app/fallback_font.rs`, tests `fallback_font`). Not done: a Reader-wide default font (★) (needs a rule for a book's "Device fonts" choice, which stores `None`) and a monospace font (needs the family mapping in F-2) | medium | partial |
| F-2 | Font-family mapping (serif, sans, mono, cursive, fantasy, emoji, fangsong, math) and "ignore publisher font names" | `readerfont.lua:324-565` | not found | medium | open |
| F-3 | Additional fallback fonts, fallback size adjust, monospace scaling % | `readerfont.lua:608-670` | not found | low-medium | open |
| F-4 | Font menu preferences (sort by recent, show names in own face), font test document | `readerfont.lua:565-700` | not found | low | open |
| F-5 | `.ttc` faces: one row per file, first face only | KOReader enumerates faces | `docs/reading-font-collections.md` | low | partial |
| F-6 | Hinting: Skrifa native versus FreeType; no cross-engine raster check | `creoptions.lua:630-637` | `docs/koreader-sdl-reference-controls.json` B-font_hinting | low | verify |
| F-7 | Typography language rows (38 selectable languages without exact CREngine dictionaries) | `readertypography.lua`, `creoptions.lua` | `docs/koreader-sdl-reference-controls.json` typography rows | medium | partial |
| F-8 | Custom hyphenation rules: four written characters, horizontal text only, 4,096 entries | `readeruserhyph.lua`; `readerhighlight.lua:221-232` | Closed. Four graphemes (the segment count includes the end boundary, so `< 5` is four), horizontal text only (`src/app/user_hyphenation.rs:238`), 4,096 entries (`src/user_hyphenation.rs`). Covered by `user_hyphenation` tests, including `eligible("word")` and `!eligible("cat")`. | low | closed |
| F-9 | Word spacing and justification: Rust float geometry versus CREngine integer widths | CREngine (not in reference) | `docs/koreader-sdl-reference-controls.json` B-word_spacing | medium | verify |

## Style tweaks and base stylesheets (plan 1.2, 1.3, 1.4)

Reader ships **91** preset entries in `PRESETS` (`src/user_styles.rs:404-1081`). `docs/style-tweaks.md` and the execution docs
still say 18; treat the code as authoritative and correct the docs (plan 0.6).

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| S-1 | Alternative-ToC hints `alt_toc_ignore_h*`, `alt_toc_level_example` (`-cr-hint: toc-ignore` / `toc-level*`) | `css_tweaks.lua:732-795` | not found; `docs/contents-list-settings.md` lists the `-cr-hint` workflow as open; `src/generated_contents.rs:97,179` treats every h1–h6 as an entry | medium | open |
| S-2 | `docfragment_page-break-before_avoid` (chapter end blank page) | `css_tweaks.lua:107-111` | not found; `docs/style-tweaks.md` says implicit chapter boundaries remain | medium | open |
| S-3 | `titles_page-break-before_avoid` (chapter start blank page) | `css_tweaks.lua:100-106` | not found | low | verify (whether the Traditional base sheet already forces h1–h3 breaks) |
| S-4 | `publisher_page-breaks_avoid` combined with heading or chapter breaks | `css_tweaks.lua:113-126` | `publisher-breaks-none` clears all break-before/after (`src/user_styles.rs:699`) | low | partial |
| S-5 | `font_no_presentational_hints` (`<font>` and body attributes) | `css_tweaks.lua:422-430` | only `table_no_presentational_hints` (`src/user_styles.rs:892`) | low | verify |
| S-6 | `ruby_inline` | `css_tweaks.lua:413-418` | the other three ruby presets exist (`src/user_styles.rs:1019-1041`) | low | open |
| S-7 | Default-on in-page footnotes (EPUB and FB2) for new installations; smaller notes (80 percent, legacy) | `css_tweaks.lua:25-29` (`DEFAULT_GLOBAL_STYLE_TWEAKS`) | Deferred on purpose. Seeding the two presets in `Data::default()` made 18 existing tests fail, because most EPUB notes would change on first open. The default waits for S-15 (facing pages, continuous scroll, nested notes). The smaller size is already Reader's default (Legacy). | medium | deferred |
| S-8 | Global user tweak files in `koreader/styletweaks/*.css` | `readerstyletweak.lua:545-590` | per-book import only (`docs/style-tweaks.md`) | low | open |
| S-9 | Tweak help: long-press shows CSS and description | `readerstyletweak.lua:40-170` | one-line descriptions for some presets (`src/app/style_tweaks.rs:731-770`) | low | open |
| S-10 | Bindable "toggle tweak" action | `readerstyletweak.lua:129-140, 249-260` | not in `Binding` (`src/app/gestures.rs`) | low | open |
| S-11 | User CSS accepts any CSS | `readerstyletweak.lua:1165-1215` | strict validator rejects the whole sheet on unsupported syntax (`docs/style-tweaks.md` "Cascade and limits") | medium | partial |
| S-12 | Base stylesheets: Obsolete and user-added sheets | `readertypeset.lua:160-262` | native approximations only (`docs/base-stylesheets.md`) | low | partial |
| S-13 | "Auto-detect TXT files layout" | `readertypeset.lua:~236-260` (`txt_preformatted`) | no toggle; paragraph detection differs (`docs/large-text-reading.md`) | low-medium | partial |
| S-14 | Render modes (legacy/flat/book/web) paused in several modes | `creoptions.lua:317-334`; `readertypeset.lua:378-447` | composed-policy pauses (`docs/render-modes.md`) | medium | partial |
| S-15 | In-page footnote fallbacks: facing pages, continuous scroll, cross-spine or nested notes | `css_tweaks.lua:920+` | `docs/in-page-footnotes.md` | medium | partial |

## Layout, navigation and reflow (plan 1.6, 1.7, 1.8, 1.9)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| N-1 | Followed-link marker (black bar about 0.5 s, removed after 1 s) | `readerrolling.lua:~770-850` | not found | low-medium | open |
| N-2 | Link to another local document: the file opens after a confirmation, with `?pos=` ignored | `readerlink.lua:771-815` (`openFileFromLink`) | `ReflowLink::LocalDocument` resolves `file:` URLs, absolute and book-relative paths that exist and are book formats (`src/app/link_taps.rs`); the link popup offers Read or Close (`src/app/external_link.rs`); tests `local_document_tests`, `local_document_popup_tests`. Not done: `?pos=` (KOReader xpointers and page numbers are not mapped to Reader positions); PDF link actions that point at other files | low-medium | partial |
| N-3 | Character-count ("stable") pages for any reflow document | `readerpagemap.lua:33-80, 448-548` | EPUB only (`src/app/character_page_settings.rs:695,781,798`) | low-medium | partial |
| N-4 | Full refresh on chapter boundaries and on pages with images | `readertoc.lua:100-122`; `readerview.lua:~218-221` | `PageRefresh` offers only Default, every 1, 5 or 10 pages (`src/reading_controls.rs:1479`) | low | open |
| N-5 | Hold a Contents row to show the full title | `readertoc.lua:~833-850` | no hold handling (`src/app/contents_view.rs`, `contents_tree.rs`) | low | open |
| N-6 | Footnote popup highlights its source link while open | `readerlink.lua:1160-1185` | not found | low | open |
| N-7 | "Partial renderings" (current chapter first, then background) | `readerrolling.lua:~1130-1260` | chapter-first or whole-book-first for bounded text EPUBs (`docs/koreader-sdl-reference-controls.json` T3-7.6) | low | partial |
| N-8 | Alternative-ToC restore and its chapter lengths for unsupported layouts | `readertoc.lua`, `readerpagemap.lua` | `docs/contents-list-settings.md`; `docs/reflow-book-map-marks.md` | low | partial |
| N-9 | Two-column odd/even first-page rule (`odd_or_even_first_page`) and two-page `_gotoPage` rules | `readerview.lua`, `readerpaging.lua` | not compared | low | verify |
| N-10 | Page-position memory per page (`page_positions`) | `readerpaging.lua` | no clear counterpart | low | verify |
| N-11 | Margin editors: paired L/R spinner, hold steps of 5 logical units, dual top/bottom editor | `creoptions.lua` editors | `docs/reflow-page-margins.md`; `src/app/numeric_editor.rs` | low | partial |

## Phase 1.4 open items (from the Phase 0 pass)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| R-1 | Landscape scan reflow return and scale round trip | none (Reader-internal) | Closed. Two test causes, no production drift: the round trip stopped at 100 instead of the stock 105, and the return cleared the transient "End of document" footer notice set by the staging turn. `scan_text_viewport_stages_rolls_back_and_uses_landscape_controls` now compares the page exactly outside the footer band. | medium | closed |

## Scan and OCR reflow (plan 1.10)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| C-1 | Reflow speckle size (`defect_size`), indentation detection (`detect_indent`), render quality, background cleanup | `koptoptions.lua:~566, 606, 668, 682` | only mentioned in `docs/koreader-source-inventory.md:105` | low-medium | open |
| C-2 | Saturation and hardware dithering | `koptoptions.lua:~511, 580` | deliberately deferred (grayscale, model-dependent; `docs/koreader-source-inventory.md:106`) | low | intentional |
| C-3 | Auto-straighten estimator | `koptoptions.lua` | Reader's own estimator (`docs/scan-controls.md:133-147`) | low | partial |

## Interaction, gestures and dispatcher (plan 3.1, 3.2)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| G-1 | Default bindings: tap top-right toggles bookmark, tap top-left toggles page flipping, tap lower-left toggles front light | `plugins/gestures.koplugin/defaults.lua` (`gesture_reader`) | `src/app/gestures.rs:544-575, 877-899` leave these unbound | high | partial: tap top-right bookmark default not bound. The top menu strip (1/8 height, `src/app/reading_bars.rs:1162`) takes the corner first, and KOReader's precedence between the menu zone and the corner gesture is not verified from source (needs the SDL reference) |
| G-2 | Dog-ear on bookmarked pages | `frontend/apps/reader/modules/readerdogear.lua`, `readerview.lua:246-248` | no page paint; bookmark state used only for the menu label (`src/app/top_config.rs:89`) and skim (`src/app/skim.rs:638`); `draw_page` (`src/session.rs:3004`) takes no bookmark state | high | done (desktop): mark drawn into the page raster by `paint_wedge` (`src/app/dogear.rs`, called from `draw_page_picture`, `src/session.rs`); test `bookmarked_page_shows_the_corner_mark_and_clears_when_unmarked`; EPUB and PDF page paths share it; device check open |
| G-3 | Multi-action sequences for every gesture (today: multiswipes only) | `plugins/gestures.koplugin/main.lua:197-213, 340-355` | `docs/reading-multiswipes.md`; single `Binding` elsewhere (`src/app/gestures.rs:408`) | medium | partial |
| G-4 | Bindable dispatcher actions with no Reader counterpart: zoom modes, toggle reflow, bookmark first/last/next/previous, add/clear location history, find-all, toggle links, highlight action and style cycling, status-bar and footer presets, tweak and typography toggles, read-timer actions, Wikipedia, translate, vocabulary, export | `frontend/dispatcher.lua:174-254` | `Binding` enum has 36 entries (`src/app/gestures.rs:269-350`) | medium (high for zoom, toggle reflow, bookmark navigation) | open |
| G-5 | Four double-tap corner gestures | `gestures.koplugin/main.lua:61-68` | only `DoubleTapLeft` and `DoubleTapRight` (`src/app/gestures.rs:47-48`) | low | open |
| G-6 | Seven gesture intervals (selection rate, tap, keyboard tap, double tap, two-finger tap, long press, swipe) | `gestures.koplugin/main.lua:651-840` | double-tap interval and two-finger duration only (`docs/long-press-behavior.md`) | low | partial |
| G-7 | Spread and pinch: zoom mode in flipping mode, otherwise font size | `readerpaging.lua`, `readerzooming.lua` | font size on reflow, ±10 % fixed zoom (`src/app/gestures.rs:966-995`); `docs/gesture-reading-actions.md` contradicts this | low | partial |
| G-8 | Read-timer dispatcher actions and profile trigger on read-timer expiry | `readtimer.koplugin/main.lua:31-41, 148-154` | read timer exists (`docs/read-timer.md`), actions not bindable | low | open |
| G-9 | Page flipping: preview overlay, proportional swipe jumps (`flip_steps`), swipe to return | `readerpaging.lua:38, 208-228, 305-322` | no flipping mode; skim is a partial substitute (`docs/skim-navigation.md`) | medium | open |
| G-10 | Free zoom: double-tap regional zoom, content width and height zoom | `readerzooming.lua:274-311, 597-613` | not found; spread and pinch step ±10 % (`src/app/gestures.rs:969-979`) | low-medium | open |
| G-11 | Gesture "always active" while touch input is off | `gestures.koplugin/main.lua:197-213, 340-355` | global swipes-always-active switch only | low | partial |
| G-12 | Multi-touch, double-tap and multiswipe candidates | `frontend/device/gesturedetector.lua` | unverified candidates in `docs/multi-touch-gestures.md`; multi-touch needs the unpublished tools API (see the build section below) | medium | partial |

## Go to, location and pages (plan 3.3)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| P-1 | Pin current page, go to pinned page, remove pinned page on hold | `readergoto.lua:55-95, 242-311`; `dispatcher.lua:180-181` | not found (`docs/go-to-page.md`) | medium | open |
| P-2 | Random page, with a no-repeat pool | `readergoto.lua:218-240`; `dispatcher.lua:176` | not found | low | open |
| P-3 | First page and last page as user actions | `readergoto.lua:200-216`; `dispatcher.lua:174-175` | only as an end-of-book action (`src/app/end_of_book.rs:316`) | low | partial |
| P-4 | Bookmark browsing mode (flip through bookmarks, return on exit) | `readerpaging.lua:229-245, 294-302`; `readerbookmark.lua:93` | only Skim's Marks mode for fixed pages (`docs/skim-navigation.md`, unvalidated) | low | partial |
| P-5 | Book Map options: alternative theme, current or initial view, grid and flat views, 10-page markers, overview on long press, tap-to-page-browser | `bookmapwidget.lua:1340-1480`; `readerthumbnail.lua:71-100` | fixed Book Map has its own slot, help and history cues (`src/app/fixed_book_map.rs`) | low | partial |
| P-6 | Page browser: preload next and previous thumbnails | `pagebrowserwidget.lua:992` | not found | low | open |
| P-7 | Skim: ±10 pages and hold chapter to first or last | `skimtowidget.lua:100-290` | "ten" referenced in `src/app/skim.rs`; not traced | low | verify |
| P-8 | Binding label "Page browser" for `Binding::Skim` routes to Skim, not the page browser | `frontend/dispatcher.lua` | not traced | low | verify |

## Highlights and annotations (plan 3.4)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| H-1 | Adjust highlight boundaries by word or character, including shrink | `readerhighlight.lua:1029-1260, 1394-1445` | Extend only unions; no shrink, no character step (`src/session.rs:~2079`; `src/app/extend_highlight.rs`) | medium | partial |
| H-2 | Edit highlighted text, reset text, filter by edited text | `readerbookmark.lua:957-961, 1301-1315, 1466-1490`; `readerhighlight.lua:1366` | not found; `text_edited` only parsed on import (`src/koreader_sidecar/annotations.rs:93`) | medium | open |
| H-3 | "Follow link" button in the selection popup | `readerhighlight.lua:236-250` | not found (`src/app/selection_popup.rs`) | low-medium | open |
| H-4 | Panel zoom for any paged document: toggle, fallback to text selection, `panel_zoom_toggle` action | `readerhighlight.lua:886-915, 1605-1635` | comic archives only, always on (`src/app.rs:7667-7684`, `src/comic_panel.rs`) | low-medium | partial |
| H-5 | Write highlights into the original PDF (on/off, reminder, write all, delete all) | `readerhighlight.lua:600-735, 2253` | separate annotated copy only (`docs/highlight-drawing-styles.md`, `src/pdf_annotations.rs`) | low | intentional (decide later) |
| H-6 | Per-style highlight options audit | `readerhighlight.lua` | `docs/highlight-drawing-styles.md` | low | verify |

## Lookup, search, Wikipedia and translation (plan 4.1, 4.2)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| L-1 | Selection cleanup before lookup (NBSP, edge punctuation, `’` to `'`, `'s`, French `l'`, `qu'`) | `readerdictionary.lua:1090-1135` | trims whitespace and soft hyphens only (`src/dictionary.rs:~295-300`, `src/app/dictionary_panel.rs:~865`) | medium | partial |
| L-2 | Dictionary images, CSS and `res/` in HTML dictionaries | `dictquicklookup.lua:154, 1146-1200, 1296, 1465-1470` | StarDict types m/t/y/h/g/x/k only (`src/dictionary.rs:1220`); resource references inert | medium | open |
| L-3 | Dictionary lookup history is unbounded | `readerdictionary.lua:1425-1436` | capped at 64 (`src/app/lookup_history.rs:8,125`) | low | partial |
| L-4 | Japanese: full 643-rule deinflection table and dictionary-driven segmentation | `plugins/japanese.koplugin/README.md`, `deinflector.lua` | independently authored subset (`src/dictionary/word_forms/japanese.rs:3`; `docs/dictionary-word-forms.md:61`) | medium for Japanese readers | partial |
| L-5 | Multiple Wikipedia languages, auto-guess from book and UI language, next-language switching, per-book language | `readerwikipedia.lua:324-365, 510-545` | one `wikipedia` field, default `en` (`src/app/online_panel.rs:90-115`) | medium | partial |
| L-6 | Language links on a full Wikipedia article | `readerwikipedia.lua:~478` | not found (`src/online_lookup.rs`) | low | open |
| L-7 | Tapping a wikipedia.org link opens a Wikipedia lookup | `readerlink.lua:29, 186-193` | not found (`src/app/external_link.rs`, `src/app/link_taps.rs`) | low | open |
| L-8 | Wikipedia history: unbounded, lookup versus full article, language and timestamp | `readerwikipedia.lua:83-126` | 8 online entries (`src/app/lookup_history.rs:122`) | low | partial |
| L-9 | Translator: "translate from book language" and auto-detect; named-language picker | `frontend/ui/translator.lua:366-440` | typed code with `auto` (`src/app/online_panel.rs:104-113`) | low-medium | partial |
| L-10 | Search: all types, regex, Find all, result handling | `readersearch.lua` | implemented (`src/search.rs`, `src/app/nearby_search.rs`) | — | implemented |

## Status, statistics, vocabulary and book data (plan 4.3 to 4.6)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| B-1 | Star rating (1 to 5) and free-text review (`summary.rating`, `summary.note`) | `readerstatus.lua:22`; `bookstatuswidget.lua:220, 444, 566` | Book Details has the four-state status, favourites and reading time only (`src/app.rs:4872-4930`); never reads or writes rating or note | medium | open |
| B-2 | "Current statistics" page: session, today, time left, finish date, averages, pages read %, highlight and note counts | `statistics.koplugin/main.lua:1508-1650` | recorded time, screens, capped time, 7-day averages (`src/app/reading_history/progress.rs:87-88`); catalog marks `T4-4.5` as workflow | medium | partial |
| B-3 | Page-duration editor accepts arithmetic and fractions | `statistics.koplugin/main.lua:1091` | whole seconds only (`T4-4.2.1`) | low | partial |
| B-4 | Vocabulary: clean all, quick delete, add virtual book, word-list go to page | `vocabbuilder.koplugin/main.lua:85, 108, 197, 1179, 1473` | one-word removal only (`src/app/vocabulary.rs`) | low-medium | open |
| B-5 | Import of `vocabulary_builder.sqlite3` | `plugins/vocabbuilder.koplugin/db.lua` | not found; `docs/vocabulary-review.md` says SQLite import is open | medium for KOReader migrants | open |
| B-6 | Low-battery and high-memory alerts | `readerdevicestatus.lua:20-120` | footer battery item only (`docs/battery-footer.md`); may belong to Launcher | low | verify |
| B-7 | Sidecar import is narrow: `summary`, `doc_props`, per-book settings, handmade TOC and flows, dictionary and Wikipedia keys, legacy `bookmarks`/`highlight` ignored | `docsettings.lua`, `readerui.lua:493-518` | progress, `annotations`, `custom_props` only (`src/koreader_sidecar.rs:256-263`) | medium for migration | partial |
| B-8 | Edit series, series index, language, keywords, description; set a custom cover | `filemanagerbookinfo.lua:36-50, 298-420`; `docsettings.lua:519-590` | title and author only, reflowable books (`docs/koreader-sdl-reference-controls.json` T7-7) | medium | partial |
| B-9 | Per-directory default document settings for new books | `docsettingtweak.koplugin/main.lua:57-95` | global per-format defaults only (`docs/document-defaults.md`) | low-medium | open |
| B-10 | Profiles: arbitrary action editor, QuickMenu, auto-run on start, wake, rotation, folder show | `profiles.koplugin/main.lua:131-200, 698-705` | snapshots applied on open and close (`docs/reading-profiles.md:22`) | low-medium | partial |
| B-11 | Cover mosaic and detailed list with progress, pages read and status | `coverbrowser.koplugin/main.lua:143-440` | text list with progress sort (`src/app/library_view.rs:636-642`) | low-medium | open |
| B-12 | Move-to-archive: copy, go to archive folder, go to last source folder | `movetoarchive.koplugin/main.lua:73-92` | single-file move (`src/app/book_archive.rs`) | low | partial |
| B-13 | Duration format Classic, Modern, Letters | `common_settings_menu_table.lua:91-140` | fixed format in footer and statistics; matrix marks it Launcher-owned (`T3-9.3.2`) | low | verify |

## Formats (plan phase 5)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| X-1 | XPS/OXPS, PPTX, XLSX, generic CFB | `pdfdocument.lua:374-395` | not found; Files offers XPS to KOReader (`docs/reading-parity-plan.md:582`) | low-medium | open |
| X-2 | JPEG XR (`jxr`, `hdp`, `wdp`) | `pdfdocument.lua:400-417` | "remain to evaluate" (`docs/reading-parity-plan.md:576`) | low | open |
| X-3 | General `.xml`, mixed-content ZIP, PDF or EPUB nested in ZIP | `credocument.lua:1625-1626`; `readerui.lua:676-710` | XML shown as text; ZIP must hold one reflowable document | low | partial |
| X-4 | PDB and PRC beyond a limited PML subset | `credocument.lua:1614-1616` | `docs/reading-parity-plan.md` | low | partial |
| X-5 | "Open with" alternate engine or forced provider | `readerui.lua:616-710` | one engine per format | low | open |
| X-6 | DjVu render modes (colour, black, mask, foreground, background) | `reader_menu_order.lua:~63` | not found in `src/djvu.rs` | low | open |

## Library and file manager

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| M-1 | Archive viewer and archive extraction | `archiveviewer.koplugin` | not audited (Files app) | low | verify |
| M-2 | Exporter filename tokens and Markdown/HTML templates | `exporter.koplugin` | formats and filters confirmed; templates not diffed | low | verify |

## Build and verification state (plan Phase 0)

- The reader's `Cargo.toml` pins the kit to tools revision `86e593e`. That revision has `TouchPan`, the viewport runtime and held-touch
  events, but no `MultiTouch` type and no `App::multi_touch` method. The local tools checkout is a different line (its `HEAD`
  `9f7e331` does not contain `86e593e`) with 41 uncommitted files.
- Reader code for multi-touch was gated behind the default-off Cargo feature `multi-touch`. A local mirror of the type keeps
  its gesture code and tests compiled in the default build. Enabling the feature requires a tools revision that provides the API.
- Default `cargo check --locked`: passes.
