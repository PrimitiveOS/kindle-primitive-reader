# Dictionary popup display

Open **Search › Dictionaries** to set how definitions appear. Reader remembers
both choices for future lookups.

- **Large window** opens the definition in the expanded panel, with more lines
  visible at once. It starts off. **Expand** and **Collapse** change the current
  lookup without changing the saved preference.
- **Justify text** widens spaces between words on wrapped lines. It starts on.
  The last line of each paragraph keeps its natural spacing. Find highlights and
  long-press word lookup follow the visible text positions.

The compact popup keeps its placement relative to the selected passage. A lookup
opened from a word in a definition inherits the window size. Back restores the
previous result and its position. Closing the last lookup returns to the
selected passage or the screen where the lookup started. [KOReader preferences](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L390-L407) · [KOReader popup layout](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L322-L330) · [KOReader definition text](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1228-L1240)

Desktop tests use authored StarDict definitions, an EPUB and a PDF. They cover settings
saved by an earlier release, reopening Reader, result selection, nested lookups
and restoring the original selection. Text tests cover paragraph endings,
nonbreaking spaces and mixed left-to-right and right-to-left word positions.
[Settings](screenshots/dictionary-popup-settings.png) · [Compact with justified text](screenshots/dictionary-justify-text.png) · [Large window in dark mode](screenshots/dictionary-large-window.png) · [Workflow tests](../src/app/dictionary_panel/tests.rs)

[Numbered senses and bilingual examples in HTML dictionary articles](dictionary-html-articles.md)
