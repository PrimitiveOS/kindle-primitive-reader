# Offline dictionaries on Kindle

Hold a word and tap **Dictionary**. The results panel stays over the passage. Use **Results** to choose a source or another meaning. Closing the panel restores the selection. Open **Dictionaries** from reading settings or the lookup tools to change the sources. Tap a dictionary to enable or disable it by default or move it in the default order. **Prefer for this book** puts it ahead of the global list, including when it is disabled by default. Each book can have several preferred dictionaries in its own order. Returning to another book restores that book’s choices. [Dictionary settings](screenshots/dictionary-priorities.png)

## Choosing among matching sources

Tap **Results** for every matching headword and its definition preview, or
hold **Results** to group the matches by dictionary. The flat list also
offers **Sources**. Tap a dictionary name to open its first match. Its
right-hand button opens the only headword or lists that dictionary's
matches when there are several. A long headword also gets a wider row.
**All Results** returns to the flat list. Reader remembers the page of
each list for the current lookup. [Sources](screenshots/dictionary-grouped-sources.png) ·
[Dictionary matches](screenshots/dictionary-grouped-senses.png)

Hold a headword or tap **Look up** in its results row to look up that
word separately. **Back** restores the chooser and its position. Closing
the lookup returns to the original passage and selection. Reader keeps
installed sources separate by their StarDict package path, including
sources with the same display name. [Nested lookup](screenshots/dictionary-grouped-nested-lookup.png) ·
[Dark PDF example](screenshots/pdf-dictionary-grouped-results-dark.png)

Mac simulation exercises EPUB and PDF selections, the active source and
result, physical page inputs, overflow, separate page positions, and
nested return. The stock script runs menu callbacks from pinned KOReader
Lua with an authored result list and simulated dialogs. [Pinned result
menus](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1976-L2170) ·
[Reproducible stock check](../scripts/compare-dictionary-grouped-results.py)

## Excluding a source for one book

Open a source in **Dictionaries** and choose **Disable for this book**.
Exact lookup, word forms, fuzzy suggestions, and automatic CJK selection
all use the book's enabled sources. **Re-enable for this book** restores
the source to its saved priority. **Reset book dictionary choices** clears
that edition's source exclusions and priorities and returns it to the library's
source list. Word forms and fuzzy settings remain separate. These choices
persist when switching books or reopening Reader. [Simulator capture](screenshots/book-dictionary-exclusions.png)

Reader's checks use authored EPUB and PDF files and exercise the local CJK
worker. They verify that one edition can exclude a source another edition
still uses, and that re-enabling restores its previous rank. KOReader also
saves edition exclusions separately from its preferred order and applies the
filtered set to the lookup command. [Pinned source controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L733-L825) · [Pinned lookup dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1454-L1477)

The source check runs KOReader's preferred-source menu callback, persisted
edition settings, lookup filter, and reset against the pinned Lua module.
[Reproducible dictionary exclusion check](../scripts/compare-dictionary-book-exclusions.py)

| Location on Kindle user storage | Behavior |
| --- | --- |
| `reader/data/dictionaries/` | Reader's own StarDict directory. Existing preferred-dictionary settings continue to work. |
| `koreader/data/dict/` | Reuse dictionaries installed with KOReader. The metadata, index, compressed definitions and optional synonyms remain in place. |
| `koreader/data/dict_ext/` | Include dictionaries from KOReader's additional directory. Preferences distinguish all three locations even if filenames match. |

## Named dictionary presets

Open **Dictionaries › Dictionary presets** and choose **Create preset from
enabled dictionaries**. Name the set, then select it to apply, update,
rename, or delete it. Applying a preset replaces the library's default
enabled set with its available members. That makes it possible to switch
between small groups of dictionaries for different languages or subjects.
Lookup still uses each edition's preferred sources and exclusions.
Applying a set with missing sources lists their saved dictionary titles.
Older presets show the dictionary's filename without its directory. Refresh
the saved titles by choosing **Update from enabled dictionaries**. If you
reinstall a missing dictionary, apply the preset again to include it.
[Landscape simulator](screenshots/landscape-dictionary-preset-unavailable.png) ·
[Pinned snapshot and apply](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1970-L2013)

