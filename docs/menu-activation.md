# Reader menu activation

Open **Settings › Menu activation** in Reader. **With a tap** and
**With a swipe** control how the top and bottom menus open. If you
turn off the last enabled gesture, Reader enables the other one.
**Auto-show bottom menu** opens the bottom controls when the top menu opens.
The bottom controls can still be opened directly when auto-show is off.
[Reader controls](screenshots/reader-menu-activation.png) ·
[Top menu only](screenshots/reader-menu-top-only.png) ·
[Both menus in a landscape PDF](screenshots/reader-menu-auto-bottom-landscape.png)

The screen-edge zones use KOReader's one-eighth height across the width
and one-fifth height across the middle half. The upper-center tap still opens
Reader's navigation shortcuts. The reading footer, Exit, selected text,
and document links retain their own input routes. Scroll previews return
to the original position when the contact opens a menu.

New profiles start with both gestures on and auto-show enabled. An older
profile that opened both toolbars only on an upper-center tap keeps that
behavior until **Auto-show bottom menu** is changed. The setting then
applies to all top-menu entry points. Reader evaluates swipes on release.

The pinned native KOReader replay uses its actual setting callbacks,
registered zones, top and bottom gesture handlers, and native settings
rows. The Mac Reader replay tests the controls on EPUB and landscape PDF,
saved choices after reopening, the footer's priority, and backward
compatibility for the older toolbar preference. Physical Kindle testing
remains open.
[KOReader setting callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/menu_activate.lua#L8-L48) ·
[Stock replay](../scripts/koreader-menu-activation-check.lua) ·
[Stock settings](screenshots/stock-menu-activation.png) ·
[Reader tests](../src/app/menu_activation_tests.rs)
