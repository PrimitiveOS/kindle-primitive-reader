# Physical page-turn inversion

Reader matches KOReader's global **Invert page-turn buttons** option. The saved
setting defaults Off. When enabled, Reader swaps hardware `PageForward` and
`PageBack` at the device-runtime boundary; touch taps, swipes, and Reader's
own generated page turns retain their original reading direction.

KOReader defines this option in
[`physical_buttons.lua`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/physical_buttons.lua#L11-L20)
and broadcasts `SwapPageTurnButtons`. Its footer's Page turning inverted item
is active for the global or side-specific hardware inversion and for a book
with inverse reading order in
[`readerfooter.lua`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L378-L381).
Oasis 3 has one previous/next pair and no DPad sides, so Reader implements the
global option only.

The optional **Page turning inverted** item is available in Reader's custom
status footer. It shows a regular or reversed arrow in Icons mode, the matching
double-arrow glyph in Compact mode, and `Pg: On` or `Pg: Off` in Letters mode.
The status follows the saved button setting and the open book's right-to-left
reading order.

Mac coverage verifies the default, persistence, hardware-key mapping, footer
states, and that non-button inputs are unchanged. The [light](screenshots/page-turn-inversion-settings.png)
and [dark](screenshots/page-turn-inversion-settings-dark.png) settings captures
and [light](screenshots/page-turn-inversion-footer.png) and
[dark](screenshots/page-turn-inversion-footer-dark.png) footer captures are
reviewed. Physical-button feel and the indicator on the foreground Kindle
remain part of the device acceptance run.
