# Unsupported EPUB spine items

KOReader's CREngine can retain an unsupported spine item as a placeholder in
relaxed-spine mode ([pinned implementation](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1879-L1897)).
Reader now keeps a local unsupported reflow spine item in its original order and
shows an explanatory notice, provided at least one linear spine item declares a
supported reflow format. This does not establish that the supported item itself
is readable. Fixed or mixed-layout books keep their existing restrictions; no
remote resource or manifest fallback is fetched. The unsupported payload is
not opened to display the notice.

Publisher page targets, CFI conversion, KOReader pointer conversion and source
HTML inspection do not treat the notice as publisher text. Reading progress
uses a fixed weight of one for each unsupported item instead of its binary or
missing member size; progress remains an approximation. Character-count page
generation stops with an explanation rather than counting the notice. Ordinary
automatic reading-position updates, bookmark creation, highlight creation and
the preference transactions updated with this change retain the last saved
real position while the notice is visible. An unset position stays unset. If a
notice anchor is saved by another path and later does not resolve, restore
fails closed rather than falling back to a page in newly supported content.
Quote reconstruction and highlight creation or extension reject ranges that
include an unsupported spine item, including ranges with raw whitespace
anchors. Duplicate endpoint paths are handled conservatively.

The guard uses the existing 64-bit source revision to recognize notice anchors;
it is not a cryptographic identity. Other annotation and navigation flows,
cross-chapter selection behavior and old or imported state still require integrated
verification. A notice does not implement or convert the unsupported format,
and this increment has not yet been behaviorally tested against real EPUBs,
KOReader, the simulator or a Kindle.
