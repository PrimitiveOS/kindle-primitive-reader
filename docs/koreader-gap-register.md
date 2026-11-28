# KOReader gap register

Written for: the Reader maintainers, who use it to choose and verify the next parity items.

Reference: KOReader v2026.07.1, commit `9192014d8bd82a91dc1012473be0f238dedfdb54` (`/tmp/koreader-ref-src` in the review
session; re-clone to reproduce). Reader: `feature/reader-parity-workspace`. Each row gives the KOReader evidence (file:line),
the Reader evidence, a reading impact (high, medium, low), and the phase of the approved parity plan that owns it (the section headings give the phase numbers).

Sources: the pinned CREngine checkout (`b32a88ff`, the revision the KOReader docs cite) is in `build/parity-review/font-render-reference/` with `crengine/` (engine) and `cr3gui/data/` (`epub.css`, `html5.css`, `txt.css`). The KOReader Lua reference is in `build/koreader-reference-v2026.07.1/`, and the Obsolete CSS files are in `build/koreader-macos-reference/runtime/koreader/data/`. Earlier entries that say CREngine is absent were written before this was found; rows marked "CREngine not in reference" need re-checking against these sources.
(base CSS, hyphenation data, line breaking, `-cr-hint`) is not verified here. "Missing" means no handler or setting was found
by search; it is not a runtime test. The control matrix (`koreader-sdl-reference-controls.json`) is partly stale and is not
used as evidence.

Status values: **open** (no Reader counterpart), **partial** (some counterpart, behaviour differs), **verify** (the audit could not
decide; check before implementing), **intentional** (kept different on purpose, with the reason).

## Intentional divergences (recorded decisions)

| ID | Item | Reason |
| --- | --- | --- |
| I-1 | Footer default items: Reader enables current page, book progress and chapter pages-left; KOReader also enables time, battery, percentage and time-to-read (`readerfooter.lua` default_settings). | Intentional divergence, per owner decision 3 in the plan: Reader keeps its footer defaults. |
| I-2 | Reflow font hinting default: Reader `Off` (`src/lib.rs:554`); KOReader `auto` (`creoptions.lua:634`). | Intentional divergence, per owner decision 3 in the plan: Reader keeps `Off`. See F-6. |
| I-3 | Reader front light and warmth controls and gesture bindings. | Phase 2 (Quick Display follows the shared system contract). Not part of Phase 1. |

## Typography and fonts (plan 1.1, 1.5, 1.6)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| F-1 | User-chosen default, fallback and monospace fonts | `frontend/apps/reader/modules/readerfont.lua:242-275` | Done. A Reader-wide default font (used by books with no font choice; "Device fonts" now means this default, or the stock fonts when none is set), a fallback font (`fallback-font.json`), and a monospace font, all chosen in the reading font chooser and saved to `font-defaults.json` (`src/app/font_defaults.rs`, `src/app/fallback_font.rs`). Tests: `font_defaults_tests`, `fallback_font`. Not done: fallback size adjust and monospace scaling (F-3). | medium | closed |
| F-2 | Font-family mapping (serif, sans, mono, cursive, fantasy, emoji, fangsong, math) and "ignore publisher font names" | `readerfont.lua:324-565` | Mapping done: each family has a chosen font, given to the engine as a generic-family role (`FontRoles` in `src/lib.rs`, `font_role_tests`). Serif and sans-serif put the chosen font first and keep the book fonts after it. Not done: "ignore publisher font names" (publisher CSS still chooses families by name). | medium | partial |
| F-3 | Additional fallback fonts, fallback size adjust, monospace scaling % | `readerfont.lua:608-670` | not found Reason: Fallback size and monospace scaling need the layout to scale individual font runs, which the engine does not do yet. | low-medium | deferred |
| F-4 | Font menu preferences: sort by recent, names shown in their own face, font test document | `readerfont.lua:565-700` | Done for sort by recent: the chooser lists the fonts you chose most recently first (`ordered`, `recent`, `src/app/font_defaults.rs`; test `recent_fonts_lead_the_chooser_in_recency_order`). Names in their own face are the existing specimen previews in the chooser (`src/app/typeface.rs`). Not done: the font test document. | low | partial |
| F-5 | `.ttc` faces: one row per file, first face only | KOReader enumerates faces | `docs/reading-font-collections.md` | low | partial |
| F-6 | Hinting: Skrifa native versus FreeType; no cross-engine raster check | `creoptions.lua:630-637` (default 2 = auto; options off, native, auto) | Intentional divergence, per owner decision 3 in the plan: Reader keeps `Off`, and KOReader's default is `auto`. Reader does not use the FreeType autohinter. The raster comparison is recorded as the reason, not as a gap to close. | low | done |
| F-7 | Typography language rows (38 selectable languages without exact CREngine dictionaries) | `readertypography.lua`, `creoptions.lua` | `docs/koreader-sdl-reference-controls.json` typography rows Reason: Typography language rows need dictionary and hyphenation data per language, which is a data task. | medium | deferred |
| F-8 | Custom hyphenation rules: four written characters, horizontal text only, 4,096 entries | `readeruserhyph.lua`; `readerhighlight.lua:221-232` | Closed. Four graphemes (the segment count includes the end boundary, so `< 5` is four), horizontal text only (`src/app/user_hyphenation.rs:238`), 4,096 entries (`src/user_hyphenation.rs`). Covered by `user_hyphenation` tests, including `eligible("word")` and `!eligible("cat")`. | low | closed |
| F-9 | Word spacing and justification: Rust float geometry versus CREngine integer widths | pinned `crengine/src/lvtextfm.cpp` (space width scaled with integer `char_width * percent / 100`, around line 2203) | Deferred to item 1.6 (line fitting). The pinned engine rounds space and word widths in integer math; Reader uses float geometry. Matching it means a rounding rule and an oracle, which belong to 1.6. | medium | deferred |

