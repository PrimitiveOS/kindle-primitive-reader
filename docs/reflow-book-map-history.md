# Reflow Book Map navigation history

The reflow Book Map adds **Previous 1–3** and **Next 1–3** cues to chapter
rows when recent navigation-history entries point to those chapters. Number 1
is the nearest entry in that direction. If several entries from one direction
fall in the same chapter, only its nearest number is displayed; both directions
can appear together. Numbering follows the original three-entry stacks, so an
unresolvable entry may leave a gap. The cues are read-only and do not change
the stacks, chapter actions, reading position, or requested reading footer.

Anchored entries must identify the open publication and a known chapter; legacy
anchorless entries use a bounded chapter index. A raw anchor in the open
chapter appears only when the current probe verifies it. For other chapters,
a cancellable worker verifies at most six recent anchors, grouped by chapter,
using an isolated layout and source-stamp checks. Until verification succeeds,
those cues are omitted. Exiting the map, sleeping, saving, or returning from
USB cancels the job, and a changed source/layout snapshot rejects its results.
The non-raw chapter-level cue does not verify the source revision or its
position within a chapter. None of these changes has been validated in
simulation or on Kindle yet.

KOReader's [location-stack integration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L839-L841)
and [map legend](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L1454-L1461)
provide the reference; Reader currently uses chapter-row text instead of
KOReader's per-page numbered map badges.
