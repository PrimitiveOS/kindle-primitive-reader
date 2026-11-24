# Offline dictionaries on Kindle

Hold a word and tap **Dictionary**. The results panel stays over the passage. Use **Results** to choose a source or another meaning. Closing the panel restores the selection. Open **Dictionaries** from reading settings or the lookup tools to change the sources. Tap a dictionary to enable or disable it by default or move it in the default order. **Prefer for this book** puts it ahead of the global list, including when it is disabled by default. Each book can have several preferred dictionaries in its own order. Returning to another book restores that book’s choices. [Dictionary settings](screenshots/dictionary-priorities.png)

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
A preset can report how many saved sources are unavailable.

KOReader's named presets also snapshot membership and disable sources added
after the snapshot. Its missing-source notice lists the source names. Reader's
Mac workflow covers management, lookup precedence, backup compatibility, and
failed-write recovery.
[Landscape simulator](screenshots/landscape-dictionary-preset.png) ·
[Pinned snapshot and apply](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1970-L2013) ·
[Executable preset management comparison](../scripts/compare-dictionary-presets.py)

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

## What determines a match

An exact headword and an alias may both resolve in one dictionary. Another dictionary may have a direct entry for the same selected word. Reader consults the optional synonym table before streaming the main index, so each dictionary needs one main-index pass. The result set retains distinct senses with identical spellings. Some bilingual indexes return legitimate alias headwords with a zero-byte definition. These appear as **Headword only**. Entries with definition text appear first within the same dictionary. Unicode case folding supplies a fallback. Each lookup has shared scan and decompression limits.

For an XDXF dictionary, the headword appears in the popup heading and its pronunciation appears in brackets in the definition. HTML definitions preserve visible text and paragraph breaks. Reader opens dictionary components read-only, keeps local preferences in its own data directory and ignores executable dictionary customization files.

## KOReader comparison

The pinned KOReader module enumerates the main and extended directories. Its Kindle build pins sdcv 0.5.5. The comparison uses the installed executable's exact lookup, alongside the metadata and indexes on the same device. For HTML, the reference parser extracts visible text. For plain and XDXF output, it uses sdcv's text directly. Whitespace is normalized when comparing results. [KOReader dictionary discovery](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerdictionary.lua#L127-L166) · [KOReader dependency](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/sdcv/CMakeLists.txt#L62-L68) · [sdcv XDXF presentation](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/libwrapper.cpp#L49-L123)

To repeat the optional real-file functional test, supply an existing `PRIMITIVE_READER_DOCUMENTS` directory and `READER_DICTIONARY_REFERENCE` JSON containing `expected_count` and `cases`. Each case supplies an absolute book path, zero-based chapter, selected word and expected dictionary-title, headword and normalized visible-text digest triples. Run `cargo test --features kindle --lib selected_words_match_installed_koreader_dictionaries -- --ignored`. The test creates temporary Reader data, finds the word in the specified chapter, holds its actual on-page coordinates, opens the popup and checks the restored position. The digest is an MD5 comparison of whitespace-normalized UTF-8 test text. Real publications and dictionary contents stay out of the repository.

Reader can look up common English and supported Japanese word forms and can search for similar spellings after a miss. Both preferences have a library default and a per-edition override. [Word forms and per-book settings](dictionary-word-forms.md)