## Style tweaks and base stylesheets (plan 1.2, 1.3, 1.4)

Reader ships **91** preset entries in `PRESETS` (`src/user_styles.rs:404-1081`). `docs/style-tweaks.md` and the execution docs
still say 18; treat the code as authoritative and correct the docs (plan 0.6).

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| S-1 | Alternative-ToC hints `alt_toc_ignore_h*`, `alt_toc_level_example` (`-cr-hint: toc-ignore` / `toc-level*`) | `css_tweaks.lua:732-795` | Done. `-cr-hint: toc-ignore` drops a heading from the alternative contents, and `toc-level1` to `toc-level9` sets its depth. The hint is computed per element (`CrTocHint`, `src/book_style.rs`) and read by the alternative contents scan (`src/generated_contents.rs`, `toc_hints`). Test: `alternative_headings_follow_the_toc_hints`. | low | closed |
| S-2 | `docfragment_page-break-before_avoid` (chapter end blank page) | `css_tweaks.lua:107-111`; pinned `epub.css` has no DocFragment rule | Decided: not added. The preset only switches off the DocFragment break that crengine puts at each chapter start. Reader has no DocFragment element and starts chapters on a new page, so there is no preset to switch off. This is not checked against a KOReader run; it rests on Reader's chapter model. | medium | decided |
| S-3 | `titles_page-break-before_avoid` (chapter start blank page) | `css_tweaks.lua:100-106` | Added as `titles_page-break-before_avoid` (`src/user_styles.rs`, CSS `h1, h2, h3 {page-break-before:auto !important;}`, priority 2). Test `every_published_preset_changes_its_supported_computed_property_through_user_origin` shows it clears an author forced break. Checked: Reader's EPUB base sheets do not force h1–h3 breaks (the only forced h1/h2 break is in the FictionBook sheet), so the preset matters only when publisher or user CSS adds one. Not compared on the KOReader engine (CREngine source not in checkout). | low | done |
| S-4 | `publisher_page-breaks_avoid` combined with heading or chapter breaks | `css_tweaks.lua:113-126`; pinned `epub.css` (heading breaks only in legacy html-document mode) | Done. `publisher-breaks-none` now also sets `h1`–`h6 {break-after:avoid}`, which KOReader's tweak keeps, so headings stay with the text after them (test `ignoring_publisher_breaks_keeps_headings_with_the_text_after_them`). The DocFragment and legacy h2/h3 restores in KOReader do not apply to EPUB documents in Reader. | low | done |
| S-5 | `font_no_presentational_hints` (`<font>` and body attributes) | `css_tweaks.lua:422-430`; pinned CREngine `cr3gui/data/html5.css` (`font[color]`, `font[face]`, `font[size]`, `body[bgcolor]`, `body[text]`) and `src/lvstsheet.cpp` (apply functions, legacy sizes 1-7) | Done. Reader maps these hints by default, below author and user rules (`src/book_style/font_hints.rs`; test `font_and_body_presentation_hints_apply_by_default_and_a_tweak_turns_them_off`; size mapping test `legacy_sizes_follow_the_pinned_named_scale_and_clamp`). The toggle preset `font_no_presentational_hints` switches them off, as KOReader's tweak does. `body[background]` (an image) is not mapped. Not compared on the KOReader engine; the default is taken from the pinned source. | low | done |
| S-6 | `ruby_inline` | `css_tweaks.lua:413-418` | the other three ruby presets exist (`src/user_styles.rs:1019-1041`) Reason: Needs a parse-time option threaded through every chapter source and its revision key, so cached layouts are not stale. Eight or more call sites. | low | deferred |
| S-7 | Default-on in-page footnotes (EPUB and FB2) for new installations; smaller notes (80 percent, legacy) | `css_tweaks.lua:25-29` (`DEFAULT_GLOBAL_STYLE_TWEAKS`) | Deferred on purpose. Seeding the two presets in `Data::default()` made 18 existing tests fail, because most EPUB notes would change on first open. The default waits for S-15 (facing pages, continuous scroll, nested notes). The smaller size is already Reader's default (Legacy). | medium | deferred |
| S-8 | Global user tweak files in `koreader/styletweaks/*.css` | `readerstyletweak.lua:545-590` | per-book import only (`docs/style-tweaks.md`) Reason: Needs the tweak presets to accept file-based entries with per-book toggles and a global default. The preset table is static today. | low | deferred |
| S-9 | Tweak help: a long press shows the CSS and the description | `readerstyletweak.lua:40-170` | Done. A long press on a preset row shows its title, its description where Reader has one (`preset_description`, `src/app/style_tweaks.rs`), and its CSS. Presets with a special effect show "Applied as layout rules". Tests: `tweak_help_*`, `long_press_on_a_tweak_row_opens_its_help_until_a_tap`. | low | closed |
| S-10 | Bindable "toggle tweak" action | `readerstyletweak.lua:129-140, 249-260` | Done. Every style tweak can be bound to a gesture: the binding picker lists the tweaks after the fixed actions, and the binding toggles that tweak for the book (`Binding::StyleTweak`, `TweakId` in `src/app/gestures.rs`; test `a_style_tweak_binding_round_trips_and_refuses_an_unknown_preset`). | low | closed |
| S-11 | User CSS accepts any CSS | `readerstyletweak.lua:1165-1215` | Done for the layout: a sheet applies every rule it can use and skips the rest (`parse_lenient`, `src/book_style/user_css.rs`; test `a_user_sheet_keeps_its_usable_rules_when_one_rule_is_unsupported`). Kept on purpose: the editor's strict check still rejects an unsupported rule so the author sees it; whole-sheet limits still fail. | medium | closed |
| S-12 | Base stylesheets: Obsolete and user-added sheets | `readertypeset.lua:160-262` | Done for user-added sheets: a book can choose a `.css` file from Style tweaks › Files (`c8ac41e7`, `d660f792`). Obsolete sheets: decided not applicable. Reader never reads KOReader's saved `css` setting (`src/koreader_sidecar*` has no reference to it), so a Reader book cannot select an Obsolete sheet, and KOReader itself marks them "don't use". | low | done |
| S-13 | "Auto-detect TXT files layout" | `readertypeset.lua:76-90` (default `txt_preformatted = 1`, pre); pinned `src/lvxml.cpp` `LVTextParser::CheckFormat` and `LVTextLineQueue::detectFormatFlags` (auto) | Default matches: KOReader leaves TXT lines as they are, and Reader does the same. Missing: the auto toggle. The pinned heuristic (line-start and line-end histograms, indent, centred and justified lines, empty-line density, PML detection) sets paragraph and header flags, so it needs a separate authored implementation and its own oracle. Deferred for that engineering cost, not for lack of source. | low-medium | deferred |
| S-14 | Render modes (legacy/flat/book/web) paused in several modes | `creoptions.lua:317-334`; `readertypeset.lua:378-447` | Partial. Two-page spreads and Publisher CSS off keep the selected mode (`publisher_render_mode_stays_on_for_facing_leaves_and_pauses_in_scroll`, `publisher_render_mode_stays_on_with_publisher_css_off_and_pauses_in_scroll`). KOReader sets render flags independently of the embedded stylesheet switch (`readertypeset.lua` `setBlockRenderingMode`), so this matches its Lua. Still paused: continuous scrolling and hidden flows; RTL and vertical text keep Reader layout. Visual result with Publisher CSS off is not compared on the KOReader engine (CREngine not in checkout). | medium | partial |
| S-15 | In-page footnote fallbacks: facing pages, continuous scroll, cross-spine or nested notes | `css_tweaks.lua:920+` | Partial. Facing pages now compose each leaf with its own bands at its own origin (`session.rs` `in_page_notes_compose_on_each_facing_leaf_at_its_own_origin`, `docs/in-page-footnotes.md`). Still open: continuous scroll, cross-spine and nested notes. Reason: those need layout work; S-7 waits for them. | medium | partial |

