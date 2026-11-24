# Corner hold gestures

Open Reading controls → Gestures to set **Ignore long-press on corners**.
While it is On, holding a corner uses the underlying reader action, including
text selection when there is a word at the contact. Tap and swipe shortcuts
keep their assignments. The setting is saved when Reader restarts.

For a temporary pause, assign **Pause corner holds** to a tap or swipe and
set **Corner hold pause**. The duration defaults to 10 seconds and accepts
5 to 30 seconds. Repeating the action during a pause keeps its original
deadline. The countdown belongs to the current Reader session.

A corner starts at the outer eighth of the reading screen in each dimension.
Reader reserves the visible footer for its own controls, including Exit.
An assigned physical hold runs once at hold start. If a pause expires
before the finger lifts, the running contact keeps its original behavior.

## Comparison and verification

The [Lua replay](../scripts/compare-corner-hold-gestures.py) checks the pinned
KOReader menu callbacks, corner geometry, dispatcher duration, and routing
back to the underlying hold handler. Reader's Mac tests also cover portrait
and landscape bounds, persistence, an assigned hold taking priority over text
selection, and a PDF corner hold that ends after a pause has expired.
[KOReader gesture routing](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/main.lua#L1310-L1381) · [KOReader duration settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua#L93-L94)

[Gesture settings](screenshots/corner-hold-gesture-settings.png) ·
[Corner text selection](screenshots/corner-hold-gesture-text-selection.png) ·
[Landscape pause duration](screenshots/landscape-corner-hold-pause.png)
