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

Raw bookmarks in the currently shaped chapter are counted immediately when the
current probe verifies them. A raw highlight entirely within that chapter is
counted when both endpoints resolve there. Other raw marks are checked by a
cancellable worker, in slices of at most four marks and 128 scanned entries;
successful results update chapter counts without moving the reader. The worker
verifies both highlight endpoints and their order, including cross-chapter
selections, but does not reconstruct their full quote. It checks the captured
book and font sources, cancels on map exit, sleep, save or storage return, and
discards results if the source or layout snapshot changes. Pending and
unresolvable marks are omitted. Exact continuation, per-page placement and
revision-aware attribution of ordinary non-raw anchors remain open.

This increment has not yet been validated in simulation or on the Kindle.