## Layout, navigation and reflow (plan 1.6, 1.7, 1.8, 1.9)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| N-1 | Followed-link marker (black bar about 0.5 s, removed after 1 s) | `readerrolling.lua:~770-850` | Done. After a followed internal link, a black bar appears in the left margin at the target's height after 0.5 s and is removed a second later (`LinkMarker`, `followed_link_marker`, `poll_link_marker`, `src/app/link_taps.rs`; the bar is a page highlight, `link_marker_highlight`, so the page picture does not cover it). Test: `a_followed_link_marks_its_target_for_a_second_after_half_a_second`. | low-medium | closed |
| N-2 | Link to another local document: opens after a confirmation; `?pos=` | `readerlink.lua:771-815` (`openFileFromLink`) | Done for fixed pages: a numeric `?pos=` opens that page of a local PDF (`LocalTarget`, `read_local_target` in `src/app/external_link.rs`; tests `local_pdf_position_tests`, resolver tests). Not done: KOReader xpointer positions for reflow documents are not mapped to Reader positions, so those open at the start. | low-medium | partial |
| N-3 | Character-count ("stable") pages for any reflow document | `readerpagemap.lua:33-80, 448-548` | Done for every reflow document: the EPUB-only gates in `src/app/character_page_settings.rs` are removed; fixed pages never reach that path. Test: `character_pages_count_a_plain_text_book_too` (`src/app/character_pages_tests.rs`). | low-medium | closed |
| N-4 | Full refresh on chapter boundaries and on pages with images | `readertoc.lua:100-122`; `readerview.lua:285` | Done. Chapter boundaries: an option (`refresh_on_chapter_boundaries`). Pages with pictures: on by default, with a menu switch (`no_refresh_on_pictures`), using the layout's visible pictures (`Reading::page_has_images`). Tests: `src/app/refresh_tests.rs`. Not done: KOReader's second-page and backward-end timing for chapter boundaries (we refresh on any crossing). | low | closed |
| N-5 | Long press on a Contents row shows the full title | `readertoc.lua:994-1010` (`onMenuHold`) | A long press on a row opens a popup with the whole title, closed by any tap (`src/app/contents_view.rs`, `contents_long_tap`, `draw_full_title_popup`; test `src/app/contents_title_tests.rs`). | low | closed |
| N-6 | Footnote popup highlights its source link while open | `readerlink.lua:1160-1185` | Done. The open note's source link is already a temporary page highlight: `note.reference` is drawn with `Paint::Temporary(220)` in the page highlights (`draw_content`, `src/app.rs`). A canvas outline was tried and removed because the page picture covers canvas overlays. | low | closed |
| N-7 | "Partial renderings" (current chapter first, then background) | `readerrolling.lua:~1130-1260` | chapter-first or whole-book-first for bounded text EPUBs (`docs/koreader-sdl-reference-controls.json` T3-7.6) Reason: Partial renderings (current chapter first, then background) change the reflow pipeline; large. | low | deferred |
| N-8 | Alternative-ToC restore and its chapter lengths for unsupported layouts | `readertoc.lua`, `readerpagemap.lua` | `docs/contents-list-settings.md`; `docs/reflow-book-map-marks.md` Reason: Book Map options (themes, grid and flat views, markers) are presentation. | low | deferred to UI pass |
| N-9 | Two-column odd/even first-page rule (`odd_or_even_first_page`) and two-page `_gotoPage` rules | `readerrolling.lua:67, 1175-1190`; `readerhighlight.lua:1866-1869` | Matches for ordinary navigation: KOReader keeps the odd page as the left leaf, and Reader pairs zero-based pages as `p / 2 * 2`, which is the same pairing. Exempt in KOReader: highlight-driven turns (`free_first_page`). The even option is never set by a menu. Selection-driven turns across a spread are not traced. | low | done |
| N-10 | Page-position memory per page (`page_positions`) | `readerpaging.lua:150-206` | Intentional divergence. KOReader saves a fractional vertical position per page for paged layouts. Reader keeps reflow positions on source anchors, which survive layout changes, and restores fixed-page views per session. No per-page offset store is planned. | low | decided |
| N-11 | Margin editors: paired L/R spinner, hold steps of 5 logical units, dual top/bottom editor | `creoptions.lua` editors | Done. Paired left and right steppers with one-pixel taps and five-pixel holds, the nine stock preset stops, the linked or independent top and bottom editor, and the exact-value editor (`docs/reflow-page-margins.md`; 39 margin tests pass, `cargo test -- margin`). Long-hold repeat timing still needs a device comparison. | low | closed |

