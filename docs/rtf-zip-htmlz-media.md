# Compressed RTF and HTMLZ illustrations

Reader recognizes `.rtf.zip` files and opens a single enclosed RTF document
through the same reading pipeline as a plain `.rtf` file. Its compressed file
and expanded text each have the existing 4 MiB RTF limit. A ZIP may contain
at most 64 entries; ambiguous, encrypted, linked, unsupported or oversized
document entries are rejected. Other archive files are not extracted. The
title derived from the filename omits both `.rtf` and `.zip`.

KOReader registers `rtf.zip` with CREngine. CREngine can select from several
eligible document formats in an archive, preferring a default HTML document;
Reader's adapter deliberately accepts exactly one RTF document. Reader's import
retains its current RTF subset; images, field instructions and other hidden
destinations are still not rendered. [KOReader format registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1617-L1618) · [CREngine archive selection](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvdocview.cpp#L4712-L4793)

HTMLZ archives now retain local GIF and SVG assets as well as the existing
PNG, JPEG and CSS. GIF uses Reader's first-frame decoder and SVG uses its
bounded offline vector renderer. The existing archive resource limits apply.
Scripts and remote resources do not become available through this change.
Other HTMLZ layout and archive restrictions remain in place. [KOReader HTMLZ registration](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/credocument.lua#L1604)

These changes have not yet been tested in the simulator, compared with
KOReader on matching files, or verified on Kindle.
