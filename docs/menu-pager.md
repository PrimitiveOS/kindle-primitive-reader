# First and last menu pages

Tap **Previous** or **Next** to move one page in a settings list or in Documents.
Hold **Previous** and lift to go straight to the first page. Hold **Next** and lift
to go straight to the last page. The jump stays in the current list and keeps the
reading position. The physical page buttons still move one page at a time within
the list. At the first page of a Documents folder, the physical back button goes
up a folder. [First settings page](screenshots/menu-pager-first.png) ·
[Last settings page](screenshots/menu-pager-last.png) ·
[Last Documents page in landscape](screenshots/documents-pager-last-dark.png)

KOReader’s top menu assigns tap and hold callbacks to its Previous and Next
chevrons. Its hold callback runs as soon as the gesture is recognized and consumes
the release. A chevron at its corresponding edge is disabled. KOReader’s separate
File Manager list offers explicit First and Last chevrons. [Top menu callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/touchmenu.lua#L546-L559) ·
[Hold dispatch](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/button.lua#L548-L576) ·
[File Manager arrows](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/menu.lua#L749-L774)

The [native KOReader replay](../scripts/koreader-menu-pager-check.lua) dispatches
tap, hold, and release events to the rendered chevrons on an authored five-page
menu. The [Reader simulator tests](../src/app/navigation_layout/pager_tests.rs)
drive the regular hold-start and hold-release input in Reading controls and a
landscape Documents folder. They verify single-page taps, direct jumps, disabled
edge controls, and the preserved reading position. [Stock first page](screenshots/stock-menu-pager-first.png) ·
[Stock last page](screenshots/stock-menu-pager-last.png)

Run `cargo test --locked --features kindle --lib menu_footer_hold`. Set
`READER_SELECTION_VIEWPORT_CAPTURES` to an output directory to write the Reader
captures during the tests.
