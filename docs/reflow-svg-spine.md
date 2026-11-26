# SVG pages in reflowable EPUBs

The local Reader candidate can open a reflowable EPUB whose spine includes a
standalone `image/svg+xml` resource with an `.svg` path. It represents the SVG
as one image-only reading chapter using the original archive member and the
existing bounded SVG illustration renderer. Reader checks the member's byte,
XML-node and static-illustration limits before creating the chapter. The
renderer retains its own resource, dimensions and memory limits; artwork that
depends on unsupported SVG features or unavailable, unsafe or external
resources may not render. No external resource is fetched.

Existing HTML/XHTML chapters, fixed-layout pages, archive paths and publication
identities are unchanged. The synthetic chapter includes a fingerprint of its
source bytes in its revision, so changed SVG source can invalidate the old
synthetic chapter's anchors. The original archive is never rewritten. The
wrapper exposes the whole illustration, not individual SVG text, element IDs,
fragment destinations or SVG-internal links. SVG spine paths without the `.svg`
extension and mixed fixed/reflowable spines remain unsupported. This candidate
has not been behaviorally tested against real EPUBs, KOReader, the simulator or
Kindle; inspect images, source-change restoration and resource limits before a
compatibility claim.

Pinned CREngine wraps SVG spine content when its relaxed-spine path is active:
[SVG wrapper](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/epubfmt.cpp#L1853-L1876).
