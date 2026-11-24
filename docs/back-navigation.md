# Back navigation

Open **Settings › Navigation settings** in the reading toolbar, or **Controls ›
Navigation settings** in the file browser, to choose what Back does. Use **Assign a Back gesture** and select
**Back (navigation settings)** for a tap, hold or swipe. The physical page buttons
still turn pages while reading. The footer Exit and the file browser Exit remain
explicit ways to leave. [Simulator settings](screenshots/back-navigation-settings.png) ·
[Simulator confirmation](screenshots/back-exit-confirmation.png)

| Setting | Choices and behavior |
| --- | --- |
| Back to exit | Ask before exiting (default), exit immediately or don't exit. Cancel keeps the current document and position. |
| Back in file browser | Follow the exit choice (default) or go to the parent folder. Parent navigation stops at Documents. |
| Back in Reader | Return to a previous jump (default), undo a recent change of page or view, return to the book's folder or follow the exit choice. |

Back skips jump history markers for the current location. In reflowable books it
compares source anchors. In fixed documents it compares page numbers. Choose
**Go to previous read page** to restore same-page zoom, rotation, crop or pan.
[Reader Back options](screenshots/back-reader-options.png) ·
[Same-page fixed view restored](screenshots/back-fixed-view.png)

If a history is empty, Back first shows a notice. The next Back can exit or ask,
according to the exit choice. With exit disabled, repeated presses only show the
notice. Ordinary page turns use the separate recent-view history when that mode
is selected. A Back restoration does not push the abandoned view onto that
history. The temporary stack holds up to 5,000 distinct visits per open book.

## Comparison and verification

KOReader's dispatch table defines Back in both the reader and the file manager.
`ReaderBack` records changes after page or scroll updates, restores recent views
separately from the link jump stack and applies the shared exit policy after the
empty-history notice. `ReaderLink` skips the current location. `ReaderPaging`
restores the preceding view on the same page, including its zoom and rotation.
`FileChooser:onBack` selects parent navigation or the exit policy. These paths
cover stock SDL controls `T3-6.1.*` through `T3-6.3.*`.
[Back dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L158-L189) · [Reader Back](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerback.lua#L82-L191) · [Jump history](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerlink.lua#L989-L1019) · [Fixed view restore](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerpaging.lua#L602-L643) · [File browser Back](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanager.lua#L450-L472)

The native Mac replay exercises each stock menu choice, both Back routes,
the jump stack, same-page fixed view restoration and Backspace setup in SDL.
Reader's Mac tests dispatch actual taps, swipes and physical-button events
through the app and open EPUB and PDF documents. They verify settings
persistence, distinct jump history, exact same-page restore, folder navigation,
the exit confirmation and direct footer Exit. On changed layouts recent views
use the saved source anchor or scan point where available. Across fixed scroll
and page modes, Back returns to the saved page in the current viewing mode.
[Stock confirmation](screenshots/stock-back-confirmation.png) ·
[Native Mac replay](../scripts/koreader-back-navigation-check.lua) ·
[Reader input and tests](../src/app/back_navigation.rs)

Replay the Reader tests with `cargo test --locked --lib app::back_navigation::tests`.
Set `READER_BACK_READER_CHOICES`, `READER_BACK_FIXED_VIEW` and
`READER_BACK_CONFIRMATION` to PNG paths to capture Reader's frames. Run the Lua
replay in the pinned Mac runtime, passing a scratch directory for its frame.
Use `EMULATE_READER_W=1264`, `EMULATE_READER_H=1680`, and
`EMULATE_READER_DPI=300` to reproduce the reference frame at Oasis size.

Stock `T3-6.4` applies to keyboards with a Backspace key, shown only when the
device has a keyboard without a Sym key. The Kindle app kit currently delivers
touch, physical page buttons and lifecycle events. A physical Backspace mapping
requires a distinct input event in that kit. Gesture and refresh acceptance on a
Kindle remains pending. [Stock Backspace capability gate](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/common_settings_menu_table.lua#L489-L501) · [Pinned app input contract](https://github.com/PrimitiveOS/kindle-primtitive-tools/blob/86e593eb865b56615108406d8a7bf7bd20a38c6d/crates/kl-core/src/ui.rs#L54-L77)
