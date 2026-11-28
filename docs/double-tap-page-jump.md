# Double-tap page jumps

KOReader's gesture detector recognizes two taps within its configurable time
window and distance threshold. The pinned Reader defaults use the left side to
jump back ten pages and the right side to jump forward ten pages. PrimitiveOS
Reader now has the same optional reading gesture for reflowable and fixed-page
documents.

The feature is off by default. In **Settings → Gestures**, enable **Double tap
page jump**. The interval defaults to 300 ms and can be set from 100 to 2,000
ms. While enabled, a qualifying first tap is held until the interval expires;
if no second tap arrives, Reader replays the original tap through its normal
handling. Two taps within 50 screen pixels use the second tap's half of the
page to choose the jump. The left and right double-tap gestures can be assigned
to other Reader actions independently.

The single-contact input API does not provide two-contact events, so pinch,
spread, and two-finger tap or swipe remain separate input-layer gaps. This
implementation is a source-aligned candidate only. It has authored unit and
Reader workflow tests, but they have not been run; simulator, Kindle, timing,
refresh, and accessibility acceptance remain open.

Reference sources: [pinned KOReader gesture detector](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/device/gesturedetector.lua) · [gesture controls](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/main.lua) · [default reader gesture bindings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/gestures.koplugin/defaults.lua)
