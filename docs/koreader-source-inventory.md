# KOReader source inventory for Kindle reading

The [current workflow comparison](koreader-controls-parity.md#current-reading-workflows) summarizes what works in original publications and where comparison is still needed.

The [reading controls audit](reading-control-evidence.md) ties seven distinct Oasis
screen states to Rust and Lua handlers and identifies the next acceptance tests.

KOReader commit `9192014d8bd82a91dc1012473be0f238dedfdb54` is the baseline for the Oasis captures. Each row names upstream identifiers, the Reader behavior to compare, and any remaining acceptance work. Some menu registrations depend on the document or device. Counts describe source entries. Functional parity requires a matching result with a real document. [Upstream tree](https://github.com/koreader/koreader/tree/v2026.07.1) · [Reader test evidence and feature comparisons](koreader-controls-parity.md)

To check coverage, run `python3 scripts/check-koreader-inventory.py /path/to/koreader`. The check reads the pinned Git objects. It covers reader modules, document and file manager modules, bundled plug-ins, top-menu registrations and the outer rows in both configuration dialogs. Dynamic submenus, translation services, the rendering engines and platform-specific hooks require additional comparisons.

## Reader modules

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `readerannotation`, `readerbookmark`, `readerdogear`, `readerhighlight` | Reader stores bookmarks, highlights and notes. The [named colors and global note-marker candidate](highlight-colors-and-note-markers.md) has desktop rendering, persistence and original-file checks. Native tests remain pending. Check cross-page selection, filters, ordering and export separately for reflow and fixed pages. |
| `readerback`, `readerflipping`, `readergoto`, `readerlink`, `readerthumbnail` | Location history, page jumps, links and thumbnail skimming are available. Check cancellation, forward history after branching, footnote previews and nonlinear flows. |
| `readerhandmade`, `readerpagemap`, `readertoc` | Publisher, generated and custom contents have dedicated workflows. The print list follows the publisher’s exact targets and exposes per-book footer and margin controls. Publisher non-linear fragments have a per-book control, main-flow traversal and explicit destinations. Mac functional tests and the installed reference cover five original EPUBs. Native Reader and reader-authored hidden ranges have separate acceptance checks. [Print page evidence](publisher-page-numbers.md) · [Non-linear fragments](nonlinear-reading.md) |
| `readerdictionary`, `readerwikipedia` | Installed dictionary checks cover English inflections, spelling suggestions, per-book source preferences and selection restoration. Compare language-specific candidate generators, vocabulary review, full Wikipedia articles and remote lookup failures separately. [Dictionary evidence](dictionary-word-forms.md) · [Network lookups](online-lookups.md) |
| `readerfooter`, `readerstatus`, `readeractivityindicator`, `readerdevicestatus` | Check independent footer display, chapter progress, hidden mode and deliberate exit. Compare each device indicator against Launcher ownership. |
| `readerconfig`, `readermenu` | Compare upper groups, lower format controls, remembered selections, current values, outside dismissal and page-button behavior on both document types. |
| `readerfont`, `readertypeset`, `readertypography`, `readeruserhyph`, `readerstyletweak` | Reader provides font selection, shaping, publisher CSS and authored hyphens. User hyphenation dictionaries, style-tweak selection and per-language behavior need separate evidence. |
| `readerhinting`, `readerpaging`, `readerrolling`, `readerscrolling`, `readerview` | Exercise eager and delayed pagination, continuous scroll, mixed writing directions, overlap, source anchors and cache behavior. |
| `readercoptlistener`, `readerkoptlistener`, `readercropping`, `readerpanning`, `readerzooming` | Compare each provider option, pan to source corners, follow a link, mark text and reopen at the same source position. |
| `readerkeyselection` | Oasis has two page buttons. Test their configured functions. D-pad-only text selection depends on a device with those keys. |
| `readersearch` | Search with case matching, Unicode text, repeated phrases, multiple chapters, cancellation and back navigation. Ignore accents retains the original UTF-8 selection in reflow and PDF text. Directional traversal and regular expressions have been compared with original EPUBs. The full-list result limit is tracked independently. [Reflow search](reflow-search.md) · [Expression tests](regular-expression-search.md) |

## File manager modules

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `filemanager`, `filemanagerbookinfo`, `filemanagercollection`, `filemanagerfilesearcher`, `filemanagerhistory` | Reader browses supported documents and tracks book info, recents and collections. Compare filtering, metadata edits, opening and resuming renamed files. |
| `filemanagermenu`, `filemanagersetdefaults`, `filemanagershortcuts`, `filemanagerutil` | Compare per-folder display defaults, book shortcuts and return from a book to its originating folder. File operations also exist in the standalone Files app. |
| `filemanagerconverter` | Open comparison for convertible documents and errors. Track each conversion separately from native reading. |

## Document modules

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `credocument` | Compare EPUB and other reflowable formats with source-anchored selection, type settings, image and direction fixtures. |
| `pdfdocument`, `djvudocument`, `koptinterface` | Compare PDF and DjVu page rasters, text and link geometry, zoom/crop, OCR, scanned reflow and page changes. |
| `picdocument` | Compare grayscale raster support, orientation and supported encodings per format. |
| `document`, `documentregistry`, `canvascontext`, `doccache`, `tilecacheitem` | Check format routing, document lifecycle, unsupported-file feedback, viewport coordinates, cache invalidation and Kindle memory against the same corpus. |

## Bundled plugins

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `gestures`, `hotkeys`, `externalkeyboard` | Assignable single-contact gestures and Oasis page buttons have Rust paths. Multi-contact gestures, sequences, external keyboard and hardware remapping require separate evidence. |
| `autoturn`, `readtimer` | Timed turns use the ordinary paced page handler. Read timer supports intervals, local alarms, repeat, expiry messages and resume in the normal Tools menu. [Clock semantics and verification](read-timer.md) |
| `exporter`, `vocabbuilder`, `statistics`, `profiles` | Reader exports annotations, saves vocabulary, time and profiles. Compare export formats, scheduled vocabulary review, calendars, daily histories and action sequences. |
| `opds`, `calibre`, `kosync`, `cloudstorage`, `wallabag`, `newsdownloader` | Networked reading workflows. Compare discovery, downloading, progress/sync conflict choice, offline cache and user-initiated transmission separately. |
| `coverbrowser`, `bookshortcuts`, `movetoarchive`, `archiveviewer` | Check browse views, document shortcuts, completed-book filing and supported archive content through Reader and Files. |
| `japanese`, `perceptionexpander`, `docsettingtweak` | Compare language deinflection, focused reading guides and per-book overrides on synthetic documents. |
| `autodim`, `autostandby`, `autosuspend`, `autowarmth`, `batterystat`, `coverimage`, `keepalive`, `timesync`, `systemstat` | System capabilities belong to Primitive Launcher. Verify access, return to the same page and device behavior for applicable Kindle workflows. |
| `SSH`, `terminal`, `httpinspector`, `hello`, `texteditor`, `qrclipboard` | Previously excluded side tools, developer facilities or external utilities. Listed to make the complete upstream plug-in inventory visible. |

## Reflow controls

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `rotation_mode`, `visible_pages`, `view_mode` | Portrait orientation, facing pages, page turns and continuous reading. Compare anchors, selected text and links after each transition. |
| `h_page_margins`, `t_page_margin`, `b_page_margin` | Reader exposes independent left, right, top and bottom margins. The same word and all rendered rows moved by the chosen offset on two actual EPUBs. [Verification](reflow-page-margins.md) |
| `sync_t_b_page_margins` | The normal controls link the two edges at their rounded average, adjust either and restore separate changes after unlinking. [Controls and screenshots](reflow-page-margins.md) |
| `font_size`, `font_fine_tune`, `line_spacing`, `word_spacing`, `font_gamma`, `font_base_weight`, `font_hinting`, `font_kerning` | [Numeric gamma and base-weight candidate](font-contrast-and-weight.md) covers reference presets, fine ranges, coverage tables and persistence. Native outline checks remain open. Other typography controls have separate comparisons. |
| `embedded_css`, `embedded_fonts` | Check publisher styles and font override independently in reflow and mixed-script titles. |
| `status_line` | Reader uses its reading footer. Check displayed information and interaction against the footer workflow. |
| `word_expansion` | Justified paragraphs have the 0–20% expansion control, sampled through live settings and original text anchors. Compare word-space reduction and nested script behavior separately. [Word expansion](word-expansion.md) |
| `block_rendering_mode`, `render_dpi`, `cjk_width_scaling` | Compare selectable publisher rendering modes, content scale and CJK advance scaling individually. |
| `smooth_scaling`, `nightmode_images` | Compare standalone and embedded pictures in day/night pages with a diagnostic image and both scaling filters. |

## Fixed-page controls

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `rotation_mode`, `trim_page`, `page_margin`, `auto_straighten` | Compare orientation, crop modes, post-crop margins and bounded deskew. Retain source geometry for links, word selections and annotations. |
| `zoom_overlap_h`, `zoom_overlap_v`, `zoom_mode_type`, `zoom_range_number`, `zoom_factor`, `zoom_mode_genus`, `zoom_direction` | Reader offers fit, zoom, pans, comic spreads and RTL. Generic rows, columns, reading order and overlap each need their own comparison. |
| `page_scroll`, `page_gap_height` | Exercise continuous mode and the selectable inter-page gap across pages of different sizes. |
| `line_spacing`, `justification`, `font_size`, `font_fine_tune`, `word_spacing`, `text_wrap` | Embedded-text and scan reflow are available in subsets. Compare text width, adjustable line and word spacing and return to the original word. |
| `contrast`, `white_threshold`, `sw_dithering` | Compare raster changes, text legibility, blank areas and unchanged source geometry. |
| `doc_language`, `forced_ocr`, `writing_direction`, `defect_size`, `detect_indent`, `max_columns` | Compare installed OCR language choices, recognizing over an existing layer, vertical and RTL reading, indentation and multi-column scans. |
| `saturation`, `hw_dithering` | Model-dependent. The Oasis display is grayscale. Hardware waveform effects need physical evidence. |
| `page_opt`, `background_cleanup`, `quality`, `nightmode_document` | Compare scan cleanup, quality levels, page optimization and document-only inversion with repeatable images. |

## Menu navi

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `autoturn`, `book_map`, `bookmark_browsing_mode`, `bookmarks`, `go_to`, `go_to_next_location`, `go_to_previous_location`, `hide_nonlinear_flows`, `navi_settings`, `page_browser`, `skim_to`, `table_of_contents`, `toggle_bookmark` | Navigation overlays, bookmark modes, map, skim and location history. Test individual commands and enabled conditions before declaring the group matched. |

## Menu navi_settings

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `bookmarks_settings`, `handmade_hidden_flows`, `handmade_settings`, `handmade_toc`, `page_map`, `toc_alt_toc`, `toc_items_font_size`, `toc_items_per_page`, `toc_items_show_chapter_length`, `toc_items_with_dots`, `toc_ticks_level_ignore` | Contents hierarchy, per-page display options, generated and hand-built navigation, reference page maps and bookmark preferences. Test individual commands and enabled conditions before declaring the group matched. |

## Menu typeset

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `change_font`, `djvu_render_mode`, `document_settings`, `highlight_options`, `page_overlap`, `panel_zoom_options`, `selection_text`, `set_render_style`, `speed_reading_module_perception_expander`, `start_content_selection`, `style_tweaks`, `switch_zoom_mode`, `typography` | Document-specific typography, zoom, styles, selection, highlighting and panel view. Test individual commands and enabled conditions before declaring the group matched. |

## Menu setting

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `device`, `document`, `frontlight`, `language`, `navigation`, `network`, `night_mode`, `screen`, `status_bar`, `taps_and_gestures` | Reader settings, system settings in Launcher and document-specific preferences have different return paths. Test individual commands and enabled conditions before declaring the group matched. |

## Menu document

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `document_end_action` | End-of-book dialog, automatic or manual reading status, next file, beginning, Library, and confirmed deletion. The same end boundary is checked for paged and continuous reading against original EPUB, AZW3 and PDF files. [Controls and reference tests](document-end.md) |
| `document_auto_save` | Close and sleep saves and the 5, 15, 30 and 60 minute intervals. Checked the timing against installed KOReader and verified original EPUB and PDF positions. [Save reading data](metadata-save.md) |
| `document_metadata_arc`, `document_metadata_location`, `language_support`, `partial_rerendering` | Durable metadata, language settings and rerender behavior. Test individual commands and enabled conditions before declaring the group matched. |

## Menu device

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `autoshutdown`, `autostandby`, `autosuspend`, `charging_led`, `cover_events`, `device_status_alarm`, `external_keyboard`, `file_ext_assoc`, `font_ui_fallbacks`, `ignore_battery_optimizations`, `ignore_open_sleepcover`, `ignore_sleepcover`, `keyboard_layout`, `mass_storage_settings`, `pageturn_power`, `screenshot`, `time`, `units` | Device-conditional controls. Compare Oasis capabilities and Launcher ownership independently. Test individual commands and enabled conditions before declaring the group matched. |

## Menu navigation

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `android_back_button`, `android_haptic_feedback`, `android_volume_keys`, `back_in_filemanager`, `back_in_reader`, `back_to_exit`, `backspace_as_back`, `opening_page_location_stack`, `physical_buttons_setup`, `skim_dialog_position` | Page keys, touch and location-stack behavior on Oasis. Android-only controls remain platform entries. Test individual commands and enabled conditions before declaring the group matched. |

## Menu network

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `network_after_wifi_action`, `network_before_wifi_action`, `network_dismiss_scan`, `network_info`, `network_powersave`, `network_proxy`, `network_restore`, `network_wifi`, `ssh` | Connectivity and SSH are managed by Launcher. Compare opt-in reading network actions offline and online. Test individual commands and enabled conditions before declaring the group matched. |

## Menu screen

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `autodim`, `autowarmth`, `color_rendering`, `coverimage`, `fullscreen`, `screen_dpi`, `screen_eink_opt`, `screen_notification`, `screen_rotation`, `screen_timeout`, `screensaver` | System overlay, display rotation, screen color, e-ink refresh, interruptions and power. Test individual commands and enabled conditions before declaring the group matched. |

## Menu taps_and_gestures

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `follow_links`, `gesture_intervals`, `gesture_manager`, `gesture_overview`, `ignore_hold_corners`, `long_press`, `menu_activate`, `page_turns`, `screen_disable_double_tap`, `scrolling` | Reading gesture assignment, links, page turns, selection and menu activation. Test individual commands and enabled conditions before declaring the group matched. |

## Menu tools

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `calibre`, `cloudstorage`, `exporter`, `more_tools`, `move_to_archive`, `news_downloader`, `profiles`, `progress_sync`, `qrclipboard`, `read_timer`, `statistics`, `text_editor`, `wallabag` | Compare reading tasks individually, including timed reading, statistics, export, sync and profiles. Test individual commands and enabled conditions before declaring the group matched. |

## Menu more_tools

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `auto_frontlight`, `battery_statistics`, `book_shortcuts`, `doc_setting_tweak`, `keep_alive`, `patch_management`, `plugin_management`, `synchronize_time`, `terminal` | Separate reading, shared-system and previously excluded side tools. Test individual commands and enabled conditions before declaring the group matched. |

## Menu search

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `bookmark_search`, `dictionary_lookup`, `dictionary_lookup_history`, `find_book_in_calibre_catalog`, `fulltext_search`, `fulltext_search_findall_results`, `search_settings`, `translate_current_page`, `vocabbuilder`, `wikipedia_history`, `wikipedia_lookup` | Per-document text search, dictionary lookup, reference services and result navigation. Test individual commands and enabled conditions before declaring the group matched. |

## Menu search_settings

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `dictionary_settings`, `fulltext_search_settings`, `translation_settings`, `wikipedia_settings` | Per-service languages, dictionary source and text-search preferences. Test individual commands and enabled conditions before declaring the group matched. |

## Menu main

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `book_info`, `book_status`, `bookmark_browser`, `collections`, `exit_menu`, `favorites`, `help`, `history`, `mass_storage_actions`, `open_previous_document`, `ota_update` | Open from history, collections or favorites, view book information and return safely to Launcher. Test individual commands and enabled conditions before declaring the group matched. |

## Menu help

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `about`, `plugins_disable_external`, `quickstart_guide`, `report_bug`, `search_menu`, `system_statistics`, `version` | Discovery, version information and access to help for the applicable reading interfaces. Test individual commands and enabled conditions before declaring the group matched. |

## Menu exit_menu

| Upstream identifiers | Reader behavior and acceptance |
| --- | --- |
| `exit`, `poweroff`, `reboot`, `restart_koreader`, `sleep`, `start_bq` | Visible Reader exit and return. Launcher owns system sleep, power and restart. Test individual commands and enabled conditions before declaring the group matched. |
