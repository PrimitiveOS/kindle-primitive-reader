# Back navigation

Open **Reading controls → Navigation settings** to choose what Back does in a book
and in the file browser. Use **Assign a Back gesture** and select
**Back (navigation settings)** for a tap, hold or swipe. The physical page buttons
still turn pages while reading. The footer Exit and the file browser Exit remain
explicit ways to leave. [Simulator settings](screenshots/back-navigation-settings.png) ·
[Simulator confirmation](screenshots/back-exit-confirmation.png)

| Setting | Choices and behavior |
| --- | --- |
| Back to exit | Ask before exiting (default), exit immediately or don't exit. Cancel keeps the current document and position. |
| Back in file browser | Follow the exit choice (default) or go to the parent folder. Parent navigation stops at Documents. |
| Back in Reader | Return to a previous jump (default), undo a recent change of page or view, return to the book's folder or follow the exit choice. |

If a history is empty, Back first shows a notice. The next Back can exit or ask,
according to the exit choice. With exit disabled, repeated presses only show the
notice. Ordinary page turns use the separate recent-view history when that mode
is selected. A Back restoration does not push the abandoned view onto that
history. The temporary stack holds up to 5,000 distinct visits per open book.

## Comparison and verification

KOReader's dispatch table defines Back in both the reader and the file manager.
`ReaderBack` records changes after page or scroll updates, restores recent views
separately from the link jump stack and applies the shared exit policy after the
empty-history notice. `FileChooser:onBack` selects parent navigation or the exit
policy. These paths cover stock SDL controls `T3-6.1.*` through `T3-6.3.*`.
[Back dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L158-L189) · [Reader Back](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerback.lua#L82-L191) · [File browser Back](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L450-L472)

The pinned Lua oracle runs both stock handlers. Reader's automated Mac tests
dispatch actual taps, swipes and physical-button events through the app, open
EPUB and PDF documents, and verify settings persistence, exact same-page return,
folder navigation, the exit confirmation, and direct footer Exit. On changed
layouts, recent-view restores use the saved source anchor or scan point where
available and otherwise use the saved page. The stock Backspace mapping
`T3-6.4` needs a distinct keyboard Back event in the app kit. Gesture and
refresh acceptance on a Kindle remains pending. [Stock callback oracle](../scripts/compare-back-navigation.py) · [Reader input and tests](../src/app/back_navigation.rs)
