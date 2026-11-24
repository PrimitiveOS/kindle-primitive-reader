# Looking up inflected words

Long-press a word on the page and tap Dictionary. If an enabled dictionary has an entry for the selected spelling or an alias, Reader shows that entry. When Word forms is on, Reader also tries common English or supported Japanese inflections in that dictionary. For example, selecting *carried* can open the headword *carry*. When those steps find nothing and Fuzzy search is on, Reader searches the enabled dictionaries for up to ten similar spellings. The popup stays over the book while results arrive. Closing it returns to the selected word. Save word remains a separate action.

![Looking up carried in the Harbour Dictionary](screenshots/dictionary-word-form.png)

In Dictionaries, **Word forms by default** sets how Reader handles inflections. **Fuzzy search by default** controls the extra spelling scan after a miss. Both are on for a fresh installation. Each book or PDF can use its own choice for either setting. **Use the default fuzzy search setting** returns that edition to the library preference. On upgrade, any existing Word forms choice supplies the initial fuzzy choice at the same scope. Each setting can then be changed separately.

![Dictionary settings with Word forms on and a book-specific fuzzy choice](screenshots/dictionary-fuzzy-settings.png)

## Fuzzy retry after a miss

With fuzzy search disabled, an ordinary miss in an enabled dictionary opens a
prefilled editor. Edit the word if needed and choose **Fuzzy search** for this
lookup. The original passage remains selected, and the book and library
preferences retain their saved values. Cancel returns to the passage or the
typed lookup editor. If the retry finds nothing, the result can be dismissed
without another prompt. [Pinned miss and retry callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1465-L1504)

The Mac tests exercise the visible landscape keyboard, an edited query from
an EPUB selection, and cancelling or exhausting a retry from typed lookup.
The definition close returns to the original selection and reading position.
[Simulator capture](screenshots/landscape-dictionary-fuzzy-retry.png)

## Reference behavior

Pinned KOReader exposes a global fuzzy default and saves a separate option for each edition. Its lookup picker passes `--exact-search` to sdcv when fuzzy is off. With fuzzy enabled, sdcv can try case and English suffix forms and then edit distance. KOReader's language-support plugins can still supply form candidates when fuzzy is off. Reader handles word forms in its own lookup and applies Fuzzy search only to the bounded spelling scan. Both skip edit-distance suggestions for CJK selection. [KOReader setting and dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L327-L351) · [KOReader candidates](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1341-L1377) · [sdcv word lookup](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/stardict_lib.cpp#L1261-L1355) · [Japanese and Chinese selection](cjk-dictionary-selection.md)

The local comparison exercised the pinned Lua menu callbacks and lookup dispatch. The Reader simulation used an authored EPUB and PDF to verify edition switching, disabling fuzzy while preserving inflections, exact and CJK lookups, restoration of both policies after reopening, and return to the original reading location. The migration test loads the earlier paired settings and changes each policy independently.

The Reader implementation ranks the candidate spellings while streaming each selected index and keeps the senses for the best available spelling within its existing result and byte limits. Tests cover direct entries, aliases, distinct results across dictionaries, casing, per-book persistence, selecting a word through the reading UI, and returning to that passage.

## Original EPUBs on the Oasis

An independent scan of the four installed StarDict dictionaries found 64 inflected word candidates in eight original EPUBs. For 55 selections, installed sdcv found no exact entry and returned the independently identified roots in ordinary lookup. These covered eight suffix families. On both the Mac and Oasis, Reader opened each original chapter, selected the word with a long press and displayed the same dictionary, headword and visible definition for all 77 results. Closing every popup restored the passage and its selection. The original EPUBs, dictionary files, saved Reader data and Browser session retained their checksums.

The median physical lookup was 0.94 seconds with other device checks running. The observed range was 0.87–9.97 seconds. The comparison covers the 55 word-form cases. The following comparison covers the remaining suggestions.

## Similar spellings

Reader compares Unicode characters and allows an adjacent transposition, an insertion, a deletion or a substitution. It uses sdcv’s cutoff of fewer than three edits and gives closer spellings priority. Each suggestion can open definitions from multiple enabled dictionaries. CJK selections use literal lookup and supported Japanese forms; they do not use edit-distance spelling suggestions. Closing the popup or putting the Kindle to sleep cancels its background search. [sdcv suggestion ranking](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/stardict_lib.cpp#L1269-L1355) · [KOReader dictionary lookup](https://github.com/koreader/koreader/blob/v2026.07.1/frontend/apps/reader/modules/readerdictionary.lua)

![Reader suggesting a headword after two adjacent letters were transposed](screenshots/dictionary-spelling.png)

Nine further long presses in five original EPUBs exercised suggestions across four installed dictionaries. Reader on the Mac and the Oasis returned all 82 distinct definitions found by installed sdcv. Each result was checked by dictionary, headword and a digest of the full visible definition. Closing every popup restored the selected passage.

One selection also displayed a direct headword that sdcv missed. An independent scan found its index record and verified its definition bytes in the compressed dictionary. The index had 108 entries out of sort order relative to their neighbors. For this headword, sdcv’s binary search selected index page 2, while the valid entry was on page 49. Reader’s sequential scan recovered it. [sdcv index search](https://github.com/Dushistov/sdcv/blob/v0.5.5/src/stardict_lib.cpp#L687-L745)

On the Oasis, the median time to the final suggestions was 3.25 seconds across these nine selections. The range was 0.90–18.35 seconds. The popup remained dismissible while suggestions were loading. The source documents, dictionaries and saved reading and Browser data retained their checksums.