## Phase 1.4 open items (from the Phase 0 pass)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| R-1 | Landscape scan reflow return and scale round trip | none (Reader-internal) | Closed. Two test causes, no production drift: the round trip stopped at 100 instead of the stock 105, and the return cleared the transient "End of document" footer notice set by the staging turn. `scan_text_viewport_stages_rolls_back_and_uses_landscape_controls` now compares the page exactly outside the footer band. | medium | closed |

## Scan and OCR reflow (plan 1.10)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| C-1 | Reflow speckle size (`defect_size`), indentation detection (`detect_indent`), render quality, background cleanup | `koptoptions.lua:~566, 606, 668, 682` | only mentioned in `docs/koreader-source-inventory.md:105` Reason: Speckle size, indent detection, render quality and background cleanup change the scan pipeline; each needs OCR-grade measurement. | low-medium | deferred |
| C-2 | Saturation and hardware dithering | `koptoptions.lua:~511, 580` | deliberately deferred (grayscale, model-dependent; `docs/koreader-source-inventory.md:106`) | low | intentional |
| C-3 | Auto-straighten estimator | `koptoptions.lua` | Reader's own estimator (`docs/scan-controls.md:133-147`) Reason: The auto-straighten estimator is a research item against KOptOptions. | low | deferred |

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
| P-7 | Skim: ±10 pages and hold chapter to first or last | `skimtowidget.lua:100-290` (`goToPage(curr ± 10)`; chapter hold to `1` or `page_count`) | Matches. Reader ±10 steps are tested both ways (`skim_ten_page_buttons_preview_in_both_directions_without_recording_jumps`). Holding a chapter button previews the first or last page, and the opening location is kept until accepted (`skim_chapter`, test at `src/app/skim.rs:84`). KOReader jumps immediately on hold; Reader's preview model is the one its Skim already uses. | low | done |
| P-8 | Binding label "Page browser" for `Binding::Skim` routes to Skim, not the page browser | `frontend/dispatcher.lua` | Label fixed to "Skim" (`src/app/gestures.rs`, test `the_skim_binding_is_labelled_for_the_skim_screen_it_opens`); the existing `assigned_swipes_respect_overlays_footer_and_page_selection` test confirms the binding opens `Screen::Skim`. Still open: no bindable action opens the page browser (`Action::Overview`). Not verified against KOReader dispatcher source (not in checkout). | low | partial |

