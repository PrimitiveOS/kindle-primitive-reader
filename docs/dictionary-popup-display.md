# Dictionary popup display

Open **Search › Dictionaries** to set how definitions appear. Reader saves
these preferences for future lookups.

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

## Definition font size

Tap **Dictionary font size** to change definition text from 8 to 32.
Tap the minus or plus to move by one, or hold to move by four. **Default**
sets the draft back to 20. **Apply** saves the size. **Cancel** discards the
draft. Reader scales the definition line height and paragraph markers with
the text and keeps its established popup controls.

The Mac checks cover portrait and landscape editing, backup, reopening,
selection and nested lookup after a definition wraps differently. They also
check that the same EPUB passage and PDF text geometry return after the
popup closes. The pinned Lua replay runs KOReader's menu callback, step
handler, clamping and popup font selection at both limits.
[KOReader settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L408-L430) · [KOReader popup](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L212-L219) · [Stock comparison](../scripts/compare-dictionary-font-size.py)

[Portrait editor](screenshots/dictionary-font-size-editor.png) ·
[Landscape editor](screenshots/landscape-dictionary-font-size-editor.png) ·
[Size 32](screenshots/dictionary-font-size-large.png) ·
[Size 8](screenshots/dictionary-font-size-small.png)

## Arrange popup buttons

Open **Search › Dictionaries › Customize buttons**. In **Sort and toggle
buttons**, select an action to show or hide it and to move it earlier or
later. Set **Max buttons in row** to a value from one to four. **Test button
layout** shows the arrangement over a sample definition. **Apply layout**
saves the draft for future lookups, and **Restore default layout** returns
to Reader's usual controls.

The supported actions include navigating results, saving a word, Find,
highlighting, searching the book, Wikipedia, Translate, expanding the
definition, and closing it. Previous and Next get narrower when paired in
a four-button row. Layouts that use more than three rows have action pages
to leave room for the definition. The top Done or Back control remains
reachable. Translate opens the usual confirmation before sending the word
to a provider.

The pinned Lua comparison runs KOReader's sort and toggle callbacks, row
limits, preview, paired arrow sizing and reset. Reader tests exercise the
saved draft and reopening, the reordered controls, action paging, the
translation confirmation and return to the original reading passage.
[Stock layout editor](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L504-L734) · [Stock button geometry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1019-L1134) · [Executable comparison](../scripts/compare-dictionary-button-layout.py)

[Layout settings](screenshots/landscape-dictionary-customize-buttons.png) ·
[Dark preview](screenshots/landscape-dictionary-button-layout-preview.png) ·
[Paged actions during a lookup](screenshots/dictionary-popup-custom-buttons.png)

## Closing nested lookups

Tap **Back** to return to the previous definition. Hold the same control to close
the lookup stack and return to the selected passage or original dictionary
editor. The hold also works with Find active or the Results chooser open. An
armed highlight becomes one saved mark for the original selection. The nested
popup's status line shows the hold gesture. [Desktop capture](screenshots/dictionary-close-stack.png)

The stock comparison executes KOReader's pinned close callbacks in Lua 5.1 with
test doubles for the window manager and selection. It verifies that a tap closes
one window, a hold closes the open Results menu and all three test windows in
reverse order, and the original armed highlight is saved once. Reader's desktop
tests exercise the same input actions on authored EPUB and PDF passages and
check the original query editor's text and caret. [KOReader close callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1618-L1672) · [Stock comparison script](../scripts/compare-dictionary-close-stack.py)

Run the comparison with a Python environment providing `lupa` and the pinned
`dictquicklookup.lua` file.

```sh
python3 scripts/compare-dictionary-close-stack.py /path/to/dictquicklookup.lua
cargo test --locked --lib dictionary_hold_close
```

[Numbered senses and bilingual examples in HTML dictionary articles](dictionary-html-articles.md)


## First and last results

Tap **Previous** or **Next** to move one definition. Hold either button
to jump to the first or last result in the lookup. At the edge, Reader
hides the unavailable direction. When Find is active, the same tap
controls move between matches in the current definition.

An armed highlight stays attached to the book's selected passage when
switching definitions, including when choosing an entry from Results.
Closing the lookup saves that highlight once. The test uses three
authored definitions, rotates an open popup, and checks the saved
selection, original reading position and landscape button feedback.

The comparison runs the pinned KOReader button callbacks and
`changeDictionary` in Lua 5.1. It exercises a single step, both held jumps,
the disabled boundaries and the armed highlight across the result changes.
[KOReader buttons](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L799-L838) ·
[Comparison script](../scripts/compare-dictionary-result-navigation.py) ·
[Last result with the highlight armed](screenshots/landscape-dictionary-last-result.png)


The Oasis build `d6e61c3` opened a local lookup with three authored
StarDict results. Held Next and Previous reached results 3 and 1.
The framebuffer showed that **Unhighlight** stayed active at both
ends. Closing the last result saved one highlight for the original
word and returned to the reading page. The test used injected contacts
through the Oasis touch input. [Last result on the Oasis](screenshots/oasis-dictionary-last-armed.png)
