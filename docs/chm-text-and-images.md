# CHM text and illustrations

Reader can use a CHM `#SYSTEM` default-font charset when its locale is missing
or unrecognized. Recognized locale code pages keep priority; an unknown font
charset falls back to Windows-1252. This helps documents whose system metadata
uses a non-Western font but omits a usable locale. The supported font charset
numbers match the pinned KOReader CREngine fallback. [CREngine locale and font handling](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/chmfmt.cpp#L685-L745)

Each topic and contents file can also declare its own HTML charset using a
`meta charset` or `http-equiv="content-type"` tag in the first 1,024 bytes.
A byte-order mark takes precedence, followed by a supported HTML declaration,
then valid UTF-8 or the archive's selected encoding. The bounded detector
examines HTML tags rather than text in comments or scripts. Reader continues
to report malformed byte sequences rather than silently replacing them.

Local GIF illustrations now join PNG and JPEG resources. Reader uses its
existing bounded, first-frame GIF decoder; remote images and unsupported image
formats remain unavailable. External scripts and active content are not run.
KOReader registers CHM through CREngine and parses its topics as HTML. [KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1588) · [CREngine HTML parsing](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/chmfmt.cpp#L876-L929)

An otherwise empty topic can now retry with locally referenced BMP images,
using the existing bounded image decoder and CHM's 4 MiB per-resource limit.
BMP signatures and decoded dimensions are checked when the image is read;
bad or oversized images cannot render. Topics that already had supported text
or images retain their original generated XHTML and anchors, so BMPs in those
mixed topics remain unavailable. External images are still rejected. This
fallback has not been behaviorally compared with KOReader or verified in the
simulator or on Kindle.

This implementation has not yet been compared with KOReader on sample CHM
files or tested in the simulator or on Kindle. Other CHM variants, merged
help files and active content remain outside the current adapter.