## Highlights and annotations (plan 3.4)

| ID | Behaviour | KOReader evidence | Reader evidence | Impact | Status |
| --- | --- | --- | --- | --- | --- |
| H-1 | Adjust highlight boundaries by word or character, including shrink | `readerhighlight.lua:1029-1260, 1394-1445` | Extend only unions; no shrink, no character step (`src/session.rs:~2079`; `src/app/extend_highlight.rs`) | medium | partial |
| H-2 | Edit highlighted text, reset text, filter by edited text | `readerbookmark.lua:957-961, 1301-1315, 1466-1490`; `readerhighlight.lua:1366` | not found; `text_edited` only parsed on import (`src/koreader_sidecar/annotations.rs:93`) | medium | open |
| H-3 | "Follow link" button in the selection popup | `readerhighlight.lua:236-250` | not found (`src/app/selection_popup.rs`) | low-medium | open |
| H-4 | Panel zoom for any paged document: toggle, fallback to text selection, `panel_zoom_toggle` action | `readerhighlight.lua:886-915, 1605-1635` | comic archives only, always on (`src/app.rs:7667-7684`, `src/comic_panel.rs`) | low-medium | partial |
| H-5 | Write highlights into the original PDF (on/off, reminder, write all, delete all) | `readerhighlight.lua:600-735, 2253` | separate annotated copy only (`docs/highlight-drawing-styles.md`, `src/pdf_annotations.rs`) | low | intentional (decide later) |
| H-6 | Per-style highlight options audit | `readerhighlight.lua:350-422` (four drawers: lighten, underscore, strikeout, invert) | Matches. Same four styles, book and reader defaults, and per-mark edit. Physical-device evidence is recorded in `docs/highlight-drawing-styles.md`. | low | done |

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
