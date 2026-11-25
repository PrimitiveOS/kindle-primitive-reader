# Reflow Book Map saved-mark counts

The reflow Book Map reports bookmarks and highlights per chapter. It also
reports a note count when a chapter contains nonblank notes attached to those
marks. Notes are included in the bookmark or highlight count as well. The
display is read-only and does not change reading position, saved marks, or the
reading footer.

The pinned KOReader Book Map uses distinct saved-mark and note indicators in
its page map (`frontend/ui/widget/bookmapwidget.lua`, bookmark marker drawing).
Reader currently shows chapter-level counts rather than reproducing those
per-page glyphs.

Anchored marks are associated using the current publication identifier and
chapter path, preferring the first matching chapter if paths repeat. A legacy
bookmark without an anchor uses its saved chapter index when that index is in
range. Non-raw anchors are not validated against the source revision. A
highlight is counted in its starting chapter, rather than repeated for every
chapter it spans.

Raw bookmarks count only in the currently shaped chapter when the current
probe verifies the anchor. A highlight containing a raw endpoint counts only
when both endpoints resolve in that same chapter. Raw anchors in other
chapters, cross-chapter raw highlights, and unresolved anchors are omitted;
the map does not load additional chapters solely to compute its counts.
Exact continuation and revision-aware chapter attribution remain open.

This increment has not yet been validated in simulation or on the Kindle.
