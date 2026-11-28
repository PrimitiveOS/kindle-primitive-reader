# Reading multiswipes

Open Reading controls → Reading gestures and turn on **Multiswipes**. Start in the
middle of the reading area and draw a shape with one finger. Keep your finger
down through each corner and lift to run the action. For example, ↑ → opens
Contents, ↑ ← opens Bookmarks, and ↓ ↑ opens the page browser.

**Preset multiswipes** contains 35 shapes. Open a shape to change its action,
choose **Do nothing**, let normal swipe handling continue with **Pass through**,
or restore its default. **Manage action sequence** adds, reorders, and removes
actions and selects whether the gesture runs every action in order, cycles one
action per repetition, or opens a quick menu over the page. The quick menu can
show each action with a matching icon and label, run one selected action or the
full sequence, appear at the center, an edge or corner, or near the gesture, and
remain open after actions that leave the reading view in place. Each action can
have a custom menu label and an optional separator.
A sequence can contain up to 16 actions. Existing single-action settings remain
valid and open as one-action sequences. **Custom multiswipes** contains
saved shapes. In **Record a multiswipe**, trace a shape, check the direction
preview, then tap **Save multiswipe** and choose an action. Each custom shape has
a removal confirmation. **Gesture overview** shows the active assignments.
Your choices persist across restarts, including after you turn multiswipes off.

## Simulator review

The [pinned KOReader visual reference](koreader-sdl-reference.md) identifies these
controls as T3-5.1.1 through T3-5.1.3 and T3-5.2. The [SDL replay](../scripts/koreader-multiswipe-check.lua)
exercises the stock detector, toggle, presets, default reset, pass-through,
duplicate handling, custom removal and overview in the pinned Mac runtime. The Reader tests drive
`primitive_app::Lifecycle::pan` and tap the visible app controls. They verify
cardinal and diagonal grouping, the strict 94-pixel threshold at 300 DPI,
release-only dispatch, saved assignments, continuous scroll rollback, edge
priority, interruption, and a fixed PDF in a landscape viewport. New, unrun
regression cases cover full-screen edge-start recognition, accepting 32
direction legs and rejecting a 33rd without matching the 32-leg prefix. New
authored, unrun regressions cover backward-compatible single-action profile
loading, persisted sequence ordering and mode, one-by-one cycling, reordering,
removal, quick-menu selection and paging, and persistence of menu position,
keep-open preference, action labels and separators.

[Stock recorder](screenshots/stock-multiswipe-recorder.png) ·
[Gesture settings](screenshots/multiswipe-gesture-settings.png) ·
[Presets](screenshots/multiswipe-presets.png) ·
[Recorded shape](screenshots/multiswipe-custom-recorded.png) ·
[Dark recorder](screenshots/multiswipe-custom-recorded-dark.png) ·
[Gesture overview](screenshots/multiswipe-gesture-overview.png) ·
[Landscape PDF Contents](screenshots/multiswipe-landscape-pdf-result.png)

Reader measures the 900 ms window from the first movement reported by the
Primitive input toolkit. KOReader measures it from contact down. The toolkit
coalesces closely spaced movement samples at 200 ms and supplies the last
finger-down point at release, so quick intermediate bends can be missed. Reader
recognizes multiswipes across the full display. Ordinary one-direction swipes
still fall through to the existing display, menu and page-turn handling. Reader
limits custom shapes to 64 saved assignments and stores up to 32 direction legs
per shape; a thirty-third distinct leg rejects the whole stroke instead of
silently truncating it to a different, actionable shape. KOReader's detector
has no fixed leg cap, although its 900 ms window and touch sampling constrain
real gestures. Exact long-stroke parity remains to compare. Each shape can run
multiple Reader actions in order, one at a time, or through the quick menu;
KOReader's dispatcher also supports action lists, custom quick-menu labels,
separators, configurable placement and keep-open behavior. Reader now represents
those quick-menu options in its native profile, while keeping the detector and
actions bounded to Reader workflows rather than exposing KOReader's plug-in
dispatcher. Action-sequence screenshots and interaction captures remain pending
the later simulator pass.
Presets for fixed-document reflow and fit-to-content and for system Wi-Fi can
be assigned an available Reader action. Device input and e-ink behavior still
require hardware verification. [Pinned KOReader multiswipe accumulator](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/gesturedetector.lua#L927-L970) · [Pinned KOReader action dispatcher](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/dispatcher.lua)
