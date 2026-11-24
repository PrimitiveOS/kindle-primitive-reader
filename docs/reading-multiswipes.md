# Reading multiswipes

Open Reading controls → Reading gestures and turn on **Multiswipes**. Start in the
middle of the reading area and draw a shape with one finger. Keep your finger
down through each corner and lift to run the action. For example, ↑ → opens
Contents, ↑ ← opens Bookmarks, and ↓ ↑ opens the page browser.

**Preset multiswipes** contains 35 shapes. Open a shape to change its action,
choose **Do nothing**, let normal swipe handling continue with **Pass through**,
or restore its default. **Custom multiswipes** contains
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
priority, interruption, and a fixed PDF in a landscape viewport.

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
reserves the edge zones for the existing display and menu gestures and limits
custom shapes to 64 saved assignments. Each shape can run one Reader action.
Presets for fixed-document reflow and fit-to-content and for system Wi-Fi can
be assigned an available Reader action. Device input and e-ink behavior still
require hardware verification.
