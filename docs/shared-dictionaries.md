# Offline dictionaries on Kindle

Hold a word and tap **Dictionary**. The results panel stays over the passage. Use **Results** to choose a source or another meaning. Closing the panel restores the selection. Open **Dictionaries** from reading settings or the lookup tools to change the sources. Tap a dictionary to enable or disable it by default or move it in the default order. **Prefer for this book** puts it ahead of the global list, including when it is disabled by default. Each book can have several preferred dictionaries in its own order. Returning to another book restores that book’s choices. [Dictionary settings](screenshots/dictionary-priorities.png)

| Location on Kindle user storage | Behavior |
| --- | --- |
| `reader/data/dictionaries/` | Reader's own StarDict directory. Existing preferred-dictionary settings continue to work. |
| `koreader/data/dict/` | Reuse dictionaries installed with KOReader. The metadata, index, compressed definitions and optional synonyms remain in place. |
| `koreader/data/dict_ext/` | Include dictionaries from KOReader's additional directory. Preferences distinguish all three locations even if filenames match. |

## What determines a match

An exact headword and an alias may both resolve in one dictionary. Another dictionary may have a direct entry for the same selected word. Reader consults the optional synonym table before streaming the main index, so each dictionary needs one main-index pass. The result set retains distinct senses with identical spellings. Some bilingual indexes return legitimate alias headwords with a zero-byte definition. These appear as **Headword only**. Entries with definition text appear first within the same dictionary. Unicode case folding supplies a fallback. Each lookup has shared scan and decompression limits.

For an XDXF dictionary, the headword appears in the popup heading and its pronunciation appears in brackets in the definition. HTML definitions preserve visible text and paragraph breaks. Reader opens dictionary components read-only, keeps local preferences in its own data directory and ignores executable dictionary customization files.

## KOReader comparison

The pinned KOReader module enumerates the main and extended directories. Its Kindle build pins sdcv 0.5.5. The comparison uses the installed executable's exact lookup, alongside the metadata and indexes on the same device. For HTML, the reference parser extracts visible text. For plain and XDXF output, it uses sdcv's text directly. Whitespace is normalized when comparing results. [KOReader dictionary discovery](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerdictionary.lua#L127-L166) · [KOReader dependency](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/sdcv/CMakeLists.txt#L62-L68) · [sdcv XDXF presentation](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/libwrapper.cpp#L49-L123)

To repeat the optional real-file functional test, supply an existing `PRIMITIVE_READER_DOCUMENTS` directory and `READER_DICTIONARY_REFERENCE` JSON containing `expected_count` and `cases`. Each case supplies an absolute book path, zero-based chapter, selected word and expected dictionary-title, headword and normalized visible-text digest triples. Run `cargo test --features kindle --lib selected_words_match_installed_koreader_dictionaries -- --ignored`. The test creates temporary Reader data, finds the word in the specified chapter, holds its actual on-page coordinates, opens the popup and checks the restored position. The digest is an MD5 comparison of whitespace-normalized UTF-8 test text. Real publications and dictionary contents stay out of the repository.