Choose **Cycle through dictionary presets** from the preset list or assign
it under **Settings › Reading gestures** to switch sets while reading.
Cycle applies presets in name order, wraps after the last one, and remembers
the last preset used by the cycle. A manual Apply or a temporary lookup
leaves that sequence in place. The gesture keeps the current reading
location when it confirms a load or reports unavailable dictionaries.
[Landscape settings](screenshots/landscape-dictionary-preset.png) ·
[Pinned cycle action](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/presets.lua#L290-L311) ·
[Executable preset comparison](../scripts/compare-dictionary-presets.py)

## Searching with a preset once

Enter a word, then tap **Search with preset** and choose a saved set.
The same action is available after a miss and when editing a word from
a definition. **Cancel** returns to the query. A preset applies to the
current lookup, including its spelling suggestions and nested definitions.
A book's preferred and excluded sources still apply. Closing the result
returns to the original editor or selection and leaves saved dictionary
settings intact. After a miss with fuzzy search already enabled, Reader
can use the same word immediately. Use **Edit** to change the word first.
[Landscape simulator](screenshots/landscape-one-query-dictionary-preset.png) ·
[Dark mode](screenshots/landscape-one-query-dictionary-preset-dark.png)

The Mac tests exercise an authored EPUB and PDF, including page buttons
in the picker, restoration of the original selection and query, and
worker cancellation. The stock check runs the pinned Lua 5.1 callbacks for
typed lookup, missing results and editing inside a dictionary result.
[Pinned one-query callback](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1153-L1230) ·
[Pinned result editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1841-L1939) ·
[Reproducible callback check](../scripts/compare-dictionary-preset-lookup.py)

## Dictionary history

Open **Search › Dictionary history** to see recent local lookups, newest
first. A heading identifies each run of lookups from the same book.
Each recorded lookup shows the device's local date and time. Tap a word
to look it up again while staying at your current reading location.
Reader keeps up to 64 records, including repeated words. Older queries
remain available with their original wording.
[Portrait simulation](screenshots/dictionary-history-grouped-portrait.png) ·
[Landscape simulation](screenshots/dictionary-history-grouped-landscape.png)

Under **Dictionaries**, turn off **Record dictionary lookups** to stop
adding new entries. Choose **Clear dictionary history** and confirm to
remove existing entries. These controls apply to local dictionary history.
Saved vocabulary and online lookups have their own controls. Local reading
data backups retain the recorded titles, words, and timestamps.
[Clear-history simulation](screenshots/dictionary-history-clear-landscape.png)

The pinned KOReader callback groups consecutive titles and reopens a word
through the current dictionary provider. Reader combines an unchanged
fuzzy retry with its first attempt. The simulator uses an authored EPUB
and PDF, and the stock check runs the logging, grouping, timestamp,
repeat, and privacy callbacks in Lua 5.1.
[Pinned history view](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L261-L290) ·
[Pinned recorder](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1425-L1436) ·
[Executable callback comparison](../scripts/compare-dictionary-history.py)

## What determines a match

An exact headword and an alias may both resolve in one dictionary. Another dictionary may have a direct entry for the same selected word. Reader consults the optional synonym table before streaming the main index, so each dictionary needs one main-index pass. The result set retains distinct senses with identical spellings. Some bilingual indexes return legitimate alias headwords with a zero-byte definition. These appear as **Headword only**. Entries with definition text appear first within the same dictionary. Unicode case folding supplies a fallback. Each lookup has shared scan and decompression limits.

For an XDXF dictionary, the headword appears in the popup heading and its pronunciation appears in brackets in the definition. HTML definitions preserve visible text and paragraph breaks. Reader opens dictionary components read-only, keeps local preferences in its own data directory and ignores executable dictionary customization files.

## KOReader comparison

The pinned KOReader module enumerates the main and extended directories. Its Kindle build pins sdcv 0.5.5. The comparison uses the installed executable's exact lookup, alongside the metadata and indexes on the same device. For HTML, the reference parser extracts visible text. For plain and XDXF output, it uses sdcv's text directly. Whitespace is normalized when comparing results. [KOReader dictionary discovery](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerdictionary.lua#L127-L166) · [KOReader dependency](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/sdcv/CMakeLists.txt#L62-L68) · [sdcv XDXF presentation](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/libwrapper.cpp#L49-L123)

To repeat the optional real-file functional test, supply an existing `PRIMITIVE_READER_DOCUMENTS` directory and `READER_DICTIONARY_REFERENCE` JSON containing `expected_count` and `cases`. Each case supplies an absolute book path, zero-based chapter, selected word and expected dictionary-title, headword and normalized visible-text digest triples. Run `cargo test --features kindle --lib selected_words_match_installed_koreader_dictionaries -- --ignored`. The test creates temporary Reader data, finds the word in the specified chapter, holds its actual on-page coordinates, opens the popup and checks the restored position. The digest is an MD5 comparison of whitespace-normalized UTF-8 test text. Real publications and dictionary contents stay out of the repository.

Reader can look up common English and supported Japanese word forms and can search for similar spellings after a miss. Both preferences have a library default and a per-edition override. [Word forms and per-book settings](dictionary-word-forms.md)
