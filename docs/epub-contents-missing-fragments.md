# EPUB Contents with missing targets

When a publisher's EPUB Contents row names a missing fragment, Reader can open
the target chapter's beginning and show a notice. The installed KOReader's
pinned CREngine uses a similar chapter-start fallback for unresolved NCX
fragment targets ([source](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp)).

The check is lazy and applies only to Contents rows. Reader first lays out the
target normally; a resolving fragment follows the existing anchored jump. If
it does not resolve, Reader reopens the chapter under the existing bounds,
requires its source digest to match the layout, and checks the complete XML
DOM for the requested ID or legacy anchor name. A present but hidden or otherwise
unresolvable target remains an error. Missing, changed, unsupported or SVG
chapters also do not use this fallback. Normal in-document links, publisher
page-list links and non-EPUB formats retain their existing behavior. Navigation
history is preserved and the original publication and saved anchors are not
rewritten.

This fallback is broader than the observed NCX path because it also applies to
EPUB 3 navigation entries. Malformed documents that cannot pass the bounded XML
check remain unavailable. Behavioral comparison with KOReader, real EPUBs,
simulator and Kindle testing remain pending.
