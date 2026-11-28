# Multi-touch reading gestures

## KOReader behavior at the pinned revision

KOReader exposes four two-finger corner taps, eight directional two-finger
swipes, spread, pinch, and clockwise or counter-clockwise rotation. Reader
defaults bind east to Contents, west to Bookmarks, north and south to
frontlight up/down when a light is available, and spread/pinch to font size
up/down. Corner taps, diagonal swipes, and rotations have no default action.
Two-finger swipes use the full screen. Corner taps use the same eighth-screen
rectangles as the single-finger corner gestures.

The two-finger tap duration defaults to 300 ms and is configurable from 100 to
2,000 ms. KOReader accepts a tap only when both contacts finish before the
configured duration and each contact moves less than 20 DPI-scaled pixels on
both axes. It scales the two-finger tap threshold from 20 dp and its pan
threshold from 35 dp; on the Oasis 3 at 300 DPI these become 38 px and 66 px.
Swipe recognition also uses the 900 ms swipe interval. Direction sectors use
the same 0.577 and 1.732 diagonal boundaries as KOReader.

Sources: [KOReader gesture defaults](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/defaults.lua),
[gesture manager and zones](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/main.lua),
[gesture detector](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/gesturedetector.lua),
and [Oasis 3 device profile](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/kindle/device.lua).

## Local implementation candidate

The shared input layer tracks Linux type-B slots and reports a completed
two-contact gesture separately from ordinary input. The Reader exposes all 16
bindings in Reading gestures and stores them in `reading-gestures.json`. Its
default actions follow KOReader. Spread/pinch changes reflow font size or fixed
page zoom. The default tap duration is 300 ms and saves a 100–2,000 ms range.
Tap, pan, swipe interval, direction-sector, corner-zone, and full-screen swipe
rules now follow the pinned detector for the Oasis 3 profile.

Touch-lock, selection, and open reading overlays block assigned gesture
actions. A classified multi-touch sequence suppresses duplicate single-touch
events. If a two-finger tap misses the movement or duration rule before
KOReader's 500 ms hold state, the input layer replays a normal tap at the first
lifted contact; a tap held into the two-finger hold state has no default action.
Contacts that lack their own position are not used to invent multi-touch data;
when KOReader's Kindle `cyttsp` quirk reports an empty second contact, the valid
first contact is replayed as an ordinary single-touch event. Sequences with
more than two contacts are rejected. These input rules and the gesture
dispatcher are source changes only; they have not been compiled or exercised.

## Remaining parity and verification work

- Confirm that the Oasis 3 controller exposes two usable type-B slots and that
  the event parser pairs their lifetimes and positions as expected.
- Compare how the Reader's completed-event dispatch behaves against KOReader's
  live two-finger pan lifecycle, especially when contacts lift at different
  times or a swipe lasts beyond the 900 ms interval.
- Exercise top/bottom bars, page Exit, touch-lock, selection, overlays, rapid
  page turns, sleep/wake, and USB interruption so one multi-touch sequence
  cannot trigger a duplicate page action or strand the Reader.
- The Reader currently pins an earlier `kindle-primtitive-tools` revision that
  does not export `MultiTouch`. Publish the shared API change and update the
  Reader dependency pin before compiling the integrated Reader.
- Unit and functional tests have been authored but not run. Simulator,
  screenshots, Kindle, release, and publication checks remain deferred to the
  verification pass.

The shared input code uses the Linux type-B slot protocol. The [kernel
documentation](https://www.kernel.org/doc/html/latest/input/multi-touch-protocol.html)
describes type A as obsolete and type B as the current slot-based protocol.
