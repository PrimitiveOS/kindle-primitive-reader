# Page-turn controls

In Reader, open the top toolbar and choose **Controls › Page turns**. **With taps** and **With swipes** govern touch page turns independently. **Swipes always active** keeps horizontal page swipes available during **Lock page touch**. Turning off swipe turns suspends that exception until swipes are enabled again. Swipe controls leave the physical page buttons, linked text, and a continuous document’s ordinary vertical scroll available. [KOReader page-turn controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/page_turns.lua#L86) · [KOReader touch-lock filter](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/main.lua#L196)

**Tap zones** offers **Default**, **Left / right**, **Top / bottom**, and **Bottom / top**. Default divides the screen into a 25% backward region and a 75% forward region. Each custom shape shares the saved backward and forward percentages. Tap − or + to change one percentage point and hold to change five. Apply saves both values. When the two add to more than 100%, the forward size takes precedence, as in KOReader. A smaller sum leaves room where taps do not turn a page. The calculation uses the accepted screen dimensions. Reading tools, selection, page links and Exit retain their touch targets. [KOReader tap geometry](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerview.lua#L1356) · [Control screen](screenshots/page-turn-controls.png) · [Zone sizes in landscape](screenshots/page-turn-zone-sizes.png)

**Invert page turn taps and swipes** changes horizontal gestures for the open edition. The vertical tap layouts keep their top and bottom roles. **Defaults for new books** has independent choices for touch direction and document dialogs. The native **Follow the publication** choice uses the edition layout’s direction. A saved tap-only mirroring preference from an earlier Reader build keeps its effect for that choice. Its switch appears in the defaults list while the older setting is active. The physical page-button inversion remains a separate setting.

**Invert document-related dialogs** mirrors the open edition’s Contents, Book Map, Page Browser and footer progress track. The Contents hierarchy expands from the opposite side while keeping its heading targets. Fixed-page grids start at the right. Track browsing, horizontal gestures and page controls follow the mirrored layout. The footer’s chapter and page caption keeps its existing separator, and Exit stays on the right. Skim’s buttons and direction indicators follow the same choice. [Mirrored Contents](screenshots/page-turn-mirrored-contents.png) · [Mirrored Page Browser](screenshots/page-turn-mirrored-browser.png)

## Mac verification

The pinned native replay exercises the stock menu callbacks, normal and filtered swipe dispatch for its reflow and fixed-page modules, all four tap shapes, the 100% clamp, horizontal inversion, document dialog mirroring and new-book defaults. [Replay script](../scripts/koreader-page-turn-settings-check.lua) · [Stock frame 078 in the control inventory](koreader-sdl-reference.md#gestures)

From the Reader worktree, run the focused simulator tests with the capture directory set to a scratch path. They use the EPUB and scanned-PDF fixtures and exercise the normal settings rows, page buttons, touch lock, exact tap boundaries, landscape settings, original reading positions, and persistence after reopening.

```sh
READER_SELECTION_VIEWPORT_CAPTURES=/tmp/reader-page-turn-captures \
  cargo test --locked --features kindle --lib page_turn_navigation
```

The Mac run passed all six new checks and 21 related controls, orientation and navigation regressions. The Kindle foreground input and E Ink checks remain open.
